//! 完整随机牌局的真实房间负载；只通过正式协议驱动，不注入暗牌或改写发牌。

use super::*;
use landlord::{
    ai::{PlayModelStats, play_model_stats},
    core::play::{PlayValidationContext, validate_play},
};
use share_type_public::LandlordPhase;
use std::collections::{HashMap, HashSet};

fn sampled_rss_kib() -> Option<u64> {
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find_map(|line| {
            line.strip_prefix("VmRSS:")?
                .split_whitespace()
                .next()?
                .parse()
                .ok()
        })
}

fn counts_since(before: PlayModelStats) -> Value {
    let after = play_model_stats();
    json!({"model":after.model-before.model,"disabled":after.disabled-before.disabled,
        "unavailable":after.unavailable-before.unavailable,"fallback":after.fallback-before.fallback})
}

async fn full_room(url: &str, index: usize) -> (Client, HashMap<String, usize>) {
    let mut owner = connect_client(url).await;
    let joined = join(&mut owner, "owner", &format!("fullgame-{index}")).await;
    assert_eq!(position_from_joined(&joined), 0);
    let mut names = HashMap::from([("owner".into(), 0)]);
    send_request(&mut owner, Routes::ADD_AI as i32, json!({"count":2})).await;
    loop {
        let event = recv_json(&mut owner, "fullgame AI roster").await;
        if event["code"] == json!(WsCode::JOIN as i32) && event["data"]["is_ai"] == json!(true) {
            names.insert(
                event["data"]["name"].as_str().unwrap().to_owned(),
                event["data"]["position"].as_u64().unwrap() as usize,
            );
        }
        if is_ok(&event, Routes::ADD_AI) {
            break;
        }
    }
    assert_eq!(names.len(), 3);
    (owner, names)
}

async fn play_full_room(
    mut owner: Client,
    names: HashMap<String, usize>,
    barrier: Arc<Barrier>,
) -> Value {
    barrier.wait().await;
    send_request(&mut owner, Routes::START as i32, json!({})).await;
    recv_until(&mut owner, "fullgame start", |v| is_ok(v, Routes::START)).await;
    let deal = recv_until(&mut owner, "fullgame own deal", |v| {
        v["code"] == json!(WsCode::DEAL as i32)
    })
    .await;
    let mut own = cards_from_deal(&deal);
    assert_eq!(own.len(), 17);
    recv_until(&mut owner, "fullgame call phase", |v| {
        v["code"] == json!(WsCode::CHANGE_PHASE as i32) && v["data"]["phase"] == json!(1)
    })
    .await;
    send_request(
        &mut owner,
        LandlordRoutes::CALL_LANDLORD as i32,
        json!({"score":3}),
    )
    .await;
    recv_until(&mut owner, "fullgame play phase", |v| {
        v["code"] == json!(WsCode::CHANGE_PHASE as i32) && v["data"]["phase"] == json!(2)
    })
    .await;
    let bottom = recv_until(&mut owner, "fullgame bottom", |v| {
        v["code"] == json!(WsCode::DEAL_OPEN_CARDS as i32)
    })
    .await;
    let bottom = cards_from_deal(&bottom);
    own.extend(&bottom);
    assert_eq!(own.iter().copied().collect::<HashSet<_>>().len(), 20);
    recv_until(&mut owner, "fullgame ready", |v| {
        v["code"] == json!(WsCode::CHANGE_DEAL as i32)
    })
    .await;
    send_request(&mut owner, Routes::PAUSE as i32, json!({})).await;
    recv_until(&mut owner, "fullgame pause", |v| is_ok(v, Routes::PAUSE)).await;
    send_request(&mut owner, Routes::AWAY as i32, json!({})).await;
    recv_until(&mut owner, "fullgame explicit takeover", |v| {
        v["code"] == json!(WsCode::AWAY as i32)
            && v["data"]["position"] == json!(0)
            && v["data"]["is_ai_takeover"] == json!(true)
    })
    .await;
    barrier.wait().await;
    send_request(&mut owner, Routes::RESUME as i32, json!({})).await;
    recv_until(&mut owner, "fullgame resume", |v| is_ok(v, Routes::RESUME)).await;
    let started = Instant::now();
    // 首手受暂停轮询影响，单列耗时；后续从公开 CHANGE_DEAL 事件计时。
    let mut turn = Some((0, Instant::now()));
    let mut first = true;
    let mut turns = Vec::new();
    let mut probes = std::collections::VecDeque::<Instant>::new();
    let mut requests = Vec::new();
    let mut played = HashSet::new();
    let mut counts = [20usize, 17, 17];
    let mut last = Vec::<i32>::new();
    let mut last_position = 0;
    let mut winner = None;
    let mut revealed = HashMap::new();
    let mut max_rss = sampled_rss_kib();
    loop {
        let event = recv_json(&mut owner, "fullgame event").await;
        if event["route"] == json!(Routes::MESSAGE as i32) {
            assert!(is_ok(&event, Routes::MESSAGE));
            requests.push(
                probes
                    .pop_front()
                    .expect("pending probe")
                    .elapsed()
                    .as_micros() as u64,
            );
        }
        if event["code"] == json!(WsCode::CHANGE_DEAL as i32) {
            let position = event["data"]["position"].as_u64().unwrap() as usize;
            assert!(position < 3);
            assert!(turn.replace((position, Instant::now())).is_none());
            if position == last_position {
                last.clear();
            }
            probes.push_back(Instant::now());
            send_request(
                &mut owner,
                Routes::MESSAGE as i32,
                json!({"message":"fullgame-probe"}),
            )
            .await;
        }
        if event["code"] == json!(WsCode::AWAY as i32) {
            panic!("unexpected away: {event}");
        }
        if event["code"] == json!(WsCode::PLAY as i32) {
            let position = names[event["data"]["name"].as_str().unwrap()];
            let (announced, turn_start) = turn.take().expect("announced turn");
            assert_eq!(announced, position);
            let cards = cards_from_deal(&event);
            assert!(cards.iter().all(|card| (1..=54).contains(card)));
            let hand = if position == 0 { &own } else { &cards };
            assert!(
                validate_play(
                    PlayValidationContext {
                        phase: LandlordPhase::Play,
                        current_position: position,
                        hand: Some(hand),
                        last_play_position: last_position,
                        last_play: &last
                    },
                    position,
                    &cards
                ),
                "invalid public play: {event}"
            );
            for card in &cards {
                assert!(played.insert(*card), "card played twice");
            }
            counts[position] = counts[position].checked_sub(cards.len()).unwrap();
            if position == 0 {
                own.retain(|card| !cards.contains(card));
            }
            if !cards.is_empty() {
                last = cards.clone();
                last_position = position;
            }
            if counts[position] == 0 {
                winner = Some(position);
            }
            turns.push(json!({"position":position,"elapsed_us":turn_start.elapsed().as_micros() as u64,"initial_resume":first,"remaining":counts,"cards":cards}));
            first = false;
            max_rss = max_rss.max(sampled_rss_kib());
        }
        if event["code"] == json!(WsCode::DEAL_OPEN_CARDS as i32) {
            let position = names[event["data"]["name"].as_str().unwrap()];
            let cards = cards_from_deal(&event);
            assert_eq!(cards.len(), counts[position]);
            for card in &cards {
                assert!(played.insert(*card), "reveal duplicates a played card");
            }
            assert!(revealed.insert(position, cards).is_none());
        }
        if event["code"] == json!(WsCode::GAME_OVER as i32) {
            assert_eq!(
                event["data"]["is_landlord"],
                json!(winner.expect("winner") == 0)
            );
            assert_eq!(revealed.len(), 3);
            assert_eq!(played.len(), 54);
            assert!(
                probes.is_empty(),
                "all ordered probes finish before game over"
            );
            break;
        }
        assert!(turns.len() < 400, "fullgame must terminate");
    }
    send_request(&mut owner, Routes::QUIT as i32, json!({})).await;
    recv_until(&mut owner, "fullgame quit", |v| is_ok(v, Routes::QUIT)).await;
    owner.close(None).await.unwrap();
    // 结算后才用全桌已公开的牌恢复初始发牌，仅供离线重放，绝不传回选牌。
    let mut initial = revealed;
    for turn in &turns {
        initial
            .get_mut(&(turn["position"].as_u64().unwrap() as usize))
            .unwrap()
            .extend(serde_json::from_value::<Vec<i32>>(turn["cards"].clone()).unwrap());
    }
    json!({"turns":turns,"requests_us":requests,"winner":winner,"initial_hands":initial,"bottom":bottom,"wall_ms":started.elapsed().as_millis(),"sampled_max_rss_kib":max_rss})
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "explicit full-game localhost room load with real AI and intentional delays"]
async fn configured_ai_fullgame_load() {
    let model = landlord::ai::initialize_play_model().expect("model configuration must be valid");
    let port = free_port();
    let listen_addr = format!("127.0.0.1:{port}");
    let url = format!("ws://{listen_addr}");
    let server = tokio::spawn(run_room_runtime(
        RuntimeConfig {
            service_name: "landlord-fullgame-benchmark",
            listen_addr,
            idle_timeout: Duration::from_secs(120),
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
        .unwrap_or_else(|_| "2".into())
        .parse::<usize>()
        .unwrap();
    assert!((1..=20).contains(&repeats));
    for rooms in [1, 4, 8, 16] {
        let before = play_model_stats();
        let mut samples = Vec::new();
        let wall = Instant::now();
        for repeat in 0..repeats {
            let mut clients = Vec::new();
            for index in 0..rooms {
                clients.push(full_room(&url, rooms * 1000 + repeat * rooms + index).await);
            }
            let barrier = Arc::new(Barrier::new(rooms));
            let mut tasks = tokio::task::JoinSet::new();
            for (owner, names) in clients {
                tasks.spawn(play_full_room(owner, names, Arc::clone(&barrier)));
            }
            tokio::time::timeout(Duration::from_secs(180), async {
                while let Some(result) = tasks.join_next().await {
                    samples.push(result.expect("complete game"));
                }
            })
            .await
            .expect("fullgame load timeout");
        }
        let stats = counts_since(before);
        let count: usize = samples
            .iter()
            .map(|row| row["turns"].as_array().unwrap().len())
            .sum();
        assert_eq!(
            stats
                .as_object()
                .unwrap()
                .values()
                .map(|v| v.as_u64().unwrap())
                .sum::<u64>(),
            count as u64,
            "quiescent formal decision counters must match applied actions"
        );
        let mut stages: [Vec<u64>; 3] = std::array::from_fn(|_| Vec::new());
        let mut roles: [Vec<u64>; 3] = std::array::from_fn(|_| Vec::new());
        let mut requests = Vec::new();
        for row in &samples {
            let turns = row["turns"].as_array().unwrap();
            for (index, turn) in turns.iter().enumerate() {
                if turn["initial_resume"] == json!(true) {
                    continue;
                }
                let elapsed = turn["elapsed_us"].as_u64().unwrap();
                stages[(index * 3 / turns.len()).min(2)].push(elapsed);
                roles[turn["position"].as_u64().unwrap() as usize].push(elapsed);
            }
            requests.extend(
                row["requests_us"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap()),
            );
        }
        println!(
            "FULLGAME_BENCH {}",
            json!({"rooms":rooms,"repeats":repeats,"games":samples.len(),"applied_actions":count,"stages":stages.into_iter().map(percentiles).collect::<Vec<_>>(),"roles":roles.into_iter().map(percentiles).collect::<Vec<_>>(),"public_requests":percentiles(requests),"model_counts":stats,"manifest_sha256":model.as_ref().map(|v|&v.manifest_sha256),"wall_ms":wall.elapsed().as_millis(),"samples":samples,"scope":"random full games with fixed human bid 3 then explicit takeover, actual websocket/room/AI; includes 300/1200ms delays; excludes external auth/database; RSS is sampled process memory"})
        );
    }
    server.abort();
}
