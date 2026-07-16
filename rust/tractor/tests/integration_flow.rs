use std::{
    net::TcpListener,
    time::{Duration, Instant},
};

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use share_type_public::{
    GameId, Routes, TractorPhase, TractorRoutes, TractorWsCode, WsCode, WsResponseCode,
};
use tokio::net::TcpListener as TokioTcpListener;
use tokio_tungstenite::{WebSocketStream, connect_async, tungstenite::Message};
use tractor::game::TractorGameHandler;
use ws_common::{RuntimeConfig, run_room_runtime};

type Client = WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

fn card_rank(card: i32) -> i32 {
    let base = ((card - 1) % 100) + 1;
    if base <= 52 {
        ((base - 1) % 13) + 2
    } else if base == 53 {
        16
    } else {
        17
    }
}

async fn connect_client(url: &str) -> Client {
    let (ws, _) = connect_async(url).await.expect("connect websocket");
    ws
}

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
        let frame = tokio::time::timeout(Duration::from_secs(5), client.next())
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
    for _ in 0..80 {
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tractor_incremental_deal_compact_deck_and_bury_flow() {
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
        TractorGameHandler::default(),
    ));

    for _ in 0..50 {
        if TokioTcpListener::bind(("127.0.0.1", port)).await.is_err() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

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
                "deck_count": 2,
                "blood_enabled": 1,
                "blood_start_score": 80,
                "blood_score_per_unit": 40,
                "target_rank": 12,
                "removed_rank_count": 3,
                "first_deal_time": 1000,
                "deal_time": 500,
                "play_time": 10
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

    let mut dealt_cards = Vec::new();
    let bottom = loop {
        let value = recv_json(&mut a, "incremental deal and bottom").await;
        match value.get("code").and_then(Value::as_i64) {
            Some(code) if code == WsCode::DEAL as i64 => {
                let cards = value["data"]["cards"].as_array().expect("deal cards");
                assert_eq!(cards.len(), 1, "deal must be incremental");
                dealt_cards.push(cards[0].as_i64().expect("card") as i32);
            }
            Some(code) if code == TractorWsCode::BOTTOM_CARDS as i64 => break value,
            _ => {}
        }
    };
    assert!(started_at.elapsed() >= Duration::from_millis(650));
    assert_eq!(dealt_cards.len(), 19);
    assert!(
        dealt_cards
            .iter()
            .all(|card| ![3, 4, 6].contains(&card_rank(*card)))
    );
    let bottom_cards = bottom["data"]["cards"]
        .as_array()
        .expect("bottom cards")
        .iter()
        .map(|card| card.as_i64().expect("bottom card") as i32)
        .collect::<Vec<_>>();
    assert_eq!(bottom_cards.len(), 8);
    assert_eq!(bottom["data"]["required_count"], json!(8));

    send_request(
        &mut a,
        TractorRoutes::BURY_BOTTOM as i32,
        json!({ "cards": bottom_cards }),
    )
    .await;
    let snapshot = recv_until(&mut a, "play snapshot", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
            && value["data"]["phase"] == json!(TractorPhase::Play as i8)
    })
    .await;
    recv_until(&mut a, "bury response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(TractorRoutes::BURY_BOTTOM as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;
    assert_eq!(snapshot["data"]["deck_count"], json!(2));
    assert_eq!(snapshot["data"]["target_rank"], json!(2));
    assert_eq!(snapshot["data"]["final_target_rank"], json!(14));
    assert_eq!(snapshot["data"]["removed_rank_count"], json!(3));
    assert_eq!(snapshot["data"]["blood_enabled"], json!(true));
    assert_eq!(snapshot["data"]["bottom_card_count"], json!(8));
    assert_eq!(snapshot["data"]["blood_start_score"], json!(80));
    assert_eq!(snapshot["data"]["blood_score_per_unit"], json!(40));
    assert_eq!(snapshot["data"]["dealt_count"], json!(76));
    assert_eq!(snapshot["data"]["total_deal_count"], json!(76));
    assert_eq!(
        snapshot["data"]["player_hand_counts"][0]["hand_count"],
        json!(19)
    );

    server.abort();
}
