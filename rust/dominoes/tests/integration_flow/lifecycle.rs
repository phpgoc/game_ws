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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn repeated_join_retains_takeover_but_transport_reconnect_returns_manual_control() {
    let (server, url, _stop) = start_server("dominoes-takeover-rejoin", test_handler()).await;
    let room = "dominoes-takeover-rejoin";
    let mut owner = connect_client(&url).await;
    let mut second = connect_client(&url).await;
    let mut third = connect_client(&url).await;
    join(&mut owner, "owner", room).await;
    join(&mut second, "second", room).await;
    join(&mut third, "third", room).await;
    request_ok(&mut owner, Routes::START).await;
    request_ok(&mut owner, Routes::PAUSE).await;
    send_request(&mut second, Routes::AWAY as i32, json!({})).await;
    recv_until(&mut owner, "second player starts takeover", |value| {
        value["code"] == json!(WsCode::AWAY as i32)
            && value["data"]["position"] == 1
            && value["data"]["is_ai_takeover"] == true
    })
    .await;
    // 同连接重复 JOIN 不改变托管，且公共成员列表不包含自己。
    let joined = join(&mut second, "second", room).await;
    assert!(
        joined["data"]["existing_members"]
            .as_array()
            .unwrap()
            .iter()
            .all(|member| member["position"] != 1)
    );
    let restored = &joined["data"]["rejoin_data"]["dominoes"];
    let player = restored["table"]["players"]
        .as_array()
        .unwrap()
        .iter()
        .find(|player| player["position"] == 1)
        .unwrap();
    assert_eq!(player["away"], true);
    assert_eq!(player["is_ai_takeover"], true);
    send_request(&mut second, Routes::BACK as i32, json!({})).await;
    recv_until(&mut second, "manual control restored", |value| {
        value["code"] == json!(WsCode::BACK as i32) && value["data"]["position"] == 1
    })
    .await;
    let snapshot = recv_until(&mut second, "returned player snapshot", |value| {
        value["code"] == json!(DominoesWsCode::TABLE_SNAPSHOT as i32)
    })
    .await;
    let player = snapshot["data"]["players"]
        .as_array()
        .unwrap()
        .iter()
        .find(|player| player["position"] == 1)
        .unwrap();
    assert_eq!(player["away"], false);
    assert_eq!(player["is_ai_takeover"], false);
    assert_eq!(
        snapshot["data"]["starter_selection"],
        restored["table"]["starter_selection"]
    );
    send_request(&mut second, Routes::AWAY as i32, json!({})).await;
    recv_until(&mut second, "takeover enabled before disconnect", |value| {
        value["code"] == json!(WsCode::AWAY as i32) && value["data"]["position"] == 1
    })
    .await;
    second
        .close(None)
        .await
        .expect("disconnect while on takeover");
    recv_until(&mut owner, "takeover player disconnects", |value| {
        value["code"] == json!(WsCode::JOIN as i32)
            && value["data"]["position"] == 1
            && value["data"]["is_active"] == false
    })
    .await;
    second = connect_client(&url).await;
    let joined = join(&mut second, "second", room).await;
    assert_eq!(joined["data"]["self_position"], 1);
    assert_eq!(joined["data"]["paused"], true);
    assert!(
        joined["data"]["existing_members"]
            .as_array()
            .unwrap()
            .iter()
            .all(|member| member["position"] != 1)
    );
    let restored = &joined["data"]["rejoin_data"]["dominoes"];
    assert_eq!(
        restored["table"]["phase"],
        json!(DominoesPhase::StarterSelection as i32)
    );
    let player = restored["table"]["players"]
        .as_array()
        .unwrap()
        .iter()
        .find(|player| player["position"] == 1)
        .unwrap();
    assert_eq!(player["away"], false);
    assert_eq!(player["is_ai_takeover"], false);
    assert_eq!(player["is_ai"], false);
    assert_eq!(
        snapshot["data"]["starter_selection"],
        restored["table"]["starter_selection"]
    );
    request_ok(&mut owner, Routes::RESUME).await;
    for (client, slot) in [(&mut owner, 0), (&mut second, 1)] {
        send_request(
            client,
            DominoesRoutes::SELECT_STARTER as i32,
            json!({ "slot_index": slot }),
        )
        .await;
        let response = recv_until(client, "manual starter selection accepted", |value| {
            value["route"] == json!(DominoesRoutes::SELECT_STARTER as i32)
        })
        .await;
        assert_eq!(response["code"], json!(WsResponseCode::OK as i32));
    }
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn returning_current_player_restores_time_in_selection_and_play() {
    let (server, url, _stop) = start_server(
        "dominoes-returned-turn",
        test_handler_with_rng_seed(0x5eed_2026),
    )
    .await;
    let room = "dominoes-returned-turn";
    let mut clients = Vec::new();
    for position in 0..3 {
        let mut client = connect_client(&url).await;
        join(&mut client, &format!("P{position}"), room).await;
        clients.push(client);
    }
    request_ok(&mut clients[0], Routes::START).await;
    let mut position = 0;
    for phase in [DominoesPhase::StarterSelection, DominoesPhase::Play] {
        let observer = (position + 1) % clients.len();
        send_request(&mut clients[position], Routes::AWAY as i32, json!({})).await;
        let short = recv_until(
            &mut clients[position],
            "takeover shortened current turn",
            |value| {
                value["code"] == json!(DominoesWsCode::TABLE_SNAPSHOT as i32)
                    && value["data"]["phase"] == json!(phase as i32)
                    && value["data"]["remaining_seconds"] == 1
            },
        )
        .await;
        let old_revision = short["data"]["turn_revision"].as_i64().unwrap();
        send_request(&mut clients[position], Routes::BACK as i32, json!({})).await;
        recv_until(&mut clients[position], "BACK accepted", |value| {
            value["code"] == json!(WsCode::BACK as i32) && value["data"]["position"] == position
        })
        .await;
        let restored = recv_until(
            &mut clients[position],
            "full manual turn after BACK",
            |value| {
                value["code"] == json!(DominoesWsCode::TABLE_SNAPSHOT as i32)
                    && value["data"]["turn_revision"]
                        .as_i64()
                        .is_some_and(|revision| revision > old_revision)
            },
        )
        .await;
        assert_eq!(restored["data"]["remaining_seconds"], 30);
        assert_eq!(restored["data"]["phase"], json!(phase as i32));
        assert_eq!(restored["data"]["current_position"], position);
        assert_eq!(restored["data"]["placements"], short["data"]["placements"]);
        let hand = recv_until(&mut clients[position], "restored manual hand", |value| {
            value["code"] == json!(DominoesWsCode::HAND_STATE as i32)
        })
        .await;

        // 同连接重复 JOIN 只读快照，不可用来反复续时。
        let repeated = join(&mut clients[position], &format!("P{position}"), room).await;
        let repeated = &repeated["data"]["rejoin_data"]["dominoes"];
        assert_eq!(
            repeated["table"]["turn_revision"],
            restored["data"]["turn_revision"]
        );
        assert_eq!(repeated["hand"], hand["data"]);

        clients[position].close(None).await.unwrap();
        recv_until(
            &mut clients[observer],
            "current player disconnected",
            |value| {
                value["code"] == json!(WsCode::JOIN as i32)
                    && value["data"]["position"] == position
                    && value["data"]["is_active"] == false
            },
        )
        .await;
        let disconnected = recv_until(
            &mut clients[observer],
            "disconnected current turn is shortened",
            |value| {
                value["code"] == json!(DominoesWsCode::TABLE_SNAPSHOT as i32)
                    && value["data"]["current_position"] == position
                    && value["data"]["remaining_seconds"]
                        .as_u64()
                        .is_some_and(|seconds| seconds <= 5)
                    && value["data"]["players"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|player| player["position"] == position && player["away"] == true)
            },
        )
        .await;
        clients[position] = connect_client(&url).await;
        let joined = join(&mut clients[position], &format!("P{position}"), room).await;
        let rejoined = &joined["data"]["rejoin_data"]["dominoes"];
        assert_eq!(rejoined["table"]["remaining_seconds"], 30);
        assert_eq!(rejoined["table"]["phase"], json!(phase as i32));
        assert_eq!(rejoined["table"]["current_position"], position);
        assert!(
            rejoined["table"]["turn_revision"].as_i64().unwrap()
                > disconnected["data"]["turn_revision"].as_i64().unwrap()
        );
        assert_eq!(
            rejoined["table"]["placements"],
            restored["data"]["placements"]
        );
        assert_eq!(rejoined["hand"], hand["data"]);
        let observed = recv_until(
            &mut clients[observer],
            "other seats see the restored manual turn",
            |value| {
                value["code"] == json!(DominoesWsCode::TABLE_SNAPSHOT as i32)
                    && value["data"]["turn_revision"] == rejoined["table"]["turn_revision"]
            },
        )
        .await;
        assert_eq!(observed["data"]["remaining_seconds"], 30);
        if phase == DominoesPhase::StarterSelection {
            // 重连只收到 JOIN 恢复数据，不会重发已经消费的首个选牌事件。
            send_request(
                &mut clients[0],
                DominoesRoutes::SELECT_STARTER as i32,
                json!({ "slot_index": 0 }),
            )
            .await;
            let accepted = recv_until(
                &mut clients[0],
                "returned starter choice accepted",
                |value| value["route"] == json!(DominoesRoutes::SELECT_STARTER as i32),
            )
            .await;
            assert_eq!(accepted["code"], json!(WsResponseCode::OK as i32));
            for (slot, client) in clients.iter_mut().enumerate() {
                if slot > 0 {
                    select_starter(client, slot as i32).await;
                }
            }
            let mut opening = Vec::new();
            for (seat, client) in clients.iter_mut().enumerate() {
                opening.push(recv_start_bundle(client, seat as i32).await);
            }
            position = opening[0].starter_position as usize;
        } else {
            let play = &rejoined["hand"]["legal_plays"][0];
            send_request(
                &mut clients[position],
                DominoesRoutes::PLAY_TILE as i32,
                json!({
                    "tile_id": play["tile_id"], "endpoint_id": play["endpoint_id"],
                }),
            )
            .await;
            let response = recv_until(
                &mut clients[position],
                "returned manual play accepted",
                |value| value["route"] == json!(DominoesRoutes::PLAY_TILE as i32),
            )
            .await;
            assert_eq!(response["code"], json!(WsResponseCode::OK as i32));
        }
    }
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
