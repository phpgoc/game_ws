use super::*;
use share_type_public::{DominoesActionSource, DominoesPhase};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn disconnect_keeps_advancing_but_quit_allows_a_fresh_running_match() {
    let (server, url, _stop) = start_server("dominoes-lifecycle", test_handler()).await;
    let room = "dominoes-lifecycle";
    let mut owner = connect_client(&url).await;
    let mut second = connect_client(&url).await;
    let mut third = connect_client(&url).await;
    join(&mut owner, "owner", room).await;
    join(&mut second, "second", room).await;
    join(&mut third, "third", room).await;
    request_ok(&mut owner, Routes::START).await;
    // START 的响应在初始选牌事件之后，因此直接提交房主的首个选择。
    send_request(
        &mut owner,
        DominoesRoutes::SELECT_STARTER as i32,
        json!({ "slot_index": 0 }),
    )
    .await;
    recv_until(&mut second, "second player's selection window", |value| {
        value["code"] == json!(DominoesWsCode::STARTER_SELECTION as i32)
            && value["data"]["selector_position"] == 1
    })
    .await;

    // 真实断开传输；没有发送 QUIT。原座位和循环必须继续存在。
    second.close(None).await.expect("disconnect second player");
    recv_until(&mut owner, "second player disconnected", |value| {
        value["code"] == json!(WsCode::JOIN as i32)
            && value["data"]["position"] == 1
            && value["data"]["is_active"] == false
    })
    .await;
    let selected = tokio::time::timeout(Duration::from_secs(10), async {
        recv_until(
            &mut owner,
            "disconnected selection still advances",
            |value| {
                value["code"] == json!(DominoesWsCode::STARTER_SELECTED as i32)
                    && value["data"]["position"] == 1
            },
        )
        .await
    })
    .await
    .expect("disconnected player's automatic selection");
    assert_eq!(
        selected["data"]["source"],
        json!(DominoesActionSource::Timeout as i32)
    );

    second = connect_client(&url).await;
    let rejoined = join(&mut second, "second", room).await;
    assert_eq!(rejoined["data"]["self_position"], 1);
    let table = &rejoined["data"]["rejoin_data"]["dominoes"]["table"];
    assert_eq!(
        table["phase"],
        json!(DominoesPhase::StarterSelection as i32)
    );
    assert_eq!(table["starter_selection"]["selector_position"], 2);
    let slot = selected["data"]["slot_index"].as_u64().unwrap() as usize;
    assert_eq!(
        table["starter_selection"]["slots"][slot]["tile"],
        selected["data"]["tile"]
    );

    let free_slot = table["starter_selection"]["slots"]
        .as_array()
        .unwrap()
        .iter()
        .position(|slot| slot["tile"].is_null())
        .expect("unselected starter tile");
    recv_until(&mut third, "third player's selection window", |value| {
        value["code"] == json!(DominoesWsCode::STARTER_SELECTION as i32)
            && value["data"]["selector_position"] == 2
    })
    .await;
    send_request(
        &mut third,
        DominoesRoutes::SELECT_STARTER as i32,
        json!({ "slot_index": free_slot }),
    )
    .await;
    let accepted = recv_until(&mut third, "third player's selection response", |value| {
        value["route"] == json!(DominoesRoutes::SELECT_STARTER as i32)
    })
    .await;
    assert_eq!(accepted["code"], json!(WsResponseCode::OK as i32));
    let opening = recv_start_bundle(&mut owner, 0).await;
    assert_eq!(opening.hand.len(), 5);
    request_ok(&mut owner, Routes::PAUSE).await;
    request_ok(&mut second, Routes::QUIT).await;
    recv_until(&mut owner, "active player quit", |value| {
        value["code"] == json!(WsCode::QUIT as i32) && value["data"]["name"] == "second"
    })
    .await;

    let mut replacement = connect_client(&url).await;
    let joined = join(&mut replacement, "replacement", room).await;
    assert_eq!(joined["data"]["self_position"], 1);
    assert!(joined["data"]["rejoin_data"]["dominoes"].is_null());
    // 退出后的房间已可重新设置，不能保留中断牌局的活动锁。
    send_request(
        &mut owner,
        Routes::SETTING as i32,
        json!({ "current_configs": { "rule": 1 } }),
    )
    .await;
    expect_ok(&mut owner, Routes::SETTING).await;
    request_ok(&mut owner, Routes::START).await;
    let restarted = join(&mut owner, "owner", room).await;
    assert_eq!(restarted["data"]["paused"], false);
    let table = &restarted["data"]["rejoin_data"]["dominoes"]["table"];
    assert_eq!(
        table["phase"],
        json!(DominoesPhase::StarterSelection as i32)
    );
    assert_eq!(table["rule"], 1);
    assert_eq!(table["round"], 1);
    assert!(
        table["starter_selection"]["slots"]
            .as_array()
            .unwrap()
            .iter()
            .all(|slot| slot["tile"].is_null())
    );

    // 让新循环自动执行一次，证明旧循环退出时没有移除新牌局。
    send_request(&mut owner, Routes::AWAY as i32, json!({})).await;
    recv_until(&mut owner, "owner requests AI takeover", |value| {
        value["code"] == json!(WsCode::AWAY as i32)
            && value["data"]["position"] == 0
            && value["data"]["is_ai_takeover"] == true
    })
    .await;
    let selected = recv_until(&mut owner, "restarted loop advances", |value| {
        value["code"] == json!(DominoesWsCode::STARTER_SELECTED as i32)
            && value["data"]["position"] == 0
    })
    .await;
    assert_eq!(
        selected["data"]["source"],
        json!(DominoesActionSource::AiTakeover as i32)
    );
    let restored = join(&mut replacement, "replacement", room).await;
    assert_eq!(
        restored["data"]["rejoin_data"]["dominoes"]["table"]["starter_selection"]["selector_position"],
        1
    );
    server.abort();
}

async fn request_ok(client: &mut Client, route: Routes) {
    send_request(client, route as i32, json!({})).await;
    expect_ok(client, route).await;
}

async fn expect_ok(client: &mut Client, route: Routes) {
    let response = recv_until(client, &format!("response for {route:?}"), |value| {
        value["route"] == json!(route as i32)
    })
    .await;
    assert_eq!(response["code"], json!(WsResponseCode::OK as i32));
}

async fn recv_until(client: &mut Client, label: &str, predicate: impl Fn(&Value) -> bool) -> Value {
    // 非当前操作的客户端也会收到每秒倒计时，不能按固定消息条数截断。
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let value = recv_json(client, label).await;
            if predicate(&value) {
                return value;
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("expected websocket frame not received for {label}"))
}
