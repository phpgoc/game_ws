//! 显式运行的本机多房间实链路基准；包含正式 300ms/1200ms 动作延迟。

use super::*;
use std::sync::Arc;
use tokio::sync::Barrier;

fn is_ok(value: &Value, route: Routes) -> bool {
    value["route"] == json!(route as i32) && value["code"] == json!(WsResponseCode::OK as i32)
}

async fn prepared_room(url: &str, index: usize) -> Client {
    let mut owner = connect_client(url).await;
    join(&mut owner, "owner", &format!("latency-room-{index}")).await;
    send_request(&mut owner, Routes::ADD_AI as i32, json!({"count":2})).await;
    recv_until(&mut owner, "add two benchmark AI players", |value| {
        is_ok(value, Routes::ADD_AI)
    })
    .await;
    owner
}

async fn measured_room(mut owner: Client, barrier: Arc<Barrier>) -> Value {
    barrier.wait().await;
    send_request(&mut owner, Routes::START as i32, json!({})).await;
    recv_until(&mut owner, "benchmark start", |value| {
        is_ok(value, Routes::START)
    })
    .await;
    recv_until(&mut owner, "benchmark call phase", |value| {
        value["code"] == json!(WsCode::CHANGE_PHASE as i32) && value["data"]["phase"] == json!(1)
    })
    .await;
    // 人类房主固定叫 3，保证统一直接进入出牌；不将此项算作叫分棋力验证。
    send_request(
        &mut owner,
        LandlordRoutes::CALL_LANDLORD as i32,
        json!({"score":3}),
    )
    .await;
    recv_until(&mut owner, "benchmark landlord", |value| {
        value["code"] == json!(WsCode::CHANGE_PHASE as i32) && value["data"]["phase"] == json!(2)
    })
    .await;
    let bottom = recv_until(&mut owner, "benchmark bottom", |value| {
        value["code"] == json!(WsCode::DEAL_OPEN_CARDS as i32)
    })
    .await;
    let cards = cards_from_deal(&bottom);
    assert_eq!(cards.len(), 3);
    recv_until(&mut owner, "benchmark landlord ready", |value| {
        value["code"] == json!(WsCode::CHANGE_DEAL as i32) && value["data"]["position"] == json!(0)
    })
    .await;
    // 暂停使其他房间也能到达同一出牌入口，再同时施压。
    send_request(&mut owner, Routes::PAUSE as i32, json!({})).await;
    recv_until(&mut owner, "benchmark pause", |value| {
        is_ok(value, Routes::PAUSE)
    })
    .await;
    barrier.wait().await;
    let started = Instant::now();
    send_request(&mut owner, Routes::RESUME as i32, json!({})).await;
    recv_until(&mut owner, "benchmark resume", |value| {
        is_ok(value, Routes::RESUME)
    })
    .await;
    // 底牌必在地主手中，取一张形成合法单张，其他私牌不在客户端共享。
    let play_sent = Instant::now();
    send_request(&mut owner, Routes::PLAY as i32, json!({"cards":[cards[0]]})).await;
    let mut ai_plays = 0;
    let mut ai_turn_started = None;
    let mut turn_us = Vec::new();
    let mut owner_ack_us = None;
    let mut request_us = Vec::new();
    let mut probes = std::collections::VecDeque::<Instant>::new();
    let mut returned_to_owner = false;
    loop {
        let value = recv_json(&mut owner, "benchmark AI turns").await;
        if value["route"] == json!(Routes::MESSAGE as i32) {
            assert!(is_ok(&value, Routes::MESSAGE));
            request_us.push(
                probes
                    .pop_front()
                    .expect("pending message probe")
                    .elapsed()
                    .as_micros() as u64,
            );
        }
        if value["route"] == json!(Routes::PLAY as i32) {
            assert!(is_ok(&value, Routes::PLAY), "human play rejected: {value}");
            owner_ack_us = Some(play_sent.elapsed().as_micros() as u64);
        }
        if value["code"] == json!(WsCode::CHANGE_DEAL as i32) {
            if value["data"]["position"] == json!(0) {
                returned_to_owner = true;
            } else {
                ai_turn_started = Some(Instant::now());
                probes.push_back(Instant::now());
                send_request(
                    &mut owner,
                    Routes::MESSAGE as i32,
                    json!({"message":"latency-probe"}),
                )
                .await;
            }
        }
        if value["code"] == json!(WsCode::AWAY as i32) {
            panic!("AI unexpectedly away: {value}");
        }
        if value["code"] == json!(WsCode::PLAY as i32) && value["data"]["name"] != json!("owner") {
            assert!(value["data"]["cards"].is_array());
            turn_us.push(
                ai_turn_started
                    .take()
                    .expect("announced AI turn")
                    .elapsed()
                    .as_micros() as u64,
            );
            ai_plays += 1;
        }
        if ai_plays == 2 && returned_to_owner && probes.is_empty() {
            break;
        }
    }
    assert_eq!(request_us.len(), 2);
    send_request(&mut owner, Routes::QUIT as i32, json!({})).await;
    recv_until(&mut owner, "benchmark quit", |value| {
        is_ok(value, Routes::QUIT)
    })
    .await;
    owner.close(None).await.expect("benchmark close");
    json!({"ai_turn_us":turn_us,"owner_ack_us":owner_ack_us.expect("owner play acknowledged"),"request_us":request_us,"round_trip_us":started.elapsed().as_micros() as u64})
}

fn percentiles(mut values: Vec<u64>) -> Value {
    values.sort_unstable();
    let at = |fraction: f64| values[((values.len() - 1) as f64 * fraction).round() as usize];
    json!({"count":values.len(),"median_us":at(0.5),"p95_us":at(0.95),"p99_us":at(0.99),"max_us":at(1.0)})
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "explicit localhost WebSocket multiroom latency benchmark"]
async fn configured_ai_multiroom_latency() {
    let model = landlord::ai::initialize_play_model().expect("configured model must load");
    if std::env::var_os("LANDLORD_AI_MODEL_DIR").is_some() {
        assert!(model.is_some());
    }
    let port = free_port();
    let listen_addr = format!("127.0.0.1:{port}");
    let url = format!("ws://{listen_addr}");
    let server = tokio::spawn(run_room_runtime(
        RuntimeConfig {
            service_name: "landlord-multiroom-benchmark",
            listen_addr,
            idle_timeout: Duration::from_secs(60),
            heartbeat_interval: Duration::from_secs(30),
        },
        TestAiLandlordHandler::default(),
    ));
    for _ in 0..50 {
        if TokioTcpListener::bind(("127.0.0.1", port)).await.is_err() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let repeats = std::env::var("LANDLORD_AI_BENCH_REPEATS")
        .unwrap_or_else(|_| "3".into())
        .parse::<usize>()
        .unwrap();
    assert!((1..=20).contains(&repeats));
    for rooms in [1, 4, 8, 16] {
        let mut samples = Vec::new();
        for repeat in 0..repeats {
            let mut clients = Vec::new();
            for index in 0..rooms {
                clients.push(prepared_room(&url, index + repeat * rooms + rooms * 100).await);
            }
            let barrier = Arc::new(Barrier::new(rooms));
            let mut tasks = tokio::task::JoinSet::new();
            for client in clients {
                tasks.spawn(measured_room(client, Arc::clone(&barrier)));
            }
            tokio::time::timeout(Duration::from_secs(45), async {
                while let Some(result) = tasks.join_next().await {
                    samples.push(result.expect("measured room completes"));
                }
            })
            .await
            .expect("room benchmark timeout");
        }
        let turns = samples
            .iter()
            .flat_map(|row| {
                row["ai_turn_us"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap())
            })
            .collect();
        let requests = samples
            .iter()
            .flat_map(|row| {
                row["request_us"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap())
            })
            .collect();
        let acknowledgements = samples
            .iter()
            .map(|row| row["owner_ack_us"].as_u64().unwrap())
            .collect();
        println!(
            "ROOM_BENCH {}",
            json!({"rooms":rooms,"repeats":repeats,"runtime_workers":1,"ai_turns":percentiles(turns),"public_request":percentiles(requests),"owner_play_ack":percentiles(acknowledgements),"samples":samples,"model_manifest_sha256":model.as_ref().map(|v|&v.manifest_sha256),"scope":"localhost real websocket, random deals, two AI plays per room, includes intentional delay and scheduling; excludes external auth/database"})
        );
    }
    server.abort();
}
