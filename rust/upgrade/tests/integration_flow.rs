use std::{
    collections::HashMap,
    sync::{Arc, mpsc::sync_channel},
    time::Duration,
};

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use share_type_public::{
    GameId, GameParam, GameParamRange, Routes, UpgradePhase, UpgradeRank, UpgradeRoutes,
    UpgradeWsCode, WsCode, WsResponseCode, WsUpgradePlayedCards, WsUpgradeSettlementEvent,
};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use upgrade::combo;
use upgrade::game::UpgradeGameHandler;
use upgrade_common::{Card, Rank, ScoreProgression};
use ws_common::{
    ClientRequest, Dispatch, GameHandler, GameState, JoinAuthorization, JoinAuthorizationFuture,
    RoomService, RuntimeConfig, RuntimeStopHandle, SessionId, SessionSenders,
    SettingsBuilderResult, run_room_runtime_until_stopped_with_ready, runtime_stop_channel,
};

type Client =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

#[derive(Default)]
struct TestUpgradeHandler(UpgradeGameHandler);

impl GameHandler for TestUpgradeHandler {
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

    fn build_game_state(&self) -> Box<dyn GameState> {
        self.0.build_game_state()
    }

    fn build_room_settings(&self) -> SettingsBuilderResult {
        let (mut settings, mut params) = self.0.build_room_settings();
        for (key, default) in [
            ("first_deal_time", 1_000),
            ("deal_time", 500),
            ("play_time", 30),
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
        room_service: Arc<tokio::sync::Mutex<RoomService>>,
    ) {
        self.0.set_context(senders, room_service);
    }
}

struct TestRuntime {
    url: String,
    stop_handle: RuntimeStopHandle,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for TestRuntime {
    fn drop(&mut self) {
        self.stop_handle.stop();
        self.task.abort();
    }
}

async fn start_test_runtime<H>(
    service_name: &'static str,
    timeout: Duration,
    handler: H,
) -> TestRuntime
where
    H: GameHandler,
{
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
            handler,
            stop_signal,
            ready_tx,
        )
        .await
        .expect("upgrade test runtime");
    });
    let stats = tokio::task::spawn_blocking(move || {
        ready_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("upgrade test runtime readiness")
    })
    .await
    .expect("read upgrade test runtime readiness");
    TestRuntime {
        url: format!("ws://{}", stats.listen_addr()),
        stop_handle,
        task,
    }
}

async fn connect_client(url: &str) -> Client {
    for _ in 0..50 {
        if let Ok((client, _)) = connect_async(url).await {
            return client;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("upgrade websocket server did not become ready");
}

async fn join(client: &mut Client, game_id: GameId, room: &str) -> Value {
    join_as(client, game_id, room, "owner").await
}

async fn join_as(client: &mut Client, game_id: GameId, room: &str, name: &str) -> Value {
    client
        .send(Message::Text(
            json!({
                "route": Routes::JOIN as i32,
                "data": {
                    "name": name,
                    "password": room,
                    "game_id": game_id as i32,
                    "avatar_url": ""
                }
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("send join");

    loop {
        let value = recv_json(client, "join response", Duration::from_secs(5)).await;
        if value.get("route").and_then(Value::as_i64) == Some(Routes::JOIN as i64) {
            return value;
        }
    }
}

async fn send_request(client: &mut Client, route: i32, data: Value) {
    client
        .send(Message::Text(
            json!({ "route": route, "data": data }).to_string().into(),
        ))
        .await
        .expect("send websocket request");
}

async fn wait_for_response(client: &mut Client, route: i32) -> Value {
    loop {
        let value = recv_json(client, "route response", Duration::from_secs(5)).await;
        if value.get("route").and_then(Value::as_i64) == Some(i64::from(route)) {
            return value;
        }
    }
}

async fn wait_for_event(client: &mut Client, code: i32) -> Value {
    loop {
        let value = recv_json(client, "event", Duration::from_secs(25)).await;
        if value.get("code").and_then(Value::as_i64) == Some(i64::from(code)) {
            return value;
        }
    }
}

async fn wait_for_snapshot_at_least(client: &mut Client, trick_index: i32) -> Value {
    loop {
        let snapshot = wait_for_event(client, WsCode::TABLE_SNAPSHOT as i32).await;
        if snapshot["data"]["trick_index"].as_i64().unwrap_or_default() >= i64::from(trick_index) {
            return snapshot;
        }
    }
}

async fn wait_for_phase(client: &mut Client, phase: share_type_public::UpgradePhase) -> Value {
    loop {
        let snapshot = wait_for_event(client, WsCode::TABLE_SNAPSHOT as i32).await;
        if snapshot["data"]["phase"] == json!(phase as i8) {
            return snapshot;
        }
    }
}

async fn recv_json(client: &mut Client, label: &str, timeout: Duration) -> Value {
    loop {
        let frame = tokio::time::timeout(timeout, client.next())
            .await
            .unwrap_or_else(|_| panic!("upgrade websocket timeout while waiting for {label}"))
            .unwrap_or_else(|| panic!("upgrade websocket closed while waiting for {label}"))
            .unwrap_or_else(|error| {
                panic!("upgrade websocket read failed while waiting for {label}: {error}")
            });
        match frame {
            Message::Text(text) => {
                return serde_json::from_str(text.as_ref()).expect("upgrade json frame");
            }
            Message::Ping(payload) => {
                client
                    .send(Message::Pong(payload))
                    .await
                    .unwrap_or_else(|error| {
                        panic!("upgrade websocket pong failed while waiting for {label}: {error}")
                    });
            }
            Message::Pong(_) => {}
            other => {
                panic!("unexpected upgrade websocket frame while waiting for {label}: {other:?}")
            }
        }
    }
}

async fn recv_json_full(client: &mut Client, label: &str) -> Value {
    recv_json(client, label, Duration::from_secs(60)).await
}

async fn collect_upgrade_hand_min(
    client: &mut Client,
    position: usize,
    minimum_hand_size: usize,
) -> Vec<i32> {
    loop {
        let value = recv_json_full(client, "upgrade hand").await;
        if value.get("code").and_then(Value::as_i64) != Some(UpgradeWsCode::HAND_UPDATED as i64) {
            continue;
        }
        assert_eq!(value["data"]["position"], json!(position));
        let cards = value["data"]["cards"]
            .as_array()
            .expect("upgrade hand cards");
        if cards.len() < minimum_hand_size {
            continue;
        }
        return cards
            .iter()
            .map(|card| card.as_i64().expect("upgrade card") as i32)
            .collect();
    }
}

async fn collect_upgrade_hands_min(
    clients: &mut [&mut Client; 4],
    minimum_hand_size: usize,
) -> [Vec<i32>; 4] {
    let (left, right) = clients.split_at_mut(2);
    let (a, b) = left.split_at_mut(1);
    let (c, d) = right.split_at_mut(1);
    let (a, b, c, d) = tokio::join!(
        collect_upgrade_hand_min(&mut *a[0], 0, minimum_hand_size),
        collect_upgrade_hand_min(&mut *b[0], 1, minimum_hand_size),
        collect_upgrade_hand_min(&mut *c[0], 2, minimum_hand_size),
        collect_upgrade_hand_min(&mut *d[0], 3, minimum_hand_size),
    );
    [a, b, c, d]
}

async fn collect_upgrade_hands(clients: &mut [&mut Client; 4]) -> [Vec<i32>; 4] {
    collect_upgrade_hands_min(clients, 38).await
}

async fn recv_upgrade_bottom(client: &mut Client, dealer_position: usize) -> Value {
    let bottom = wait_for_event(client, UpgradeWsCode::BOTTOM_CARDS as i32).await;
    assert_eq!(bottom["data"]["position"], json!(dealer_position));
    bottom
}

fn upgrade_suit(value: i64) -> upgrade_common::Suit {
    match value {
        0 => upgrade_common::Suit::Spade,
        1 => upgrade_common::Suit::Heart,
        2 => upgrade_common::Suit::Club,
        3 => upgrade_common::Suit::Diamond,
        _ => panic!("invalid upgrade suit"),
    }
}

fn upgrade_rank(value: i64) -> Rank {
    match value {
        2 => Rank::Two,
        3 => Rank::Three,
        4 => Rank::Four,
        5 => Rank::Five,
        6 => Rank::Six,
        7 => Rank::Seven,
        8 => Rank::Eight,
        9 => Rank::Nine,
        10 => Rank::Ten,
        11 => Rank::Jack,
        12 => Rank::Queen,
        13 => Rank::King,
        14 => Rank::Ace,
        16 => Rank::SmallJoker,
        17 => Rank::BigJoker,
        _ => panic!("invalid upgrade rank"),
    }
}

fn find_failed_upgrade_throw_candidate(
    hands: &[Vec<i32>; 4],
    position: usize,
    rules: combo::UpgradeComboRules,
) -> Option<(Vec<i32>, Vec<i32>)> {
    let decoded_hand = hands[position]
        .iter()
        .copied()
        .map(Card::try_from)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    let mut by_group: HashMap<Option<upgrade_common::Suit>, HashMap<u8, Vec<Card>>> =
        HashMap::new();
    for card in decoded_hand {
        by_group
            .entry(combo::card_group(card, rules))
            .or_default()
            .entry(card.identity())
            .or_default()
            .push(card);
    }

    for identity_groups in by_group.into_values() {
        let components = identity_groups
            .into_values()
            .filter(|cards| cards.len() >= 2)
            .map(|mut cards| {
                cards.sort_by_key(|card| card.encoded());
                cards
            })
            .collect::<Vec<_>>();
        for left in 0..components.len() {
            for right in (left + 1)..components.len() {
                for left_count in 2..=components[left].len().min(3) {
                    for right_count in 2..=components[right].len().min(3) {
                        let mut attempted = components[left][..left_count].to_vec();
                        attempted.extend_from_slice(&components[right][..right_count]);
                        let Some(classified) = combo::classify(&attempted, rules) else {
                            continue;
                        };
                        if !matches!(classified.kind, combo::ComboKind::Throw { .. }) {
                            continue;
                        }
                        let fallback = hands
                            .iter()
                            .enumerate()
                            .filter(|(opponent, _)| *opponent != position)
                            .filter_map(|(_, opponent_hand)| {
                                let opponent = opponent_hand
                                    .iter()
                                    .copied()
                                    .map(Card::try_from)
                                    .collect::<Result<Vec<_>, _>>()
                                    .ok()?;
                                combo::failed_throw_component(&attempted, &opponent, rules)
                            })
                            .min_by_key(|component| {
                                (
                                    component.len(),
                                    component
                                        .first()
                                        .map(|card| combo::card_strength(*card, rules))
                                        .unwrap_or_default(),
                                    component
                                        .first()
                                        .map(|card| card.encoded())
                                        .unwrap_or_default(),
                                )
                            });
                        if let Some(fallback) = fallback {
                            return Some((
                                attempted.iter().map(|card| card.encoded()).collect(),
                                fallback.iter().map(|card| card.encoded()).collect(),
                            ));
                        }
                    }
                }
            }
        }
    }
    None
}

fn upgrade_trick_winner(
    trick: &[WsUpgradePlayedCards],
    rules: combo::UpgradeComboRules,
) -> Option<usize> {
    let lead = trick.first()?;
    let lead_cards = lead
        .cards
        .iter()
        .copied()
        .map(|card| Card::try_from(card).ok())
        .collect::<Option<Vec<_>>>()?;
    let lead_combo = combo::classify(&lead_cards, rules)?;
    let mut winner = usize::try_from(lead.position).ok()?;
    let mut best_priority = i32::from(lead_combo.group.is_none());
    let mut best = lead_cards
        .iter()
        .filter(|card| combo::card_group(**card, rules) == lead_combo.group)
        .map(|card| combo::card_strength(*card, rules))
        .max()?;
    for played in trick.iter().skip(1) {
        let cards = played
            .cards
            .iter()
            .copied()
            .map(|card| Card::try_from(card).ok())
            .collect::<Option<Vec<_>>>()?;
        let candidate = combo::classify(&cards, rules)?;
        if !combo::can_compete_with_lead(&cards, &lead_combo, rules) {
            continue;
        }
        let competes = match (lead_combo.group, candidate.group) {
            (Some(lead_group), Some(candidate_group)) => lead_group == candidate_group,
            (None, None) | (Some(_), None) => true,
            (None, Some(_)) => false,
        };
        if !competes {
            continue;
        }
        let priority = i32::from(candidate.group.is_none());
        let Some(value) = cards
            .iter()
            .filter(|card| combo::card_group(**card, rules) == candidate.group)
            .map(|card| combo::card_strength(*card, rules))
            .max()
        else {
            continue;
        };
        if priority > best_priority || (priority == best_priority && value > best) {
            best_priority = priority;
            best = value;
            winner = usize::try_from(played.position).ok()?;
        }
    }
    Some(winner)
}

fn upgrade_points(cards: &[i32]) -> i32 {
    cards
        .iter()
        .copied()
        .filter_map(|card| Card::try_from(card).ok())
        .map(|card| i32::from(card.points()))
        .sum()
}

async fn wait_upgrade_play(client: &mut Client, position: usize) -> Value {
    loop {
        let value = recv_json_full(client, "upgrade play event").await;
        if value.get("code").and_then(Value::as_i64) == Some(WsCode::PLAY as i64)
            && value["data"]["position"] == json!(position)
        {
            return value;
        }
    }
}

async fn wait_upgrade_private_deal(client: &mut Client, position: usize) -> i32 {
    loop {
        let value = recv_json_full(client, "upgrade private deal").await;
        if value.get("code").and_then(Value::as_i64) != Some(WsCode::DEAL as i64) {
            continue;
        }
        assert_eq!(value["data"]["position"], json!(position));
        let cards = value["data"]["cards"]
            .as_array()
            .expect("upgrade private deal cards");
        assert_eq!(cards.len(), 1, "upgrade deal must remain incremental");
        return cards[0].as_i64().expect("upgrade private deal card") as i32;
    }
}

async fn wait_upgrade_snapshot(
    client: &mut Client,
    phase: UpgradePhase,
    trick_index: i64,
) -> Value {
    loop {
        let value = recv_json_full(client, "upgrade table snapshot").await;
        if value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
            && value["data"]["phase"] == json!(phase as i8)
            && value["data"]["trick_index"] == json!(trick_index)
        {
            return value;
        }
    }
}

async fn wait_upgrade_round_snapshot(
    client: &mut Client,
    phase: UpgradePhase,
    trick_index: i64,
    round_index: i64,
) -> Value {
    loop {
        let value = recv_json_full(client, "upgrade round table snapshot").await;
        if value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
            && value["data"]["phase"] == json!(phase as i8)
            && value["data"]["trick_index"] == json!(trick_index)
            && value["data"]["round_index"] == json!(round_index)
        {
            return value;
        }
    }
}

async fn play_complete_upgrade_round(
    clients: &mut [&mut Client; 4],
    hands: &mut [Vec<i32>; 4],
    dealer_position: usize,
    round_index: i64,
    rules: combo::UpgradeComboRules,
) -> (WsUpgradeSettlementEvent, Value) {
    let total_play_count = hands.iter().map(Vec::len).sum::<usize>();
    let mut current_position = dealer_position;
    let mut lead_combo = None;
    for play_index in 0..total_play_count {
        let hand = &hands[current_position];
        let hand_cards = hand
            .iter()
            .copied()
            .map(|card| Card::try_from(card).expect("complete upgrade round hand card"))
            .collect::<Vec<_>>();
        let requested_card = if let Some(lead) = lead_combo.as_ref() {
            hand.iter()
                .copied()
                .find(|candidate| {
                    let candidate =
                        Card::try_from(*candidate).expect("complete upgrade round follow card");
                    combo::follow_is_legal(&hand_cards, &[candidate], lead, rules)
                })
                .expect("complete upgrade round legal follow")
        } else {
            *hand.first().expect("complete upgrade round lead card")
        };
        send_request(
            &mut *clients[current_position],
            Routes::PLAY as i32,
            json!({ "cards": [requested_card] }),
        )
        .await;
        let played = wait_upgrade_play(&mut *clients[current_position], current_position).await;
        let played_cards = played["data"]["cards"]
            .as_array()
            .expect("complete upgrade round played cards")
            .iter()
            .map(|card| card.as_i64().expect("complete upgrade round played card") as i32)
            .collect::<Vec<_>>();
        assert_eq!(played_cards.len(), 1);
        let played_card = played_cards[0];
        let index = hands[current_position]
            .iter()
            .position(|candidate| *candidate == played_card)
            .expect("complete upgrade round played card was in hand");
        hands[current_position].remove(index);

        let final_play = play_index + 1 == total_play_count;
        let expected_phase = if final_play {
            UpgradePhase::Settlement
        } else {
            UpgradePhase::Play
        };
        let snapshot = wait_upgrade_round_snapshot(
            &mut *clients[current_position],
            expected_phase,
            ((play_index + 1) / 4) as i64,
            round_index,
        )
        .await;
        if final_play {
            let game_over =
                wait_for_event(&mut *clients[current_position], WsCode::GAME_OVER as i32).await;
            let response =
                wait_for_response(&mut *clients[current_position], Routes::PLAY as i32).await;
            assert_eq!(response["code"], json!(WsResponseCode::OK as i32));
            assert!(hands.iter().all(Vec::is_empty));
            return (
                serde_json::from_value(game_over["data"].clone())
                    .expect("complete upgrade round settlement payload"),
                snapshot,
            );
        }

        let response =
            wait_for_response(&mut *clients[current_position], Routes::PLAY as i32).await;
        assert_eq!(response["code"], json!(WsResponseCode::OK as i32));
        current_position = snapshot["data"]["current_position"]
            .as_i64()
            .expect("complete upgrade round next position") as usize;
        lead_combo = if snapshot["data"]["current_trick"]
            .as_array()
            .is_some_and(|trick| trick.is_empty())
        {
            None
        } else {
            let lead_card = snapshot["data"]["current_trick"][0]["cards"][0]
                .as_i64()
                .expect("complete upgrade round lead card") as i32;
            Some(
                combo::classify(
                    &[Card::try_from(lead_card).expect("complete upgrade round lead")],
                    rules,
                )
                .expect("complete upgrade round lead combo"),
            )
        };
    }
    panic!("complete upgrade round ended without settlement");
}

struct ConcurrentUpgradeRoomCase {
    room: &'static str,
    deck_setting: i32,
    deck_count: i32,
    removed_rank_count: i32,
    target_rank: UpgradeRank,
    expected_hand_size: usize,
    expected_bottom_size: usize,
}

async fn run_concurrent_upgrade_room(url: &str, case: ConcurrentUpgradeRoomCase) -> Value {
    let ConcurrentUpgradeRoomCase {
        room,
        deck_setting,
        deck_count,
        removed_rank_count,
        target_rank,
        expected_hand_size,
        expected_bottom_size,
    } = case;
    let mut a = connect_client(url).await;
    let mut b = connect_client(url).await;
    let mut c = connect_client(url).await;
    let mut d = connect_client(url).await;
    for (position, client) in [&mut a, &mut b, &mut c, &mut d].into_iter().enumerate() {
        let joined = join_as(
            client,
            GameId::UPGRADE,
            room,
            &format!("{room}-player-{position}"),
        )
        .await;
        assert_eq!(joined["data"]["self_position"], json!(position));
        assert_eq!(joined["data"]["current_configs"]["deck_count"], json!(0));
    }
    send_request(
        &mut a,
        Routes::SETTING as i32,
        json!({
            "current_configs": {
                "deck_count": deck_setting,
                "removed_rank_count": removed_rank_count,
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
    let setting = wait_for_response(&mut a, Routes::SETTING as i32).await;
    assert_eq!(setting["code"], json!(WsResponseCode::OK as i32));
    assert_eq!(
        setting["data"]["current_configs"]["deck_count"],
        json!(deck_setting)
    );
    assert_eq!(
        setting["data"]["current_configs"]["removed_rank_count"],
        json!(removed_rank_count)
    );
    send_request(&mut a, Routes::START as i32, Value::Null).await;
    let started = wait_for_response(&mut a, Routes::START as i32).await;
    assert_eq!(started["code"], json!(WsResponseCode::OK as i32));

    let mut clients: [&mut Client; 4] = [&mut a, &mut b, &mut c, &mut d];
    let declaration = wait_for_event(&mut *clients[0], UpgradeWsCode::TRUMP_DECLARED as i32).await;
    assert_eq!(
        declaration["data"]["target_rank"],
        json!(target_rank as i32)
    );
    let dealer_position = declaration["data"]["position"]
        .as_i64()
        .expect("concurrent upgrade room dealer") as usize;
    let mut hands = collect_upgrade_hands_min(&mut clients, expected_hand_size).await;
    assert_eq!(
        hands[dealer_position].len(),
        expected_hand_size + expected_bottom_size
    );
    assert!(
        hands
            .iter()
            .enumerate()
            .all(|(position, hand)| position == dealer_position || hand.len() == expected_hand_size)
    );
    let bottom_event = recv_upgrade_bottom(&mut *clients[dealer_position], dealer_position).await;
    let bottom_cards = bottom_event["data"]["cards"]
        .as_array()
        .expect("concurrent upgrade room bottom")
        .iter()
        .map(|card| card.as_i64().expect("concurrent upgrade room bottom card") as i32)
        .collect::<Vec<_>>();
    assert_eq!(bottom_cards.len(), expected_bottom_size);
    for card in &bottom_cards {
        let index = hands[dealer_position]
            .iter()
            .position(|candidate| candidate == card)
            .expect("concurrent upgrade bottom card in dealer hand");
        hands[dealer_position].remove(index);
    }
    send_request(
        &mut *clients[dealer_position],
        UpgradeRoutes::BURY_BOTTOM as i32,
        json!({ "cards": bottom_cards }),
    )
    .await;
    let play_snapshot =
        wait_upgrade_round_snapshot(&mut *clients[dealer_position], UpgradePhase::Play, 0, 0).await;
    let buried = wait_for_response(
        &mut *clients[dealer_position],
        UpgradeRoutes::BURY_BOTTOM as i32,
    )
    .await;
    assert_eq!(buried["code"], json!(WsResponseCode::OK as i32));
    assert_eq!(play_snapshot["data"]["deck_count"], json!(deck_count));
    assert_eq!(
        play_snapshot["data"]["removed_rank_count"],
        json!(removed_rank_count)
    );
    assert_eq!(
        play_snapshot["data"]["total_deal_count"],
        json!(expected_hand_size * 4)
    );
    let rules = combo::UpgradeComboRules {
        target_rank: upgrade_rank(
            play_snapshot["data"]["target_rank"]
                .as_i64()
                .expect("concurrent upgrade target rank"),
        ),
        trump_suit: Some(upgrade_suit(
            play_snapshot["data"]["trump_suit"]
                .as_i64()
                .expect("concurrent upgrade trump suit"),
        )),
    };
    let mut current_position = dealer_position;
    let mut lead_combo = None;
    let mut final_snapshot = None;
    for play_index in 0..4 {
        let hand = &hands[current_position];
        let hand_cards = hand
            .iter()
            .copied()
            .map(|card| Card::try_from(card).expect("concurrent upgrade hand card"))
            .collect::<Vec<_>>();
        let requested_card = if let Some(lead) = lead_combo.as_ref() {
            hand.iter()
                .copied()
                .find(|candidate| {
                    let candidate =
                        Card::try_from(*candidate).expect("concurrent upgrade follow candidate");
                    combo::follow_is_legal(&hand_cards, &[candidate], lead, rules)
                })
                .expect("concurrent upgrade legal follow")
        } else {
            *hand.first().expect("concurrent upgrade lead card")
        };
        send_request(
            &mut *clients[current_position],
            Routes::PLAY as i32,
            json!({ "cards": [requested_card] }),
        )
        .await;
        let played = wait_upgrade_play(&mut *clients[current_position], current_position).await;
        assert_eq!(
            played["data"]["name"],
            json!(format!("{room}-player-{current_position}")),
            "upgrade play events must not cross room boundaries"
        );
        let played_card = played["data"]["cards"][0]
            .as_i64()
            .expect("concurrent upgrade played card") as i32;
        let index = hands[current_position]
            .iter()
            .position(|candidate| *candidate == played_card)
            .expect("concurrent upgrade played card in hand");
        hands[current_position].remove(index);
        let snapshot = wait_upgrade_round_snapshot(
            &mut *clients[current_position],
            UpgradePhase::Play,
            if play_index == 3 { 1 } else { 0 },
            0,
        )
        .await;
        let response =
            wait_for_response(&mut *clients[current_position], Routes::PLAY as i32).await;
        assert_eq!(response["code"], json!(WsResponseCode::OK as i32));
        if play_index == 3 {
            final_snapshot = Some(snapshot);
            break;
        }
        current_position = snapshot["data"]["current_position"]
            .as_i64()
            .expect("concurrent upgrade next position") as usize;
        let lead_card = snapshot["data"]["current_trick"][0]["cards"][0]
            .as_i64()
            .expect("concurrent upgrade trick lead") as i32;
        lead_combo = Some(
            combo::classify(
                &[Card::try_from(lead_card).expect("concurrent upgrade lead")],
                rules,
            )
            .expect("concurrent upgrade lead combo"),
        );
    }
    final_snapshot.expect("concurrent upgrade room must finish its first trick")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn upgrade_server_accepts_only_its_own_game_id() {
    let runtime = start_test_runtime(
        "upgrade-integration-test",
        Duration::from_secs(30),
        UpgradeGameHandler::default(),
    )
    .await;
    let url = runtime.url.clone();

    let mut wrong_client = connect_client(&url).await;
    let wrong = join(&mut wrong_client, GameId::TRACTOR, "wrong-upgrade-room").await;
    assert_eq!(wrong["code"], json!(WsResponseCode::WRONG_GAME as i32));

    let mut upgrade_client = connect_client(&url).await;
    let accepted = join(&mut upgrade_client, GameId::UPGRADE, "upgrade-room").await;
    assert_eq!(accepted["code"], json!(WsResponseCode::JOINED as i32));
    assert_eq!(accepted["data"]["self_position"], json!(0));
    assert_eq!(accepted["data"]["current_configs"]["deck_count"], json!(0));
    assert_eq!(accepted["data"]["current_configs"]["play_time"], json!(30));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn upgrade_ws_keeps_concurrent_rooms_isolated() {
    let runtime = start_test_runtime(
        "upgrade-concurrent-rooms-test",
        Duration::from_secs(60),
        TestUpgradeHandler::default(),
    )
    .await;
    let (three_deck, four_deck) = tokio::join!(
        run_concurrent_upgrade_room(
            &runtime.url,
            ConcurrentUpgradeRoomCase {
                room: "upgrade-room-three",
                deck_setting: 0,
                deck_count: 3,
                removed_rank_count: 0,
                target_rank: UpgradeRank::THREE,
                expected_hand_size: 38,
                expected_bottom_size: 10,
            },
        ),
        run_concurrent_upgrade_room(
            &runtime.url,
            ConcurrentUpgradeRoomCase {
                room: "upgrade-room-four",
                deck_setting: 1,
                deck_count: 4,
                removed_rank_count: 2,
                target_rank: UpgradeRank::FIVE,
                expected_hand_size: 44,
                expected_bottom_size: 8,
            },
        ),
    );
    assert_eq!(three_deck["data"]["deck_count"], json!(3));
    assert_eq!(three_deck["data"]["removed_rank_count"], json!(0));
    assert_eq!(three_deck["data"]["trick_index"], json!(1));
    assert_eq!(four_deck["data"]["deck_count"], json!(4));
    assert_eq!(four_deck["data"]["removed_rank_count"], json!(2));
    assert_eq!(four_deck["data"]["trick_index"], json!(1));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn upgrade_ws_accepts_a_level_three_declaration_during_first_deal() {
    let runtime = start_test_runtime(
        "upgrade-first-declaration-test",
        Duration::from_secs(45),
        TestUpgradeHandler::default(),
    )
    .await;
    let url = runtime.url.clone();

    let mut a = connect_client(&url).await;
    let mut b = connect_client(&url).await;
    let mut c = connect_client(&url).await;
    let mut d = connect_client(&url).await;
    let room = "upgrade-first-declaration-room";
    for (position, client) in [&mut a, &mut b, &mut c, &mut d].into_iter().enumerate() {
        let joined = join_as(
            client,
            GameId::UPGRADE,
            room,
            &format!("declaration-player-{position}"),
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
                "deck_count": 3,
                "removed_rank_count": 0,
                "first_deal_time": 15_000,
                "deal_time": 3_000,
                "play_time": 30
            }
        }),
    )
    .await;
    assert_eq!(
        wait_for_response(&mut a, Routes::SETTING as i32).await["code"],
        json!(WsResponseCode::OK as i32)
    );
    send_request(&mut a, Routes::START as i32, Value::Null).await;
    assert_eq!(
        wait_for_response(&mut a, Routes::START as i32).await["code"],
        json!(WsResponseCode::OK as i32)
    );

    let clients: [&mut Client; 4] = [&mut a, &mut b, &mut c, &mut d];
    let expected_hand_count = 79;
    let mut dealt_counts = [0_usize; 4];
    let mut dealt_level_cards: [HashMap<u8, Vec<i32>>; 4] = std::array::from_fn(|_| HashMap::new());
    let mut declared = None;
    for _ in 0..expected_hand_count {
        for position in 0..4 {
            let card = wait_upgrade_private_deal(&mut *clients[position], position).await;
            dealt_counts[position] += 1;
            let decoded = Card::try_from(card).expect("upgrade declaration candidate");
            if decoded.rank() != Rank::Three || decoded.suit().is_none() {
                continue;
            }
            dealt_level_cards[position]
                .entry(decoded.identity())
                .or_default()
                .push(card);

            send_request(
                &mut *clients[position],
                UpgradeRoutes::DECLARE_TRUMP as i32,
                json!({ "cards": [card] }),
            )
            .await;
            assert_eq!(
                wait_for_response(&mut *clients[position], UpgradeRoutes::DECLARE_TRUMP as i32)
                    .await["code"],
                json!(WsResponseCode::OK as i32)
            );
            let observer_position = (position + 1) % 4;
            let declaration = wait_for_event(
                &mut *clients[observer_position],
                UpgradeWsCode::TRUMP_DECLARED as i32,
            )
            .await;
            assert_eq!(declaration["data"]["position"], json!(position));
            assert_eq!(declaration["data"]["cards"], json!([card]));
            assert_eq!(declaration["data"]["strength"], json!(1));
            assert_eq!(declaration["data"]["target_rank"], json!(3));
            assert_eq!(
                upgrade_suit(
                    declaration["data"]["trump_suit"]
                        .as_i64()
                        .expect("upgrade declaration suit")
                ),
                decoded.suit().expect("suited level card")
            );
            declared = Some((position, card));
            break;
        }
        if declared.is_some() {
            break;
        }
    }
    let (declaring_position, declared_card) =
        declared.expect("a six-deck deal must expose a suited level three outside the bottom");
    let declared_suit = Card::try_from(declared_card)
        .expect("declared upgrade card")
        .suit()
        .expect("declared upgrade card suit");

    let mut rejected_equal_declaration = None;
    'deal: while dealt_counts
        .iter()
        .any(|count| *count < expected_hand_count)
    {
        for position in 0..4 {
            if dealt_counts[position] >= expected_hand_count {
                continue;
            }
            let card = wait_upgrade_private_deal(&mut *clients[position], position).await;
            dealt_counts[position] += 1;
            let decoded = Card::try_from(card).expect("upgrade counter declaration candidate");
            if decoded.rank() != Rank::Three || decoded.suit().is_none() {
                continue;
            }
            dealt_level_cards[position]
                .entry(decoded.identity())
                .or_default()
                .push(card);

            send_request(
                &mut *clients[position],
                UpgradeRoutes::DECLARE_TRUMP as i32,
                json!({ "cards": [card] }),
            )
            .await;
            assert_eq!(
                wait_for_response(&mut *clients[position], UpgradeRoutes::DECLARE_TRUMP as i32)
                    .await["code"],
                json!(WsResponseCode::NO_PERMISSION as i32)
            );
            let snapshot =
                wait_for_event(&mut *clients[position], WsCode::TABLE_SNAPSHOT as i32).await;
            assert_eq!(
                snapshot["data"]["declaration"]["position"],
                json!(declaring_position)
            );
            assert_eq!(snapshot["data"]["declaration"]["strength"], json!(1));
            assert_eq!(
                upgrade_suit(
                    snapshot["data"]["declaration"]["trump_suit"]
                        .as_i64()
                        .expect("retained upgrade declaration suit")
                ),
                declared_suit
            );
            rejected_equal_declaration = Some((position, card));
            break 'deal;
        }
    }
    assert!(
        rejected_equal_declaration.is_some(),
        "another dealt level three must not replace an equal-strength declaration"
    );

    let find_pair = |cards: &[HashMap<u8, Vec<i32>>; 4]| {
        cards.iter().enumerate().find_map(|(position, groups)| {
            groups
                .values()
                .find(|copies| copies.len() >= 2)
                .map(|copies| (position, copies[..2].to_vec()))
        })
    };
    let mut stronger_declaration = find_pair(&dealt_level_cards);
    'deal: while stronger_declaration.is_none()
        && dealt_counts
            .iter()
            .any(|count| *count < expected_hand_count)
    {
        for position in 0..4 {
            if dealt_counts[position] >= expected_hand_count {
                continue;
            }
            let card = wait_upgrade_private_deal(&mut *clients[position], position).await;
            dealt_counts[position] += 1;
            let decoded = Card::try_from(card).expect("upgrade stronger declaration candidate");
            if decoded.rank() != Rank::Three || decoded.suit().is_none() {
                continue;
            }
            let copies = dealt_level_cards[position]
                .entry(decoded.identity())
                .or_default();
            copies.push(card);
            if copies.len() >= 2 {
                stronger_declaration = Some((position, copies[..2].to_vec()));
                break 'deal;
            }
        }
    }
    let (stronger_position, stronger_cards) = stronger_declaration
        .expect("a six-deck deal must expose two identical level threes to one player");
    send_request(
        &mut *clients[stronger_position],
        UpgradeRoutes::DECLARE_TRUMP as i32,
        json!({ "cards": stronger_cards }),
    )
    .await;
    assert_eq!(
        wait_for_response(
            &mut *clients[stronger_position],
            UpgradeRoutes::DECLARE_TRUMP as i32
        )
        .await["code"],
        json!(WsResponseCode::OK as i32)
    );
    let observer_position = (stronger_position + 1) % 4;
    let declaration = loop {
        let candidate = wait_for_event(
            &mut *clients[observer_position],
            UpgradeWsCode::TRUMP_DECLARED as i32,
        )
        .await;
        if candidate["data"]["position"] == json!(stronger_position)
            && candidate["data"]["strength"] == json!(2)
        {
            break candidate;
        }
    };
    assert_eq!(declaration["data"]["position"], json!(stronger_position));
    assert_eq!(declaration["data"]["cards"], json!(stronger_cards));
    assert_eq!(declaration["data"]["strength"], json!(2));
    assert_eq!(declaration["data"]["target_rank"], json!(3));
    let stronger_suit = Card::try_from(stronger_cards[0])
        .expect("stronger upgrade declaration card")
        .suit()
        .expect("stronger upgrade declaration suit");
    assert_eq!(
        upgrade_suit(
            declaration["data"]["trump_suit"]
                .as_i64()
                .expect("stronger upgrade declaration protocol suit")
        ),
        stronger_suit
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn four_players_can_deal_bury_and_play_first_round() {
    use share_type_public::{UpgradePhase, UpgradeRoutes, UpgradeWsCode};

    let runtime = start_test_runtime(
        "upgrade-round-integration-test",
        Duration::from_secs(30),
        UpgradeGameHandler::default(),
    )
    .await;
    let url = runtime.url.clone();

    let mut clients = Vec::new();
    for position in 0..4 {
        let mut client = connect_client(&url).await;
        let joined = join_as(
            &mut client,
            GameId::UPGRADE,
            "upgrade-round-room",
            &format!("player-{position}"),
        )
        .await;
        assert_eq!(joined["code"], json!(WsResponseCode::JOINED as i32));
        assert_eq!(joined["data"]["self_position"], json!(position));
        clients.push(client);
    }

    send_request(&mut clients[0], Routes::START as i32, Value::Null).await;
    let started = wait_for_response(&mut clients[0], Routes::START as i32).await;
    assert_eq!(started["code"], json!(WsResponseCode::OK as i32));
    let declaration = wait_for_event(&mut clients[0], UpgradeWsCode::TRUMP_DECLARED as i32).await;
    assert_eq!(declaration["data"]["target_rank"], json!(3));
    let dealer = declaration["data"]["position"].as_u64().unwrap() as usize;
    let mut hands = Vec::new();
    for (position, client) in clients.iter_mut().enumerate() {
        let hand = wait_for_event(client, UpgradeWsCode::HAND_UPDATED as i32).await;
        assert_eq!(hand["data"]["position"], json!(position));
        assert_eq!(
            hand["data"]["cards"].as_array().unwrap().len(),
            if position == dealer { 48 } else { 38 }
        );
        hands.push(
            hand["data"]["cards"]
                .as_array()
                .unwrap()
                .iter()
                .map(|card| card.as_i64().unwrap() as i32)
                .collect::<Vec<_>>(),
        );
    }
    let bottom = wait_for_event(&mut clients[dealer], UpgradeWsCode::BOTTOM_CARDS as i32).await;
    let bottom_cards = bottom["data"]["cards"].clone();
    assert_eq!(bottom_cards.as_array().unwrap().len(), 10);

    send_request(
        &mut clients[dealer],
        UpgradeRoutes::SELECT_TRUMP as i32,
        json!({ "trump_suit": 0 }),
    )
    .await;
    let first_round_select =
        wait_for_response(&mut clients[dealer], UpgradeRoutes::SELECT_TRUMP as i32).await;
    assert_eq!(
        first_round_select["code"],
        json!(WsResponseCode::NO_PERMISSION as i32)
    );

    send_request(
        &mut clients[dealer],
        UpgradeRoutes::BURY_BOTTOM as i32,
        json!({ "cards": bottom_cards }),
    )
    .await;
    let snapshot = wait_for_phase(&mut clients[dealer], UpgradePhase::Play).await;
    assert_eq!(snapshot["data"]["phase"], json!(UpgradePhase::Play as i8));
    let buried = wait_for_response(&mut clients[dealer], UpgradeRoutes::BURY_BOTTOM as i32).await;
    assert_eq!(buried["code"], json!(WsResponseCode::OK as i32));

    send_request(
        &mut clients[dealer],
        Routes::PLAY as i32,
        json!({ "cards": [999] }),
    )
    .await;
    let invalid_play = wait_for_response(&mut clients[dealer], Routes::PLAY as i32).await;
    assert_eq!(
        invalid_play["code"],
        json!(WsResponseCode::NO_PERMISSION as i32)
    );
    let trump_suit = match snapshot["data"]["trump_suit"].as_i64().unwrap() {
        0 => upgrade_common::Suit::Spade,
        1 => upgrade_common::Suit::Heart,
        2 => upgrade_common::Suit::Club,
        3 => upgrade_common::Suit::Diamond,
        _ => panic!("invalid trump suit"),
    };

    for card in bottom["data"]["cards"].as_array().unwrap() {
        let card = card.as_i64().unwrap() as i32;
        let index = hands[dealer]
            .iter()
            .position(|candidate| *candidate == card)
            .unwrap();
        hands[dealer].remove(index);
    }
    let lead = hands[dealer][0];
    let lead_card = upgrade_common::Card::try_from(lead).unwrap();
    let lead_group = if lead_card.suit() == Some(trump_suit)
        || lead_card.suit().is_none()
        || lead_card.rank() == upgrade_common::Rank::Three
    {
        None
    } else {
        lead_card.suit()
    };
    for play_index in 0..4 {
        let position = (dealer + play_index) % 4;
        let card = if position == dealer {
            lead
        } else {
            hands[position]
                .iter()
                .copied()
                .find(|candidate| {
                    let decoded = upgrade_common::Card::try_from(*candidate).unwrap();
                    let group = if decoded.suit() == Some(trump_suit)
                        || decoded.suit().is_none()
                        || decoded.rank() == upgrade_common::Rank::Three
                    {
                        None
                    } else {
                        decoded.suit()
                    };
                    group == lead_group
                })
                .unwrap_or(hands[position][0])
        };
        let client = &mut clients[position];
        send_request(client, Routes::PLAY as i32, json!({ "cards": [card] })).await;
        let played = wait_for_event(client, WsCode::PLAY as i32).await;
        assert_eq!(played["data"]["cards"].as_array().unwrap().len(), 1);
        let play_snapshot =
            wait_for_snapshot_at_least(client, if play_index == 3 { 1 } else { 0 }).await;
        if play_index == 3 {
            assert_eq!(play_snapshot["data"]["trick_index"], json!(1));
        }
        let response = wait_for_response(client, Routes::PLAY as i32).await;
        assert_eq!(response["code"], json!(WsResponseCode::OK as i32));
        let index = hands[position]
            .iter()
            .position(|candidate| *candidate == card)
            .unwrap();
        hands[position].remove(index);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn upgrade_server_completes_round_and_enters_later_round() {
    let runtime = start_test_runtime(
        "upgrade-full-round-test",
        Duration::from_secs(90),
        TestUpgradeHandler::default(),
    )
    .await;
    let url = runtime.url.clone();

    let mut a = connect_client(&url).await;
    let mut b = connect_client(&url).await;
    let mut c = connect_client(&url).await;
    let mut d = connect_client(&url).await;
    let room = "upgrade-full-round-room";
    for (position, client) in [&mut a, &mut b, &mut c, &mut d].into_iter().enumerate() {
        let joined = join_as(
            client,
            GameId::UPGRADE,
            room,
            &format!("upgrade-player-{position}"),
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
                "deck_count": 0,
                "removed_rank_count": 0,
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
    assert_eq!(
        wait_for_response(&mut a, Routes::SETTING as i32).await["code"],
        json!(WsResponseCode::OK as i32)
    );
    send_request(&mut a, Routes::START as i32, Value::Null).await;
    assert_eq!(
        wait_for_response(&mut a, Routes::START as i32).await["code"],
        json!(WsResponseCode::OK as i32)
    );

    let mut clients: [&mut Client; 4] = [&mut a, &mut b, &mut c, &mut d];
    let declaration = wait_for_event(&mut *clients[0], UpgradeWsCode::TRUMP_DECLARED as i32).await;
    let first_dealer = declaration["data"]["position"]
        .as_i64()
        .expect("upgrade first dealer") as usize;
    assert!(first_dealer < 4);

    let mut hands = collect_upgrade_hands(&mut clients).await;
    assert!(hands.iter().all(|hand| hand.len() >= 38));
    assert_eq!(hands[first_dealer].len(), 48);
    assert!(
        hands
            .iter()
            .enumerate()
            .all(|(position, hand)| position == first_dealer || hand.len() == 38)
    );
    let bottom_event = recv_upgrade_bottom(&mut *clients[first_dealer], first_dealer).await;
    let bottom_cards = bottom_event["data"]["cards"]
        .as_array()
        .expect("upgrade bottom cards")
        .iter()
        .map(|card| card.as_i64().expect("upgrade bottom card") as i32)
        .collect::<Vec<_>>();
    assert_eq!(bottom_cards.len(), 10);
    for card in &bottom_cards {
        let index = hands[first_dealer]
            .iter()
            .position(|candidate| candidate == card)
            .expect("bottom card is in dealer hand");
        hands[first_dealer].remove(index);
    }

    send_request(
        &mut *clients[first_dealer],
        UpgradeRoutes::SELECT_TRUMP as i32,
        json!({ "trump_suit": 0 }),
    )
    .await;
    assert_eq!(
        wait_for_response(
            &mut *clients[first_dealer],
            UpgradeRoutes::SELECT_TRUMP as i32
        )
        .await["code"],
        json!(WsResponseCode::NO_PERMISSION as i32)
    );

    send_request(
        &mut *clients[first_dealer],
        UpgradeRoutes::BURY_BOTTOM as i32,
        json!({ "cards": bottom_cards }),
    )
    .await;
    let first_play_snapshot =
        wait_upgrade_snapshot(&mut *clients[first_dealer], UpgradePhase::Play, 0).await;
    assert_eq!(first_play_snapshot["data"]["round_index"], json!(0));
    assert_eq!(first_play_snapshot["data"]["deck_count"], json!(3));
    assert_eq!(first_play_snapshot["data"]["bottom_card_count"], json!(10));
    assert_eq!(first_play_snapshot["data"]["hand_count"], json!(38));
    assert_eq!(
        wait_for_response(
            &mut *clients[first_dealer],
            UpgradeRoutes::BURY_BOTTOM as i32
        )
        .await["code"],
        json!(WsResponseCode::OK as i32)
    );

    let rules = upgrade::combo::UpgradeComboRules {
        target_rank: upgrade_rank(
            first_play_snapshot["data"]["target_rank"]
                .as_i64()
                .expect("upgrade target rank"),
        ),
        trump_suit: Some(upgrade_suit(
            first_play_snapshot["data"]["trump_suit"]
                .as_i64()
                .expect("upgrade trump suit"),
        )),
    };
    let mut current_position = first_dealer;
    let mut lead_combo = None;
    let mut current_trick = Vec::<WsUpgradePlayedCards>::new();
    let mut collected_scores = [0_i32; 4];
    let mut later_dealer = None;
    for play_index in 0..152usize {
        let hand = &hands[current_position];
        let hand_cards = hand
            .iter()
            .copied()
            .map(|card| Card::try_from(card).expect("hand card"))
            .collect::<Vec<_>>();
        let cards = if let Some(lead) = lead_combo.as_ref() {
            let card = hand
                .iter()
                .copied()
                .find(|candidate| {
                    let candidate = Card::try_from(*candidate).expect("follow card");
                    combo::follow_is_legal(&hand_cards, &[candidate], lead, rules)
                })
                .expect("legal upgrade follow");
            vec![card]
        } else {
            vec![*hand.first().expect("upgrade lead card")]
        };
        send_request(
            &mut *clients[current_position],
            Routes::PLAY as i32,
            json!({ "cards": cards }),
        )
        .await;
        let played = wait_upgrade_play(&mut *clients[current_position], current_position).await;
        let played_cards = played["data"]["cards"]
            .as_array()
            .expect("upgrade played cards")
            .iter()
            .map(|card| card.as_i64().expect("played card") as i32)
            .collect::<Vec<_>>();
        assert_eq!(played_cards.len(), 1);
        current_trick.push(
            serde_json::from_value(played["data"].clone()).expect("upgrade played event payload"),
        );
        for card in &played_cards {
            let index = hands[current_position]
                .iter()
                .position(|candidate| candidate == card)
                .expect("upgrade played card in hand");
            hands[current_position].remove(index);
        }
        let expected_trick_index = ((play_index + 1) / 4) as i64;
        let expected_phase = if play_index + 1 == 152 {
            UpgradePhase::Settlement
        } else {
            UpgradePhase::Play
        };
        let snapshot = wait_upgrade_snapshot(
            &mut *clients[current_position],
            expected_phase,
            expected_trick_index,
        )
        .await;
        assert_eq!(
            snapshot["data"]["player_hand_counts"]
                .as_array()
                .expect("upgrade player hand counts")
                .iter()
                .map(|entry| entry["hand_count"].as_i64().expect("hand count"))
                .sum::<i64>(),
            (152 - play_index - 1) as i64
        );
        if play_index + 1 == 152 {
            assert_eq!(current_trick.len(), 4);
            let trick_winner = upgrade_trick_winner(&current_trick, rules).expect("trick winner");
            collected_scores[trick_winner] += upgrade_points(
                &current_trick
                    .iter()
                    .flat_map(|played| played.cards.iter().copied())
                    .collect::<Vec<_>>(),
            );
            let winning_cards = current_trick
                .iter()
                .find(|played| played.position == trick_winner as i32)
                .expect("winning play")
                .cards
                .clone();
            collected_scores[trick_winner] += upgrade_points(&bottom_cards)
                * combo::bottom_multiplier(
                    &winning_cards
                        .iter()
                        .copied()
                        .map(|card| Card::try_from(card).expect("winning card"))
                        .collect::<Vec<_>>(),
                ) as i32;
            let expected_score = [(first_dealer + 1) % 4, (first_dealer + 3) % 4]
                .into_iter()
                .map(|position| collected_scores[position])
                .sum::<i32>();
            let game_over = loop {
                let value =
                    recv_json_full(&mut *clients[current_position], "upgrade game over").await;
                if value.get("code").and_then(Value::as_i64) == Some(WsCode::GAME_OVER as i64) {
                    break value;
                }
            };
            let settlement: WsUpgradeSettlementEvent =
                serde_json::from_value(game_over["data"].clone()).expect("upgrade settlement");
            assert_eq!(settlement.score, expected_score);
            let expected_winners = if expected_score >= 80 {
                vec![(first_dealer + 1) as i32 % 4, (first_dealer + 3) as i32 % 4]
            } else {
                vec![first_dealer as i32, (first_dealer + 2) as i32 % 4]
            };
            assert_eq!(settlement.winner_positions, expected_winners);
            let expected_levels = ScoreProgression::new(80, 40, 1)
                .expect("score progression")
                .outcome(expected_score)
                .levels as i32;
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
            assert_eq!(
                game_over["data"]["player_scores"]
                    .as_object()
                    .map(|scores| scores.len()),
                Some(4)
            );
            later_dealer = game_over["data"]["winner_positions"]
                .as_array()
                .and_then(|winners| winners.first())
                .and_then(Value::as_i64)
                .map(|winner| {
                    if game_over["data"]["winner_positions"]
                        .as_array()
                        .is_some_and(|winners| {
                            winners
                                .iter()
                                .any(|position| position == &json!(first_dealer))
                        })
                    {
                        first_dealer
                    } else {
                        winner as usize
                    }
                });
            assert_eq!(
                wait_for_response(&mut *clients[current_position], Routes::PLAY as i32).await["code"],
                json!(WsResponseCode::OK as i32)
            );
            break;
        }
        if current_trick.len() == 4 {
            let trick_winner = upgrade_trick_winner(&current_trick, rules).expect("trick winner");
            collected_scores[trick_winner] += upgrade_points(
                &current_trick
                    .iter()
                    .flat_map(|played| played.cards.iter().copied())
                    .collect::<Vec<_>>(),
            );
            current_trick.clear();
        }
        assert_eq!(
            wait_for_response(&mut *clients[current_position], Routes::PLAY as i32).await["code"],
            json!(WsResponseCode::OK as i32)
        );
        current_position = snapshot["data"]["current_position"]
            .as_i64()
            .expect("upgrade next position") as usize;
        lead_combo = if snapshot["data"]["current_trick"]
            .as_array()
            .is_some_and(|trick| trick.is_empty())
        {
            None
        } else {
            let lead_card = snapshot["data"]["current_trick"][0]["cards"][0]
                .as_i64()
                .expect("upgrade lead card") as i32;
            Some(
                combo::classify(&[Card::try_from(lead_card).expect("upgrade lead")], rules)
                    .expect("upgrade lead combo"),
            )
        };
    }
    assert!(hands.iter().all(Vec::is_empty));

    let later_dealer = later_dealer.expect("upgrade later dealer");
    assert!(later_dealer < 4);
    let mut later_hands = collect_upgrade_hands(&mut clients).await;
    assert_eq!(later_hands[later_dealer].len(), 48);
    assert!(
        later_hands
            .iter()
            .enumerate()
            .all(|(position, hand)| position == later_dealer || hand.len() == 38)
    );
    let later_bottom_event = recv_upgrade_bottom(&mut *clients[later_dealer], later_dealer).await;
    let later_bottom = later_bottom_event["data"]["cards"]
        .as_array()
        .expect("later upgrade bottom")
        .iter()
        .map(|card| card.as_i64().expect("later bottom card") as i32)
        .collect::<Vec<_>>();
    assert_eq!(later_bottom.len(), 10);
    for card in &later_bottom {
        let index = later_hands[later_dealer]
            .iter()
            .position(|candidate| candidate == card)
            .expect("later bottom in dealer hand");
        later_hands[later_dealer].remove(index);
    }

    // 让共享窗口先消耗一点时间；若选主重置窗口，下面的快照会错误地回到 90 秒。
    tokio::time::sleep(Duration::from_secs(2)).await;
    send_request(
        &mut *clients[later_dealer],
        UpgradeRoutes::SELECT_TRUMP as i32,
        json!({ "trump_suit": 0 }),
    )
    .await;
    let later_selected_snapshot = loop {
        let value = recv_json_full(&mut *clients[later_dealer], "upgrade selected snapshot").await;
        if value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
            && value["data"]["phase"] == json!(UpgradePhase::Bury as i8)
            && value["data"]["round_index"] == json!(1)
            && value["data"]["trump_suit"] != Value::Null
        {
            break value;
        }
    };
    assert_eq!(later_selected_snapshot["data"]["round_index"], json!(1));
    assert_ne!(later_selected_snapshot["data"]["trump_suit"], Value::Null);
    // 后续局选主和埋底共用一个窗口；选主不能把三倍出牌倒计时重置。
    assert!(
        later_selected_snapshot["data"]["turn_countdown"]
            .as_i64()
            .is_some_and(|countdown| countdown < 90)
    );
    assert_eq!(
        wait_for_response(
            &mut *clients[later_dealer],
            UpgradeRoutes::SELECT_TRUMP as i32
        )
        .await["code"],
        json!(WsResponseCode::OK as i32)
    );

    send_request(
        &mut *clients[later_dealer],
        UpgradeRoutes::BURY_BOTTOM as i32,
        json!({ "cards": later_bottom }),
    )
    .await;
    let later_play_snapshot =
        wait_upgrade_snapshot(&mut *clients[later_dealer], UpgradePhase::Play, 0).await;
    assert_eq!(later_play_snapshot["data"]["round_index"], json!(1));
    assert_eq!(
        wait_for_response(
            &mut *clients[later_dealer],
            UpgradeRoutes::BURY_BOTTOM as i32
        )
        .await["code"],
        json!(WsResponseCode::OK as i32)
    );
    let later_rules = upgrade::combo::UpgradeComboRules {
        target_rank: upgrade_rank(
            later_play_snapshot["data"]["target_rank"]
                .as_i64()
                .expect("later upgrade target rank"),
        ),
        trump_suit: Some(upgrade_suit(
            later_play_snapshot["data"]["trump_suit"]
                .as_i64()
                .expect("later upgrade trump suit"),
        )),
    };
    let later_card = later_hands[later_dealer]
        .first()
        .copied()
        .expect("later upgrade first card");
    assert!(
        combo::classify(
            &[Card::try_from(later_card).expect("later card")],
            later_rules,
        )
        .is_some()
    );
    send_request(
        &mut *clients[later_dealer],
        Routes::PLAY as i32,
        json!({ "cards": [later_card] }),
    )
    .await;
    let later_play = wait_upgrade_play(&mut *clients[later_dealer], later_dealer).await;
    assert_eq!(later_play["data"]["cards"], json!([later_card]));
    wait_upgrade_snapshot(&mut *clients[later_dealer], UpgradePhase::Play, 0).await;
    assert_eq!(
        wait_for_response(&mut *clients[later_dealer], Routes::PLAY as i32).await["code"],
        json!(WsResponseCode::OK as i32)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn upgrade_ws_finishes_a_multi_round_match_at_ace() {
    let runtime = start_test_runtime(
        "upgrade-complete-match-test",
        Duration::from_secs(90),
        TestUpgradeHandler::default(),
    )
    .await;
    let url = runtime.url.clone();

    let mut a = connect_client(&url).await;
    let mut b = connect_client(&url).await;
    let mut c = connect_client(&url).await;
    let mut d = connect_client(&url).await;
    let room = "upgrade-complete-match-room";
    for (position, client) in [&mut a, &mut b, &mut c, &mut d].into_iter().enumerate() {
        let joined = join_as(
            client,
            GameId::UPGRADE,
            room,
            &format!("complete-upgrade-player-{position}"),
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
                "deck_count": 0,
                "removed_rank_count": 7,
                "attacking_win_score": 400,
                "score_per_level": 5,
                "shutout_bonus_levels": 3,
                "first_deal_time": 1000,
                "deal_time": 500,
                "play_time": 30
            }
        }),
    )
    .await;
    assert_eq!(
        wait_for_response(&mut a, Routes::SETTING as i32).await["code"],
        json!(WsResponseCode::OK as i32)
    );
    send_request(&mut a, Routes::START as i32, Value::Null).await;
    assert_eq!(
        wait_for_response(&mut a, Routes::START as i32).await["code"],
        json!(WsResponseCode::OK as i32)
    );

    let mut clients: [&mut Client; 4] = [&mut a, &mut b, &mut c, &mut d];
    let declaration = wait_for_event(&mut *clients[0], UpgradeWsCode::TRUMP_DECLARED as i32).await;
    assert_eq!(
        declaration["data"]["target_rank"],
        json!(UpgradeRank::FIVE as i32),
        "the compact rank path must retain the scoring rank five"
    );
    let mut dealer_position = declaration["data"]["position"]
        .as_i64()
        .expect("complete upgrade match first dealer") as usize;
    let mut completed_rounds = 0_usize;
    let mut final_settlement = None;

    for round_index in 0..10_i64 {
        let mut hands = collect_upgrade_hands_min(&mut clients, 17).await;
        assert_eq!(hands[dealer_position].len(), 27);
        assert!(
            hands
                .iter()
                .enumerate()
                .all(|(position, hand)| position == dealer_position || hand.len() == 17)
        );
        let bottom_event =
            recv_upgrade_bottom(&mut *clients[dealer_position], dealer_position).await;
        let bottom_cards = bottom_event["data"]["cards"]
            .as_array()
            .expect("complete upgrade match bottom")
            .iter()
            .map(|card| card.as_i64().expect("complete upgrade match bottom card") as i32)
            .collect::<Vec<_>>();
        assert_eq!(bottom_cards.len(), 10);
        for card in &bottom_cards {
            let index = hands[dealer_position]
                .iter()
                .position(|candidate| candidate == card)
                .expect("complete upgrade match bottom card in dealer hand");
            hands[dealer_position].remove(index);
        }

        if round_index > 0 {
            send_request(
                &mut *clients[dealer_position],
                UpgradeRoutes::SELECT_TRUMP as i32,
                json!({ "trump_suit": 0 }),
            )
            .await;
            let selected = wait_for_response(
                &mut *clients[dealer_position],
                UpgradeRoutes::SELECT_TRUMP as i32,
            )
            .await;
            assert_eq!(selected["code"], json!(WsResponseCode::OK as i32));
        }
        send_request(
            &mut *clients[dealer_position],
            UpgradeRoutes::BURY_BOTTOM as i32,
            json!({ "cards": bottom_cards }),
        )
        .await;
        let play_snapshot = wait_upgrade_round_snapshot(
            &mut *clients[dealer_position],
            UpgradePhase::Play,
            0,
            round_index,
        )
        .await;
        let buried = wait_for_response(
            &mut *clients[dealer_position],
            UpgradeRoutes::BURY_BOTTOM as i32,
        )
        .await;
        assert_eq!(buried["code"], json!(WsResponseCode::OK as i32));
        assert_eq!(play_snapshot["data"]["removed_rank_count"], json!(7));
        assert_eq!(play_snapshot["data"]["hand_count"], json!(17));
        assert_eq!(play_snapshot["data"]["bottom_card_count"], json!(10));
        let rules = combo::UpgradeComboRules {
            target_rank: upgrade_rank(
                play_snapshot["data"]["target_rank"]
                    .as_i64()
                    .expect("complete upgrade match target rank"),
            ),
            trump_suit: Some(upgrade_suit(
                play_snapshot["data"]["trump_suit"]
                    .as_i64()
                    .expect("complete upgrade match trump suit"),
            )),
        };
        let (settlement, settlement_snapshot) = play_complete_upgrade_round(
            &mut clients,
            &mut hands,
            dealer_position,
            round_index,
            rules,
        )
        .await;
        completed_rounds += 1;
        assert_eq!(
            settlement_snapshot["data"]["round_index"],
            json!(round_index)
        );
        assert_eq!(settlement.target_rank as i32, rules.target_rank as i32);
        assert_eq!(settlement.team_target_ranks.len(), 2);
        if settlement.match_finished {
            final_settlement = Some(settlement);
            break;
        }

        assert!(settlement.next_target_rank.is_some());
        let winners = settlement
            .winner_positions
            .iter()
            .map(|position| *position as usize)
            .collect::<Vec<_>>();
        if !winners.contains(&dealer_position) {
            dealer_position = winners[0];
        }
    }

    let final_settlement =
        final_settlement.expect("compact upgrade match must finish by round ten");
    assert!((2..=10).contains(&completed_rounds));
    assert_eq!(final_settlement.target_rank, UpgradeRank::A);
    assert_eq!(final_settlement.next_target_rank, None);
    assert!(final_settlement.team_target_ranks.contains(&UpgradeRank::A));

    let unexpected_next_deal = tokio::time::timeout(
        Duration::from_secs(4),
        wait_for_event(&mut *clients[0], WsCode::DEAL as i32),
    )
    .await;
    assert!(
        unexpected_next_deal.is_err(),
        "finished upgrade match must not deal another round"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn upgrade_ws_rejoin_preserves_running_bury_state() {
    let runtime = start_test_runtime(
        "upgrade-rejoin-bury-test",
        Duration::from_secs(30),
        TestUpgradeHandler::default(),
    )
    .await;
    let url = runtime.url.clone();

    let mut a = connect_client(&url).await;
    let mut b = connect_client(&url).await;
    let mut c = connect_client(&url).await;
    let mut d = connect_client(&url).await;
    let room = "upgrade-rejoin-bury-room";
    for (position, client) in [&mut a, &mut b, &mut c, &mut d].into_iter().enumerate() {
        let joined = join_as(
            client,
            GameId::UPGRADE,
            room,
            &format!("rejoin-player-{position}"),
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
                "deck_count": 0,
                "removed_rank_count": 0,
                "first_deal_time": 1,
                "deal_time": 1,
                "play_time": 3
            }
        }),
    )
    .await;
    assert_eq!(
        wait_for_response(&mut a, Routes::SETTING as i32).await["code"],
        json!(WsResponseCode::OK as i32)
    );
    send_request(&mut a, Routes::START as i32, Value::Null).await;
    assert_eq!(
        wait_for_response(&mut a, Routes::START as i32).await["code"],
        json!(WsResponseCode::OK as i32)
    );

    let mut clients: [&mut Client; 4] = [&mut a, &mut b, &mut c, &mut d];
    let declaration = wait_for_event(&mut *clients[0], UpgradeWsCode::TRUMP_DECLARED as i32).await;
    let dealer_position = declaration["data"]["position"]
        .as_i64()
        .expect("upgrade rejoin dealer") as usize;
    let hands = collect_upgrade_hands(&mut clients).await;
    let bottom = recv_upgrade_bottom(&mut *clients[dealer_position], dealer_position).await;
    let bottom_cards = bottom["data"]["cards"]
        .as_array()
        .expect("upgrade rejoin bottom")
        .iter()
        .map(|card| card.as_i64().expect("upgrade rejoin bottom card") as i32)
        .collect::<Vec<_>>();
    assert_eq!(bottom_cards.len(), 10);

    // Reconnect while the single combined select/bury window is still open.
    // The replacement must recover the same seat and the exact authoritative
    // hand rather than starting a new room or receiving a fresh position.
    clients[dealer_position]
        .close(None)
        .await
        .expect("close upgrade dealer socket");
    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut rejoined = connect_client(&url).await;
    let dealer_name = format!("rejoin-player-{dealer_position}");
    let joined = join_as(&mut rejoined, GameId::UPGRADE, room, &dealer_name).await;
    assert_eq!(joined["data"]["self_position"], json!(dealer_position));
    let hand_update = wait_for_event(&mut rejoined, UpgradeWsCode::HAND_UPDATED as i32).await;
    let restored_hand = hand_update["data"]["cards"]
        .as_array()
        .expect("upgrade rejoined hand")
        .iter()
        .map(|card| card.as_i64().expect("upgrade rejoined card") as i32)
        .collect::<Vec<_>>();
    let mut restored_hand_sorted = restored_hand;
    let mut expected_hand_sorted = hands[dealer_position].clone();
    restored_hand_sorted.sort_unstable();
    expected_hand_sorted.sort_unstable();
    assert_eq!(restored_hand_sorted, expected_hand_sorted);
    let snapshot = wait_for_phase(&mut rejoined, UpgradePhase::Bury).await;
    assert_eq!(snapshot["data"]["round_index"], json!(0));
    assert_eq!(snapshot["data"]["bottom_card_count"], json!(10));
    assert_eq!(snapshot["data"]["turn_countdown"], json!(9));

    send_request(
        &mut rejoined,
        UpgradeRoutes::BURY_BOTTOM as i32,
        json!({ "cards": bottom_cards }),
    )
    .await;
    let play_snapshot = wait_for_phase(&mut rejoined, UpgradePhase::Play).await;
    assert_eq!(play_snapshot["data"]["round_index"], json!(0));
    assert_eq!(play_snapshot["data"]["turn_countdown"], json!(3));
    assert_eq!(
        wait_for_response(&mut rejoined, UpgradeRoutes::BURY_BOTTOM as i32).await["code"],
        json!(WsResponseCode::OK as i32)
    );

    let mut dealer_hand = hands[dealer_position].clone();
    for card in &bottom_cards {
        let index = dealer_hand
            .iter()
            .position(|candidate| candidate == card)
            .expect("upgrade buried card in dealer hand");
        dealer_hand.remove(index);
    }
    let lead_card = *dealer_hand.first().expect("upgrade dealer lead card");
    send_request(
        &mut rejoined,
        Routes::PLAY as i32,
        json!({ "cards": [lead_card] }),
    )
    .await;
    let lead = wait_upgrade_play(&mut rejoined, dealer_position).await;
    assert_eq!(lead["data"]["cards"], json!([lead_card]));
    let next_position = (dealer_position + 1) % 4;
    let lead_snapshot = wait_upgrade_snapshot(&mut rejoined, UpgradePhase::Play, 0).await;
    assert_eq!(
        lead_snapshot["data"]["current_position"],
        json!(next_position)
    );
    assert_eq!(
        wait_for_response(&mut rejoined, Routes::PLAY as i32).await["code"],
        json!(WsResponseCode::OK as i32)
    );

    clients[next_position]
        .close(None)
        .await
        .expect("close upgrade current player socket");
    tokio::time::sleep(Duration::from_millis(1_200)).await;

    let mut play_rejoined = connect_client(&url).await;
    let next_name = format!("rejoin-player-{next_position}");
    let joined = join_as(&mut play_rejoined, GameId::UPGRADE, room, &next_name).await;
    assert_eq!(joined["data"]["self_position"], json!(next_position));
    let restored_hand_event =
        wait_for_event(&mut play_rejoined, UpgradeWsCode::HAND_UPDATED as i32).await;
    let mut restored_hand = restored_hand_event["data"]["cards"]
        .as_array()
        .expect("upgrade play rejoined hand")
        .iter()
        .map(|card| card.as_i64().expect("upgrade play rejoined card") as i32)
        .collect::<Vec<_>>();
    let mut expected_hand = hands[next_position].clone();
    restored_hand.sort_unstable();
    expected_hand.sort_unstable();
    assert_eq!(restored_hand, expected_hand);
    let rejoined_snapshot = wait_for_phase(&mut play_rejoined, UpgradePhase::Play).await;
    assert_eq!(
        rejoined_snapshot["data"]["current_position"],
        json!(next_position)
    );
    assert_eq!(rejoined_snapshot["data"]["turn_countdown"], json!(3));

    let rules = combo::UpgradeComboRules {
        target_rank: upgrade_rank(
            rejoined_snapshot["data"]["target_rank"]
                .as_i64()
                .expect("upgrade rejoined target rank"),
        ),
        trump_suit: Some(upgrade_suit(
            rejoined_snapshot["data"]["trump_suit"]
                .as_i64()
                .expect("upgrade rejoined trump suit"),
        )),
    };
    let hand_cards = expected_hand
        .iter()
        .copied()
        .map(|card| Card::try_from(card).expect("upgrade rejoined hand card"))
        .collect::<Vec<_>>();
    let lead_combo = combo::classify(
        &[Card::try_from(lead_card).expect("upgrade rejoined lead")],
        rules,
    )
    .expect("upgrade rejoined lead combo");
    let legal_follow = expected_hand
        .iter()
        .copied()
        .find(|card| {
            combo::follow_is_legal(
                &hand_cards,
                &[Card::try_from(*card).expect("upgrade legal follow card")],
                &lead_combo,
                rules,
            )
        })
        .expect("upgrade rejoined legal follow");
    send_request(
        &mut play_rejoined,
        Routes::PLAY as i32,
        json!({ "cards": [legal_follow] }),
    )
    .await;
    let followed = wait_upgrade_play(&mut play_rejoined, next_position).await;
    assert_eq!(followed["data"]["cards"], json!([legal_follow]));
    assert_eq!(
        wait_for_response(&mut play_rejoined, Routes::PLAY as i32).await["code"],
        json!(WsResponseCode::OK as i32)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn upgrade_five_deck_ws_deals_and_buries_the_correct_counts() {
    assert_upgrade_deck_ws_deals_and_buries(2, 5, 10, 65).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn upgrade_six_deck_ws_deals_and_buries_the_correct_counts() {
    assert_upgrade_deck_ws_deals_and_buries(3, 6, 8, 79).await;
}

async fn assert_upgrade_deck_ws_deals_and_buries(
    deck_setting: i32,
    expected_deck_count: usize,
    expected_bottom_count: usize,
    expected_hand_count: usize,
) {
    let runtime = start_test_runtime(
        "upgrade-multi-deck-test",
        Duration::from_secs(45),
        TestUpgradeHandler::default(),
    )
    .await;
    let url = runtime.url.clone();

    let mut a = connect_client(&url).await;
    let mut b = connect_client(&url).await;
    let mut c = connect_client(&url).await;
    let mut d = connect_client(&url).await;
    let room = "upgrade-multi-deck-room";
    for (position, client) in [&mut a, &mut b, &mut c, &mut d].into_iter().enumerate() {
        let joined = join_as(
            client,
            GameId::UPGRADE,
            room,
            &format!("multi-deck-player-{position}"),
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
                "deck_count": deck_setting,
                "removed_rank_count": 0,
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
    assert_eq!(
        wait_for_response(&mut a, Routes::SETTING as i32).await["code"],
        json!(WsResponseCode::OK as i32)
    );
    send_request(&mut a, Routes::START as i32, Value::Null).await;
    assert_eq!(
        wait_for_response(&mut a, Routes::START as i32).await["code"],
        json!(WsResponseCode::OK as i32)
    );

    let mut clients: [&mut Client; 4] = [&mut a, &mut b, &mut c, &mut d];
    let declaration = wait_for_event(&mut *clients[0], UpgradeWsCode::TRUMP_DECLARED as i32).await;
    let dealer_position = declaration["data"]["position"]
        .as_i64()
        .expect("multi-deck dealer") as usize;
    assert!(dealer_position < 4);
    let hands = collect_upgrade_hands(&mut clients).await;
    assert_eq!(
        hands[dealer_position].len(),
        expected_hand_count + expected_bottom_count
    );
    assert!(
        hands.iter().enumerate().all(
            |(position, hand)| position == dealer_position || hand.len() == expected_hand_count
        )
    );
    let bottom_event = recv_upgrade_bottom(&mut *clients[dealer_position], dealer_position).await;
    let bottom_cards = bottom_event["data"]["cards"]
        .as_array()
        .expect("multi-deck bottom cards")
        .iter()
        .map(|card| card.as_i64().expect("multi-deck bottom card") as i32)
        .collect::<Vec<_>>();
    assert_eq!(bottom_cards.len(), expected_bottom_count);
    assert_eq!(
        bottom_event["data"]["required_count"],
        json!(expected_bottom_count)
    );

    send_request(
        &mut *clients[dealer_position],
        UpgradeRoutes::BURY_BOTTOM as i32,
        json!({ "cards": &bottom_cards[..expected_bottom_count - 1] }),
    )
    .await;
    assert_eq!(
        wait_for_response(
            &mut *clients[dealer_position],
            UpgradeRoutes::BURY_BOTTOM as i32
        )
        .await["code"],
        json!(WsResponseCode::NO_PERMISSION as i32)
    );
    send_request(
        &mut *clients[dealer_position],
        UpgradeRoutes::BURY_BOTTOM as i32,
        json!({ "cards": bottom_cards }),
    )
    .await;
    let snapshot =
        wait_upgrade_snapshot(&mut *clients[dealer_position], UpgradePhase::Play, 0).await;
    let expected_dealt_count = expected_hand_count * 4;
    assert_eq!(snapshot["data"]["deck_count"], json!(expected_deck_count));
    assert_eq!(
        snapshot["data"]["bottom_card_count"],
        json!(expected_bottom_count)
    );
    assert_eq!(snapshot["data"]["hand_count"], json!(expected_hand_count));
    assert_eq!(snapshot["data"]["dealt_count"], json!(expected_dealt_count));
    assert_eq!(
        snapshot["data"]["total_deal_count"],
        json!(expected_dealt_count)
    );
    assert_eq!(
        snapshot["data"]["player_hand_counts"]
            .as_array()
            .expect("multi-deck hand counts")
            .iter()
            .map(|entry| entry["hand_count"].as_i64().expect("hand count"))
            .sum::<i64>(),
        expected_dealt_count as i64
    );
    assert_eq!(
        wait_for_response(
            &mut *clients[dealer_position],
            UpgradeRoutes::BURY_BOTTOM as i32
        )
        .await["code"],
        json!(WsResponseCode::OK as i32)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn upgrade_four_deck_removed_ranks_ws_flow_skips_removed_levels() {
    let runtime = start_test_runtime(
        "upgrade-four-deck-removed-ranks-test",
        Duration::from_secs(45),
        TestUpgradeHandler::default(),
    )
    .await;
    let url = runtime.url.clone();

    let mut a = connect_client(&url).await;
    let mut b = connect_client(&url).await;
    let mut c = connect_client(&url).await;
    let mut d = connect_client(&url).await;
    let room = "upgrade-four-deck-removed-ranks-room";
    for (position, client) in [&mut a, &mut b, &mut c, &mut d].into_iter().enumerate() {
        let joined = join_as(
            client,
            GameId::UPGRADE,
            room,
            &format!("removed-ranks-player-{position}"),
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
                "removed_rank_count": 2,
                "first_deal_time": 1,
                "deal_time": 1,
                "play_time": 30
            }
        }),
    )
    .await;
    assert_eq!(
        wait_for_response(&mut a, Routes::SETTING as i32).await["code"],
        json!(WsResponseCode::OK as i32)
    );

    send_request(&mut a, Routes::START as i32, Value::Null).await;
    assert_eq!(
        wait_for_response(&mut a, Routes::START as i32).await["code"],
        json!(WsResponseCode::OK as i32)
    );

    let mut clients: [&mut Client; 4] = [&mut a, &mut b, &mut c, &mut d];
    let declaration = wait_for_event(&mut *clients[0], UpgradeWsCode::TRUMP_DECLARED as i32).await;
    assert_eq!(declaration["data"]["target_rank"], json!(5));
    let dealer_position = declaration["data"]["position"]
        .as_i64()
        .expect("removed-ranks dealer") as usize;
    assert!(dealer_position < 4);

    let hands = collect_upgrade_hands_min(&mut clients, 44).await;
    assert_eq!(hands[dealer_position].len(), 52);
    assert!(
        hands
            .iter()
            .enumerate()
            .all(|(position, hand)| position == dealer_position || hand.len() == 44)
    );
    assert!(hands.iter().flatten().all(|card| {
        let rank = Card::try_from(*card).expect("removed-ranks card").rank();
        !matches!(rank, Rank::Three | Rank::Four)
    }));

    let bottom_event = recv_upgrade_bottom(&mut *clients[dealer_position], dealer_position).await;
    let bottom_cards = bottom_event["data"]["cards"]
        .as_array()
        .expect("removed-ranks bottom cards")
        .iter()
        .map(|card| card.as_i64().expect("removed-ranks bottom card") as i32)
        .collect::<Vec<_>>();
    assert_eq!(bottom_cards.len(), 8);
    assert_eq!(bottom_event["data"]["required_count"], json!(8));

    send_request(
        &mut *clients[dealer_position],
        UpgradeRoutes::BURY_BOTTOM as i32,
        json!({ "cards": bottom_cards }),
    )
    .await;
    let snapshot =
        wait_upgrade_snapshot(&mut *clients[dealer_position], UpgradePhase::Play, 0).await;
    assert_eq!(snapshot["data"]["deck_count"], json!(4));
    assert_eq!(snapshot["data"]["removed_rank_count"], json!(2));
    assert_eq!(snapshot["data"]["target_rank"], json!(5));
    assert_eq!(snapshot["data"]["bottom_card_count"], json!(8));
    assert_eq!(snapshot["data"]["hand_count"], json!(44));
    assert_eq!(snapshot["data"]["dealt_count"], json!(176));
    assert_eq!(snapshot["data"]["total_deal_count"], json!(176));
    assert_eq!(
        wait_for_response(
            &mut *clients[dealer_position],
            UpgradeRoutes::BURY_BOTTOM as i32
        )
        .await["code"],
        json!(WsResponseCode::OK as i32)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn upgrade_ws_auto_buries_after_three_play_windows() {
    let runtime = start_test_runtime(
        "upgrade-auto-bury-window-test",
        Duration::from_secs(30),
        TestUpgradeHandler::default(),
    )
    .await;
    let url = runtime.url.clone();

    let mut a = connect_client(&url).await;
    let mut b = connect_client(&url).await;
    let mut c = connect_client(&url).await;
    let mut d = connect_client(&url).await;
    let room = "upgrade-auto-bury-window-room";
    for (position, client) in [&mut a, &mut b, &mut c, &mut d].into_iter().enumerate() {
        let joined = join_as(
            client,
            GameId::UPGRADE,
            room,
            &format!("auto-bury-player-{position}"),
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
                "deck_count": 0,
                "first_deal_time": 1,
                "deal_time": 1,
                "play_time": 1
            }
        }),
    )
    .await;
    assert_eq!(
        wait_for_response(&mut a, Routes::SETTING as i32).await["code"],
        json!(WsResponseCode::OK as i32)
    );
    send_request(&mut a, Routes::START as i32, Value::Null).await;
    assert_eq!(
        wait_for_response(&mut a, Routes::START as i32).await["code"],
        json!(WsResponseCode::OK as i32)
    );

    let clients: [&mut Client; 4] = [&mut a, &mut b, &mut c, &mut d];
    let declaration = wait_for_event(&mut *clients[0], UpgradeWsCode::TRUMP_DECLARED as i32).await;
    let dealer_position = declaration["data"]["position"]
        .as_i64()
        .expect("upgrade auto-bury dealer") as usize;
    assert!(dealer_position < 4);
    let bottom = wait_for_event(
        &mut *clients[dealer_position],
        UpgradeWsCode::BOTTOM_CARDS as i32,
    )
    .await;
    assert_eq!(bottom["data"]["required_count"], json!(10));

    let bury_snapshot = loop {
        let value = recv_json_full(&mut *clients[dealer_position], "upgrade bury countdown").await;
        if value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
            && value["data"]["phase"] == json!(UpgradePhase::Bury as i8)
            && value["data"]["turn_countdown"] == json!(3)
        {
            break value;
        }
    };
    assert_eq!(bury_snapshot["data"]["turn_countdown"], json!(3));

    let buried = wait_for_event(
        &mut *clients[dealer_position],
        UpgradeWsCode::BOTTOM_BURIED as i32,
    )
    .await;
    assert_eq!(buried["data"]["position"], json!(dealer_position));
    assert_eq!(buried["data"]["bottom_card_count"], json!(10));
    let play_snapshot = wait_for_phase(&mut *clients[dealer_position], UpgradePhase::Play).await;
    assert_eq!(play_snapshot["data"]["round_index"], json!(0));
    assert_eq!(play_snapshot["data"]["turn_countdown"], json!(1));

    // The first play window must also be server-driven: after the same
    // timeout contract expires, the dealer's legal opening single is played
    // and the table advances instead of remaining stuck in Play.
    let auto_play = wait_for_event(&mut *clients[dealer_position], WsCode::PLAY as i32).await;
    assert_eq!(auto_play["data"]["position"], json!(dealer_position));
    assert_eq!(auto_play["data"]["cards"].as_array().map(Vec::len), Some(1));
    assert_eq!(auto_play["data"]["remaining_hand_count"], json!(37));
    let after_auto_play = wait_for_phase(&mut *clients[dealer_position], UpgradePhase::Play).await;
    assert_eq!(
        after_auto_play["data"]["current_position"],
        json!((dealer_position + 1) % 4)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn upgrade_ws_rejects_an_off_group_follow_and_accepts_the_next_legal_card() {
    let runtime = start_test_runtime(
        "upgrade-illegal-follow-test",
        Duration::from_secs(30),
        TestUpgradeHandler::default(),
    )
    .await;
    let url = runtime.url.clone();

    let mut a = connect_client(&url).await;
    let mut b = connect_client(&url).await;
    let mut c = connect_client(&url).await;
    let mut d = connect_client(&url).await;
    let room = "upgrade-illegal-follow-room";
    for (position, client) in [&mut a, &mut b, &mut c, &mut d].into_iter().enumerate() {
        let joined = join_as(
            client,
            GameId::UPGRADE,
            room,
            &format!("illegal-follow-player-{position}"),
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
                "deck_count": 0,
                "first_deal_time": 1,
                "deal_time": 1,
                "play_time": 3
            }
        }),
    )
    .await;
    assert_eq!(
        wait_for_response(&mut a, Routes::SETTING as i32).await["code"],
        json!(WsResponseCode::OK as i32)
    );
    send_request(&mut a, Routes::START as i32, Value::Null).await;
    assert_eq!(
        wait_for_response(&mut a, Routes::START as i32).await["code"],
        json!(WsResponseCode::OK as i32)
    );

    let mut clients: [&mut Client; 4] = [&mut a, &mut b, &mut c, &mut d];
    let declaration = wait_for_event(&mut *clients[0], UpgradeWsCode::TRUMP_DECLARED as i32).await;
    let dealer_position = declaration["data"]["position"]
        .as_i64()
        .expect("upgrade illegal-follow dealer") as usize;
    let mut hands = collect_upgrade_hands(&mut clients).await;
    let bottom = recv_upgrade_bottom(&mut *clients[dealer_position], dealer_position).await;
    let bottom_cards = bottom["data"]["cards"]
        .as_array()
        .expect("upgrade illegal-follow bottom")
        .iter()
        .map(|card| card.as_i64().expect("upgrade bottom card") as i32)
        .collect::<Vec<_>>();
    assert_eq!(bottom_cards.len(), 10);
    for card in &bottom_cards {
        let index = hands[dealer_position]
            .iter()
            .position(|candidate| candidate == card)
            .expect("upgrade bottom card in dealer hand");
        hands[dealer_position].remove(index);
    }

    send_request(
        &mut *clients[dealer_position],
        UpgradeRoutes::BURY_BOTTOM as i32,
        json!({ "cards": bottom_cards }),
    )
    .await;
    let snapshot = wait_for_phase(&mut *clients[dealer_position], UpgradePhase::Play).await;
    assert_eq!(
        wait_for_response(
            &mut *clients[dealer_position],
            UpgradeRoutes::BURY_BOTTOM as i32
        )
        .await["code"],
        json!(WsResponseCode::OK as i32)
    );
    let rules = combo::UpgradeComboRules {
        target_rank: upgrade_rank(snapshot["data"]["target_rank"].as_i64().unwrap()),
        trump_suit: Some(upgrade_suit(
            snapshot["data"]["trump_suit"].as_i64().unwrap(),
        )),
    };
    let follower_position = (dealer_position + 1) % 4;
    let mut case = None;
    for lead_card in hands[dealer_position].iter().copied() {
        let lead = Card::try_from(lead_card).expect("upgrade lead card");
        let lead_group = combo::card_group(lead, rules);
        let legal = hands[follower_position].iter().copied().find(|card| {
            combo::card_group(Card::try_from(*card).expect("upgrade legal card"), rules)
                == lead_group
        });
        let illegal = hands[follower_position].iter().copied().find(|card| {
            combo::card_group(Card::try_from(*card).expect("upgrade illegal card"), rules)
                != lead_group
        });
        if let (Some(legal), Some(illegal)) = (legal, illegal) {
            case = Some((lead_card, legal, illegal));
            break;
        }
    }
    let (lead_card, legal_card, illegal_card) =
        case.expect("upgrade deal should expose an off-group follow case");

    send_request(
        &mut *clients[dealer_position],
        Routes::PLAY as i32,
        json!({ "cards": [lead_card] }),
    )
    .await;
    let lead_event = wait_upgrade_play(&mut *clients[dealer_position], dealer_position).await;
    assert_eq!(lead_event["data"]["cards"], json!([lead_card]));
    assert_eq!(
        wait_for_response(&mut *clients[dealer_position], Routes::PLAY as i32).await["code"],
        json!(WsResponseCode::OK as i32)
    );

    send_request(
        &mut *clients[follower_position],
        Routes::PLAY as i32,
        json!({ "cards": [illegal_card] }),
    )
    .await;
    assert_eq!(
        wait_for_response(&mut *clients[follower_position], Routes::PLAY as i32).await["code"],
        json!(WsResponseCode::NO_PERMISSION as i32)
    );

    send_request(
        &mut *clients[follower_position],
        Routes::PLAY as i32,
        json!({ "cards": [legal_card] }),
    )
    .await;
    let legal_event = wait_upgrade_play(&mut *clients[follower_position], follower_position).await;
    assert_eq!(legal_event["data"]["cards"], json!([legal_card]));
    assert_eq!(
        wait_for_response(&mut *clients[follower_position], Routes::PLAY as i32).await["code"],
        json!(WsResponseCode::OK as i32)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn upgrade_ws_failed_throw_reports_attempted_and_played_components() {
    let runtime = start_test_runtime(
        "upgrade-failed-throw-test",
        Duration::from_secs(30),
        TestUpgradeHandler::default(),
    )
    .await;
    let url = runtime.url.clone();

    let mut a = connect_client(&url).await;
    let mut b = connect_client(&url).await;
    let mut c = connect_client(&url).await;
    let mut d = connect_client(&url).await;
    let room = "upgrade-failed-throw-room";
    for (position, client) in [&mut a, &mut b, &mut c, &mut d].into_iter().enumerate() {
        let joined = join_as(
            client,
            GameId::UPGRADE,
            room,
            &format!("failed-throw-player-{position}"),
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
                "deck_count": 0,
                "first_deal_time": 1,
                "deal_time": 1,
                "play_time": 3
            }
        }),
    )
    .await;
    assert_eq!(
        wait_for_response(&mut a, Routes::SETTING as i32).await["code"],
        json!(WsResponseCode::OK as i32)
    );
    send_request(&mut a, Routes::START as i32, Value::Null).await;
    assert_eq!(
        wait_for_response(&mut a, Routes::START as i32).await["code"],
        json!(WsResponseCode::OK as i32)
    );

    let mut clients: [&mut Client; 4] = [&mut a, &mut b, &mut c, &mut d];
    let declaration = wait_for_event(&mut *clients[0], UpgradeWsCode::TRUMP_DECLARED as i32).await;
    let dealer_position = declaration["data"]["position"]
        .as_i64()
        .expect("upgrade failed-throw dealer") as usize;
    let mut hands = collect_upgrade_hands(&mut clients).await;
    let bottom = recv_upgrade_bottom(&mut *clients[dealer_position], dealer_position).await;
    let bottom_cards = bottom["data"]["cards"]
        .as_array()
        .expect("upgrade failed-throw bottom")
        .iter()
        .map(|card| card.as_i64().expect("upgrade failed-throw card") as i32)
        .collect::<Vec<_>>();
    assert_eq!(bottom_cards.len(), 10);
    for card in &bottom_cards {
        let index = hands[dealer_position]
            .iter()
            .position(|candidate| candidate == card)
            .expect("failed-throw bottom card in dealer hand");
        hands[dealer_position].remove(index);
    }

    send_request(
        &mut *clients[dealer_position],
        UpgradeRoutes::BURY_BOTTOM as i32,
        json!({ "cards": bottom_cards }),
    )
    .await;
    let snapshot = wait_for_phase(&mut *clients[dealer_position], UpgradePhase::Play).await;
    assert_eq!(
        wait_for_response(
            &mut *clients[dealer_position],
            UpgradeRoutes::BURY_BOTTOM as i32,
        )
        .await["code"],
        json!(WsResponseCode::OK as i32)
    );
    let rules = combo::UpgradeComboRules {
        target_rank: upgrade_rank(snapshot["data"]["target_rank"].as_i64().unwrap()),
        trump_suit: Some(upgrade_suit(
            snapshot["data"]["trump_suit"].as_i64().unwrap(),
        )),
    };
    let (attempted, expected_played) =
        find_failed_upgrade_throw_candidate(&hands, dealer_position, rules)
            .expect("three-deck upgrade deal should expose a beatable throw");
    assert!(expected_played.len() < attempted.len());

    send_request(
        &mut *clients[dealer_position],
        Routes::PLAY as i32,
        json!({ "cards": attempted.clone() }),
    )
    .await;
    let played = wait_upgrade_play(&mut *clients[dealer_position], dealer_position).await;
    assert_eq!(played["data"]["cards"], json!(expected_played.clone()));
    assert_eq!(
        played["data"]["failed_throw"]["attempted_cards"],
        json!(attempted.clone())
    );
    assert_eq!(
        played["data"]["failed_throw"]["played_cards"],
        json!(expected_played.clone())
    );
    let failed_snapshot = loop {
        let value = recv_json_full(
            &mut *clients[dealer_position],
            "upgrade failed throw snapshot",
        )
        .await;
        if value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
            && value["data"]["failed_throws"]
                .as_array()
                .is_some_and(|items| !items.is_empty())
        {
            break value;
        }
    };
    assert_eq!(
        failed_snapshot["data"]["failed_throws"][0]["attempted_cards"],
        json!(attempted.clone())
    );
    assert_eq!(
        failed_snapshot["data"]["failed_throws"][0]["played_cards"],
        json!(expected_played.clone())
    );
    assert_eq!(
        wait_for_response(&mut *clients[dealer_position], Routes::PLAY as i32).await["code"],
        json!(WsResponseCode::OK as i32)
    );

    hands[dealer_position].retain(|card| !expected_played.contains(card));
    assert_eq!(hands[dealer_position].len(), 38 - expected_played.len());
}
