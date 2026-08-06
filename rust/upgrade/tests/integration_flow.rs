use std::{net::TcpListener, sync::Arc, time::Duration};

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use share_type_public::{
    GameId, GameParam, GameParamRange, Routes, UpgradePhase, UpgradeRoutes, UpgradeWsCode, WsCode,
    WsResponseCode, WsUpgradePlayedCards, WsUpgradeSettlementEvent,
};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use upgrade::combo;
use upgrade::game::UpgradeGameHandler;
use upgrade_common::{Card, Rank, ScoreProgression};
use ws_common::{
    ClientRequest, Dispatch, GameHandler, GameState, JoinAuthorization, JoinAuthorizationFuture,
    RoomService, RuntimeConfig, SessionId, SessionSenders, SettingsBuilderResult, run_room_runtime,
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

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind free port")
        .local_addr()
        .expect("local addr")
        .port()
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
        let frame = tokio::time::timeout(Duration::from_secs(5), client.next())
            .await
            .expect("join timeout")
            .expect("join frame")
            .expect("valid join frame");
        if let Message::Text(text) = frame {
            let value: Value = serde_json::from_str(text.as_ref()).expect("json frame");
            if value.get("route").and_then(Value::as_i64) == Some(Routes::JOIN as i64) {
                return value;
            }
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
        let frame = tokio::time::timeout(Duration::from_secs(5), client.next())
            .await
            .expect("response timeout")
            .expect("response frame")
            .expect("valid response frame");
        if let Message::Text(text) = frame {
            let value: Value = serde_json::from_str(text.as_ref()).expect("json response");
            if value.get("route").and_then(Value::as_i64) == Some(i64::from(route)) {
                return value;
            }
        }
    }
}

async fn wait_for_event(client: &mut Client, code: i32) -> Value {
    loop {
        let frame = tokio::time::timeout(Duration::from_secs(25), client.next())
            .await
            .expect("event timeout")
            .expect("event frame")
            .expect("valid event frame");
        if let Message::Text(text) = frame {
            let value: Value = serde_json::from_str(text.as_ref()).expect("json event");
            if value.get("code").and_then(Value::as_i64) == Some(i64::from(code)) {
                return value;
            }
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

async fn recv_json_full(client: &mut Client, label: &str) -> Value {
    loop {
        let frame = tokio::time::timeout(Duration::from_secs(60), client.next())
            .await
            .unwrap_or_else(|_| panic!("upgrade websocket timeout while waiting for {label}"))
            .expect("upgrade websocket frame")
            .expect("upgrade websocket frame ok");
        if let Message::Text(text) = frame {
            return serde_json::from_str(text.as_ref()).expect("upgrade json frame");
        }
    }
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn upgrade_server_accepts_only_its_own_game_id() {
    let port = free_port();
    let listen_addr = format!("127.0.0.1:{port}");
    let url = format!("ws://{listen_addr}");
    let server = tokio::spawn(run_room_runtime(
        RuntimeConfig {
            service_name: "upgrade-integration-test",
            listen_addr,
            idle_timeout: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(30),
        },
        UpgradeGameHandler::default(),
    ));

    let mut wrong_client = connect_client(&url).await;
    let wrong = join(&mut wrong_client, GameId::TRACTOR, "wrong-upgrade-room").await;
    assert_eq!(wrong["code"], json!(WsResponseCode::WRONG_GAME as i32));

    let mut upgrade_client = connect_client(&url).await;
    let accepted = join(&mut upgrade_client, GameId::UPGRADE, "upgrade-room").await;
    assert_eq!(accepted["code"], json!(WsResponseCode::JOINED as i32));
    assert_eq!(accepted["data"]["self_position"], json!(0));
    assert_eq!(accepted["data"]["current_configs"]["deck_count"], json!(0));
    assert_eq!(accepted["data"]["current_configs"]["play_time"], json!(30));

    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn four_players_can_deal_bury_and_play_first_round() {
    use share_type_public::{UpgradePhase, UpgradeRoutes, UpgradeWsCode};

    let port = free_port();
    let listen_addr = format!("127.0.0.1:{port}");
    let url = format!("ws://{listen_addr}");
    let server = tokio::spawn(run_room_runtime(
        RuntimeConfig {
            service_name: "upgrade-round-integration-test",
            listen_addr,
            idle_timeout: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(30),
        },
        UpgradeGameHandler::default(),
    ));

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

    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn upgrade_server_completes_round_and_enters_later_round() {
    let port = free_port();
    let listen_addr = format!("127.0.0.1:{port}");
    let url = format!("ws://{listen_addr}");
    let server = tokio::spawn(run_room_runtime(
        RuntimeConfig {
            service_name: "upgrade-full-round-test",
            listen_addr,
            idle_timeout: Duration::from_secs(90),
            heartbeat_interval: Duration::from_secs(90),
        },
        TestUpgradeHandler::default(),
    ));

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
                let expected_player_score = if expected_winners.contains(&(position as i32)) {
                    expected_score
                } else {
                    -expected_score
                };
                assert_eq!(
                    settlement.player_scores.get(&(position as i32)).copied(),
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

    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn upgrade_ws_rejoin_preserves_running_bury_state() {
    let port = free_port();
    let listen_addr = format!("127.0.0.1:{port}");
    let url = format!("ws://{listen_addr}");
    let server = tokio::spawn(run_room_runtime(
        RuntimeConfig {
            service_name: "upgrade-rejoin-bury-test",
            listen_addr,
            idle_timeout: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(30),
        },
        TestUpgradeHandler::default(),
    ));

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
    clients[0]
        .close(None)
        .await
        .expect("close upgrade player socket");
    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut rejoined = connect_client(&url).await;
    let joined = join_as(&mut rejoined, GameId::UPGRADE, room, "rejoin-player-0").await;
    assert_eq!(joined["data"]["self_position"], json!(0));
    let hand_update = wait_for_event(&mut rejoined, UpgradeWsCode::HAND_UPDATED as i32).await;
    let restored_hand = hand_update["data"]["cards"]
        .as_array()
        .expect("upgrade rejoined hand")
        .iter()
        .map(|card| card.as_i64().expect("upgrade rejoined card") as i32)
        .collect::<Vec<_>>();
    let mut restored_hand_sorted = restored_hand;
    let mut expected_hand_sorted = hands[0].clone();
    restored_hand_sorted.sort_unstable();
    expected_hand_sorted.sort_unstable();
    assert_eq!(restored_hand_sorted, expected_hand_sorted);
    let snapshot = wait_for_phase(&mut rejoined, UpgradePhase::Bury).await;
    assert_eq!(snapshot["data"]["round_index"], json!(0));
    assert_eq!(snapshot["data"]["bottom_card_count"], json!(10));

    if dealer_position == 0 {
        send_request(
            &mut rejoined,
            UpgradeRoutes::BURY_BOTTOM as i32,
            json!({ "cards": bottom_cards }),
        )
        .await;
        assert_eq!(
            wait_for_phase(&mut rejoined, UpgradePhase::Play).await["data"]["round_index"],
            json!(0)
        );
        assert_eq!(
            wait_for_response(&mut rejoined, UpgradeRoutes::BURY_BOTTOM as i32).await["code"],
            json!(WsResponseCode::OK as i32)
        );
    }

    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn upgrade_six_deck_ws_deals_and_buries_the_correct_counts() {
    let port = free_port();
    let listen_addr = format!("127.0.0.1:{port}");
    let url = format!("ws://{listen_addr}");
    let server = tokio::spawn(run_room_runtime(
        RuntimeConfig {
            service_name: "upgrade-six-deck-test",
            listen_addr,
            idle_timeout: Duration::from_secs(45),
            heartbeat_interval: Duration::from_secs(45),
        },
        TestUpgradeHandler::default(),
    ));

    let mut a = connect_client(&url).await;
    let mut b = connect_client(&url).await;
    let mut c = connect_client(&url).await;
    let mut d = connect_client(&url).await;
    let room = "upgrade-six-deck-room";
    for (position, client) in [&mut a, &mut b, &mut c, &mut d].into_iter().enumerate() {
        let joined = join_as(
            client,
            GameId::UPGRADE,
            room,
            &format!("six-deck-player-{position}"),
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
        .expect("six-deck dealer") as usize;
    assert!(dealer_position < 4);
    let hands = collect_upgrade_hands(&mut clients).await;
    assert_eq!(hands[dealer_position].len(), 87);
    assert!(
        hands
            .iter()
            .enumerate()
            .all(|(position, hand)| position == dealer_position || hand.len() == 79)
    );
    let bottom_event = recv_upgrade_bottom(&mut *clients[dealer_position], dealer_position).await;
    let bottom_cards = bottom_event["data"]["cards"]
        .as_array()
        .expect("six-deck bottom cards")
        .iter()
        .map(|card| card.as_i64().expect("six-deck bottom card") as i32)
        .collect::<Vec<_>>();
    assert_eq!(bottom_cards.len(), 8);
    assert_eq!(bottom_event["data"]["required_count"], json!(8));

    send_request(
        &mut *clients[dealer_position],
        UpgradeRoutes::BURY_BOTTOM as i32,
        json!({ "cards": &bottom_cards[..7] }),
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
    assert_eq!(snapshot["data"]["deck_count"], json!(6));
    assert_eq!(snapshot["data"]["bottom_card_count"], json!(8));
    assert_eq!(snapshot["data"]["hand_count"], json!(79));
    assert_eq!(snapshot["data"]["dealt_count"], json!(316));
    assert_eq!(snapshot["data"]["total_deal_count"], json!(316));
    assert_eq!(
        snapshot["data"]["player_hand_counts"]
            .as_array()
            .expect("six-deck hand counts")
            .iter()
            .map(|entry| entry["hand_count"].as_i64().expect("hand count"))
            .sum::<i64>(),
        316
    );
    assert_eq!(
        wait_for_response(
            &mut *clients[dealer_position],
            UpgradeRoutes::BURY_BOTTOM as i32
        )
        .await["code"],
        json!(WsResponseCode::OK as i32)
    );

    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn upgrade_four_deck_removed_ranks_ws_flow_skips_removed_levels() {
    let port = free_port();
    let listen_addr = format!("127.0.0.1:{port}");
    let url = format!("ws://{listen_addr}");
    let server = tokio::spawn(run_room_runtime(
        RuntimeConfig {
            service_name: "upgrade-four-deck-removed-ranks-test",
            listen_addr,
            idle_timeout: Duration::from_secs(45),
            heartbeat_interval: Duration::from_secs(45),
        },
        TestUpgradeHandler::default(),
    ));

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

    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn upgrade_ws_auto_buries_after_three_play_windows() {
    let port = free_port();
    let listen_addr = format!("127.0.0.1:{port}");
    let url = format!("ws://{listen_addr}");
    let server = tokio::spawn(run_room_runtime(
        RuntimeConfig {
            service_name: "upgrade-auto-bury-window-test",
            listen_addr,
            idle_timeout: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(30),
        },
        TestUpgradeHandler::default(),
    ));

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

    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn upgrade_ws_rejects_an_off_group_follow_and_accepts_the_next_legal_card() {
    let port = free_port();
    let listen_addr = format!("127.0.0.1:{port}");
    let url = format!("ws://{listen_addr}");
    let server = tokio::spawn(run_room_runtime(
        RuntimeConfig {
            service_name: "upgrade-illegal-follow-test",
            listen_addr,
            idle_timeout: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(30),
        },
        TestUpgradeHandler::default(),
    ));

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

    server.abort();
}
