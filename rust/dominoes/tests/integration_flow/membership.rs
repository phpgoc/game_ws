use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn join_reports_current_membership_to_the_dominoes_owner() {
    let (server, url, _stop) = start_server(
        "dominoes-membership-snapshot",
        test_handler_with_members(None, &["member-owner"]),
    )
    .await;
    let mut owner = connect_client(&url).await;
    let mut guest = connect_client(&url).await;
    let room = "dominoes-membership-snapshot";

    let owner_join = join(&mut owner, "member-owner", room).await;
    let guest_join = join(&mut guest, "guest", room).await;

    assert_eq!(owner_join["data"]["active_membership"], json!(true));
    assert_eq!(guest_join["data"]["active_membership"], json!(false));

    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn start_refresh_rejects_expired_owner_then_allows_renewed_owner() {
    let (handler, active_membership) = test_handler_with_membership_state(None, &["owner"]);
    let (server, url, _stop) = start_server("dominoes-start-membership-refresh", handler).await;
    let room = "dominoes-start-membership-refresh";
    let mut owner = connect_client(&url).await;
    let mut second = connect_client(&url).await;
    let mut third = connect_client(&url).await;
    join(&mut owner, "owner", room).await;
    join(&mut second, "second", room).await;
    join(&mut third, "third", room).await;

    active_membership
        .lock()
        .expect("test membership lock")
        .remove(&test_user_id("owner"));
    send_request(&mut owner, Routes::START as i32, json!({})).await;
    let expired = recv_until(&mut owner, "expired owner start response", |value| {
        value["route"] == json!(Routes::START as i32)
    })
    .await;
    assert_eq!(expired["code"], json!(WsResponseCode::NO_PERMISSION as i32));

    active_membership
        .lock()
        .expect("test membership lock")
        .insert(test_user_id("owner"));
    send_request(&mut owner, Routes::START as i32, json!({})).await;
    let renewed = recv_until(&mut owner, "renewed owner start response", |value| {
        value["route"] == json!(Routes::START as i32)
    })
    .await;
    assert_eq!(renewed["code"], json!(WsResponseCode::OK as i32));

    for (position, client) in [&mut owner, &mut second, &mut third]
        .into_iter()
        .enumerate()
    {
        select_starter(client, position as i32).await;
    }
    let started = recv_start_bundle(&mut owner, 0).await;
    assert_eq!(started.hand.len(), 5);

    server.abort();
}
