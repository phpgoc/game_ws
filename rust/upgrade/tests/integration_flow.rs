use std::{net::TcpListener, time::Duration};

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use share_type_public::{GameId, Routes, WsCode, WsResponseCode};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use upgrade::game::UpgradeGameHandler;
use ws_common::{RuntimeConfig, run_room_runtime};

type Client =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

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
