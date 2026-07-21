#![cfg(not(feature = "official"))]

#[cfg(feature = "official")]
use std::time::Instant;
use std::{collections::HashMap, net::TcpListener, time::Duration};

use futures_util::{SinkExt, StreamExt};
use landlord::game::LandlordGameHandler;
use serde_json::{Value, json};
use share_type_public::{GameId, LandlordRoutes, Routes, WsCode, WsResponseCode};
use tokio::net::TcpListener as TokioTcpListener;
use tokio_tungstenite::{WebSocketStream, connect_async, tungstenite::Message};
use ws_common::{RuntimeConfig, run_room_runtime};

type Client = WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

fn cards_from_deal(event: &Value) -> Vec<i32> {
    event
        .get("data")
        .and_then(|data| data.get("cards"))
        .and_then(Value::as_array)
        .expect("deal cards")
        .iter()
        .map(|card| card.as_i64().expect("card number") as i32)
        .collect()
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
            "game_id": GameId::LANDLORD as i32,
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn landlord_nonofficial_rejects_ai_management() {
    let port = free_port();
    let listen_addr = format!("127.0.0.1:{port}");
    let url = format!("ws://{listen_addr}");
    let server = tokio::spawn(run_room_runtime(
        RuntimeConfig {
            service_name: "landlord-no-ai-test",
            listen_addr,
            idle_timeout: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(30),
        },
        LandlordGameHandler::default(),
    ));

    for _ in 0..50 {
        if TokioTcpListener::bind(("127.0.0.1", port)).await.is_err() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let mut owner = connect_client(&url).await;
    let _ = join(&mut owner, "owner", "landlord-no-ai-room").await;
    for (route, data) in [
        (Routes::ADD_AI, json!({ "count": 1 })),
        (Routes::REMOVE_AI, json!({ "position": 1 })),
    ] {
        send_request(&mut owner, route as i32, data).await;
        let response = recv_until(&mut owner, "AI management rejected", |value| {
            value.get("route").and_then(Value::as_i64) == Some(route as i64)
        })
        .await;
        assert_eq!(
            response.get("code").and_then(Value::as_i64),
            Some(WsResponseCode::NO_PERMISSION as i64)
        );
    }

    server.abort();
}

#[cfg(feature = "official")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn landlord_ai_seats_call_and_play_without_becoming_away() {
    let port = free_port();
    let listen_addr = format!("127.0.0.1:{port}");
    let url = format!("ws://{listen_addr}");
    let server = tokio::spawn(run_room_runtime(
        RuntimeConfig {
            service_name: "landlord-ai-test",
            listen_addr,
            idle_timeout: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(30),
        },
        LandlordGameHandler::default(),
    ));

    for _ in 0..50 {
        if TokioTcpListener::bind(("127.0.0.1", port)).await.is_err() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let mut owner = connect_client(&url).await;
    let room = "landlord-ai-room";
    let joined = join(&mut owner, "owner", room).await;
    assert_eq!(position_from_joined(&joined), 0);

    send_request(&mut owner, Routes::ADD_AI as i32, json!({ "count": 2 })).await;
    for expected_position in 1..=2 {
        let joined_ai = recv_until(&mut owner, "ai join event", |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::JOIN as i64)
                && value
                    .get("data")
                    .and_then(|data| data.get("is_ai"))
                    .and_then(Value::as_bool)
                    == Some(true)
        })
        .await;
        assert_eq!(joined_ai["data"]["position"], json!(expected_position));
        assert_eq!(joined_ai["data"]["is_ai_takeover"], json!(false));
        assert_eq!(joined_ai["data"]["away"], json!(false));
    }
    recv_until(&mut owner, "add ai ok", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::ADD_AI as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;

    send_request(&mut owner, Routes::START as i32, json!({})).await;
    recv_until(&mut owner, "start ok", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::START as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;
    let deal = recv_until(&mut owner, "owner deal", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::DEAL as i64)
    })
    .await;
    let mut owner_hand = cards_from_deal(&deal);

    let call_phase = recv_until(&mut owner, "owner call phase", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::CHANGE_PHASE as i64)
            && value["data"]["phase"] == json!(1)
    })
    .await;
    assert_eq!(call_phase["data"]["position"], json!(0));

    send_request(
        &mut owner,
        LandlordRoutes::CALL_LANDLORD as i32,
        json!({ "score": 2 }),
    )
    .await;
    recv_until(&mut owner, "owner call ok", |value| {
        value.get("route").and_then(Value::as_i64) == Some(LandlordRoutes::CALL_LANDLORD as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;

    let started_calling = Instant::now();
    let mut ai_call_count = 0;
    let landlord_position = loop {
        let value = recv_json(&mut owner, "ai call or play phase").await;
        let code = value.get("code").and_then(Value::as_i64);
        if code == Some(WsCode::AWAY as i64)
            && matches!(value["data"]["position"].as_i64(), Some(1 | 2))
        {
            panic!("AI seat was incorrectly marked away while calling: {value}");
        }
        if code == Some(WsCode::CALL_LANDLORD as i64) && value["data"]["name"] != json!("owner") {
            ai_call_count += 1;
        }
        if code == Some(WsCode::CHANGE_PHASE as i64) && value["data"]["phase"] == json!(2) {
            break value["data"]["position"]
                .as_u64()
                .expect("landlord position") as usize;
        }
    };
    assert!(ai_call_count >= 1);
    assert!(
        started_calling.elapsed() < Duration::from_secs(3),
        "AI bidding waited too long: {:?}",
        started_calling.elapsed()
    );

    let expected_ai_position = if landlord_position == 0 {
        let hidden = recv_until(&mut owner, "owner hidden cards", |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::DEAL_OPEN_CARDS as i64)
        })
        .await;
        owner_hand.extend(cards_from_deal(&hidden));
        owner_hand.sort_unstable();

        send_request(
            &mut owner,
            Routes::PLAY as i32,
            json!({ "cards": [owner_hand[0]] }),
        )
        .await;
        recv_until(&mut owner, "owner play ok", |value| {
            value.get("route").and_then(Value::as_i64) == Some(Routes::PLAY as i64)
                && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
        })
        .await;
        1
    } else {
        landlord_position
    };

    let started_waiting = Instant::now();
    let mut saw_ai_turn = false;
    loop {
        let value = recv_json(&mut owner, "first ai play").await;
        let code = value.get("code").and_then(Value::as_i64);
        if code == Some(WsCode::AWAY as i64)
            && matches!(value["data"]["position"].as_i64(), Some(1 | 2))
        {
            panic!("AI seat was incorrectly marked away: {value}");
        }
        if code == Some(WsCode::CHANGE_DEAL as i64)
            && value["data"]["position"] == json!(expected_ai_position)
        {
            saw_ai_turn = true;
            assert_eq!(value["data"]["turn_countdown"], json!(1));
        }
        if code == Some(WsCode::PLAY as i64) && value["data"]["name"] != json!("owner") {
            assert!(saw_ai_turn);
            assert!(value["data"]["cards"].is_array());
            assert!(
                started_waiting.elapsed() < Duration::from_secs(2),
                "AI action waited too long: {:?}",
                started_waiting.elapsed()
            );
            break;
        }
    }

    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn landlord_three_players_can_start_call_and_play_over_ws() {
    let port = free_port();
    let listen_addr = format!("127.0.0.1:{port}");
    let url = format!("ws://{listen_addr}");
    let server = tokio::spawn(run_room_runtime(
        RuntimeConfig {
            service_name: "landlord-test",
            listen_addr,
            idle_timeout: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(30),
        },
        LandlordGameHandler::default(),
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
    let room = "landlord-flow-room";

    let a_join = join(&mut a, "a", room).await;
    let b_join = join(&mut b, "b", room).await;
    let c_join = join(&mut c, "c", room).await;
    let positions = [
        position_from_joined(&a_join),
        position_from_joined(&b_join),
        position_from_joined(&c_join),
    ];
    assert_eq!(positions, [0, 1, 2]);

    send_request(
        &mut a,
        Routes::SETTING as i32,
        json!({
            "current_configs": {
                "settlement_time": 2
            }
        }),
    )
    .await;
    recv_until(&mut a, "setting ok", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::SETTING as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;

    send_request(&mut a, Routes::START as i32, json!({})).await;
    recv_until(&mut a, "start ok", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::START as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;

    let a_deal = recv_until(&mut a, "a deal", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::DEAL as i64)
    })
    .await;
    let b_deal = recv_until(&mut b, "b deal", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::DEAL as i64)
    })
    .await;
    let c_deal = recv_until(&mut c, "c deal", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::DEAL as i64)
    })
    .await;
    let mut hands: HashMap<usize, Vec<i32>> = HashMap::from([
        (0, cards_from_deal(&a_deal)),
        (1, cards_from_deal(&b_deal)),
        (2, cards_from_deal(&c_deal)),
    ]);
    assert!(hands.values().all(|cards| cards.len() == 17));

    let call_phase = recv_until(&mut a, "call phase", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::CHANGE_PHASE as i64)
            && value
                .get("data")
                .and_then(|data| data.get("phase"))
                .and_then(Value::as_i64)
                == Some(1)
    })
    .await;
    let first_caller = call_phase["data"]["position"]
        .as_u64()
        .expect("first caller") as usize;

    let clients = [&mut a, &mut b, &mut c];
    send_request(
        clients[first_caller],
        LandlordRoutes::CALL_LANDLORD as i32,
        json!({ "score": 3 }),
    )
    .await;
    recv_until(clients[first_caller], "call landlord ok", |value| {
        value.get("route").and_then(Value::as_i64) == Some(LandlordRoutes::CALL_LANDLORD as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;

    let play_phase = recv_until(&mut a, "play phase", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::CHANGE_PHASE as i64)
            && value
                .get("data")
                .and_then(|data| data.get("phase"))
                .and_then(Value::as_i64)
                == Some(2)
    })
    .await;
    let landlord_position = play_phase["data"]["position"]
        .as_u64()
        .expect("landlord position") as usize;
    assert_eq!(landlord_position, first_caller);

    let hidden_event = recv_until(&mut a, "hidden cards", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::DEAL_OPEN_CARDS as i64)
    })
    .await;
    let hidden_cards = cards_from_deal(&hidden_event);
    assert_eq!(hidden_cards.len(), 3);
    hands
        .get_mut(&landlord_position)
        .expect("landlord hand")
        .extend(hidden_cards);
    hands
        .get_mut(&landlord_position)
        .expect("landlord hand")
        .sort_unstable();

    let play_card = hands[&landlord_position][0];
    let clients = [&mut a, &mut b, &mut c];
    send_request(
        clients[landlord_position],
        Routes::PLAY as i32,
        json!({ "cards": [play_card] }),
    )
    .await;
    recv_until(clients[landlord_position], "play ok", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::PLAY as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;

    let play_event_observer = (landlord_position + 1) % 3;
    let clients = [&mut a, &mut b, &mut c];
    let play_event = recv_until(clients[play_event_observer], "play event", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::PLAY as i64)
            && value
                .get("data")
                .and_then(|data| data.get("cards"))
                .and_then(Value::as_array)
                .is_some_and(|cards| {
                    cards.first().and_then(Value::as_i64) == Some(play_card as i64)
                })
    })
    .await;
    assert_eq!(play_event["data"]["cards"], json!([play_card]));

    let next_turn = recv_until(&mut a, "next turn", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::CHANGE_DEAL as i64)
    })
    .await;
    let expected_next = (landlord_position + 1) % 3;
    assert_eq!(next_turn["data"]["position"], json!(expected_next as i32));

    server.abort();
}

fn position_from_joined(response: &Value) -> usize {
    response
        .get("data")
        .and_then(|data| data.get("existing_members"))
        .and_then(Value::as_array)
        .map(|members| {
            let used: Vec<usize> = members
                .iter()
                .filter_map(|item| {
                    item.get("position")
                        .and_then(Value::as_u64)
                        .map(|v| v as usize)
                })
                .collect();
            (0..3).find(|pos| !used.contains(pos)).unwrap_or(0)
        })
        .unwrap_or(0)
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
