#[cfg(not(feature = "official"))]
use std::net::TcpListener;
use std::time::Duration;

#[cfg(feature = "official")]
use std::sync::mpsc::sync_channel;
#[cfg(not(feature = "official"))]
use std::time::Instant;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use share_type_public::{GameId, Routes, TractorWsCode, WsCode, WsResponseCode};
use share_type_public::{GameParam, GameParamRange};
#[cfg(not(feature = "official"))]
use share_type_public::{TractorPhase, TractorRank, TractorRoutes, TractorSuit};
use tokio_tungstenite::{WebSocketStream, connect_async, tungstenite::Message};
#[cfg(not(feature = "official"))]
use tractor::combo;
use tractor::game::TractorGameHandler;
#[cfg(not(feature = "official"))]
use tractor::game_state::TractorRules;
use ws_common::RuntimeConfig;
#[cfg(not(feature = "official"))]
use ws_common::run_room_runtime;
use ws_common::{
    ClientRequest, Dispatch, GameHandler, GameState, JoinAuthorization, JoinAuthorizationFuture,
    RoomService, SessionId, SessionSenders, SettingsBuilderResult,
};
#[cfg(feature = "official")]
use ws_common::{run_room_runtime_until_stopped_with_ready, runtime_stop_channel};

type Client = WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

#[derive(Default)]
struct TestTractorHandler(TractorGameHandler);

impl GameHandler for TestTractorHandler {
    fn after_common_request(
        &mut self,
        room_service: &mut RoomService,
        session_id: SessionId,
        request: &ClientRequest,
        dispatch: &mut Dispatch,
    ) {
        self.0
            .after_common_request(room_service, session_id, request, dispatch);
    }

    fn authorize_join(&self, _join: &share_type_public::WsJoinRequest) -> JoinAuthorizationFuture {
        Box::pin(async { JoinAuthorization::ALLOW_NONMEMBER })
    }

    fn supports_ai_players(&self) -> bool {
        self.0.supports_ai_players()
    }

    fn build_game_state(&self) -> Box<dyn GameState> {
        self.0.build_game_state()
    }

    fn build_room_settings(&self) -> SettingsBuilderResult {
        let (mut settings, mut params) = self.0.build_room_settings();
        // Internal timing controls are injected only to keep this test fast.
        // The public play-time range is widened below for the same reason.
        for (key, default) in [
            ("first_deal_time", 1_000),
            ("deal_time", 500),
            ("ai_action_time", 20),
            ("away_time", 1),
            ("play_time", 1),
            ("settlement_time", 1),
        ] {
            settings.values.insert(key.to_owned(), default);
            params.insert(
                key.to_owned(),
                GameParam::Range(GameParamRange {
                    default,
                    min: 1,
                    max: 60_000,
                }),
            );
        }
        (settings, params)
    }

    fn game_id(&self) -> GameId {
        self.0.game_id()
    }

    fn handle_game_request(
        &mut self,
        room_service: &mut RoomService,
        session_id: SessionId,
        request: ClientRequest,
    ) -> Dispatch {
        self.0
            .handle_game_request(room_service, session_id, request)
    }

    fn set_context(
        &mut self,
        senders: SessionSenders,
        room_service: std::sync::Arc<tokio::sync::Mutex<RoomService>>,
    ) {
        self.0.set_context(senders, room_service);
    }
}

async fn connect_client(url: &str) -> Client {
    for _ in 0..100 {
        if let Ok(Ok((client, _))) =
            tokio::time::timeout(Duration::from_millis(250), connect_async(url)).await
        {
            return client;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("tractor websocket server did not become ready");
}

#[cfg(not(feature = "official"))]
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind free port")
        .local_addr()
        .expect("local addr")
        .port()
}

async fn join(client: &mut Client, name: &str, password: &str) -> Value {
    send_request(
        client,
        Routes::JOIN as i32,
        json!({
            "name": name,
            "password": password,
            "game_id": GameId::TRACTOR as i32,
            "avatar_url": ""
        }),
    )
    .await;
    recv_until(client, "join response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::JOIN as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::JOINED as i64)
    })
    .await
}

async fn recv_json(client: &mut Client, label: &str) -> Value {
    loop {
        let frame = tokio::time::timeout(Duration::from_secs(60), client.next())
            .await
            .unwrap_or_else(|_| panic!("websocket message timeout while waiting for {label}"))
            .expect("websocket frame")
            .expect("websocket frame ok");
        match frame {
            Message::Text(text) => return serde_json::from_str(text.as_ref()).expect("json frame"),
            Message::Ping(_) | Message::Pong(_) => continue,
            other => panic!("unexpected frame: {other:?}"),
        }
    }
}

async fn recv_until<F>(client: &mut Client, label: &str, mut pred: F) -> Value
where
    F: FnMut(&Value) -> bool,
{
    let mut recent = Vec::new();
    // A complete four-seat deal emits one snapshot per dealt card in addition
    // to the observer's private deal frames, so events near the phase change
    // can legitimately arrive after more than 100 frames.
    for _ in 0..256 {
        let value = recv_json(client, label).await;
        if pred(&value) {
            return value;
        }
        recent.push(value);
        if recent.len() > 8 {
            recent.remove(0);
        }
    }
    panic!("expected websocket frame not received for {label}; recent={recent:?}");
}

async fn send_request(client: &mut Client, route: i32, data: Value) {
    client
        .send(Message::Text(
            json!({ "route": route, "data": data }).to_string().into(),
        ))
        .await
        .expect("send request");
}

#[cfg(not(feature = "official"))]
async fn collect_tractor_hand(
    client: &mut Client,
    position: usize,
    expected_hand_size: usize,
) -> Vec<i32> {
    let mut hand = Vec::with_capacity(expected_hand_size);
    while hand.len() < expected_hand_size {
        let value = recv_json(client, "tractor private deal").await;
        if value.get("code").and_then(Value::as_i64) != Some(WsCode::DEAL as i64) {
            continue;
        }
        assert_eq!(value["data"]["position"], json!(position));
        let cards = value["data"]["cards"].as_array().expect("deal cards");
        assert_eq!(cards.len(), 1, "deal must be incremental");
        hand.push(cards[0].as_i64().expect("card") as i32);
    }
    hand
}

#[cfg(not(feature = "official"))]
async fn collect_tractor_hands(
    clients: &mut [&mut Client; 4],
    expected_hand_size: usize,
) -> [Vec<i32>; 4] {
    let (left, right) = clients.split_at_mut(2);
    let (a, b) = left.split_at_mut(1);
    let (c, d) = right.split_at_mut(1);
    let (a, b, c, d) = tokio::join!(
        collect_tractor_hand(&mut *a[0], 0, expected_hand_size),
        collect_tractor_hand(&mut *b[0], 1, expected_hand_size),
        collect_tractor_hand(&mut *c[0], 2, expected_hand_size),
        collect_tractor_hand(&mut *d[0], 3, expected_hand_size),
    );
    [a, b, c, d]
}

#[cfg(not(feature = "official"))]
async fn recv_tractor_bottom(client: &mut Client, dealer_position: usize) -> Value {
    let bottom = recv_until(client, "tractor bottom cards", |value| {
        value.get("code").and_then(Value::as_i64) == Some(TractorWsCode::BOTTOM_CARDS as i64)
    })
    .await;
    assert_eq!(bottom["data"]["position"], json!(dealer_position));
    bottom
}

#[cfg(not(feature = "official"))]
fn find_failed_throw_candidate(
    hands: &[Vec<i32>; 4],
    position: usize,
    rules: &TractorRules,
) -> Option<(Vec<i32>, Vec<i32>)> {
    let hand = &hands[position];
    for first in 0..hand.len() {
        for second in (first + 1)..hand.len() {
            let candidate = vec![hand[first], hand[second]];
            let Some(lead) = combo::classify(&candidate, rules) else {
                continue;
            };
            if lead.suit.is_none() || !matches!(lead.kind, combo::ComboKind::Throw { .. }) {
                continue;
            }
            let components = combo::throw_components(&candidate, rules)?;
            let fallback = components
                .into_iter()
                .filter(|component| {
                    let Some(component_lead) = combo::classify(component, rules) else {
                        return false;
                    };
                    let Some(component_value) =
                        combo::combo_win_value(component, &component_lead, rules)
                    else {
                        return false;
                    };
                    hands.iter().enumerate().any(|(opponent, opponent_hand)| {
                        opponent != position
                            && combo::enumerate_follows(opponent_hand, &component_lead, rules)
                                .into_iter()
                                .filter_map(|reply| {
                                    combo::combo_win_value(&reply, &component_lead, rules)
                                })
                                .any(|reply_value| reply_value > component_value)
                    })
                })
                .min_by_key(|component| {
                    let component_lead = combo::classify(component, rules)
                        .expect("failed throw component remains valid");
                    combo::combo_win_value(component, &component_lead, rules)
                        .expect("failed throw component has a value")
                });
            if let Some(fallback) = fallback {
                return Some((candidate, fallback));
            }
        }
    }
    None
}

#[cfg(not(feature = "official"))]
fn find_illegal_follow_case(
    hands: &[Vec<i32>; 4],
    dealer_position: usize,
    rules: &TractorRules,
) -> Option<(i32, usize, i32, i32)> {
    let follower_position = (dealer_position + 1) % 4;
    for lead_card in hands[dealer_position].iter().copied() {
        let lead = combo::classify(&[lead_card], rules)?;
        let legal = hands[follower_position]
            .iter()
            .copied()
            .find(|card| combo::card_in_group(*card, lead.suit, rules));
        let illegal = hands[follower_position]
            .iter()
            .copied()
            .find(|card| !combo::card_in_group(*card, lead.suit, rules));
        if let (Some(legal), Some(illegal)) = (legal, illegal) {
            return Some((lead_card, follower_position, legal, illegal));
        }
    }
    None
}

#[cfg(not(feature = "official"))]
async fn recv_first_declaration(client: &mut Client) -> (Value, Option<Value>) {
    let mut bottom = None;
    loop {
        let value = recv_json(client, "first tractor declaration").await;
        match value.get("code").and_then(Value::as_i64) {
            Some(code) if code == TractorWsCode::BOTTOM_CARDS as i64 => bottom = Some(value),
            Some(code) if code == TractorWsCode::TRUMP_DECLARED as i64 => {
                return (value, bottom);
            }
            _ => {}
        }
    }
}

#[cfg(feature = "official")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tractor_official_ai_buries_and_leads_over_websocket() {
    let (stop_handle, stop_signal) = runtime_stop_channel();
    let (ready_tx, ready_rx) = sync_channel(1);
    let server = tokio::spawn(run_room_runtime_until_stopped_with_ready(
        RuntimeConfig {
            service_name: "tractor-ai-test",
            listen_addr: "127.0.0.1:0".to_owned(),
            idle_timeout: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(30),
        },
        TestTractorHandler::default(),
        stop_signal,
        ready_tx,
    ));
    let stats = tokio::task::spawn_blocking(move || {
        ready_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("tractor runtime readiness")
    })
    .await
    .expect("read tractor runtime readiness");
    let url = format!("ws://{}", stats.listen_addr());

    let mut owner = connect_client(&url).await;
    let room = "tractor-ai-room";
    let joined = join(&mut owner, "owner", room).await;
    assert_eq!(joined["data"]["self_position"], json!(0));

    send_request(&mut owner, Routes::ADD_AI as i32, json!({ "count": 3 })).await;
    for expected_position in 1..=3 {
        let joined_ai = recv_until(&mut owner, "AI join event", |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::JOIN as i64)
                && value["data"]["is_ai"] == json!(true)
        })
        .await;
        assert_eq!(joined_ai["data"]["position"], json!(expected_position));
        assert_eq!(joined_ai["data"]["away"], json!(false));
        assert_eq!(joined_ai["data"]["is_ai_takeover"], json!(false));
    }
    recv_until(&mut owner, "add AI response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::ADD_AI as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;

    send_request(
        &mut owner,
        Routes::SETTING as i32,
        json!({
            "current_configs": {
                "first_deal_time": 1000,
                "deal_time": 500,
                "ai_action_time": 20,
                "play_time": 1
            }
        }),
    )
    .await;
    recv_until(&mut owner, "setting response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::SETTING as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;

    send_request(&mut owner, Routes::START as i32, json!({})).await;
    recv_until(&mut owner, "start response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::START as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;

    let buried = loop {
        let value = recv_json(&mut owner, "AI declaration or bottom buried").await;
        match value.get("code").and_then(Value::as_i64) {
            Some(code) if code == TractorWsCode::TRUMP_DECLARED as i64 => {}
            Some(code) if code == TractorWsCode::BOTTOM_BURIED as i64 => break value,
            _ => {}
        }
    };
    let dealer_position = buried["data"]["position"]
        .as_i64()
        .expect("AI or AI-takeover dealer position");
    assert!((0..=3).contains(&dealer_position));
    assert_eq!(buried["data"]["position"], json!(dealer_position));

    let lead = recv_until(&mut owner, "AI opening play", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::PLAY as i64)
    })
    .await;
    assert_eq!(lead["data"]["position"], json!(dealer_position));
    let played_count = lead["data"]["cards"]
        .as_array()
        .filter(|cards| !cards.is_empty())
        .map(Vec::len)
        .expect("non-empty AI opening play");
    assert_eq!(
        lead["data"]["remaining_hand_count"],
        json!(25 - played_count)
    );

    stop_handle.stop();
    server.abort();
}

#[cfg(not(feature = "official"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tractor_incremental_deal_full_deck_and_bury_flow() {
    let port = free_port();
    let listen_addr = format!("127.0.0.1:{port}");
    let url = format!("ws://{listen_addr}");
    let server = tokio::spawn(run_room_runtime(
        RuntimeConfig {
            service_name: "tractor-test",
            listen_addr,
            idle_timeout: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(30),
        },
        TestTractorHandler::default(),
    ));

    let mut a = connect_client(&url).await;
    let mut b = connect_client(&url).await;
    let mut c = connect_client(&url).await;
    let mut d = connect_client(&url).await;
    let room = "tractor-flow-room";

    join(&mut a, "a", room).await;
    join(&mut b, "b", room).await;
    join(&mut c, "c", room).await;
    join(&mut d, "d", room).await;

    send_request(
        &mut a,
        Routes::SETTING as i32,
        json!({
            "current_configs": {
                "deck_count": 0,
                "attacking_win_score": 80,
                "score_per_level": 40,
                "shutout_bonus_levels": 1,
                "target_rank": 11,
                "first_deal_time": 1000,
                "deal_time": 500,
                "play_time": 30
            }
        }),
    )
    .await;
    recv_until(&mut a, "setting ok", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::SETTING as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;

    let started_at = Instant::now();
    send_request(&mut a, Routes::START as i32, json!({})).await;

    let mut clients: [&mut Client; 4] = [&mut a, &mut b, &mut c, &mut d];
    let mut hands: [Vec<i32>; 4] = std::array::from_fn(|_| Vec::new());
    let mut dealt_cards = Vec::new();
    let mut saw_declaration = false;
    let mut dealer_position = None;
    let mut bottom = None;
    while bottom.is_none() || !saw_declaration {
        for (client_position, client) in clients.iter_mut().enumerate() {
            let value = recv_json(client, "incremental deal, declaration and bottom").await;
            match value.get("code").and_then(Value::as_i64) {
                Some(code) if code == WsCode::DEAL as i64 => {
                    let cards = value["data"]["cards"].as_array().expect("deal cards");
                    assert_eq!(cards.len(), 1, "deal must be incremental");
                    let card = cards[0].as_i64().expect("card") as i32;
                    hands[client_position].push(card);
                    if client_position == 0 {
                        dealt_cards.push(card);
                    }
                }
                Some(code) if code == TractorWsCode::TRUMP_DECLARED as i64 => {
                    saw_declaration = true;
                    assert!(!value["data"]["cards"].as_array().unwrap().is_empty());
                }
                Some(code) if code == TractorWsCode::BOTTOM_CARDS as i64 && bottom.is_none() => {
                    dealer_position = value["data"]["position"].as_u64();
                    bottom = Some(value);
                }
                _ => {}
            }
        }
    }
    assert!(saw_declaration, "first deal must declare trump");
    let dealer_position = dealer_position.expect("bottom event dealer position") as usize;
    assert!(dealer_position < clients.len());
    let bottom = bottom.expect("bottom event");
    assert!(started_at.elapsed() >= Duration::from_millis(1_100));
    assert_eq!(dealt_cards.len(), 25);
    let bottom_cards = bottom["data"]["cards"]
        .as_array()
        .expect("bottom cards")
        .iter()
        .map(|card| card.as_i64().expect("bottom card") as i32)
        .collect::<Vec<_>>();
    assert_eq!(bottom_cards.len(), 8);
    assert_eq!(bottom["data"]["required_count"], json!(8));

    let dealer = &mut *clients[dealer_position];
    send_request(
        dealer,
        TractorRoutes::BURY_BOTTOM as i32,
        json!({ "cards": bottom_cards.clone() }),
    )
    .await;
    let snapshot = recv_until(dealer, "play snapshot", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
            && value["data"]["phase"] == json!(TractorPhase::Play as i8)
    })
    .await;
    recv_until(dealer, "bury response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(TractorRoutes::BURY_BOTTOM as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;
    assert_eq!(snapshot["data"]["deck_count"], json!(2));
    assert_eq!(snapshot["data"]["target_rank"], json!(3));
    assert_eq!(snapshot["data"]["final_target_rank"], json!(14));
    assert_eq!(snapshot["data"]["removed_rank_count"], json!(0));
    assert_eq!(snapshot["data"]["bottom_card_count"], json!(8));
    assert_eq!(snapshot["data"]["attacking_win_score"], json!(80));
    assert_eq!(snapshot["data"]["score_per_level"], json!(40));
    assert_eq!(snapshot["data"]["shutout_bonus_levels"], json!(1));
    assert_eq!(snapshot["data"]["dealt_count"], json!(100));
    assert_eq!(snapshot["data"]["total_deal_count"], json!(100));
    assert_eq!(
        snapshot["data"]["player_hand_counts"][0]["hand_count"],
        json!(25)
    );

    send_request(dealer, Routes::PLAY as i32, json!({ "cards": [999] })).await;
    let invalid_play = recv_until(dealer, "invalid tractor play response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::PLAY as i64)
            && value.get("code").and_then(Value::as_i64)
                == Some(WsResponseCode::NO_PERMISSION as i64)
    })
    .await;
    assert_eq!(
        invalid_play["code"],
        json!(WsResponseCode::NO_PERMISSION as i32)
    );

    hands[dealer_position].extend(bottom_cards.iter().copied());
    for card in &bottom_cards {
        let index = hands[dealer_position]
            .iter()
            .position(|candidate| candidate == card)
            .expect("bottom card was dealt to dealer");
        hands[dealer_position].remove(index);
    }
    let trump_suit = snapshot["data"]["trump_suit"]
        .as_i64()
        .map(|suit| match suit {
            0 => TractorSuit::SPADE,
            1 => TractorSuit::HEART,
            2 => TractorSuit::CLUB,
            3 => TractorSuit::DIAMOND,
            _ => panic!("invalid trump suit"),
        });
    let rules = TractorRules {
        attacking_win_score: 80,
        score_per_level: 40,
        shutout_bonus_levels: 1,
        bottom_card_count: 8,
        deck_count: 2,
        final_target_rank: TractorRank::A,
        target_rank: TractorRank::THREE,
        trump_suit,
    };
    let lead = hands[dealer_position]
        .first()
        .copied()
        .expect("dealer has a card after burying");
    let lead_combo = combo::classify(&[lead], &rules).expect("single lead is valid");
    for play_index in 0..4 {
        let position = (dealer_position + play_index) % 4;
        let card = if play_index == 0 {
            lead
        } else {
            let hand = &hands[position];
            hand.iter()
                .copied()
                .find(|candidate| combo::follow_is_legal(hand, &[*candidate], &lead_combo, &rules))
                .or_else(|| hand.first().copied())
                .expect("follower has a card")
        };
        let client = &mut *clients[position];
        send_request(client, Routes::PLAY as i32, json!({ "cards": [card] })).await;
        let played = recv_until(client, "tractor play event", |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::PLAY as i64)
                && value["data"]["position"] == json!(position)
        })
        .await;
        assert_eq!(
            played["data"]["cards"],
            json!([card]),
            "position={position} dealer={dealer_position} requested={card} event={played} hands={hands:?}"
        );
        let play_snapshot = recv_until(client, "tractor trick snapshot", |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
                && value["data"]["phase"] == json!(TractorPhase::Play as i8)
                && value["data"]["trick_index"] == json!(if play_index == 3 { 1 } else { 0 })
        })
        .await;
        if play_index == 3 {
            assert_eq!(play_snapshot["data"]["trick_index"], json!(1));
        }
        let response = recv_until(client, "tractor play response", |value| {
            value.get("route").and_then(Value::as_i64) == Some(Routes::PLAY as i64)
                && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
        })
        .await;
        assert_eq!(response["code"], json!(WsResponseCode::OK as i32));
        let index = hands[position]
            .iter()
            .position(|candidate| *candidate == card)
            .expect("played card was in the private hand");
        hands[position].remove(index);
    }

    server.abort();
}

#[cfg(not(feature = "official"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tractor_ws_rejoin_preserves_running_bury_state() {
    let port = free_port();
    let listen_addr = format!("127.0.0.1:{port}");
    let url = format!("ws://{listen_addr}");
    let server = tokio::spawn(run_room_runtime(
        RuntimeConfig {
            service_name: "tractor-rejoin-bury-test",
            listen_addr,
            idle_timeout: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(30),
        },
        TestTractorHandler::default(),
    ));

    let mut a = connect_client(&url).await;
    let mut b = connect_client(&url).await;
    let mut c = connect_client(&url).await;
    let mut d = connect_client(&url).await;
    let room = "tractor-rejoin-bury-room";
    join(&mut a, "a", room).await;
    join(&mut b, "b", room).await;
    join(&mut c, "c", room).await;
    join(&mut d, "d", room).await;

    send_request(
        &mut a,
        Routes::SETTING as i32,
        json!({
            "current_configs": {
                "deck_count": 0,
                "target_rank": 11,
                "first_deal_time": 1,
                "deal_time": 1,
                "play_time": 30
            }
        }),
    )
    .await;
    recv_until(&mut a, "rejoin setting response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::SETTING as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;
    send_request(&mut a, Routes::START as i32, json!({})).await;
    recv_until(&mut a, "rejoin start response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::START as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;

    let mut clients: [&mut Client; 4] = [&mut a, &mut b, &mut c, &mut d];
    let hands = collect_tractor_hands(&mut clients, 25).await;
    let (declaration, bottom_seen_by_a) = recv_first_declaration(&mut *clients[0]).await;
    let dealer_position = declaration["data"]["position"]
        .as_i64()
        .expect("rejoin dealer position") as usize;
    let bottom = if dealer_position == 0 {
        match bottom_seen_by_a {
            Some(bottom) => bottom,
            None => recv_tractor_bottom(&mut *clients[0], 0).await,
        }
    } else {
        recv_tractor_bottom(&mut *clients[dealer_position], dealer_position).await
    };
    let bottom_cards = bottom["data"]["cards"]
        .as_array()
        .expect("rejoin bottom cards")
        .iter()
        .map(|card| card.as_i64().expect("rejoin bottom card") as i32)
        .collect::<Vec<_>>();
    assert_eq!(bottom_cards.len(), 8);

    // The old socket is deliberately closed while the game is waiting for
    // the dealer to bury. The replacement must reclaim the same seat instead
    // of receiving a fresh position or resetting the game state.
    clients[0].close(None).await.expect("close player a socket");
    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut rejoined = connect_client(&url).await;
    let joined = join(&mut rejoined, "a", room).await;
    assert_eq!(joined["data"]["self_position"], json!(0));
    let hand_update = recv_until(&mut rejoined, "rejoined private hand", |value| {
        value.get("code").and_then(Value::as_i64) == Some(TractorWsCode::HAND_UPDATED as i64)
    })
    .await;
    let restored_hand = hand_update["data"]["cards"]
        .as_array()
        .expect("rejoined hand cards")
        .iter()
        .map(|card| card.as_i64().expect("rejoined card") as i32)
        .collect::<Vec<_>>();
    let mut expected_hand = hands[0].clone();
    if dealer_position == 0 {
        expected_hand.extend(bottom_cards.iter().copied());
    }
    let mut restored_hand_sorted = restored_hand.clone();
    let mut expected_hand_sorted = expected_hand;
    restored_hand_sorted.sort_unstable();
    expected_hand_sorted.sort_unstable();
    assert_eq!(restored_hand_sorted, expected_hand_sorted);
    let snapshot = recv_until(&mut rejoined, "rejoined bury snapshot", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
    })
    .await;
    assert_eq!(snapshot["data"]["phase"], json!(TractorPhase::Bury as i8));
    assert_eq!(snapshot["data"]["round_index"], json!(0));

    if dealer_position == 0 {
        send_request(
            &mut rejoined,
            TractorRoutes::BURY_BOTTOM as i32,
            json!({ "cards": bottom_cards }),
        )
        .await;
        let buried = recv_until(&mut rejoined, "rejoined bury event", |value| {
            value.get("code").and_then(Value::as_i64) == Some(TractorWsCode::BOTTOM_BURIED as i64)
                && value["data"]["position"] == json!(0)
        })
        .await;
        assert_eq!(buried["data"]["position"], json!(0));
        let bury_response = recv_until(&mut rejoined, "rejoined bury response", |value| {
            value.get("route").and_then(Value::as_i64) == Some(TractorRoutes::BURY_BOTTOM as i64)
                && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
        })
        .await;
        assert_eq!(bury_response["code"], json!(WsResponseCode::OK as i32));
    }

    server.abort();
}

#[cfg(not(feature = "official"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tractor_server_completes_round_and_enters_later_round() {
    let port = free_port();
    let listen_addr = format!("127.0.0.1:{port}");
    let url = format!("ws://{listen_addr}");
    let server = tokio::spawn(run_room_runtime(
        RuntimeConfig {
            service_name: "tractor-full-round-test",
            listen_addr,
            idle_timeout: Duration::from_secs(60),
            heartbeat_interval: Duration::from_secs(60),
        },
        TestTractorHandler::default(),
    ));

    let mut a = connect_client(&url).await;
    let mut b = connect_client(&url).await;
    let mut c = connect_client(&url).await;
    let mut d = connect_client(&url).await;
    let room = "tractor-full-round-room";
    join(&mut a, "a", room).await;
    join(&mut b, "b", room).await;
    join(&mut c, "c", room).await;
    join(&mut d, "d", room).await;

    send_request(
        &mut a,
        Routes::SETTING as i32,
        json!({
            "current_configs": {
                "deck_count": 0,
                "attacking_win_score": 80,
                "score_per_level": 40,
                "shutout_bonus_levels": 1,
                "target_rank": 11,
                "first_deal_time": 1000,
                "deal_time": 500,
                "play_time": 30,
                "settlement_time": 1
            }
        }),
    )
    .await;
    recv_until(&mut a, "full tractor setting response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::SETTING as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;
    send_request(&mut a, Routes::START as i32, json!({})).await;
    recv_until(&mut a, "full tractor start response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::START as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;

    let mut clients: [&mut Client; 4] = [&mut a, &mut b, &mut c, &mut d];
    let mut first_hands = collect_tractor_hands(&mut clients, 25).await;
    assert!(first_hands.iter().all(|hand| hand.len() == 25));
    let (declaration, bottom_seen_by_first_client) = recv_first_declaration(&mut *clients[0]).await;
    let first_dealer = declaration["data"]["position"]
        .as_i64()
        .expect("first dealer") as usize;
    assert!(first_dealer < 4);
    let bottom = if first_dealer == 0 {
        match bottom_seen_by_first_client {
            Some(bottom) => bottom,
            None => recv_tractor_bottom(&mut *clients[first_dealer], first_dealer).await,
        }
    } else {
        recv_tractor_bottom(&mut *clients[first_dealer], first_dealer).await
    };
    let first_bottom = bottom["data"]["cards"]
        .as_array()
        .expect("first bottom")
        .iter()
        .map(|card| card.as_i64().expect("bottom card") as i32)
        .collect::<Vec<_>>();
    assert_eq!(first_bottom.len(), 8);

    send_request(
        &mut *clients[first_dealer],
        TractorRoutes::BURY_BOTTOM as i32,
        json!({ "cards": first_bottom }),
    )
    .await;
    let first_play_snapshot = recv_until(
        &mut *clients[first_dealer],
        "full tractor play phase",
        |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
                && value["data"]["phase"] == json!(TractorPhase::Play as i8)
        },
    )
    .await;
    recv_until(
        &mut *clients[first_dealer],
        "full tractor bury response",
        |value| {
            value.get("route").and_then(Value::as_i64) == Some(TractorRoutes::BURY_BOTTOM as i64)
                && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
        },
    )
    .await;
    assert_eq!(first_play_snapshot["data"]["round_index"], json!(0));
    assert_eq!(
        first_play_snapshot["data"]["phase"],
        json!(TractorPhase::Play as i8)
    );

    let trump_suit = first_play_snapshot["data"]["trump_suit"]
        .as_i64()
        .map(|suit| match suit {
            0 => TractorSuit::SPADE,
            1 => TractorSuit::HEART,
            2 => TractorSuit::CLUB,
            3 => TractorSuit::DIAMOND,
            _ => panic!("invalid trump suit"),
        });
    let rules = TractorRules {
        attacking_win_score: 80,
        score_per_level: 40,
        shutout_bonus_levels: 1,
        bottom_card_count: 8,
        deck_count: 2,
        final_target_rank: TractorRank::A,
        target_rank: TractorRank::THREE,
        trump_suit,
    };
    let mut current_position = first_dealer;
    let mut lead_combo = None;
    let mut later_dealer = None;
    for play_index in 0..100usize {
        let hand = &first_hands[current_position];
        let cards = match lead_combo {
            None => vec![*hand.first().expect("lead hand")],
            Some(ref lead) => combo::forced_follow(hand, lead, &rules).expect("legal follow"),
        };
        send_request(
            &mut *clients[current_position],
            Routes::PLAY as i32,
            json!({ "cards": cards }),
        )
        .await;
        let played = recv_until(
            &mut *clients[current_position],
            "full tractor play event",
            |value| {
                value.get("code").and_then(Value::as_i64) == Some(WsCode::PLAY as i64)
                    && value["data"]["position"] == json!(current_position)
            },
        )
        .await;
        let played_cards = played["data"]["cards"]
            .as_array()
            .expect("played cards")
            .iter()
            .map(|card| card.as_i64().expect("played card") as i32)
            .collect::<Vec<_>>();
        assert!(!played_cards.is_empty());
        for card in &played_cards {
            let index = first_hands[current_position]
                .iter()
                .position(|candidate| candidate == card)
                .expect("played card was in hand");
            first_hands[current_position].remove(index);
        }
        let expected_trick_index = ((play_index + 1) / 4) as i64;
        let expected_phase = if play_index + 1 == 100 {
            TractorPhase::Settlement
        } else {
            TractorPhase::Play
        };
        let snapshot = recv_until(
            &mut *clients[current_position],
            "full tractor play snapshot",
            |value| {
                value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
                    && value["data"]["phase"] == json!(expected_phase as i8)
                    && value["data"]["trick_index"] == json!(expected_trick_index)
            },
        )
        .await;
        assert_eq!(
            snapshot["data"]["player_hand_counts"]
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| entry["hand_count"].as_i64().unwrap())
                .sum::<i64>(),
            (100 - play_index - 1) as i64
        );
        if play_index + 1 == 100 {
            let game_over = recv_until(
                &mut *clients[current_position],
                "tractor game over",
                |value| value.get("code").and_then(Value::as_i64) == Some(WsCode::GAME_OVER as i64),
            )
            .await;
            assert!(
                game_over["data"]["winner_positions"]
                    .as_array()
                    .is_some_and(|winners| !winners.is_empty())
            );
            assert!(
                game_over["data"]["player_scores"]
                    .as_object()
                    .is_some_and(|scores| scores.len() == 4)
            );
            later_dealer = game_over["data"]["winner_positions"]
                .as_array()
                .and_then(|winners| winners.first())
                .and_then(Value::as_i64)
                .map(|position| position as usize);
            recv_until(
                &mut *clients[current_position],
                "full tractor final play response",
                |value| {
                    value.get("route").and_then(Value::as_i64) == Some(Routes::PLAY as i64)
                        && value.get("code").and_then(Value::as_i64)
                            == Some(WsResponseCode::OK as i64)
                },
            )
            .await;
            break;
        }
        recv_until(
            &mut *clients[current_position],
            "full tractor play response",
            |value| {
                value.get("route").and_then(Value::as_i64) == Some(Routes::PLAY as i64)
                    && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
            },
        )
        .await;
        current_position = snapshot["data"]["current_position"]
            .as_i64()
            .expect("next tractor position") as usize;
        if snapshot["data"]["current_trick"]
            .as_array()
            .is_some_and(|trick| trick.is_empty())
        {
            lead_combo = None;
        } else {
            let lead_card = snapshot["data"]["current_trick"][0]["cards"][0]
                .as_i64()
                .expect("lead card") as i32;
            lead_combo = Some(combo::classify(&[lead_card], &rules).expect("single lead combo"));
        }
    }
    assert!(first_hands.iter().all(Vec::is_empty));

    let later_dealer = later_dealer.expect("later dealer from settlement");
    assert!(later_dealer < 4);
    let mut later_hands = collect_tractor_hands(&mut clients, 25).await;
    let later_bottom_event = recv_tractor_bottom(&mut *clients[later_dealer], later_dealer).await;
    let later_bottom = later_bottom_event["data"]["cards"]
        .as_array()
        .expect("later tractor bottom")
        .iter()
        .map(|card| card.as_i64().expect("bottom card") as i32)
        .collect::<Vec<_>>();
    assert_eq!(later_bottom.len(), 8);
    let non_dealer = (later_dealer + 1) % 4;
    send_request(
        &mut *clients[non_dealer],
        TractorRoutes::SELECT_TRUMP as i32,
        json!({ "trump_suit": 0 }),
    )
    .await;
    let non_dealer_select = recv_until(
        &mut *clients[non_dealer],
        "non-dealer later tractor select response",
        |value| {
            value.get("route").and_then(Value::as_i64) == Some(TractorRoutes::SELECT_TRUMP as i64)
                && value.get("code").and_then(Value::as_i64)
                    == Some(WsResponseCode::NO_PERMISSION as i64)
        },
    )
    .await;
    assert_eq!(
        non_dealer_select["code"],
        json!(WsResponseCode::NO_PERMISSION as i32)
    );
    send_request(
        &mut *clients[later_dealer],
        TractorRoutes::BURY_BOTTOM as i32,
        json!({ "cards": later_bottom }),
    )
    .await;
    let bury_before_select = recv_until(
        &mut *clients[later_dealer],
        "tractor bury before select response",
        |value| {
            value.get("route").and_then(Value::as_i64) == Some(TractorRoutes::BURY_BOTTOM as i64)
                && value.get("code").and_then(Value::as_i64)
                    == Some(WsResponseCode::NO_PERMISSION as i64)
        },
    )
    .await;
    assert_eq!(
        bury_before_select["code"],
        json!(WsResponseCode::NO_PERMISSION as i32)
    );
    send_request(
        &mut *clients[later_dealer],
        TractorRoutes::SELECT_TRUMP as i32,
        json!({ "trump_suit": 0 }),
    )
    .await;
    let selected_snapshot = recv_until(
        &mut *clients[later_dealer],
        "later tractor selected snapshot",
        |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
                && value["data"]["round_index"] == json!(1)
                && value["data"]["phase"] == json!(TractorPhase::Bury as i8)
                && value["data"]["trump_suit"] != Value::Null
        },
    )
    .await;
    assert_eq!(selected_snapshot["data"]["trump_suit"], json!(0));
    let selected = recv_until(
        &mut *clients[later_dealer],
        "later tractor select response",
        |value| {
            value.get("route").and_then(Value::as_i64) == Some(TractorRoutes::SELECT_TRUMP as i64)
                && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
        },
    )
    .await;
    assert_eq!(selected["code"], json!(WsResponseCode::OK as i32));
    send_request(
        &mut *clients[later_dealer],
        TractorRoutes::BURY_BOTTOM as i32,
        json!({ "cards": later_bottom }),
    )
    .await;
    let later_play = recv_until(
        &mut *clients[later_dealer],
        "later tractor play snapshot",
        |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
                && value["data"]["round_index"] == json!(1)
                && value["data"]["phase"] == json!(TractorPhase::Play as i8)
        },
    )
    .await;
    recv_until(
        &mut *clients[later_dealer],
        "later tractor bury response",
        |value| {
            value.get("route").and_then(Value::as_i64) == Some(TractorRoutes::BURY_BOTTOM as i64)
                && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
        },
    )
    .await;
    let later_card = later_hands[later_dealer].pop().expect("later dealer card");
    send_request(
        &mut *clients[later_dealer],
        Routes::PLAY as i32,
        json!({ "cards": [later_card] }),
    )
    .await;
    let later_play_event = recv_until(
        &mut *clients[later_dealer],
        "later tractor first play",
        |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::PLAY as i64)
                && value["data"]["position"] == json!(later_dealer)
        },
    )
    .await;
    assert_eq!(later_play_event["data"]["cards"], json!([later_card]));
    assert_eq!(later_play["data"]["round_index"], json!(1));
    recv_until(
        &mut *clients[later_dealer],
        "later tractor first play response",
        |value| {
            value.get("route").and_then(Value::as_i64) == Some(Routes::PLAY as i64)
                && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
        },
    )
    .await;

    server.abort();
}

#[cfg(not(feature = "official"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tractor_three_deck_ws_deals_and_buries_the_correct_counts() {
    let port = free_port();
    let listen_addr = format!("127.0.0.1:{port}");
    let url = format!("ws://{listen_addr}");
    let server = tokio::spawn(run_room_runtime(
        RuntimeConfig {
            service_name: "tractor-three-deck-test",
            listen_addr,
            idle_timeout: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(30),
        },
        TestTractorHandler::default(),
    ));

    let mut a = connect_client(&url).await;
    let mut b = connect_client(&url).await;
    let mut c = connect_client(&url).await;
    let mut d = connect_client(&url).await;
    let room = "tractor-three-deck-room";
    join(&mut a, "a", room).await;
    join(&mut b, "b", room).await;
    join(&mut c, "c", room).await;
    join(&mut d, "d", room).await;

    send_request(
        &mut a,
        Routes::SETTING as i32,
        json!({
            "current_configs": {
                "deck_count": 1,
                "attacking_win_score": 80,
                "score_per_level": 40,
                "shutout_bonus_levels": 1,
                "first_deal_time": 1000,
                "deal_time": 500,
                "play_time": 30
            }
        }),
    )
    .await;
    recv_until(&mut a, "three-deck setting response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::SETTING as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;
    send_request(&mut a, Routes::START as i32, json!({})).await;
    recv_until(&mut a, "three-deck start response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::START as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;

    let mut clients: [&mut Client; 4] = [&mut a, &mut b, &mut c, &mut d];
    let hands = collect_tractor_hands(&mut clients, 38).await;
    assert!(hands.iter().all(|hand| hand.len() == 38));
    let (declaration, bottom_seen_by_first_client) = recv_first_declaration(&mut *clients[0]).await;
    let dealer_position = declaration["data"]["position"]
        .as_i64()
        .expect("three-deck dealer") as usize;
    assert!(dealer_position < 4);
    let bottom_event = if dealer_position == 0 {
        match bottom_seen_by_first_client {
            Some(bottom) => bottom,
            None => recv_tractor_bottom(&mut *clients[dealer_position], dealer_position).await,
        }
    } else {
        recv_tractor_bottom(&mut *clients[dealer_position], dealer_position).await
    };
    let bottom_cards = bottom_event["data"]["cards"]
        .as_array()
        .expect("three-deck bottom cards")
        .iter()
        .map(|card| card.as_i64().expect("three-deck bottom card") as i32)
        .collect::<Vec<_>>();
    assert_eq!(bottom_cards.len(), 10);
    assert_eq!(bottom_event["data"]["required_count"], json!(10));

    send_request(
        &mut *clients[dealer_position],
        TractorRoutes::BURY_BOTTOM as i32,
        json!({ "cards": &bottom_cards[..9] }),
    )
    .await;
    let invalid_bury = recv_until(
        &mut *clients[dealer_position],
        "three-deck invalid bury",
        |value| {
            value.get("route").and_then(Value::as_i64) == Some(TractorRoutes::BURY_BOTTOM as i64)
                && value.get("code").and_then(Value::as_i64)
                    == Some(WsResponseCode::NO_PERMISSION as i64)
        },
    )
    .await;
    assert_eq!(
        invalid_bury["code"],
        json!(WsResponseCode::NO_PERMISSION as i32)
    );

    send_request(
        &mut *clients[dealer_position],
        TractorRoutes::BURY_BOTTOM as i32,
        json!({ "cards": bottom_cards }),
    )
    .await;
    let snapshot = recv_until(
        &mut *clients[dealer_position],
        "three-deck play snapshot",
        |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
                && value["data"]["phase"] == json!(TractorPhase::Play as i8)
        },
    )
    .await;
    assert_eq!(snapshot["data"]["deck_count"], json!(3));
    assert_eq!(snapshot["data"]["bottom_card_count"], json!(10));
    assert_eq!(snapshot["data"]["hand_count"], json!(38));
    assert_eq!(snapshot["data"]["dealt_count"], json!(152));
    assert_eq!(snapshot["data"]["total_deal_count"], json!(152));
    assert_eq!(
        snapshot["data"]["player_hand_counts"]
            .as_array()
            .expect("three-deck hand counts")
            .iter()
            .map(|entry| entry["hand_count"].as_i64().expect("hand count"))
            .sum::<i64>(),
        152
    );
    let buried = recv_until(
        &mut *clients[dealer_position],
        "three-deck bury response",
        |value| {
            value.get("route").and_then(Value::as_i64) == Some(TractorRoutes::BURY_BOTTOM as i64)
                && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
        },
    )
    .await;
    assert_eq!(buried["code"], json!(WsResponseCode::OK as i32));

    server.abort();
}

#[cfg(not(feature = "official"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tractor_ws_failed_throw_reports_attempted_and_played_components() {
    let port = free_port();
    let listen_addr = format!("127.0.0.1:{port}");
    let url = format!("ws://{listen_addr}");
    let server = tokio::spawn(run_room_runtime(
        RuntimeConfig {
            service_name: "tractor-failed-throw-test",
            listen_addr,
            idle_timeout: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(30),
        },
        TestTractorHandler::default(),
    ));

    let mut a = connect_client(&url).await;
    let mut b = connect_client(&url).await;
    let mut c = connect_client(&url).await;
    let mut d = connect_client(&url).await;
    let room = "tractor-failed-throw-room";
    join(&mut a, "a", room).await;
    join(&mut b, "b", room).await;
    join(&mut c, "c", room).await;
    join(&mut d, "d", room).await;

    send_request(
        &mut a,
        Routes::SETTING as i32,
        json!({
            "current_configs": {
                "deck_count": 1,
                "first_deal_time": 1000,
                "deal_time": 500,
                "play_time": 30
            }
        }),
    )
    .await;
    recv_until(&mut a, "failed throw setting response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::SETTING as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;
    send_request(&mut a, Routes::START as i32, json!({})).await;
    recv_until(&mut a, "failed throw start response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::START as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;

    let mut clients: [&mut Client; 4] = [&mut a, &mut b, &mut c, &mut d];
    let mut hands = collect_tractor_hands(&mut clients, 38).await;
    let (declaration, bottom_seen_by_first_client) = recv_first_declaration(&mut *clients[0]).await;
    let dealer_position = declaration["data"]["position"]
        .as_i64()
        .expect("failed throw dealer") as usize;
    let bottom = if dealer_position == 0 {
        match bottom_seen_by_first_client {
            Some(bottom) => bottom,
            None => recv_tractor_bottom(&mut *clients[dealer_position], dealer_position).await,
        }
    } else {
        recv_tractor_bottom(&mut *clients[dealer_position], dealer_position).await
    };
    let bottom_cards = bottom["data"]["cards"]
        .as_array()
        .expect("failed throw bottom cards")
        .iter()
        .map(|card| card.as_i64().expect("failed throw bottom card") as i32)
        .collect::<Vec<_>>();
    assert_eq!(bottom_cards.len(), 10);

    send_request(
        &mut *clients[dealer_position],
        TractorRoutes::BURY_BOTTOM as i32,
        json!({ "cards": bottom_cards }),
    )
    .await;
    let snapshot = recv_until(
        &mut *clients[dealer_position],
        "failed throw play snapshot",
        |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
                && value["data"]["phase"] == json!(TractorPhase::Play as i8)
        },
    )
    .await;
    recv_until(
        &mut *clients[dealer_position],
        "failed throw bury response",
        |value| {
            value.get("route").and_then(Value::as_i64) == Some(TractorRoutes::BURY_BOTTOM as i64)
                && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
        },
    )
    .await;

    let trump_suit = snapshot["data"]["trump_suit"]
        .as_i64()
        .map(|suit| match suit {
            0 => TractorSuit::SPADE,
            1 => TractorSuit::HEART,
            2 => TractorSuit::CLUB,
            3 => TractorSuit::DIAMOND,
            _ => panic!("invalid failed throw trump suit"),
        });
    let rules = TractorRules {
        attacking_win_score: 80,
        score_per_level: 40,
        shutout_bonus_levels: 1,
        bottom_card_count: 10,
        deck_count: 3,
        final_target_rank: TractorRank::A,
        target_rank: TractorRank::THREE,
        trump_suit,
    };
    let (attempted, expected_played) = find_failed_throw_candidate(&hands, dealer_position, &rules)
        .expect("three-deck deal should expose a beatable plain-suit throw");
    send_request(
        &mut *clients[dealer_position],
        Routes::PLAY as i32,
        json!({ "cards": attempted.clone() }),
    )
    .await;
    let played = recv_until(
        &mut *clients[dealer_position],
        "failed throw play event",
        |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::PLAY as i64)
                && value["data"]["position"] == json!(dealer_position)
        },
    )
    .await;
    assert_eq!(played["data"]["cards"], json!(expected_played.clone()));
    assert_eq!(
        played["data"]["failed_throw"]["attempted_cards"],
        json!(attempted.clone())
    );
    assert_eq!(
        played["data"]["failed_throw"]["played_cards"],
        json!(expected_played.clone())
    );
    let failed_throws = recv_until(
        &mut *clients[dealer_position],
        "failed throw snapshot",
        |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
                && value["data"]["failed_throws"]
                    .as_array()
                    .is_some_and(|items| !items.is_empty())
        },
    )
    .await;
    assert_eq!(
        failed_throws["data"]["failed_throws"][0]["attempted_cards"],
        json!(attempted)
    );
    assert_eq!(
        failed_throws["data"]["failed_throws"][0]["played_cards"],
        json!(expected_played)
    );
    recv_until(
        &mut *clients[dealer_position],
        "failed throw play response",
        |value| {
            value.get("route").and_then(Value::as_i64) == Some(Routes::PLAY as i64)
                && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
        },
    )
    .await;

    hands[dealer_position].retain(|card| !expected_played.contains(card));
    assert_eq!(hands[dealer_position].len(), 37);
    server.abort();
}

#[cfg(not(feature = "official"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tractor_ws_rejects_an_off_suit_follow_and_accepts_the_next_legal_card() {
    let port = free_port();
    let listen_addr = format!("127.0.0.1:{port}");
    let url = format!("ws://{listen_addr}");
    let server = tokio::spawn(run_room_runtime(
        RuntimeConfig {
            service_name: "tractor-illegal-follow-test",
            listen_addr,
            idle_timeout: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(30),
        },
        TestTractorHandler::default(),
    ));

    let mut a = connect_client(&url).await;
    let mut b = connect_client(&url).await;
    let mut c = connect_client(&url).await;
    let mut d = connect_client(&url).await;
    let room = "tractor-illegal-follow-room";
    join(&mut a, "a", room).await;
    join(&mut b, "b", room).await;
    join(&mut c, "c", room).await;
    join(&mut d, "d", room).await;

    send_request(
        &mut a,
        Routes::SETTING as i32,
        json!({
            "current_configs": {
                "deck_count": 1,
                "first_deal_time": 1000,
                "deal_time": 500,
                "play_time": 30
            }
        }),
    )
    .await;
    recv_until(&mut a, "illegal follow setting response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::SETTING as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;
    send_request(&mut a, Routes::START as i32, json!({})).await;
    recv_until(&mut a, "illegal follow start response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::START as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;

    let mut clients: [&mut Client; 4] = [&mut a, &mut b, &mut c, &mut d];
    let hands = collect_tractor_hands(&mut clients, 38).await;
    let (declaration, bottom_seen_by_first_client) = recv_first_declaration(&mut *clients[0]).await;
    let dealer_position = declaration["data"]["position"]
        .as_i64()
        .expect("illegal follow dealer") as usize;
    let bottom = if dealer_position == 0 {
        match bottom_seen_by_first_client {
            Some(bottom) => bottom,
            None => recv_tractor_bottom(&mut *clients[dealer_position], dealer_position).await,
        }
    } else {
        recv_tractor_bottom(&mut *clients[dealer_position], dealer_position).await
    };
    let bottom_cards = bottom["data"]["cards"]
        .as_array()
        .expect("illegal follow bottom cards")
        .iter()
        .map(|card| card.as_i64().expect("illegal follow bottom card") as i32)
        .collect::<Vec<_>>();
    assert_eq!(bottom_cards.len(), 10);

    send_request(
        &mut *clients[dealer_position],
        TractorRoutes::BURY_BOTTOM as i32,
        json!({ "cards": bottom_cards }),
    )
    .await;
    let snapshot = recv_until(
        &mut *clients[dealer_position],
        "illegal follow play snapshot",
        |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
                && value["data"]["phase"] == json!(TractorPhase::Play as i8)
        },
    )
    .await;
    recv_until(
        &mut *clients[dealer_position],
        "illegal follow bury response",
        |value| {
            value.get("route").and_then(Value::as_i64) == Some(TractorRoutes::BURY_BOTTOM as i64)
                && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
        },
    )
    .await;

    let trump_suit = snapshot["data"]["trump_suit"]
        .as_i64()
        .map(|suit| match suit {
            0 => TractorSuit::SPADE,
            1 => TractorSuit::HEART,
            2 => TractorSuit::CLUB,
            3 => TractorSuit::DIAMOND,
            _ => panic!("invalid illegal follow trump suit"),
        });
    let rules = TractorRules {
        attacking_win_score: 80,
        score_per_level: 40,
        shutout_bonus_levels: 1,
        bottom_card_count: 10,
        deck_count: 3,
        final_target_rank: TractorRank::A,
        target_rank: TractorRank::THREE,
        trump_suit,
    };
    let (lead_card, follower_position, legal_card, illegal_card) =
        find_illegal_follow_case(&hands, dealer_position, &rules)
            .expect("three-deck deal should expose an off-suit follow case");

    send_request(
        &mut *clients[dealer_position],
        Routes::PLAY as i32,
        json!({ "cards": [lead_card] }),
    )
    .await;
    let lead_event = recv_until(
        &mut *clients[dealer_position],
        "illegal follow lead event",
        |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::PLAY as i64)
                && value["data"]["position"] == json!(dealer_position)
        },
    )
    .await;
    assert_eq!(lead_event["data"]["cards"], json!([lead_card]));
    recv_until(
        &mut *clients[dealer_position],
        "illegal follow lead response",
        |value| {
            value.get("route").and_then(Value::as_i64) == Some(Routes::PLAY as i64)
                && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
        },
    )
    .await;

    send_request(
        &mut *clients[follower_position],
        Routes::PLAY as i32,
        json!({ "cards": [illegal_card] }),
    )
    .await;
    let invalid = recv_until(
        &mut *clients[follower_position],
        "illegal follow response",
        |value| {
            value.get("route").and_then(Value::as_i64) == Some(Routes::PLAY as i64)
                && value.get("code").and_then(Value::as_i64)
                    == Some(WsResponseCode::NO_PERMISSION as i64)
        },
    )
    .await;
    assert_eq!(invalid["code"], json!(WsResponseCode::NO_PERMISSION as i32));

    send_request(
        &mut *clients[follower_position],
        Routes::PLAY as i32,
        json!({ "cards": [legal_card] }),
    )
    .await;
    let legal_event = recv_until(
        &mut *clients[follower_position],
        "legal follow event",
        |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::PLAY as i64)
                && value["data"]["position"] == json!(follower_position)
        },
    )
    .await;
    assert_eq!(legal_event["data"]["cards"], json!([legal_card]));
    recv_until(
        &mut *clients[follower_position],
        "legal follow response",
        |value| {
            value.get("route").and_then(Value::as_i64) == Some(Routes::PLAY as i64)
                && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
        },
    )
    .await;

    server.abort();
}

#[cfg(not(feature = "official"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tractor_ws_auto_buries_after_three_play_windows() {
    let port = free_port();
    let listen_addr = format!("127.0.0.1:{port}");
    let url = format!("ws://{listen_addr}");
    let server = tokio::spawn(run_room_runtime(
        RuntimeConfig {
            service_name: "tractor-auto-bury-window-test",
            listen_addr,
            idle_timeout: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(30),
        },
        TestTractorHandler::default(),
    ));

    let mut a = connect_client(&url).await;
    let mut b = connect_client(&url).await;
    let mut c = connect_client(&url).await;
    let mut d = connect_client(&url).await;
    let room = "tractor-auto-bury-window-room";
    join(&mut a, "a", room).await;
    join(&mut b, "b", room).await;
    join(&mut c, "c", room).await;
    join(&mut d, "d", room).await;

    send_request(
        &mut a,
        Routes::SETTING as i32,
        json!({
            "current_configs": {
                "deck_count": 0,
                "first_deal_time": 1,
                "deal_time": 1,
                "play_time": 1
            }
        }),
    )
    .await;
    recv_until(&mut a, "auto bury window setting response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::SETTING as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;
    send_request(&mut a, Routes::START as i32, json!({})).await;
    recv_until(&mut a, "auto bury window start response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::START as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;

    let clients: [&mut Client; 4] = [&mut a, &mut b, &mut c, &mut d];
    let (declaration, bottom_seen_by_first_client) = recv_first_declaration(&mut *clients[0]).await;
    let dealer_position = declaration["data"]["position"]
        .as_i64()
        .expect("auto bury dealer") as usize;
    let bottom = if dealer_position == 0 {
        match bottom_seen_by_first_client {
            Some(bottom) => bottom,
            None => recv_tractor_bottom(&mut *clients[dealer_position], dealer_position).await,
        }
    } else {
        recv_tractor_bottom(&mut *clients[dealer_position], dealer_position).await
    };
    assert_eq!(
        bottom["data"]["required_count"],
        json!(8),
        "two-deck tractor uses eight bottom cards"
    );

    let bury_snapshot = recv_until(
        &mut *clients[dealer_position],
        "auto bury countdown snapshot",
        |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
                && value["data"]["phase"] == json!(TractorPhase::Bury as i8)
                && value["data"]["turn_countdown"] == json!(3)
        },
    )
    .await;
    assert_eq!(bury_snapshot["data"]["turn_countdown"], json!(3));

    let buried = recv_until(&mut *clients[dealer_position], "auto bury event", |value| {
        value.get("code").and_then(Value::as_i64) == Some(TractorWsCode::BOTTOM_BURIED as i64)
            && value["data"]["position"] == json!(dealer_position)
    })
    .await;
    assert_eq!(buried["data"]["bottom_card_count"], json!(8));
    let play_snapshot = recv_until(
        &mut *clients[dealer_position],
        "auto bury play snapshot",
        |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
                && value["data"]["phase"] == json!(TractorPhase::Play as i8)
        },
    )
    .await;
    assert_eq!(play_snapshot["data"]["round_index"], json!(0));
    assert!(
        play_snapshot["data"]["turn_countdown"]
            .as_i64()
            .is_some_and(|countdown| countdown > 0)
    );

    let auto_play = recv_until(&mut *clients[dealer_position], "auto play event", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::PLAY as i64)
            && value["data"]["position"] == json!(dealer_position)
    })
    .await;
    assert_eq!(auto_play["data"]["cards"].as_array().map(Vec::len), Some(1));
    assert_eq!(auto_play["data"]["remaining_hand_count"], json!(24));
    let after_auto_play = recv_until(
        &mut *clients[dealer_position],
        "auto play snapshot",
        |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
                && value["data"]["phase"] == json!(TractorPhase::Play as i8)
                && value["data"]["trick_index"] == json!(0)
                && value["data"]["current_position"] != json!(dealer_position)
        },
    )
    .await;
    assert_ne!(
        after_auto_play["data"]["current_position"],
        json!(dealer_position)
    );

    server.abort();
}
