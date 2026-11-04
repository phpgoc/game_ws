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
