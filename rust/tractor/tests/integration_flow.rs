#[cfg(not(feature = "official"))]
use std::collections::HashMap;
use std::time::Duration;

use std::sync::mpsc::sync_channel;
#[cfg(not(feature = "official"))]
use std::time::Instant;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use share_type_public::{GameId, Routes, TractorWsCode, WsCode, WsResponseCode};
use share_type_public::{GameParam, GameParamRange};
#[cfg(not(feature = "official"))]
use share_type_public::{TractorPhase, TractorRank, TractorRoutes, TractorSuit};
#[cfg(not(feature = "official"))]
use share_type_public::{WsTractorPlayedCards, WsTractorSettlementEvent};
use tokio_tungstenite::{WebSocketStream, connect_async, tungstenite::Message};
#[cfg(not(feature = "official"))]
use tractor::combo;
use tractor::game::TractorGameHandler;
#[cfg(not(feature = "official"))]
use tractor::game_state::TractorRules;
#[cfg(not(feature = "official"))]
use upgrade_common::{Card, Rank};
#[cfg(not(feature = "official"))]
use ws_common::RuntimeStopHandle;
use ws_common::{
    ClientRequest, Dispatch, GameHandler, GameState, JoinAuthorization, JoinAuthorizationFuture,
    RoomService, RuntimeConfig, SessionId, SessionSenders, SettingsBuilderResult,
    run_room_runtime_until_stopped_with_ready, runtime_stop_channel,
};

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
struct TestRuntime {
    url: String,
    stop_handle: RuntimeStopHandle,
    task: tokio::task::JoinHandle<()>,
}

#[cfg(not(feature = "official"))]
impl Drop for TestRuntime {
    fn drop(&mut self) {
        self.stop_handle.stop();
        self.task.abort();
    }
}

#[cfg(not(feature = "official"))]
async fn start_test_runtime(service_name: &'static str, timeout: Duration) -> TestRuntime {
    let (stop_handle, stop_signal) = runtime_stop_channel();
    let (ready_tx, ready_rx) = sync_channel(1);
    let task = tokio::spawn(async move {
        run_room_runtime_until_stopped_with_ready(
            RuntimeConfig {
                service_name,
                listen_addr: "127.0.0.1:0".to_owned(),
                idle_timeout: timeout,
                heartbeat_interval: Duration::from_secs(5),
            },
            TestTractorHandler::default(),
            stop_signal,
            ready_tx,
        )
        .await
        .expect("tractor test runtime");
    });
    let stats = tokio::task::spawn_blocking(move || {
        ready_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("tractor test runtime readiness")
    })
    .await
    .expect("read tractor test runtime readiness");
    TestRuntime {
        url: format!("ws://{}", stats.listen_addr()),
        stop_handle,
        task,
    }
}

#[cfg(not(feature = "official"))]
async fn start_default_test_runtime(service_name: &'static str, timeout: Duration) -> TestRuntime {
    let (stop_handle, stop_signal) = runtime_stop_channel();
    let (ready_tx, ready_rx) = sync_channel(1);
    let task = tokio::spawn(async move {
        run_room_runtime_until_stopped_with_ready(
            RuntimeConfig {
                service_name,
                listen_addr: "127.0.0.1:0".to_owned(),
                idle_timeout: timeout,
                heartbeat_interval: Duration::from_secs(5),
            },
            TractorGameHandler::default(),
            stop_signal,
            ready_tx,
        )
        .await
        .expect("default tractor test runtime");
    });
    let stats = tokio::task::spawn_blocking(move || {
        ready_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("default tractor test runtime readiness")
    })
    .await
    .expect("read default tractor test runtime readiness");
    TestRuntime {
        url: format!("ws://{}", stats.listen_addr()),
        stop_handle,
        task,
    }
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
            .unwrap_or_else(|| panic!("websocket closed while waiting for {label}"))
            .unwrap_or_else(|error| {
                panic!("websocket read failed while waiting for {label}: {error}")
            });
        match frame {
            Message::Text(text) => return serde_json::from_str(text.as_ref()).expect("json frame"),
            Message::Ping(payload) => {
                client
                    .send(Message::Pong(payload))
                    .await
                    .unwrap_or_else(|error| {
                        panic!("websocket pong failed while waiting for {label}: {error}")
                    });
                continue;
            }
            Message::Pong(_) => continue,
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
async fn recv_tractor_private_deal(client: &mut Client, position: usize) -> i32 {
    let value = recv_until(client, "tractor private deal card", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::DEAL as i64)
    })
    .await;
    assert_eq!(value["data"]["position"], json!(position));
    let cards = value["data"]["cards"]
        .as_array()
        .expect("tractor private deal cards");
    assert_eq!(cards.len(), 1, "tractor deal must remain incremental");
    cards[0].as_i64().expect("tractor private deal card") as i32
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
struct FirstDealObservation {
    hand: Vec<i32>,
    declaration: Value,
    bottom: Option<Value>,
    dealer_position: usize,
}

#[cfg(not(feature = "official"))]
async fn observe_first_tractor_deal(
    client: &mut Client,
    position: usize,
    expected_hand_size: usize,
) -> FirstDealObservation {
    let mut hand = Vec::with_capacity(expected_hand_size);
    let mut declaration = None;
    let mut bottom = None;
    loop {
        let value = recv_json(client, "incremental deal, declaration and bottom").await;
        match value.get("code").and_then(Value::as_i64) {
            Some(code) if code == WsCode::DEAL as i64 => {
                assert_eq!(value["data"]["position"], json!(position));
                let cards = value["data"]["cards"].as_array().expect("deal cards");
                assert_eq!(cards.len(), 1, "deal must be incremental");
                hand.push(cards[0].as_i64().expect("card") as i32);
            }
            Some(code) if code == TractorWsCode::TRUMP_DECLARED as i64 => {
                declaration = Some(value);
            }
            Some(code) if code == TractorWsCode::BOTTOM_CARDS as i64 => {
                bottom = Some(value);
            }
            Some(code)
                if code == WsCode::TABLE_SNAPSHOT as i64
                    && value["data"]["phase"] == json!(TractorPhase::Bury as i8) =>
            {
                assert_eq!(
                    hand.len(),
                    expected_hand_size,
                    "bury phase must follow a complete private hand"
                );
                let dealer_position = value["data"]["dealer_position"]
                    .as_u64()
                    .expect("first deal snapshot dealer")
                    as usize;
                if dealer_position == position {
                    assert!(
                        bottom.is_some(),
                        "dealer must receive bottom before bury snapshot"
                    );
                }
                return FirstDealObservation {
                    hand,
                    declaration: declaration.expect("first deal must declare trump"),
                    bottom,
                    dealer_position,
                };
            }
            _ => {}
        }
    }
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
async fn play_complete_tractor_round(
    clients: &mut [&mut Client; 4],
    hands: &mut [Vec<i32>; 4],
    dealer_position: usize,
    rules: &TractorRules,
) -> (WsTractorSettlementEvent, Value) {
    let total_play_count = hands.iter().map(Vec::len).sum::<usize>();
    let mut current_position = dealer_position;
    let mut lead_combo = None;
    for play_index in 0..total_play_count {
        let hand = &hands[current_position];
        let requested_cards = match lead_combo {
            None => vec![*hand.first().expect("tractor round lead card")],
            Some(ref lead) => {
                combo::forced_follow(hand, lead, rules).expect("tractor round legal follow")
            }
        };
        send_request(
            &mut *clients[current_position],
            Routes::PLAY as i32,
            json!({ "cards": requested_cards }),
        )
        .await;
        let played = recv_until(
            &mut *clients[current_position],
            "complete tractor round play event",
            |value| {
                value.get("code").and_then(Value::as_i64) == Some(WsCode::PLAY as i64)
                    && value["data"]["position"] == json!(current_position)
            },
        )
        .await;
        let played_cards = played["data"]["cards"]
            .as_array()
            .expect("complete tractor round played cards")
            .iter()
            .map(|card| card.as_i64().expect("complete tractor round played card") as i32)
            .collect::<Vec<_>>();
        assert!(!played_cards.is_empty());
        for card in &played_cards {
            let index = hands[current_position]
                .iter()
                .position(|candidate| candidate == card)
                .expect("complete tractor round played card was in hand");
            hands[current_position].remove(index);
        }

        let final_play = play_index + 1 == total_play_count;
        let expected_phase = if final_play {
            TractorPhase::Settlement
        } else {
            TractorPhase::Play
        };
        let snapshot = recv_until(
            &mut *clients[current_position],
            "complete tractor round snapshot",
            |value| {
                value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
                    && value["data"]["phase"] == json!(expected_phase as i8)
            },
        )
        .await;
        if final_play {
            let game_over = recv_until(
                &mut *clients[current_position],
                "complete tractor round settlement",
                |value| value.get("code").and_then(Value::as_i64) == Some(WsCode::GAME_OVER as i64),
            )
            .await;
            recv_until(
                &mut *clients[current_position],
                "complete tractor round final play response",
                |value| value.get("route").and_then(Value::as_i64) == Some(Routes::PLAY as i64),
            )
            .await;
            assert!(hands.iter().all(Vec::is_empty));
            return (
                serde_json::from_value(game_over["data"].clone())
                    .expect("complete tractor round settlement payload"),
                snapshot,
            );
        }

        recv_until(
            &mut *clients[current_position],
            "complete tractor round play response",
            |value| value.get("route").and_then(Value::as_i64) == Some(Routes::PLAY as i64),
        )
        .await;
        current_position = snapshot["data"]["current_position"]
            .as_i64()
            .expect("complete tractor round next position") as usize;
        if snapshot["data"]["current_trick"]
            .as_array()
            .is_some_and(|trick| trick.is_empty())
        {
            lead_combo = None;
        } else {
            let lead_card = snapshot["data"]["current_trick"][0]["cards"][0]
                .as_i64()
                .expect("complete tractor round lead card") as i32;
            lead_combo = Some(
                combo::classify(&[lead_card], rules)
                    .expect("complete tractor round single lead combo"),
            );
        }
    }
    panic!("complete tractor round ended without settlement");
}

#[cfg(not(feature = "official"))]
async fn run_concurrent_tractor_room(
    url: &str,
    room: &str,
    deck_setting: i32,
    deck_count: usize,
    expected_hand_size: usize,
    expected_bottom_size: usize,
) -> Value {
    let mut a = connect_client(url).await;
    let mut b = connect_client(url).await;
    let mut c = connect_client(url).await;
    let mut d = connect_client(url).await;
    for (position, client) in [&mut a, &mut b, &mut c, &mut d].into_iter().enumerate() {
        let joined = join(client, &format!("{room}-player-{position}"), room).await;
        assert_eq!(joined["data"]["self_position"], json!(position));
        assert_eq!(joined["data"]["current_configs"]["deck_count"], json!(0));
    }
    send_request(
        &mut a,
        Routes::SETTING as i32,
        json!({
            "current_configs": {
                "deck_count": deck_setting,
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
    let setting = recv_until(&mut a, "concurrent tractor room setting", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::SETTING as i64)
    })
    .await;
    assert_eq!(setting["code"], json!(WsResponseCode::OK as i32));
    assert_eq!(
        setting["data"]["current_configs"]["deck_count"],
        json!(deck_setting)
    );
    send_request(&mut a, Routes::START as i32, json!({})).await;
    let started = recv_until(&mut a, "concurrent tractor room start", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::START as i64)
    })
    .await;
    assert_eq!(started["code"], json!(WsResponseCode::OK as i32));

    let mut clients: [&mut Client; 4] = [&mut a, &mut b, &mut c, &mut d];
    let mut hands = collect_tractor_hands(&mut clients, expected_hand_size).await;
    let (declaration, bottom_seen_by_first_client) = recv_first_declaration(&mut *clients[0]).await;
    let dealer_position = declaration["data"]["position"]
        .as_i64()
        .expect("concurrent tractor room dealer") as usize;
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
        .expect("concurrent tractor room bottom")
        .iter()
        .map(|card| card.as_i64().expect("concurrent tractor room bottom card") as i32)
        .collect::<Vec<_>>();
    assert_eq!(bottom_cards.len(), expected_bottom_size);
    send_request(
        &mut *clients[dealer_position],
        TractorRoutes::BURY_BOTTOM as i32,
        json!({ "cards": bottom_cards }),
    )
    .await;
    let play_snapshot = recv_until(
        &mut *clients[dealer_position],
        "concurrent tractor room play phase",
        |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
                && value["data"]["phase"] == json!(TractorPhase::Play as i8)
        },
    )
    .await;
    let buried = recv_until(
        &mut *clients[dealer_position],
        "concurrent tractor room bury response",
        |value| {
            value.get("route").and_then(Value::as_i64) == Some(TractorRoutes::BURY_BOTTOM as i64)
        },
    )
    .await;
    assert_eq!(buried["code"], json!(WsResponseCode::OK as i32));
    assert_eq!(play_snapshot["data"]["deck_count"], json!(deck_count));
    assert_eq!(
        play_snapshot["data"]["total_deal_count"],
        json!(expected_hand_size * 4)
    );
    let trump_suit = play_snapshot["data"]["trump_suit"]
        .as_i64()
        .map(|suit| match suit {
            0 => TractorSuit::SPADE,
            1 => TractorSuit::HEART,
            2 => TractorSuit::CLUB,
            3 => TractorSuit::DIAMOND,
            _ => panic!("invalid concurrent tractor trump suit"),
        });
    let rules = TractorRules {
        attacking_win_score: 80,
        score_per_level: 40,
        shutout_bonus_levels: 1,
        bottom_card_count: expected_bottom_size,
        deck_count,
        final_target_rank: TractorRank::A,
        target_rank: TractorRank::THREE,
        trump_suit,
    };
    let mut current_position = dealer_position;
    let mut lead_combo = None;
    let mut final_snapshot = None;
    for play_index in 0..4 {
        let hand = &hands[current_position];
        let cards = match lead_combo.as_ref() {
            None => vec![*hand.first().expect("concurrent tractor lead card")],
            Some(lead) => {
                combo::forced_follow(hand, lead, &rules).expect("concurrent tractor legal follow")
            }
        };
        send_request(
            &mut *clients[current_position],
            Routes::PLAY as i32,
            json!({ "cards": cards }),
        )
        .await;
        let played = recv_until(
            &mut *clients[current_position],
            "concurrent tractor room play",
            |value| {
                value.get("code").and_then(Value::as_i64) == Some(WsCode::PLAY as i64)
                    && value["data"]["position"] == json!(current_position)
            },
        )
        .await;
        assert_eq!(
            played["data"]["name"],
            json!(format!("{room}-player-{current_position}")),
            "play events must not cross room boundaries"
        );
        let played_card = played["data"]["cards"][0]
            .as_i64()
            .expect("concurrent tractor played card") as i32;
        let index = hands[current_position]
            .iter()
            .position(|candidate| *candidate == played_card)
            .expect("concurrent tractor played card in hand");
        hands[current_position].remove(index);
        let snapshot = recv_until(
            &mut *clients[current_position],
            "concurrent tractor room play snapshot",
            |value| {
                value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
                    && value["data"]["trick_index"] == json!(if play_index == 3 { 1 } else { 0 })
            },
        )
        .await;
        let response = recv_until(
            &mut *clients[current_position],
            "concurrent tractor room play response",
            |value| value.get("route").and_then(Value::as_i64) == Some(Routes::PLAY as i64),
        )
        .await;
        assert_eq!(response["code"], json!(WsResponseCode::OK as i32));
        if play_index == 3 {
            final_snapshot = Some(snapshot);
            break;
        }
        current_position = snapshot["data"]["current_position"]
            .as_i64()
            .expect("concurrent tractor next position") as usize;
        let lead_card = snapshot["data"]["current_trick"][0]["cards"][0]
            .as_i64()
            .expect("concurrent tractor trick lead") as i32;
        lead_combo = Some(
            combo::classify(&[lead_card], &rules).expect("concurrent tractor single lead combo"),
        );
    }
    final_snapshot.expect("concurrent tractor room must finish its first trick")
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

#[cfg(not(feature = "official"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tractor_server_accepts_only_its_own_game_id() {
    let runtime = start_test_runtime("tractor-game-id-test", Duration::from_secs(30)).await;
    let url = runtime.url.clone();

    let mut wrong_client = connect_client(&url).await;
    send_request(
        &mut wrong_client,
        Routes::JOIN as i32,
        json!({
            "name": "wrong-game",
            "password": "tractor-game-id-room",
            "game_id": GameId::UPGRADE as i32,
            "avatar_url": ""
        }),
    )
    .await;
    let wrong = recv_until(&mut wrong_client, "wrong tractor game id", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::JOIN as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::WRONG_GAME as i64)
    })
    .await;
    assert_eq!(wrong["code"], json!(WsResponseCode::WRONG_GAME as i32));

    let mut tractor_client = connect_client(&url).await;
    let accepted = join(
        &mut tractor_client,
        "tractor-player",
        "tractor-game-id-room",
    )
    .await;
    assert_eq!(accepted["code"], json!(WsResponseCode::JOINED as i32));
    assert_eq!(accepted["data"]["self_position"], json!(0));
    assert_eq!(accepted["data"]["current_configs"]["deck_count"], json!(0));
}

#[cfg(not(feature = "official"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tractor_ws_exposes_only_twenty_to_forty_second_play_time() {
    let runtime = start_default_test_runtime(
        "tractor-production-timing-setting-test",
        Duration::from_secs(45),
    )
    .await;
    let url = runtime.url.clone();

    let mut a = connect_client(&url).await;
    let mut b = connect_client(&url).await;
    let mut c = connect_client(&url).await;
    let mut d = connect_client(&url).await;
    let room = "tractor-production-timing-setting-room";
    let owner_join = join(&mut a, "timing-a", room).await;
    join(&mut b, "timing-b", room).await;
    join(&mut c, "timing-c", room).await;
    join(&mut d, "timing-d", room).await;

    assert_eq!(
        owner_join["data"]["current_configs"]["play_time"],
        json!(30)
    );
    let current_configs = owner_join["data"]["current_configs"]
        .as_object()
        .expect("production tractor current configs");
    let param_descriptions = owner_join["data"]["param_descriptions"]
        .as_object()
        .expect("production tractor parameter descriptions");
    for internal in [
        "first_deal_time",
        "deal_time",
        "ai_action_time",
        "away_time",
        "settlement_time",
    ] {
        assert!(
            !current_configs.contains_key(internal),
            "internal timing setting {internal} must not be exposed as a current config"
        );
        assert!(
            !param_descriptions.contains_key(internal),
            "internal timing setting {internal} must not be client-configurable"
        );
    }
    assert!(param_descriptions.contains_key("play_time"));

    for rejected in [19, 41] {
        send_request(
            &mut a,
            Routes::SETTING as i32,
            json!({ "current_configs": { "play_time": rejected } }),
        )
        .await;
        let response = recv_until(&mut a, "rejected production play time", |value| {
            value.get("route").and_then(Value::as_i64) == Some(Routes::SETTING as i64)
        })
        .await;
        assert_eq!(
            response["code"],
            json!(WsResponseCode::ERROR_FORMAT as i32),
            "production play time {rejected} must be rejected"
        );
    }

    send_request(
        &mut a,
        Routes::SETTING as i32,
        json!({ "current_configs": { "play_time": 20 } }),
    )
    .await;
    let lower = recv_until(&mut a, "minimum production play time", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::SETTING as i64)
    })
    .await;
    assert_eq!(lower["code"], json!(WsResponseCode::OK as i32));
    assert_eq!(lower["data"]["current_configs"]["play_time"], json!(20));

    send_request(
        &mut a,
        Routes::SETTING as i32,
        json!({ "current_configs": { "play_time": 40 } }),
    )
    .await;
    let upper = recv_until(&mut a, "maximum production play time", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::SETTING as i64)
    })
    .await;
    assert_eq!(upper["code"], json!(WsResponseCode::OK as i32));
    assert_eq!(upper["data"]["current_configs"]["play_time"], json!(40));

    send_request(
        &mut a,
        Routes::SETTING as i32,
        json!({ "current_configs": { "first_deal_time": 15_000 } }),
    )
    .await;
    let internal = recv_until(&mut a, "internal production timing rejection", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::SETTING as i64)
    })
    .await;
    assert_eq!(internal["code"], json!(WsResponseCode::ERROR_FORMAT as i32));

    send_request(&mut a, Routes::START as i32, json!({})).await;
    let started = recv_until(&mut a, "production timing start", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::START as i64)
    })
    .await;
    assert_eq!(started["code"], json!(WsResponseCode::OK as i32));

    let mut clients: [&mut Client; 4] = [&mut a, &mut b, &mut c, &mut d];
    let hands = collect_tractor_hands(&mut clients, 25).await;
    assert!(hands.iter().all(|hand| hand.len() == 25));
    let bury_snapshot = recv_until(&mut *clients[0], "production bottom window", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
            && value["data"]["phase"] == json!(TractorPhase::Bury as i8)
    })
    .await;
    assert_eq!(
        bury_snapshot["data"]["turn_countdown"],
        json!(120),
        "the shared select/bury window must be three times the forty-second play time"
    );
}

#[cfg(not(feature = "official"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tractor_ws_canonicalizes_bottom_count_before_broadcasting_settings() {
    let runtime = start_default_test_runtime(
        "tractor-bottom-setting-canonicalization-test",
        Duration::from_secs(30),
    )
    .await;
    let url = runtime.url.clone();

    let mut a = connect_client(&url).await;
    let mut b = connect_client(&url).await;
    let mut c = connect_client(&url).await;
    let mut d = connect_client(&url).await;
    let room = "tractor-bottom-setting-canonicalization-room";
    join(&mut a, "canonical-a", room).await;
    join(&mut b, "canonical-b", room).await;
    join(&mut c, "canonical-c", room).await;
    join(&mut d, "canonical-d", room).await;

    send_request(
        &mut a,
        Routes::SETTING as i32,
        json!({
            "current_configs": {
                "deck_count": 0,
                "bottom_card_count": 9
            }
        }),
    )
    .await;
    let response = recv_until(&mut a, "canonical two-deck setting response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::SETTING as i64)
    })
    .await;
    assert_eq!(response["code"], json!(WsResponseCode::OK as i32));
    assert_eq!(response["data"]["current_configs"]["deck_count"], json!(0));
    assert_eq!(
        response["data"]["current_configs"]["bottom_card_count"],
        json!(12),
        "2-deck bottom count 9 must be canonicalized to an equal-hand value"
    );
    let broadcast = recv_until(&mut b, "canonical two-deck setting broadcast", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::SETTING as i64)
    })
    .await;
    assert_eq!(
        broadcast["data"]["current_configs"]["bottom_card_count"],
        json!(12)
    );

    send_request(
        &mut a,
        Routes::SETTING as i32,
        json!({
            "current_configs": {
                "deck_count": 1,
                "bottom_card_count": 11
            }
        }),
    )
    .await;
    let response = recv_until(&mut a, "canonical three-deck setting response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::SETTING as i64)
    })
    .await;
    assert_eq!(response["code"], json!(WsResponseCode::OK as i32));
    assert_eq!(response["data"]["current_configs"]["deck_count"], json!(1));
    assert_eq!(
        response["data"]["current_configs"]["bottom_card_count"],
        json!(14),
        "3-deck bottom count 11 must be canonicalized to an equal-hand value"
    );
    let broadcast = recv_until(&mut b, "canonical three-deck setting broadcast", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::SETTING as i64)
    })
    .await;
    assert_eq!(
        broadcast["data"]["current_configs"]["bottom_card_count"],
        json!(14)
    );
}

#[cfg(not(feature = "official"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tractor_ws_keeps_concurrent_rooms_isolated() {
    let runtime =
        start_test_runtime("tractor-concurrent-rooms-test", Duration::from_secs(60)).await;
    let (two_deck, three_deck) = tokio::join!(
        run_concurrent_tractor_room(&runtime.url, "tractor-room-two", 0, 2, 25, 8),
        run_concurrent_tractor_room(&runtime.url, "tractor-room-three", 1, 3, 38, 10),
    );
    assert_eq!(two_deck["data"]["deck_count"], json!(2));
    assert_eq!(two_deck["data"]["trick_index"], json!(1));
    assert_eq!(three_deck["data"]["deck_count"], json!(3));
    assert_eq!(three_deck["data"]["trick_index"], json!(1));
}

#[cfg(not(feature = "official"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tractor_ws_accepts_a_level_three_declaration_during_first_deal() {
    let runtime =
        start_test_runtime("tractor-first-declaration-test", Duration::from_secs(45)).await;
    let url = runtime.url.clone();

    let mut a = connect_client(&url).await;
    let mut b = connect_client(&url).await;
    let mut c = connect_client(&url).await;
    let mut d = connect_client(&url).await;
    let room = "tractor-first-declaration-room";
    for (position, client) in [&mut a, &mut b, &mut c, &mut d].into_iter().enumerate() {
        let joined = join(client, &format!("declaration-player-{position}"), room).await;
        assert_eq!(joined["code"], json!(WsResponseCode::JOINED as i32));
        assert_eq!(joined["data"]["self_position"], json!(position));
    }

    send_request(
        &mut a,
        Routes::SETTING as i32,
        json!({
            "current_configs": {
                "deck_count": 1,
                "attacking_win_score": 120,
                "score_per_level": 60,
                "shutout_bonus_levels": 1,
                "target_rank": 11,
                "first_deal_time": 15_000,
                "deal_time": 3_000,
                "play_time": 30
            }
        }),
    )
    .await;
    recv_until(&mut a, "tractor declaration setting response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::SETTING as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;
    send_request(&mut a, Routes::START as i32, json!({})).await;
    recv_until(&mut a, "tractor declaration start response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::START as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;

    let clients: [&mut Client; 4] = [&mut a, &mut b, &mut c, &mut d];
    let mut dealt_counts = [0_usize; 4];
    let mut declared = None;
    for _ in 0..38 {
        for position in 0..4 {
            let card = recv_tractor_private_deal(&mut *clients[position], position).await;
            dealt_counts[position] += 1;
            let decoded = Card::try_from(card).expect("tractor declaration candidate");
            if decoded.rank() != Rank::Three || decoded.suit().is_none() {
                continue;
            }

            send_request(
                &mut *clients[position],
                TractorRoutes::DECLARE_TRUMP as i32,
                json!({ "cards": [card] }),
            )
            .await;
            let observer_position = (position + 1) % 4;
            let declaration = recv_until(
                &mut *clients[observer_position],
                "tractor player declaration event",
                |value| {
                    value.get("code").and_then(Value::as_i64)
                        == Some(TractorWsCode::TRUMP_DECLARED as i64)
                },
            )
            .await;
            let expected_suit = match decoded.suit().expect("suited level card") {
                upgrade_common::Suit::Spade => TractorSuit::SPADE,
                upgrade_common::Suit::Heart => TractorSuit::HEART,
                upgrade_common::Suit::Club => TractorSuit::CLUB,
                upgrade_common::Suit::Diamond => TractorSuit::DIAMOND,
            };
            assert_eq!(declaration["data"]["position"], json!(position));
            assert_eq!(declaration["data"]["cards"], json!([card]));
            assert_eq!(declaration["data"]["strength"], json!(1));
            assert_eq!(
                declaration["data"]["target_rank"],
                json!(TractorRank::THREE as i8)
            );
            assert_eq!(
                declaration["data"]["trump_suit"],
                json!(expected_suit as i8)
            );
            recv_until(
                &mut *clients[position],
                "tractor player declaration response",
                |value| {
                    value.get("route").and_then(Value::as_i64)
                        == Some(TractorRoutes::DECLARE_TRUMP as i64)
                        && value.get("code").and_then(Value::as_i64)
                            == Some(WsResponseCode::OK as i64)
                },
            )
            .await;
            declared = Some((position, card));
            break;
        }
        if declared.is_some() {
            break;
        }
    }
    let (declaring_position, declared_card) = declared
        .expect("a three-deck tractor deal must expose a suited level three outside the bottom");
    let declared_suit = match Card::try_from(declared_card)
        .expect("declared tractor card")
        .suit()
        .expect("declared tractor card suit")
    {
        upgrade_common::Suit::Spade => TractorSuit::SPADE,
        upgrade_common::Suit::Heart => TractorSuit::HEART,
        upgrade_common::Suit::Club => TractorSuit::CLUB,
        upgrade_common::Suit::Diamond => TractorSuit::DIAMOND,
    };

    let mut rejected_equal_declaration = None;
    'deal: while dealt_counts.iter().any(|count| *count < 38) {
        for position in 0..4 {
            if dealt_counts[position] >= 38 {
                continue;
            }
            let card = recv_tractor_private_deal(&mut *clients[position], position).await;
            dealt_counts[position] += 1;
            let decoded = Card::try_from(card).expect("tractor counter declaration candidate");
            if decoded.rank() != Rank::Three || decoded.suit().is_none() {
                continue;
            }

            send_request(
                &mut *clients[position],
                TractorRoutes::DECLARE_TRUMP as i32,
                json!({ "cards": [card] }),
            )
            .await;
            recv_until(
                &mut *clients[position],
                "tractor equal declaration rejection",
                |value| {
                    value.get("route").and_then(Value::as_i64)
                        == Some(TractorRoutes::DECLARE_TRUMP as i64)
                        && value.get("code").and_then(Value::as_i64)
                            == Some(WsResponseCode::NO_PERMISSION as i64)
                },
            )
            .await;
            let snapshot = recv_until(
                &mut *clients[position],
                "tractor retained declaration snapshot",
                |value| {
                    value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
                },
            )
            .await;
            assert_eq!(
                snapshot["data"]["declaration"]["position"],
                json!(declaring_position)
            );
            assert_eq!(snapshot["data"]["declaration"]["strength"], json!(1));
            assert_eq!(
                snapshot["data"]["declaration"]["trump_suit"],
                json!(declared_suit as i8)
            );
            rejected_equal_declaration = Some((position, card));
            break 'deal;
        }
    }
    assert!(
        rejected_equal_declaration.is_some(),
        "another dealt level three must not replace an equal-strength tractor declaration"
    );
}

#[cfg(not(feature = "official"))]
async fn try_tractor_pair_counter_declaration(attempt: usize) -> bool {
    let runtime = start_test_runtime(
        "tractor-pair-counter-declaration-test",
        Duration::from_secs(45),
    )
    .await;
    let url = runtime.url.clone();

    let mut a = connect_client(&url).await;
    let mut b = connect_client(&url).await;
    let mut c = connect_client(&url).await;
    let mut d = connect_client(&url).await;
    let room = format!("tractor-pair-counter-declaration-room-{attempt}");
    for (position, client) in [&mut a, &mut b, &mut c, &mut d].into_iter().enumerate() {
        let joined = join(
            client,
            &format!("pair-declaration-player-{attempt}-{position}"),
            &room,
        )
        .await;
        assert_eq!(joined["code"], json!(WsResponseCode::JOINED as i32));
        assert_eq!(joined["data"]["self_position"], json!(position));
    }

    send_request(
        &mut a,
        Routes::SETTING as i32,
        json!({
            "current_configs": {
                "deck_count": 1,
                "attacking_win_score": 120,
                "score_per_level": 60,
                "shutout_bonus_levels": 1,
                "target_rank": 11,
                "first_deal_time": 15_000,
                "deal_time": 3_000,
                "play_time": 30
            }
        }),
    )
    .await;
    recv_until(
        &mut a,
        "tractor pair declaration setting response",
        |value| {
            value.get("route").and_then(Value::as_i64) == Some(Routes::SETTING as i64)
                && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
        },
    )
    .await;
    send_request(&mut a, Routes::START as i32, json!({})).await;
    recv_until(&mut a, "tractor pair declaration start response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::START as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;

    let clients: [&mut Client; 4] = [&mut a, &mut b, &mut c, &mut d];
    let mut level_cards: [HashMap<u8, Vec<i32>>; 4] = std::array::from_fn(|_| HashMap::new());
    let mut declaring_position = None;
    for _ in 0..38 {
        for position in 0..4 {
            let card = recv_tractor_private_deal(&mut *clients[position], position).await;
            let decoded = Card::try_from(card).expect("tractor pair declaration candidate");
            if decoded.rank() != Rank::Three || decoded.suit().is_none() {
                continue;
            }
            let copies = level_cards[position].entry(decoded.identity()).or_default();
            copies.push(card);

            if declaring_position.is_none() {
                send_request(
                    &mut *clients[position],
                    TractorRoutes::DECLARE_TRUMP as i32,
                    json!({ "cards": [card] }),
                )
                .await;
                let response = recv_until(
                    &mut *clients[position],
                    "tractor initial declaration response before pair counter",
                    |value| {
                        value.get("route").and_then(Value::as_i64)
                            == Some(TractorRoutes::DECLARE_TRUMP as i64)
                    },
                )
                .await;
                if response["code"] != json!(WsResponseCode::OK as i32) {
                    return false;
                }
                let observer_position = (position + 1) % 4;
                let declaration = recv_until(
                    &mut *clients[observer_position],
                    "tractor initial declaration before pair counter",
                    |value| {
                        value.get("code").and_then(Value::as_i64)
                            == Some(TractorWsCode::TRUMP_DECLARED as i64)
                            && value["data"]["position"] == json!(position)
                            && value["data"]["strength"] == json!(1)
                    },
                )
                .await;
                assert_eq!(declaration["data"]["cards"], json!([card]));
                assert_eq!(
                    declaration["data"]["target_rank"],
                    json!(TractorRank::THREE as i8)
                );
                declaring_position = Some(position);
                continue;
            }

            if declaring_position == Some(position) || copies.len() < 2 {
                continue;
            }
            let stronger_cards = copies[..2].to_vec();
            send_request(
                &mut *clients[position],
                TractorRoutes::DECLARE_TRUMP as i32,
                json!({ "cards": stronger_cards }),
            )
            .await;
            let response = recv_until(
                &mut *clients[position],
                "tractor pair counter declaration response",
                |value| {
                    value.get("route").and_then(Value::as_i64)
                        == Some(TractorRoutes::DECLARE_TRUMP as i64)
                },
            )
            .await;
            if response["code"] != json!(WsResponseCode::OK as i32) {
                return false;
            }
            let observer_position = (position + 1) % 4;
            let declaration = recv_until(
                &mut *clients[observer_position],
                "tractor pair counter declaration event",
                |value| {
                    value.get("code").and_then(Value::as_i64)
                        == Some(TractorWsCode::TRUMP_DECLARED as i64)
                        && value["data"]["position"] == json!(position)
                        && value["data"]["strength"] == json!(2)
                },
            )
            .await;
            assert_eq!(declaration["data"]["cards"], json!(stronger_cards));
            assert_eq!(
                declaration["data"]["target_rank"],
                json!(TractorRank::THREE as i8)
            );
            let stronger_suit = match decoded.suit().expect("paired suited level card") {
                upgrade_common::Suit::Spade => TractorSuit::SPADE,
                upgrade_common::Suit::Heart => TractorSuit::HEART,
                upgrade_common::Suit::Club => TractorSuit::CLUB,
                upgrade_common::Suit::Diamond => TractorSuit::DIAMOND,
            };
            assert_eq!(
                declaration["data"]["trump_suit"],
                json!(stronger_suit as i8)
            );
            return true;
        }
    }

    false
}

#[cfg(not(feature = "official"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tractor_ws_rejects_declaration_while_paused_and_accepts_it_after_resume() {
    let runtime =
        start_test_runtime("tractor-paused-declaration-test", Duration::from_secs(45)).await;
    let url = runtime.url.clone();

    let mut a = connect_client(&url).await;
    let mut b = connect_client(&url).await;
    let mut c = connect_client(&url).await;
    let mut d = connect_client(&url).await;
    let room = "tractor-paused-declaration-room";
    for (position, client) in [&mut a, &mut b, &mut c, &mut d].into_iter().enumerate() {
        let joined = join(client, &format!("paused-player-{position}"), room).await;
        assert_eq!(joined["data"]["self_position"], json!(position));
    }

    send_request(
        &mut a,
        Routes::SETTING as i32,
        json!({
            "current_configs": {
                "deck_count": 1,
                "first_deal_time": 5000,
                "deal_time": 1000,
                "play_time": 30
            }
        }),
    )
    .await;
    recv_until(&mut a, "paused declaration setting response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::SETTING as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;
    send_request(&mut a, Routes::START as i32, json!({})).await;
    recv_until(&mut a, "paused declaration start response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::START as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;

    let clients: [&mut Client; 4] = [&mut a, &mut b, &mut c, &mut d];
    let mut declaration = None;
    'deal: for _ in 0..38 {
        for position in 0..4 {
            let card = recv_tractor_private_deal(&mut *clients[position], position).await;
            let decoded = Card::try_from(card).expect("paused declaration candidate");
            if decoded.rank() == Rank::Three && decoded.suit().is_some() {
                declaration = Some((position, card));
                break 'deal;
            }
        }
    }
    let (declaring_position, declared_card) =
        declaration.expect("three-deck deal must expose a suited level three");

    send_request(&mut *clients[0], Routes::PAUSE as i32, json!({})).await;
    recv_until(
        &mut *clients[0],
        "pause before declaration response",
        |value| {
            value.get("route").and_then(Value::as_i64) == Some(Routes::PAUSE as i64)
                && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
        },
    )
    .await;

    send_request(
        &mut *clients[declaring_position],
        TractorRoutes::DECLARE_TRUMP as i32,
        json!({ "cards": [declared_card] }),
    )
    .await;
    let rejected = recv_until(
        &mut *clients[declaring_position],
        "declaration rejected while paused",
        |value| {
            value.get("route").and_then(Value::as_i64) == Some(TractorRoutes::DECLARE_TRUMP as i64)
        },
    )
    .await;
    assert_eq!(
        rejected["code"],
        json!(WsResponseCode::NO_PERMISSION as i32)
    );

    send_request(&mut *clients[0], Routes::RESUME as i32, json!({})).await;
    recv_until(
        &mut *clients[0],
        "resume before declaration response",
        |value| {
            value.get("route").and_then(Value::as_i64) == Some(Routes::RESUME as i64)
                && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
        },
    )
    .await;
    send_request(
        &mut *clients[declaring_position],
        TractorRoutes::DECLARE_TRUMP as i32,
        json!({ "cards": [declared_card] }),
    )
    .await;
    let accepted = recv_until(
        &mut *clients[declaring_position],
        "declaration accepted after resume",
        |value| {
            value.get("route").and_then(Value::as_i64) == Some(TractorRoutes::DECLARE_TRUMP as i64)
        },
    )
    .await;
    assert_eq!(accepted["code"], json!(WsResponseCode::OK as i32));
}

#[cfg(not(feature = "official"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tractor_ws_pair_of_level_threes_overrides_a_single_declaration() {
    for attempt in 0..5 {
        if try_tractor_pair_counter_declaration(attempt).await {
            return;
        }
    }
    panic!("five independent three-deck deals exposed no opponent pair of suited level threes");
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
    let runtime = start_test_runtime("tractor-test", Duration::from_secs(30)).await;
    let url = runtime.url.clone();

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

    let (a_deal, b_deal, c_deal, d_deal) = tokio::join!(
        observe_first_tractor_deal(&mut a, 0, 25),
        observe_first_tractor_deal(&mut b, 1, 25),
        observe_first_tractor_deal(&mut c, 2, 25),
        observe_first_tractor_deal(&mut d, 3, 25),
    );
    let observations = [a_deal, b_deal, c_deal, d_deal];
    let dealer_position = observations[0].dealer_position;
    assert!(dealer_position < observations.len());
    assert!(
        observations
            .iter()
            .all(|observation| observation.dealer_position == dealer_position),
        "all clients must observe the same first dealer"
    );
    let declaration = &observations[0].declaration;
    assert_eq!(
        declaration["data"]["target_rank"],
        json!(TractorRank::THREE as i8)
    );
    let declaration_cards = declaration["data"]["cards"]
        .as_array()
        .expect("first-round declaration cards");
    assert!(!declaration_cards.is_empty());
    assert!(declaration_cards.iter().all(|card| {
        Card::try_from(card.as_i64().expect("declaration card") as i32)
            .is_ok_and(|card| card.rank() == Rank::Three)
    }));
    let bottom = observations[dealer_position]
        .bottom
        .clone()
        .expect("dealer bottom event");
    let mut hands = observations.map(|observation| observation.hand);
    assert!(started_at.elapsed() >= Duration::from_millis(1_100));
    assert_eq!(hands[0].len(), 25);
    let bottom_cards = bottom["data"]["cards"]
        .as_array()
        .expect("bottom cards")
        .iter()
        .map(|card| card.as_i64().expect("bottom card") as i32)
        .collect::<Vec<_>>();
    assert_eq!(bottom_cards.len(), 8);
    assert_eq!(bottom["data"]["required_count"], json!(8));

    let clients: [&mut Client; 4] = [&mut a, &mut b, &mut c, &mut d];

    let dealer = &mut *clients[dealer_position];
    send_request(
        dealer,
        TractorRoutes::SELECT_TRUMP as i32,
        json!({ "trump_suit": TractorSuit::SPADE as i8 }),
    )
    .await;
    let first_round_select = recv_until(dealer, "first-round select trump response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(TractorRoutes::SELECT_TRUMP as i64)
    })
    .await;
    assert_eq!(
        first_round_select["code"],
        json!(WsResponseCode::NO_PERMISSION as i32)
    );
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
}

#[cfg(not(feature = "official"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tractor_ws_rejoin_during_first_deal_restores_the_complete_private_hand() {
    let runtime = start_test_runtime("tractor-rejoin-deal-test", Duration::from_secs(30)).await;
    let url = runtime.url.clone();

    let mut a = connect_client(&url).await;
    let mut b = connect_client(&url).await;
    let mut c = connect_client(&url).await;
    let mut d = connect_client(&url).await;
    let room = "tractor-rejoin-deal-room";
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
                "first_deal_time": 3000,
                "deal_time": 500,
                "play_time": 30
            }
        }),
    )
    .await;
    recv_until(&mut a, "deal rejoin setting response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::SETTING as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;
    send_request(&mut a, Routes::START as i32, json!({})).await;
    recv_until(&mut a, "deal rejoin start response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::START as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;

    let mut hand_before_disconnect = Vec::new();
    while hand_before_disconnect.len() < 3 {
        hand_before_disconnect.push(recv_tractor_private_deal(&mut a, 0).await);
    }
    a.close(None).await.expect("close player during first deal");

    // Leave enough time for several cards belonging to this seat to be dealt
    // while its old WebSocket is absent. Rejoining must recover those cards
    // from the authoritative game state instead of restarting the deal.
    tokio::time::sleep(Duration::from_millis(400)).await;

    let mut rejoined = connect_client(&url).await;
    let joined = join(&mut rejoined, "a", room).await;
    assert_eq!(joined["data"]["self_position"], json!(0));

    let hand_update = recv_until(&mut rejoined, "rejoined deal hand", |value| {
        value.get("code").and_then(Value::as_i64) == Some(TractorWsCode::HAND_UPDATED as i64)
    })
    .await;
    let mut restored_hand = hand_update["data"]["cards"]
        .as_array()
        .expect("rejoined deal hand cards")
        .iter()
        .map(|card| card.as_i64().expect("rejoined deal card") as i32)
        .collect::<Vec<_>>();
    assert!(
        restored_hand.len() > hand_before_disconnect.len(),
        "rejoin must include cards dealt while the socket was absent"
    );
    assert!(
        restored_hand.len() < 25,
        "the test must reconnect before the first deal finishes"
    );
    assert!(
        hand_before_disconnect
            .iter()
            .all(|card| restored_hand.contains(card)),
        "the restored hand must retain every card observed before disconnect"
    );

    let mut continued_deal_count = 0;
    let mut rejoined_bottom = None;
    let bury_snapshot = loop {
        let value = recv_json(&mut rejoined, "continued first deal after rejoin").await;
        match value.get("code").and_then(Value::as_i64) {
            Some(code) if code == WsCode::DEAL as i64 => {
                assert_eq!(value["data"]["position"], json!(0));
                let cards = value["data"]["cards"]
                    .as_array()
                    .expect("continued private deal cards");
                assert_eq!(cards.len(), 1, "continued deal must remain incremental");
                let card = cards[0].as_i64().expect("continued private card") as i32;
                assert!(
                    !restored_hand.contains(&card),
                    "a recovered card must not be dealt to the rejoined client twice"
                );
                restored_hand.push(card);
                continued_deal_count += 1;
            }
            Some(code) if code == TractorWsCode::BOTTOM_CARDS as i64 => {
                rejoined_bottom = Some(value);
            }
            Some(code)
                if code == WsCode::TABLE_SNAPSHOT as i64
                    && value["data"]["phase"] == json!(TractorPhase::Bury as i8) =>
            {
                break value;
            }
            _ => {}
        }
    };

    assert!(
        continued_deal_count > 0,
        "rejoined client must receive later deal events"
    );
    assert_eq!(
        restored_hand.len(),
        25,
        "two-deck player hand must finish at 25 cards"
    );
    let unique_cards = restored_hand
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(
        unique_cards.len(),
        restored_hand.len(),
        "rejoin must neither duplicate nor lose physical cards"
    );
    assert_eq!(bury_snapshot["data"]["round_index"], json!(0));
    assert_eq!(bury_snapshot["data"]["dealt_count"], json!(100));
    assert_eq!(bury_snapshot["data"]["total_deal_count"], json!(100));
    assert!(
        !bury_snapshot["data"]["declaration"].is_null(),
        "first deal must still complete trump declaration before bury"
    );

    let dealer_position = bury_snapshot["data"]["dealer_position"]
        .as_u64()
        .expect("dealer after rejoined first deal") as usize;
    let dealer = match dealer_position {
        0 => &mut rejoined,
        1 => &mut b,
        2 => &mut c,
        3 => &mut d,
        _ => panic!("invalid dealer position {dealer_position}"),
    };
    let bottom = if dealer_position == 0 {
        match rejoined_bottom {
            Some(bottom) => bottom,
            None => recv_tractor_bottom(dealer, dealer_position).await,
        }
    } else {
        recv_tractor_bottom(dealer, dealer_position).await
    };
    let bottom_cards = bottom["data"]["cards"]
        .as_array()
        .expect("bottom cards after rejoined first deal")
        .iter()
        .map(|card| card.as_i64().expect("bottom card after deal rejoin") as i32)
        .collect::<Vec<_>>();
    assert_eq!(bottom_cards.len(), 8);

    send_request(
        dealer,
        TractorRoutes::BURY_BOTTOM as i32,
        json!({ "cards": bottom_cards }),
    )
    .await;
    recv_until(dealer, "bury after rejoined first deal", |value| {
        value.get("code").and_then(Value::as_i64) == Some(TractorWsCode::BOTTOM_BURIED as i64)
            && value["data"]["position"] == json!(dealer_position)
    })
    .await;
    recv_until(dealer, "play after rejoined first deal", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
            && value["data"]["phase"] == json!(TractorPhase::Play as i8)
    })
    .await;
}

#[cfg(not(feature = "official"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tractor_disconnected_first_dealer_keeps_the_full_bottom_window_for_rejoin() {
    let runtime = start_test_runtime(
        "tractor-disconnected-dealer-window-test",
        Duration::from_secs(45),
    )
    .await;
    let url = runtime.url.clone();

    let mut a = connect_client(&url).await;
    let mut b = connect_client(&url).await;
    let mut c = connect_client(&url).await;
    let mut d = connect_client(&url).await;
    let room = "tractor-disconnected-dealer-window-room";
    for (position, client) in [&mut a, &mut b, &mut c, &mut d].into_iter().enumerate() {
        let joined = join(client, &format!("bottom-window-player-{position}"), room).await;
        assert_eq!(joined["data"]["self_position"], json!(position));
    }

    send_request(
        &mut a,
        Routes::SETTING as i32,
        json!({
            "current_configs": {
                "deck_count": 1,
                "target_rank": 11,
                "first_deal_time": 5000,
                "deal_time": 1000,
                "play_time": 30,
                "away_time": 4
            }
        }),
    )
    .await;
    recv_until(&mut a, "disconnected dealer setting response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::SETTING as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;
    send_request(&mut a, Routes::START as i32, json!({})).await;
    recv_until(&mut a, "disconnected dealer start response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::START as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;

    let clients: [&mut Client; 4] = [&mut a, &mut b, &mut c, &mut d];
    let mut declaring = None;
    'deal: for _ in 0..38 {
        for position in 0..4 {
            let card = recv_tractor_private_deal(&mut *clients[position], position).await;
            let decoded = Card::try_from(card).expect("disconnected dealer declaration card");
            if decoded.rank() != Rank::Three || decoded.suit().is_none() {
                continue;
            }
            send_request(
                &mut *clients[position],
                TractorRoutes::DECLARE_TRUMP as i32,
                json!({ "cards": [card] }),
            )
            .await;
            let observer_position = (position + 1) % 4;
            let declaration = recv_until(
                &mut *clients[observer_position],
                "declaration before dealer disconnect",
                |value| {
                    value.get("code").and_then(Value::as_i64)
                        == Some(TractorWsCode::TRUMP_DECLARED as i64)
                        && value["data"]["position"] == json!(position)
                },
            )
            .await;
            assert_eq!(
                declaration["data"]["target_rank"],
                json!(TractorRank::THREE as i8)
            );
            declaring = Some((position, observer_position));
            break 'deal;
        }
    }
    let (dealer_position, observer_position) =
        declaring.expect("three-deck deal must expose a level three before finishing");
    clients[dealer_position]
        .close(None)
        .await
        .expect("disconnect first dealer during deal");

    let bury_snapshot = recv_until(
        &mut *clients[observer_position],
        "full bottom window for disconnected first dealer",
        |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
                && value["data"]["phase"] == json!(TractorPhase::Bury as i8)
        },
    )
    .await;
    assert_eq!(
        bury_snapshot["data"]["dealer_position"],
        json!(dealer_position)
    );
    assert_eq!(
        bury_snapshot["data"]["turn_countdown"],
        json!(90),
        "a disconnected human dealer must keep the same three-times bottom window"
    );

    let mut rejoined = connect_client(&url).await;
    let player_names = [
        "bottom-window-player-0",
        "bottom-window-player-1",
        "bottom-window-player-2",
        "bottom-window-player-3",
    ];
    let joined = join(&mut rejoined, player_names[dealer_position], room).await;
    assert_eq!(joined["data"]["self_position"], json!(dealer_position));
    let hand_update = recv_until(&mut rejoined, "rejoined dealer bottom hand", |value| {
        value.get("code").and_then(Value::as_i64) == Some(TractorWsCode::HAND_UPDATED as i64)
    })
    .await;
    let restored_hand = hand_update["data"]["cards"]
        .as_array()
        .expect("rejoined dealer bottom hand cards")
        .clone();
    assert_eq!(
        restored_hand.len(),
        48,
        "three-deck dealer receives 38 + 10 cards"
    );
    let rejoined_snapshot = recv_until(&mut rejoined, "rejoined dealer bottom snapshot", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
    })
    .await;
    assert_eq!(
        rejoined_snapshot["data"]["phase"],
        json!(TractorPhase::Bury as i8)
    );
    assert!(
        rejoined_snapshot["data"]["turn_countdown"]
            .as_i64()
            .is_some_and(|countdown| (80..=90).contains(&countdown)),
        "rejoin must resume the remaining bottom window: {rejoined_snapshot}"
    );

    let bottom_cards = restored_hand.into_iter().take(10).collect::<Vec<_>>();
    send_request(
        &mut rejoined,
        TractorRoutes::BURY_BOTTOM as i32,
        json!({ "cards": bottom_cards }),
    )
    .await;
    recv_until(&mut rejoined, "rejoined dealer completes bottom", |value| {
        value.get("code").and_then(Value::as_i64) == Some(TractorWsCode::BOTTOM_BURIED as i64)
            && value["data"]["position"] == json!(dealer_position)
    })
    .await;
    recv_until(&mut rejoined, "rejoined dealer enters play", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
            && value["data"]["phase"] == json!(TractorPhase::Play as i8)
    })
    .await;
}

#[cfg(not(feature = "official"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tractor_ws_rejoin_preserves_running_bury_state() {
    let runtime = start_test_runtime("tractor-rejoin-bury-test", Duration::from_secs(30)).await;
    let url = runtime.url.clone();

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
    clients[dealer_position]
        .close(None)
        .await
        .expect("close dealer socket");
    tokio::time::sleep(Duration::from_millis(1_200)).await;

    let mut rejoined = connect_client(&url).await;
    let player_names = ["a", "b", "c", "d"];
    let joined = join(&mut rejoined, player_names[dealer_position], room).await;
    assert_eq!(joined["data"]["self_position"], json!(dealer_position));
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
    let mut expected_hand = hands[dealer_position].clone();
    expected_hand.extend(bottom_cards.iter().copied());
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
    assert!(
        snapshot["data"]["turn_countdown"]
            .as_i64()
            .is_some_and(|countdown| (1..=89).contains(&countdown)),
        "rejoin must preserve the elapsed bury countdown: {snapshot}"
    );

    send_request(
        &mut rejoined,
        TractorRoutes::BURY_BOTTOM as i32,
        json!({ "cards": bottom_cards }),
    )
    .await;
    let buried = recv_until(&mut rejoined, "rejoined bury event", |value| {
        value.get("code").and_then(Value::as_i64) == Some(TractorWsCode::BOTTOM_BURIED as i64)
            && value["data"]["position"] == json!(dealer_position)
    })
    .await;
    assert_eq!(buried["data"]["position"], json!(dealer_position));
    let bury_response = recv_until(&mut rejoined, "rejoined bury response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(TractorRoutes::BURY_BOTTOM as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;
    assert_eq!(bury_response["code"], json!(WsResponseCode::OK as i32));
}

#[cfg(not(feature = "official"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tractor_server_completes_round_and_enters_later_round() {
    let runtime = start_test_runtime("tractor-full-round-test", Duration::from_secs(60)).await;
    let url = runtime.url.clone();

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
    let mut current_trick = Vec::<WsTractorPlayedCards>::new();
    let mut collected_scores = [0_i32; 4];
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
        current_trick
            .push(serde_json::from_value(played["data"].clone()).expect("played event payload"));
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
            assert_eq!(current_trick.len(), 4);
            let trick_winner = combo::trick_winner(&current_trick, &rules).expect("trick winner");
            collected_scores[trick_winner] += combo::trick_points(&current_trick);
            let winning_cards = current_trick
                .iter()
                .find(|entry| entry.position == trick_winner as i32)
                .expect("winning play")
                .cards
                .clone();
            let bottom_points = combo::trick_points(&[WsTractorPlayedCards {
                position: first_dealer as i32,
                name: String::new(),
                cards: first_bottom.clone(),
            }]);
            collected_scores[trick_winner] +=
                bottom_points * combo::bottom_multiplier(&winning_cards, &rules);
            let expected_score = [(first_dealer + 1) % 4, (first_dealer + 3) % 4]
                .into_iter()
                .map(|position| collected_scores[position])
                .sum::<i32>();
            let game_over = recv_until(
                &mut *clients[current_position],
                "tractor game over",
                |value| value.get("code").and_then(Value::as_i64) == Some(WsCode::GAME_OVER as i64),
            )
            .await;
            let settlement: WsTractorSettlementEvent =
                serde_json::from_value(game_over["data"].clone()).expect("tractor settlement");
            assert_eq!(settlement.score, expected_score);
            let expected_winners = if expected_score >= rules.attacking_win_score {
                vec![(first_dealer + 1) as i32 % 4, (first_dealer + 3) as i32 % 4]
            } else {
                vec![first_dealer as i32, (first_dealer + 2) as i32 % 4]
            };
            assert_eq!(settlement.winner_positions, expected_winners);
            let expected_levels = rules.score_progression().outcome(expected_score).levels as i32;
            assert_eq!(settlement.level_change, expected_levels);
            for position in 0..4 {
                let expected_player_score = if expected_winners.contains(&position) {
                    expected_score
                } else {
                    -expected_score
                };
                assert_eq!(
                    settlement.player_scores.get(&position).copied(),
                    Some(expected_player_score)
                );
            }
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
        if current_trick.len() == 4 {
            let trick_winner = combo::trick_winner(&current_trick, &rules).expect("trick winner");
            collected_scores[trick_winner] += combo::trick_points(&current_trick);
            current_trick.clear();
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
    // 让共享窗口先消耗一点时间；若选主重置窗口，下面的快照会错误地回到 90 秒。
    tokio::time::sleep(Duration::from_secs(2)).await;
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
    // 后续局选主和埋底共用一个窗口；选主不能把三倍出牌倒计时重置。
    assert!(
        selected_snapshot["data"]["turn_countdown"]
            .as_i64()
            .is_some_and(|countdown| countdown < 90)
    );
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

    let selected_countdown = selected_snapshot["data"]["turn_countdown"]
        .as_i64()
        .expect("later selected bottom countdown");
    clients[later_dealer]
        .close(None)
        .await
        .expect("disconnect later dealer after selecting trump");
    tokio::time::sleep(Duration::from_millis(1_200)).await;

    let mut rejoined_later_dealer = connect_client(&url).await;
    let player_names = ["a", "b", "c", "d"];
    let joined = join(&mut rejoined_later_dealer, player_names[later_dealer], room).await;
    assert_eq!(joined["data"]["self_position"], json!(later_dealer));

    let hand_update = recv_until(
        &mut rejoined_later_dealer,
        "later dealer hand after selected-trump rejoin",
        |value| {
            value.get("code").and_then(Value::as_i64) == Some(TractorWsCode::HAND_UPDATED as i64)
        },
    )
    .await;
    let mut restored_hand = hand_update["data"]["cards"]
        .as_array()
        .expect("later dealer restored hand")
        .iter()
        .map(|card| card.as_i64().expect("later dealer restored card") as i32)
        .collect::<Vec<_>>();
    let mut expected_hand = later_hands[later_dealer].clone();
    expected_hand.extend(later_bottom.iter().copied());
    restored_hand.sort_unstable();
    expected_hand.sort_unstable();
    assert_eq!(
        restored_hand, expected_hand,
        "rejoining after selecting trump must neither duplicate nor lose the bottom"
    );

    let rejoined_snapshot = recv_until(
        &mut rejoined_later_dealer,
        "later dealer selected-trump snapshot after rejoin",
        |value| value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64),
    )
    .await;
    assert_eq!(rejoined_snapshot["data"]["round_index"], json!(1));
    assert_eq!(
        rejoined_snapshot["data"]["phase"],
        json!(TractorPhase::Bury as i8)
    );
    assert_eq!(
        rejoined_snapshot["data"]["dealer_position"],
        json!(later_dealer)
    );
    assert_eq!(rejoined_snapshot["data"]["trump_suit"], json!(0));
    assert_eq!(
        rejoined_snapshot["data"]["declaration"]["position"],
        json!(later_dealer)
    );
    assert_eq!(
        rejoined_snapshot["data"]["declaration"]["trump_suit"],
        json!(0)
    );
    let rejoined_countdown = rejoined_snapshot["data"]["turn_countdown"]
        .as_i64()
        .expect("later rejoined bottom countdown");
    assert!(
        rejoined_countdown <= selected_countdown
            && rejoined_countdown >= selected_countdown.saturating_sub(5),
        "selecting trump and rejoining must resume the same bottom window: selected={selected_countdown}, rejoined={rejoined_countdown}"
    );

    send_request(
        &mut rejoined_later_dealer,
        TractorRoutes::BURY_BOTTOM as i32,
        json!({ "cards": later_bottom }),
    )
    .await;
    let later_play = recv_until(
        &mut rejoined_later_dealer,
        "later tractor play snapshot",
        |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
                && value["data"]["round_index"] == json!(1)
                && value["data"]["phase"] == json!(TractorPhase::Play as i8)
        },
    )
    .await;
    recv_until(
        &mut rejoined_later_dealer,
        "later tractor bury response",
        |value| {
            value.get("route").and_then(Value::as_i64) == Some(TractorRoutes::BURY_BOTTOM as i64)
                && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
        },
    )
    .await;
    let later_card = later_hands[later_dealer].pop().expect("later dealer card");
    send_request(
        &mut rejoined_later_dealer,
        Routes::PLAY as i32,
        json!({ "cards": [later_card] }),
    )
    .await;
    let later_play_event = recv_until(
        &mut rejoined_later_dealer,
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
        &mut rejoined_later_dealer,
        "later tractor first play response",
        |value| {
            value.get("route").and_then(Value::as_i64) == Some(Routes::PLAY as i64)
                && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
        },
    )
    .await;
}

#[cfg(not(feature = "official"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tractor_later_round_timeout_selects_trump_and_buries_in_one_window() {
    let runtime = start_test_runtime(
        "tractor-later-auto-bottom-window-test",
        Duration::from_secs(60),
    )
    .await;
    let url = runtime.url.clone();

    let mut a = connect_client(&url).await;
    let mut b = connect_client(&url).await;
    let mut c = connect_client(&url).await;
    let mut d = connect_client(&url).await;
    let room = "tractor-later-auto-bottom-window-room";
    for (position, client) in [&mut a, &mut b, &mut c, &mut d].into_iter().enumerate() {
        let joined = join(client, &format!("later-auto-player-{position}"), room).await;
        assert_eq!(joined["data"]["self_position"], json!(position));
    }

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
                "play_time": 3,
                "settlement_time": 1
            }
        }),
    )
    .await;
    let setting = recv_until(&mut a, "later auto bottom setting", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::SETTING as i64)
    })
    .await;
    assert_eq!(setting["code"], json!(WsResponseCode::OK as i32));
    send_request(&mut a, Routes::START as i32, json!({})).await;
    let started = recv_until(&mut a, "later auto bottom start", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::START as i64)
    })
    .await;
    assert_eq!(started["code"], json!(WsResponseCode::OK as i32));

    let mut clients: [&mut Client; 4] = [&mut a, &mut b, &mut c, &mut d];
    let mut first_hands = collect_tractor_hands(&mut clients, 25).await;
    let (declaration, bottom_seen_by_first_client) = recv_first_declaration(&mut *clients[0]).await;
    let first_dealer = declaration["data"]["position"]
        .as_i64()
        .expect("later auto first dealer") as usize;
    let first_bottom_event = if first_dealer == 0 {
        match bottom_seen_by_first_client {
            Some(bottom) => bottom,
            None => recv_tractor_bottom(&mut *clients[first_dealer], first_dealer).await,
        }
    } else {
        recv_tractor_bottom(&mut *clients[first_dealer], first_dealer).await
    };
    let first_bottom = first_bottom_event["data"]["cards"]
        .as_array()
        .expect("later auto first bottom")
        .iter()
        .map(|card| card.as_i64().expect("later auto first bottom card") as i32)
        .collect::<Vec<_>>();
    send_request(
        &mut *clients[first_dealer],
        TractorRoutes::BURY_BOTTOM as i32,
        json!({ "cards": first_bottom }),
    )
    .await;
    let first_play_snapshot = recv_until(
        &mut *clients[first_dealer],
        "later auto first play snapshot",
        |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
                && value["data"]["phase"] == json!(TractorPhase::Play as i8)
        },
    )
    .await;
    let first_bury_response = recv_until(
        &mut *clients[first_dealer],
        "later auto first bury response",
        |value| {
            value.get("route").and_then(Value::as_i64) == Some(TractorRoutes::BURY_BOTTOM as i64)
        },
    )
    .await;
    assert_eq!(
        first_bury_response["code"],
        json!(WsResponseCode::OK as i32)
    );

    let first_trump_suit =
        first_play_snapshot["data"]["trump_suit"]
            .as_i64()
            .map(|suit| match suit {
                0 => TractorSuit::SPADE,
                1 => TractorSuit::HEART,
                2 => TractorSuit::CLUB,
                3 => TractorSuit::DIAMOND,
                _ => panic!("invalid later auto first trump suit"),
            });
    let first_rules = TractorRules {
        attacking_win_score: 80,
        score_per_level: 40,
        shutout_bonus_levels: 1,
        bottom_card_count: 8,
        deck_count: 2,
        final_target_rank: TractorRank::A,
        target_rank: TractorRank::THREE,
        trump_suit: first_trump_suit,
    };
    let (settlement, _) =
        play_complete_tractor_round(&mut clients, &mut first_hands, first_dealer, &first_rules)
            .await;
    let later_dealer = *settlement
        .winner_positions
        .first()
        .expect("later auto dealer from settlement") as usize;

    let later_hands = collect_tractor_hands(&mut clients, 25).await;
    let later_bottom_event = recv_tractor_bottom(&mut *clients[later_dealer], later_dealer).await;
    let later_bottom = later_bottom_event["data"]["cards"]
        .as_array()
        .expect("later auto bottom")
        .iter()
        .map(|card| card.as_i64().expect("later auto bottom card") as i32)
        .collect::<Vec<_>>();
    assert_eq!(later_bottom.len(), 8);

    let bury_snapshot = recv_until(
        &mut *clients[later_dealer],
        "later auto shared bottom window",
        |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
                && value["data"]["round_index"] == json!(1)
                && value["data"]["phase"] == json!(TractorPhase::Bury as i8)
        },
    )
    .await;
    assert_eq!(bury_snapshot["data"]["trump_suit"], Value::Null);
    assert_eq!(bury_snapshot["data"]["declaration"], Value::Null);
    assert_eq!(bury_snapshot["data"]["turn_countdown"], json!(9));

    let mut automatic_away = None;
    let mut automatic_declaration = None;
    let mut automatic_buried = None;
    let mut automatic_hand = None;
    let play_snapshot = loop {
        let value = recv_json(&mut *clients[later_dealer], "later auto bottom completion").await;
        match value.get("code").and_then(Value::as_i64) {
            Some(code) if code == WsCode::AWAY as i64 => {
                assert!(automatic_away.is_none());
                automatic_away = Some(value);
            }
            Some(code) if code == TractorWsCode::TRUMP_DECLARED as i64 => {
                assert!(automatic_away.is_some());
                assert!(automatic_declaration.is_none());
                automatic_declaration = Some(value);
            }
            Some(code) if code == TractorWsCode::BOTTOM_BURIED as i64 => {
                assert!(
                    automatic_declaration.is_some(),
                    "automatic trump selection must be broadcast before automatic burial"
                );
                automatic_buried = Some(value);
            }
            Some(code) if code == TractorWsCode::HAND_UPDATED as i64 => {
                assert!(automatic_buried.is_some());
                automatic_hand = Some(value);
            }
            Some(code)
                if code == WsCode::TABLE_SNAPSHOT as i64
                    && value["data"]["round_index"] == json!(1)
                    && value["data"]["phase"] == json!(TractorPhase::Play as i8) =>
            {
                assert!(automatic_hand.is_some());
                break value;
            }
            _ => {}
        }
    };

    let automatic_away = automatic_away.expect("automatic later away event");
    assert_eq!(automatic_away["data"]["position"], json!(later_dealer));
    assert_eq!(automatic_away["data"]["is_ai_takeover"], json!(false));
    let automatic_declaration = automatic_declaration.expect("automatic later declaration");
    assert_eq!(
        automatic_declaration["data"]["position"],
        json!(later_dealer)
    );
    assert!(automatic_declaration["data"]["trump_suit"].is_number());
    let automatic_buried = automatic_buried.expect("automatic later burial");
    assert_eq!(automatic_buried["data"]["position"], json!(later_dealer));
    assert_eq!(automatic_buried["data"]["bottom_card_count"], json!(8));

    let automatic_hand = automatic_hand.expect("automatic later private hand");
    let private_cards = automatic_hand["data"]["cards"]
        .as_array()
        .expect("automatic later private cards")
        .iter()
        .map(|card| card.as_i64().expect("automatic later private card") as i32)
        .collect::<Vec<_>>();
    assert_eq!(private_cards.len(), 25);
    let mut cards_before_bury = later_hands[later_dealer].clone();
    cards_before_bury.extend(later_bottom);
    for card in &private_cards {
        let index = cards_before_bury
            .iter()
            .position(|candidate| candidate == card)
            .expect("automatic burial must retain only cards from the dealer hand");
        cards_before_bury.remove(index);
    }
    assert_eq!(
        cards_before_bury.len(),
        8,
        "automatic burial must remove exactly the bottom-card count"
    );
    assert_eq!(
        play_snapshot["data"]["dealer_position"],
        json!(later_dealer)
    );
    assert_eq!(
        play_snapshot["data"]["trump_suit"],
        automatic_declaration["data"]["trump_suit"]
    );
    assert_eq!(
        play_snapshot["data"]["turn_countdown"],
        json!(1),
        "a dealer who exhausted the shared bottom window must continue on the away timer"
    );

    let played = recv_until(
        &mut *clients[later_dealer],
        "later automatic opening play",
        |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::PLAY as i64)
                && value["data"]["position"] == json!(later_dealer)
        },
    )
    .await;
    let played_cards = played["data"]["cards"]
        .as_array()
        .expect("later automatic opening cards")
        .iter()
        .map(|card| card.as_i64().expect("later automatic opening card") as i32)
        .collect::<Vec<_>>();
    assert!(!played_cards.is_empty());
    let mut hand_after_bury = private_cards.clone();
    for card in &played_cards {
        let index = hand_after_bury
            .iter()
            .position(|candidate| candidate == card)
            .expect("later automatic opening card must come from the restored dealer hand");
        hand_after_bury.remove(index);
    }
    assert_eq!(
        played["data"]["remaining_hand_count"],
        json!(25 - played_cards.len())
    );
}

#[cfg(not(feature = "official"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tractor_ws_finishes_when_one_team_wins_at_the_configured_final_rank() {
    let runtime = start_test_runtime("tractor-complete-match-test", Duration::from_secs(60)).await;
    let url = runtime.url.clone();

    let mut a = connect_client(&url).await;
    let mut b = connect_client(&url).await;
    let mut c = connect_client(&url).await;
    let mut d = connect_client(&url).await;
    let room = "tractor-complete-match-room";
    join(&mut a, "match-a", room).await;
    join(&mut b, "match-b", room).await;
    join(&mut c, "match-c", room).await;
    join(&mut d, "match-d", room).await;

    send_request(
        &mut a,
        Routes::SETTING as i32,
        json!({
            "current_configs": {
                "deck_count": 0,
                "attacking_win_score": 80,
                "score_per_level": 40,
                "shutout_bonus_levels": 1,
                "target_rank": 1,
                "first_deal_time": 1000,
                "deal_time": 500,
                "play_time": 30,
                "settlement_time": 1
            }
        }),
    )
    .await;
    let setting = recv_until(&mut a, "complete tractor match setting response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::SETTING as i64)
    })
    .await;
    assert_eq!(setting["code"], json!(WsResponseCode::OK as i32));
    send_request(&mut a, Routes::START as i32, json!({})).await;
    let started = recv_until(&mut a, "complete tractor match start response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::START as i64)
    })
    .await;
    assert_eq!(started["code"], json!(WsResponseCode::OK as i32));

    let mut clients: [&mut Client; 4] = [&mut a, &mut b, &mut c, &mut d];
    let mut first_hands = collect_tractor_hands(&mut clients, 25).await;
    let (declaration, bottom_seen_by_first_client) = recv_first_declaration(&mut *clients[0]).await;
    let first_dealer = declaration["data"]["position"]
        .as_i64()
        .expect("complete match first dealer") as usize;
    let first_bottom_event = if first_dealer == 0 {
        match bottom_seen_by_first_client {
            Some(bottom) => bottom,
            None => recv_tractor_bottom(&mut *clients[first_dealer], first_dealer).await,
        }
    } else {
        recv_tractor_bottom(&mut *clients[first_dealer], first_dealer).await
    };
    let first_bottom = first_bottom_event["data"]["cards"]
        .as_array()
        .expect("complete match first bottom")
        .iter()
        .map(|card| card.as_i64().expect("complete match first bottom card") as i32)
        .collect::<Vec<_>>();
    send_request(
        &mut *clients[first_dealer],
        TractorRoutes::BURY_BOTTOM as i32,
        json!({ "cards": first_bottom }),
    )
    .await;
    let first_play_snapshot = recv_until(
        &mut *clients[first_dealer],
        "complete tractor match first play phase",
        |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
                && value["data"]["phase"] == json!(TractorPhase::Play as i8)
        },
    )
    .await;
    let first_bury = recv_until(
        &mut *clients[first_dealer],
        "complete tractor match first bury response",
        |value| {
            value.get("route").and_then(Value::as_i64) == Some(TractorRoutes::BURY_BOTTOM as i64)
        },
    )
    .await;
    assert_eq!(first_bury["code"], json!(WsResponseCode::OK as i32));
    assert_eq!(first_play_snapshot["data"]["round_index"], json!(0));
    assert_eq!(first_play_snapshot["data"]["target_rank"], json!(3));
    assert_eq!(first_play_snapshot["data"]["final_target_rank"], json!(4));
    let first_trump_suit =
        first_play_snapshot["data"]["trump_suit"]
            .as_i64()
            .map(|suit| match suit {
                0 => TractorSuit::SPADE,
                1 => TractorSuit::HEART,
                2 => TractorSuit::CLUB,
                3 => TractorSuit::DIAMOND,
                _ => panic!("invalid complete match first trump suit"),
            });
    let first_rules = TractorRules {
        attacking_win_score: 80,
        score_per_level: 40,
        shutout_bonus_levels: 1,
        bottom_card_count: 8,
        deck_count: 2,
        final_target_rank: TractorRank::FOUR,
        target_rank: TractorRank::THREE,
        trump_suit: first_trump_suit,
    };
    let (first_settlement, first_settlement_snapshot) =
        play_complete_tractor_round(&mut clients, &mut first_hands, first_dealer, &first_rules)
            .await;
    assert_eq!(first_settlement_snapshot["data"]["round_index"], json!(0));
    assert_eq!(first_settlement.target_rank, TractorRank::THREE);
    assert!(!first_settlement.match_finished);
    assert_eq!(first_settlement.next_target_rank, Some(TractorRank::FOUR));
    assert_eq!(
        first_settlement
            .team_target_ranks
            .iter()
            .filter(|rank| **rank == TractorRank::FOUR)
            .count(),
        1
    );

    let mut previous_settlement = first_settlement;
    let mut final_round = None;
    for round_index in 1..=2 {
        let later_dealer = *previous_settlement
            .winner_positions
            .first()
            .expect("complete match previous winners") as usize;
        let team_ranks_before = previous_settlement.team_target_ranks.clone();
        let mut later_hands = collect_tractor_hands(&mut clients, 25).await;
        let later_bottom_event =
            recv_tractor_bottom(&mut *clients[later_dealer], later_dealer).await;
        let later_bottom = later_bottom_event["data"]["cards"]
            .as_array()
            .expect("complete match later bottom")
            .iter()
            .map(|card| card.as_i64().expect("complete match later bottom card") as i32)
            .collect::<Vec<_>>();
        send_request(
            &mut *clients[later_dealer],
            TractorRoutes::SELECT_TRUMP as i32,
            json!({ "trump_suit": TractorSuit::SPADE as i8 }),
        )
        .await;
        let selected = recv_until(
            &mut *clients[later_dealer],
            "complete tractor match later select response",
            |value| {
                value.get("route").and_then(Value::as_i64)
                    == Some(TractorRoutes::SELECT_TRUMP as i64)
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
        let later_play_snapshot = recv_until(
            &mut *clients[later_dealer],
            "complete tractor match later play phase",
            |value| {
                value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
                    && value["data"]["phase"] == json!(TractorPhase::Play as i8)
                    && value["data"]["round_index"] == json!(round_index)
            },
        )
        .await;
        let later_bury = recv_until(
            &mut *clients[later_dealer],
            "complete tractor match later bury response",
            |value| {
                value.get("route").and_then(Value::as_i64)
                    == Some(TractorRoutes::BURY_BOTTOM as i64)
            },
        )
        .await;
        assert_eq!(later_bury["code"], json!(WsResponseCode::OK as i32));
        assert_eq!(later_play_snapshot["data"]["target_rank"], json!(4));
        assert_eq!(
            later_play_snapshot["data"]["team_target_ranks"],
            json!(team_ranks_before)
        );
        let later_rules = TractorRules {
            target_rank: TractorRank::FOUR,
            trump_suit: Some(TractorSuit::SPADE),
            ..first_rules.clone()
        };
        let (settlement, snapshot) =
            play_complete_tractor_round(&mut clients, &mut later_hands, later_dealer, &later_rules)
                .await;
        assert_eq!(snapshot["data"]["round_index"], json!(round_index));
        assert_eq!(settlement.target_rank, TractorRank::FOUR);
        let winning_team = settlement.winner_positions[0] as usize % 2;
        let other_team = (winning_team + 1) % 2;
        assert_eq!(
            settlement.team_target_ranks[other_team],
            team_ranks_before[other_team]
        );
        if settlement.match_finished {
            assert_eq!(team_ranks_before[winning_team], TractorRank::FOUR);
            assert_eq!(settlement.next_target_rank, None);
            assert_eq!(settlement.team_target_ranks, team_ranks_before);
            final_round = Some((settlement, snapshot));
            break;
        }
        assert_eq!(team_ranks_before[winning_team], TractorRank::THREE);
        assert_eq!(settlement.next_target_rank, Some(TractorRank::FOUR));
        assert_eq!(
            settlement.team_target_ranks[winning_team],
            TractorRank::FOUR
        );
        previous_settlement = settlement;
    }
    let (final_settlement, final_snapshot) =
        final_round.expect("one team must win while already playing the final rank");
    assert!(final_settlement.match_finished);
    assert!((1..=2).contains(&final_snapshot["data"]["round_index"].as_i64().unwrap()));

    let unexpected_next_deal = tokio::time::timeout(
        Duration::from_secs(2),
        recv_until(
            &mut *clients[0],
            "unexpected tractor round after match finish",
            |value| value.get("code").and_then(Value::as_i64) == Some(WsCode::DEAL as i64),
        ),
    )
    .await;
    assert!(
        unexpected_next_deal.is_err(),
        "finished tractor match must not deal another round"
    );
}

#[cfg(not(feature = "official"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tractor_three_deck_ws_deals_and_buries_the_correct_counts() {
    let runtime = start_test_runtime("tractor-three-deck-test", Duration::from_secs(30)).await;
    let url = runtime.url.clone();

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
}

#[cfg(not(feature = "official"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tractor_ws_failed_throw_reports_attempted_and_played_components() {
    let runtime = start_test_runtime("tractor-failed-throw-test", Duration::from_secs(30)).await;
    let url = runtime.url.clone();

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
    let hand_count_before = hands[dealer_position].len();
    let expected_remaining_hand_count = hand_count_before - expected_played.len();
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
        played["data"]["remaining_hand_count"],
        json!(expected_remaining_hand_count)
    );
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
        json!(expected_played.clone())
    );
    assert_eq!(
        failed_throws["data"]["current_trick"][0]["cards"],
        json!(expected_played.clone())
    );
    let snapshot_hand_count = failed_throws["data"]["player_hand_counts"]
        .as_array()
        .expect("failed throw hand counts")
        .iter()
        .find(|entry| entry["position"] == json!(dealer_position))
        .and_then(|entry| entry["hand_count"].as_i64())
        .expect("failed throw dealer hand count");
    assert_eq!(snapshot_hand_count, expected_remaining_hand_count as i64);
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
    assert_eq!(hands[dealer_position].len(), expected_remaining_hand_count);
}

#[cfg(not(feature = "official"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tractor_ws_rejects_an_off_suit_follow_and_accepts_the_next_legal_card() {
    let runtime = start_test_runtime("tractor-illegal-follow-test", Duration::from_secs(30)).await;
    let url = runtime.url.clone();

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
}

#[cfg(not(feature = "official"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tractor_ws_auto_buries_after_three_play_windows() {
    let runtime =
        start_test_runtime("tractor-auto-bury-window-test", Duration::from_secs(30)).await;
    let url = runtime.url.clone();

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
}

#[cfg(not(feature = "official"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tractor_ws_uses_away_time_after_play_passes_to_a_disconnected_player() {
    let runtime = start_test_runtime(
        "tractor-disconnected-next-turn-test",
        Duration::from_secs(30),
    )
    .await;
    let url = runtime.url.clone();

    let mut a = connect_client(&url).await;
    let mut b = connect_client(&url).await;
    let mut c = connect_client(&url).await;
    let mut d = connect_client(&url).await;
    let room = "tractor-disconnected-next-turn-room";
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
                "play_time": 30,
                "away_time": 5
            }
        }),
    )
    .await;
    recv_until(&mut a, "disconnected next setting response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::SETTING as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;
    send_request(&mut a, Routes::START as i32, json!({})).await;
    recv_until(&mut a, "disconnected next start response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::START as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;

    let mut clients: [&mut Client; 4] = [&mut a, &mut b, &mut c, &mut d];
    let hands = collect_tractor_hands(&mut clients, 25).await;
    let (declaration, bottom_seen_by_first_client) = recv_first_declaration(&mut *clients[0]).await;
    let dealer_position = declaration["data"]["position"]
        .as_i64()
        .expect("disconnected next dealer") as usize;
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
        .expect("disconnected next bottom")
        .iter()
        .map(|card| card.as_i64().expect("bottom card") as i32)
        .collect::<Vec<_>>();

    send_request(
        &mut *clients[dealer_position],
        TractorRoutes::BURY_BOTTOM as i32,
        json!({ "cards": bottom_cards }),
    )
    .await;
    recv_until(
        &mut *clients[dealer_position],
        "disconnected next play phase",
        |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
                && value["data"]["phase"] == json!(TractorPhase::Play as i8)
        },
    )
    .await;
    recv_until(
        &mut *clients[dealer_position],
        "disconnected next bury response",
        |value| {
            value.get("route").and_then(Value::as_i64) == Some(TractorRoutes::BURY_BOTTOM as i64)
                && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
        },
    )
    .await;

    let disconnected_position = (dealer_position + 1) % 4;
    clients[disconnected_position]
        .close(None)
        .await
        .expect("close next tractor player");
    let inactive = recv_until(
        &mut *clients[dealer_position],
        "next tractor player disconnected",
        |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::JOIN as i64)
                && value["data"]["position"] == json!(disconnected_position)
                && value["data"]["is_active"] == json!(false)
        },
    )
    .await;
    assert_eq!(inactive["data"]["away"], json!(true));

    let lead_card = *hands[dealer_position]
        .first()
        .expect("dealer has a lead card");
    send_request(
        &mut *clients[dealer_position],
        Routes::PLAY as i32,
        json!({ "cards": [lead_card] }),
    )
    .await;
    recv_until(
        &mut *clients[dealer_position],
        "manual lead before disconnected player",
        |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::PLAY as i64)
                && value["data"]["position"] == json!(dealer_position)
        },
    )
    .await;
    let next_snapshot = recv_until(
        &mut *clients[dealer_position],
        "disconnected player turn countdown",
        |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
                && value["data"]["phase"] == json!(TractorPhase::Play as i8)
                && value["data"]["current_position"] == json!(disconnected_position)
                && value["data"]["trick_index"] == json!(0)
        },
    )
    .await;
    assert_eq!(next_snapshot["data"]["turn_countdown"], json!(5));
    recv_until(
        &mut *clients[dealer_position],
        "manual lead response before disconnected player",
        |value| {
            value.get("route").and_then(Value::as_i64) == Some(Routes::PLAY as i64)
                && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
        },
    )
    .await;

    let mut rejoined = connect_client(&url).await;
    let player_names = ["a", "b", "c", "d"];
    let joined = join(&mut rejoined, player_names[disconnected_position], room).await;
    assert_eq!(
        joined["data"]["self_position"],
        json!(disconnected_position)
    );
    let restored_hand_event = recv_until(&mut rejoined, "rejoined play hand", |value| {
        value.get("code").and_then(Value::as_i64) == Some(TractorWsCode::HAND_UPDATED as i64)
            && value["data"]["position"] == json!(disconnected_position)
    })
    .await;
    let mut restored_hand = restored_hand_event["data"]["cards"]
        .as_array()
        .expect("rejoined play hand cards")
        .iter()
        .map(|card| card.as_i64().expect("rejoined play card") as i32)
        .collect::<Vec<_>>();
    let mut expected_hand = hands[disconnected_position].clone();
    restored_hand.sort_unstable();
    expected_hand.sort_unstable();
    assert_eq!(restored_hand, expected_hand);
    let rejoined_snapshot = recv_until(&mut rejoined, "rejoined current turn snapshot", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
            && value["data"]["phase"] == json!(TractorPhase::Play as i8)
            && value["data"]["current_position"] == json!(disconnected_position)
            && value["data"]["current_trick"]
                .as_array()
                .is_some_and(|trick| trick.len() == 1)
    })
    .await;
    assert!(
        rejoined_snapshot["data"]["turn_countdown"]
            .as_i64()
            .is_some_and(|countdown| (1..=5).contains(&countdown)),
        "rejoin must not replace the five-second away countdown: {rejoined_snapshot}"
    );

    let trump_suit = rejoined_snapshot["data"]["trump_suit"]
        .as_i64()
        .map(|suit| match suit {
            0 => TractorSuit::SPADE,
            1 => TractorSuit::HEART,
            2 => TractorSuit::CLUB,
            3 => TractorSuit::DIAMOND,
            _ => panic!("invalid rejoined play trump suit"),
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
    let lead_combo = combo::classify(&[lead_card], &rules).expect("rejoined single lead combo");
    let legal_follow = expected_hand
        .iter()
        .copied()
        .find(|card| combo::follow_is_legal(&expected_hand, &[*card], &lead_combo, &rules))
        .expect("rejoined player has a legal follow");
    send_request(
        &mut rejoined,
        Routes::PLAY as i32,
        json!({ "cards": [legal_follow] }),
    )
    .await;
    let followed = recv_until(&mut rejoined, "rejoined legal follow", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::PLAY as i64)
            && value["data"]["position"] == json!(disconnected_position)
    })
    .await;
    assert_eq!(followed["data"]["cards"], json!([legal_follow]));
    recv_until(&mut rejoined, "rejoined legal follow response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::PLAY as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;
}
