use std::{net::TcpListener, time::Duration};

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use share_type_public::{GameId, Routes, WsResponseCode};
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
        let frame = tokio::time::timeout(Duration::from_secs(5), client.next())
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
async fn four_players_can_deal_bury_and_select_trump() {
    use share_type_public::{UpgradePhase, UpgradeRoutes, UpgradeSuit, UpgradeWsCode, WsCode};

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
    let owner_hand = wait_for_event(&mut clients[0], UpgradeWsCode::HAND_UPDATED as i32).await;
    assert_eq!(owner_hand["data"]["position"], json!(0));
    assert_eq!(owner_hand["data"]["cards"].as_array().unwrap().len(), 48);
    let bottom = wait_for_event(&mut clients[0], UpgradeWsCode::BOTTOM_CARDS as i32).await;
    let bottom_cards = bottom["data"]["cards"].clone();
    assert_eq!(bottom_cards.as_array().unwrap().len(), 10);
    let started = wait_for_response(&mut clients[0], Routes::START as i32).await;
    assert_eq!(started["code"], json!(WsResponseCode::OK as i32));

    for client in clients.iter_mut().skip(1) {
        let hand = wait_for_event(client, UpgradeWsCode::HAND_UPDATED as i32).await;
        assert_eq!(hand["data"]["cards"].as_array().unwrap().len(), 38);
    }

    send_request(
        &mut clients[0],
        UpgradeRoutes::BURY_BOTTOM as i32,
        json!({ "cards": bottom_cards }),
    )
    .await;
    let buried = wait_for_response(&mut clients[0], UpgradeRoutes::BURY_BOTTOM as i32).await;
    assert_eq!(buried["code"], json!(WsResponseCode::OK as i32));

    send_request(
        &mut clients[0],
        UpgradeRoutes::SELECT_TRUMP as i32,
        json!({ "trump_suit": UpgradeSuit::HEART as i8 }),
    )
    .await;
    let snapshot = wait_for_event(&mut clients[0], WsCode::TABLE_SNAPSHOT as i32).await;
    assert_eq!(snapshot["data"]["phase"], json!(UpgradePhase::Play as i8));
    assert_eq!(
        snapshot["data"]["trump_suit"],
        json!(UpgradeSuit::HEART as i8)
    );
    let selected = wait_for_response(&mut clients[0], UpgradeRoutes::SELECT_TRUMP as i32).await;
    assert_eq!(selected["code"], json!(WsResponseCode::OK as i32));

    server.abort();
}
