use super::*;
use share_type_public::DominoesPhase;

type Writer = futures_util::stream::SplitSink<Client, Message>;

// 真实 WebSocket 的四名玩家只使用各自收到的私牌和合法动作，不读取服务器状态。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn four_players_finish_a_match_and_rejoin_each_major_phase() {
    let (server, url, _stop) = start_server(
        "dominoes-match-rejoin",
        test_handler_with_members(Some(0x5eed_2026), &["P0", "P1", "P2", "P3"]),
    )
    .await;
    let room = "dominoes-match-rejoin";
    let mut clients = Vec::new();
    for position in 0..4 {
        let mut client = connect_client(&url).await;
        join(&mut client, &format!("P{position}"), room).await;
        clients.push(client);
    }
    request_ok(
        &mut clients[0],
        Routes::SETTING,
        json!({ "current_configs": {
            "rule": 0, "no_playable_tiles": 0, "target_score": 1, "settlement_time": 3
        }}),
    )
    .await;
    request_ok(&mut clients[0], Routes::START, json!({})).await;

    // 先手暗牌选择阶段重连，未选牌的牌面不得泄露。
    request_ok(&mut clients[0], Routes::PAUSE, json!({})).await;
    let (owner, others) = clients.split_at_mut(1);
    let selection = reconnect(&mut others[0], &mut owner[0], &url, room, 1).await;
    assert_eq!(selection["data"]["paused"], true);
    let restored = &selection["data"]["rejoin_data"]["dominoes"];
    assert_eq!(
        restored["table"]["phase"],
        json!(DominoesPhase::StarterSelection as i32)
    );
    assert!(restored["hand"]["hand"].as_array().unwrap().is_empty());
    assert!(
        restored["table"]["starter_selection"]["slots"]
            .as_array()
            .unwrap()
            .iter()
            .all(|slot| slot["tile"].is_null())
    );
    request_ok(&mut clients[0], Routes::RESUME, json!({})).await;
    // 使用恢复的 selector；重连的新 socket 不会再收到已发过的首个选择事件。
    for (position, client) in clients.iter_mut().enumerate().take(4) {
        send_request(
            client,
            DominoesRoutes::SELECT_STARTER as i32,
            json!({ "slot_index": position }),
        )
        .await;
        recv_until(client, "selection accepted", |v| {
            v["route"] == json!(DominoesRoutes::SELECT_STARTER as i32)
        })
        .await;
    }

    // 暂停后重连，桌面、四人手牌数和本座位私牌应一致。
    request_ok(&mut clients[0], Routes::PAUSE, json!({})).await;
    let (owner, others) = clients.split_at_mut(1);
    let playing = reconnect(&mut others[0], &mut owner[0], &url, room, 1).await;
    let restored = &playing["data"]["rejoin_data"]["dominoes"];
    assert_eq!(
        restored["table"]["phase"],
        json!(DominoesPhase::Play as i32)
    );
    assert_eq!(restored["hand"]["hand"].as_array().unwrap().len(), 5);
    assert_hand_state_matches_snapshot(&restored["table"], &restored["hand"], 1);
    request_ok(&mut clients[0], Routes::RESUME, json!({})).await;

    // 分离收发，持续消费四个连接，避免某个座位的旧快照阻塞对局。
    let (events_tx, mut events) = tokio::sync::mpsc::unbounded_channel();
    let mut writers = Vec::new();
    let mut readers = Vec::new();
    for (position, client) in clients.into_iter().enumerate() {
        let (writer, mut reader) = client.split();
        writers.push(writer);
        let tx = events_tx.clone();
        readers.push(tokio::spawn(async move {
            while let Some(Ok(message)) = reader.next().await {
                if let Message::Text(text) = message {
                    let value = serde_json::from_str::<Value>(text.as_ref()).expect("event JSON");
                    if tx.send((position, value)).is_err() {
                        break;
                    }
                }
            }
        }));
    }
    let mut tables = vec![Value::Null; 4];
    let mut hands = vec![Value::Null; 4];
    let mut sent_revisions = std::collections::HashSet::new();
    let mut round_results = std::collections::BTreeMap::new();
    let mut last_scores = vec![0_i64; 4];
    let mut final_result = None;
    let mut settlement_rejoined = false;
    // 用幂等 JOIN 获得刚重连玩家及其他座位的当前完整状态。
    for (position, writer) in writers.iter_mut().enumerate() {
        write_request(
            writer,
            Routes::JOIN as i32,
            json!({
                "name": format!("P{position}"), "password": room,
                "game_id": GameId::DOMINOES as i32, "avatar_url": ""
            }),
        )
        .await;
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(90);
    while tokio::time::Instant::now() < deadline {
        let (position, value) = tokio::time::timeout(Duration::from_secs(6), events.recv())
            .await
            .expect("match event deadline")
            .expect("match events");
        if value["route"] == json!(Routes::JOIN as i32) {
            assert_eq!(value["code"], json!(WsResponseCode::JOINED as i32));
            let rejoin = &value["data"]["rejoin_data"]["dominoes"];
            tables[position] = rejoin["table"].clone();
            hands[position] = rejoin["hand"].clone();
        } else if value.get("route").is_some() {
            assert_eq!(value["code"], json!(WsResponseCode::OK as i32), "{value}");
        } else if value["code"] == json!(DominoesWsCode::TABLE_SNAPSHOT as i32) {
            tables[position] = value["data"].clone();
            hands[position] = Value::Null;
        } else if value["code"] == json!(DominoesWsCode::HAND_STATE as i32) {
            hands[position] = value["data"].clone();
            assert_hand_state_matches_snapshot(
                &tables[position],
                &hands[position],
                position as i32,
            );
        } else if position == 0 && value["code"] == json!(DominoesWsCode::ROUND_OVER as i32) {
            let result = &value["data"];
            let scores = (0..4)
                .map(|p| result["scores"][p.to_string()].as_i64().unwrap())
                .collect::<Vec<_>>();
            for p in 0..4 {
                assert!(scores[p] >= last_scores[p]);
            }
            last_scores = scores;
            round_results.insert(result["round"].as_i64().unwrap(), result.clone());
            if !settlement_rejoined && tables[0]["phase"] == json!(DominoesPhase::RoundOver as i32)
            {
                write_request(&mut writers[0], Routes::PAUSE as i32, json!({})).await;
                loop {
                    let (seat, event) = tokio::time::timeout(Duration::from_secs(5), events.recv())
                        .await
                        .unwrap()
                        .unwrap();
                    if seat == 0 && event["route"] == json!(Routes::PAUSE as i32) {
                        assert_eq!(event["code"], json!(WsResponseCode::OK as i32));
                        break;
                    }
                }
                writers[1].close().await.expect("disconnect at settlement");
                loop {
                    let (seat, event) = tokio::time::timeout(Duration::from_secs(5), events.recv())
                        .await
                        .unwrap()
                        .unwrap();
                    if seat == 0
                        && event["code"] == json!(WsCode::JOIN as i32)
                        && event["data"]["position"] == 1
                        && event["data"]["is_active"] == false
                    {
                        break;
                    }
                }
                let mut client = connect_client(&url).await;
                let joined = join(&mut client, "P1", room).await;
                let restored = &joined["data"]["rejoin_data"]["dominoes"];
                assert_eq!(
                    restored["table"]["phase"],
                    json!(DominoesPhase::RoundOver as i32)
                );
                assert_eq!(joined["data"]["paused"], true);
                assert_hand_state_matches_snapshot(&restored["table"], &restored["hand"], 1);
                let replay = recv_until(&mut client, "settlement replay", |v| {
                    v["code"] == json!(DominoesWsCode::ROUND_OVER as i32)
                })
                .await;
                for field in [
                    "winner_position",
                    "round",
                    "round_score",
                    "scores",
                    "remaining_hands",
                ] {
                    assert_eq!(replay["data"][field], result[field], "{field}");
                }
                let (writer, mut reader) = client.split();
                writers[1] = writer;
                readers[1].abort();
                let tx = events_tx.clone();
                readers[1] = tokio::spawn(async move {
                    while let Some(Ok(message)) = reader.next().await {
                        if let Message::Text(text) = message {
                            let value = serde_json::from_str::<Value>(text.as_ref()).unwrap();
                            if tx.send((1, value)).is_err() {
                                break;
                            }
                        }
                    }
                });
                write_request(&mut writers[0], Routes::RESUME as i32, json!({})).await;
                settlement_rejoined = true;
            }
        } else if position == 0 && value["code"] == json!(DominoesWsCode::GAME_OVER as i32) {
            final_result = Some(value["data"].clone());
            break;
        }
        let table = &tables[position];
        if table["phase"] != json!(DominoesPhase::Play as i32)
            || table["current_position"] != json!(position)
            || hands[position].is_null()
        {
            continue;
        }
        let revision = (table["round"].as_i64(), table["turn_revision"].as_i64());
        if sent_revisions.contains(&revision) {
            continue;
        }
        if let Some(play) = hands[position]["legal_plays"]
            .as_array()
            .and_then(|v| v.last())
        {
            write_request(
                &mut writers[position],
                DominoesRoutes::PLAY_TILE as i32,
                json!({
                    "tile_id": play["tile_id"], "endpoint_id": play["endpoint_id"]
                }),
            )
            .await;
            sent_revisions.insert(revision);
        }
    }
    let result = final_result.expect("four-player match must reach GAME_OVER");
    assert!(
        round_results.len() >= 2,
        "match did not exercise a round transition"
    );
    assert!(
        settlement_rejoined,
        "round settlement rejoin was not exercised"
    );
    let highest = *last_scores.iter().max().unwrap();
    assert!(highest >= 61);
    let expected_winners = (0..4)
        .filter(|p| last_scores[*p] == highest)
        .collect::<Vec<_>>();
    assert_eq!(result["winner_positions"], json!(expected_winners));

    // 比赛结束后真正断线重连，最后一轮结算与最终胜者必须重放。
    writers[1].close().await.expect("close final socket");
    let mut rejoined = connect_client(&url).await;
    // 等待实际断线事件，避免 JOIN 与 close 在不同连接间的到达顺序竞态。
    loop {
        let (position, event) = tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .unwrap()
            .unwrap();
        if position == 0
            && event["code"] == json!(WsCode::JOIN as i32)
            && event["data"]["position"] == 1
            && event["data"]["is_active"] == false
        {
            break;
        }
    }
    let final_join = join(&mut rejoined, "P1", room).await;
    assert_eq!(
        final_join["data"]["rejoin_data"]["dominoes"]["table"]["phase"],
        json!(DominoesPhase::GameOver as i32)
    );
    let replayed_round = recv_until(&mut rejoined, "last round replay", |v| {
        v["code"] == json!(DominoesWsCode::ROUND_OVER as i32)
    })
    .await;
    assert_eq!(Some(&replayed_round["data"]), round_results.values().last());
    let replayed_game = recv_until(&mut rejoined, "game result replay", |v| {
        v["code"] == json!(DominoesWsCode::GAME_OVER as i32)
    })
    .await;
    assert_eq!(replayed_game["data"], result);

    // 同一房间完成结算后改规则、换一位玩家、再次开局，后台必须真正继续推进。
    write_request(
        &mut writers[0],
        Routes::SETTING as i32,
        json!({
            "current_configs": { "rule": 1, "target_score": 0 }
        }),
    )
    .await;
    let setting = owner_event(&mut events, |v| v["route"] == json!(Routes::SETTING as i32)).await;
    assert_eq!(setting["code"], json!(WsResponseCode::OK as i32));
    write_request(&mut writers[3], Routes::QUIT as i32, json!({})).await;
    owner_event(&mut events, |v| v["code"] == json!(WsCode::QUIT as i32)).await;
    let mut newcomer = connect_client(&url).await;
    let joined = join(&mut newcomer, "newcomer", room).await;
    assert_eq!(joined["data"]["self_position"], 3);
    assert!(joined["data"]["rejoin_data"]["dominoes"].is_null());

    write_request(&mut writers[0], Routes::START as i32, json!({})).await;
    let next = owner_event(&mut events, |v| {
        v["code"] == json!(DominoesWsCode::TABLE_SNAPSHOT as i32)
            && v["data"]["phase"] == json!(DominoesPhase::StarterSelection as i32)
    })
    .await;
    assert_eq!(next["data"]["rule"], 1);
    assert_eq!(next["data"]["target_score"], 35);
    assert_eq!(next["data"]["round"], 1);
    assert!(
        next["data"]["players"]
            .as_array()
            .unwrap()
            .iter()
            .all(|player| player["score"] == 0)
    );
    for position in [0, 2] {
        write_request(&mut writers[position], Routes::AWAY as i32, json!({})).await;
    }
    send_request(&mut rejoined, Routes::AWAY as i32, json!({})).await;
    send_request(&mut newcomer, Routes::AWAY as i32, json!({})).await;
    let opening = owner_event(&mut events, |v| {
        v["code"] == json!(DominoesWsCode::PLAY_TILE as i32)
    })
    .await;
    assert_eq!(
        opening["data"]["source"],
        json!(share_type_public::DominoesActionSource::AiTakeover as i32)
    );
    for reader in readers {
        reader.abort();
    }
    server.abort();
}

async fn owner_event(
    events: &mut tokio::sync::mpsc::UnboundedReceiver<(usize, Value)>,
    predicate: impl Fn(&Value) -> bool,
) -> Value {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let (position, value) = events.recv().await.expect("room events");
            if position == 0 && predicate(&value) {
                return value;
            }
        }
    })
    .await
    .expect("owner event deadline")
}

async fn request_ok(client: &mut Client, route: Routes, data: Value) {
    send_request(client, route as i32, data).await;
    let reply = recv_until(client, "common request response", |v| {
        v["route"] == json!(route as i32)
    })
    .await;
    assert_eq!(reply["code"], json!(WsResponseCode::OK as i32));
}

async fn reconnect(
    client: &mut Client,
    observer: &mut Client,
    url: &str,
    room: &str,
    position: usize,
) -> Value {
    client.close(None).await.expect("close socket");
    recv_until(observer, "disconnected member event", |v| {
        v["code"] == json!(WsCode::JOIN as i32)
            && v["data"]["position"] == json!(position)
            && v["data"]["is_active"] == false
    })
    .await;
    *client = connect_client(url).await;
    let response = join(client, &format!("P{position}"), room).await;
    assert_eq!(response["data"]["self_position"], json!(position));
    response
}

async fn write_request(writer: &mut Writer, route: i32, data: Value) {
    writer
        .send(Message::Text(
            json!({ "route": route, "data": data }).to_string().into(),
        ))
        .await
        .expect("send match request");
}
