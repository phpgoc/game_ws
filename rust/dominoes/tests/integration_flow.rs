use std::net::TcpListener;
use std::time::Duration;

use dominoes::game::DominoesGameHandler;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use share_type_public::{DominoesRoutes, DominoesWsCode, GameId, Routes, WsCode, WsResponseCode};
use tokio::net::TcpListener as TokioTcpListener;
use tokio_tungstenite::{WebSocketStream, connect_async, tungstenite::Message};
#[cfg(feature = "official")]
use ws_common::{
    ClientRequest, Dispatch, GameHandler, GameState, JoinAuthorization, JoinAuthorizationFuture,
    RoomService, SessionId, SessionSenders, SettingsBuilderResult,
};
use ws_common::{RuntimeConfig, run_room_runtime};

type Client = WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

#[cfg(feature = "official")]
#[derive(Default)]
struct TestAiDominoesHandler(DominoesGameHandler);

#[cfg(feature = "official")]
impl GameHandler for TestAiDominoesHandler {
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
        self.0.build_room_settings()
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn three_players_can_start_and_place_the_opening_tile() {
    let port = free_port();
    let listen_addr = format!("127.0.0.1:{port}");
    let url = format!("ws://{listen_addr}");
    let handler = {
        #[cfg(feature = "official")]
        {
            TestAiDominoesHandler::default()
        }
        #[cfg(not(feature = "official"))]
        {
            DominoesGameHandler::default()
        }
    };
    let server = tokio::spawn(run_room_runtime(
        RuntimeConfig {
            service_name: "dominoes-integration-test",
            listen_addr,
            idle_timeout: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(30),
        },
        handler,
    ));
    wait_for_server(port).await;

    let mut owner = connect_client(&url).await;
    let mut second = connect_client(&url).await;
    let mut third = connect_client(&url).await;
    let room = "dominoes-three-player-room";
    join(&mut owner, "owner", room).await;
    join(&mut second, "second", room).await;
    join(&mut third, "third", room).await;

    send_request(&mut owner, Routes::START as i32, json!({})).await;
    let owner_start = recv_start_bundle(&mut owner, true).await;
    let second_start = recv_start_bundle(&mut second, false).await;
    let third_start = recv_start_bundle(&mut third, false).await;
    let starter = owner_start.starter_position;
    assert_eq!(second_start.starter_position, starter);
    assert_eq!(third_start.starter_position, starter);
    assert_eq!(owner_start.hand.len(), 5);
    assert_eq!(second_start.hand.len(), 5);
    assert_eq!(third_start.hand.len(), 5);

    let (client, hand) = match starter {
        0 => (&mut owner, &owner_start.hand),
        1 => (&mut second, &second_start.hand),
        2 => (&mut third, &third_start.hand),
        other => panic!("unexpected starter position {other}"),
    };
    let tile_id = hand[0]["id"].as_i64().expect("tile id") as i32;
    send_request(
        client,
        DominoesRoutes::PLAY_TILE as i32,
        json!({ "tile_id": tile_id, "endpoint_id": null }),
    )
    .await;
    let play = recv_until(client, "opening play event", |value| {
        value.get("code").and_then(Value::as_i64) == Some(DominoesWsCode::PLAY_TILE as i64)
    })
    .await;
    assert_eq!(play["data"]["position"], json!(starter));
    assert_eq!(play["data"]["placement"]["tile"]["id"], json!(tile_id));
    let response = recv_until(client, "opening play response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(DominoesRoutes::PLAY_TILE as i64)
    })
    .await;
    assert_eq!(
        response.get("code").and_then(Value::as_i64),
        Some(WsResponseCode::OK as i64)
    );

    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ai_seats_can_reach_and_drop_below_the_three_player_start_limit() {
    let port = free_port();
    let listen_addr = format!("127.0.0.1:{port}");
    let url = format!("ws://{listen_addr}");
    let server = tokio::spawn(run_room_runtime(
        RuntimeConfig {
            service_name: "dominoes-ai-seat-test",
            listen_addr,
            idle_timeout: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(30),
        },
        DominoesGameHandler::default(),
    ));
    wait_for_server(port).await;

    let mut owner = connect_client(&url).await;
    join(&mut owner, "owner", "dominoes-ai-seat-room").await;

    send_request(&mut owner, Routes::START as i32, json!({})).await;
    let too_few = recv_until(&mut owner, "one-player start response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::START as i64)
    })
    .await;
    assert_eq!(too_few["code"], json!(WsResponseCode::NOT_IN_RANGE as i32));

    send_request(&mut owner, Routes::ADD_AI as i32, json!({ "count": 2 })).await;
    let mut ai_names = Vec::new();
    for expected_position in 1..=2 {
        let joined = recv_until(&mut owner, "AI join event", |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::JOIN as i64)
                && value["data"]["is_ai"] == json!(true)
        })
        .await;
        assert_eq!(joined["data"]["position"], json!(expected_position));
        ai_names.push(joined["data"]["name"].clone());
    }
    let added = recv_until(&mut owner, "add AI response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::ADD_AI as i64)
    })
    .await;
    assert_eq!(added["code"], json!(WsResponseCode::OK as i32));

    send_request(
        &mut owner,
        Routes::REMOVE_AI as i32,
        json!({ "position": 2 }),
    )
    .await;
    let removed = recv_until(&mut owner, "AI quit event", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::QUIT as i64)
            && value["data"]["name"] == ai_names[1]
    })
    .await;
    assert_eq!(removed["data"]["name"], ai_names[1]);
    let removed_response = recv_until(&mut owner, "remove AI response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::REMOVE_AI as i64)
    })
    .await;
    assert_eq!(removed_response["code"], json!(WsResponseCode::OK as i32));

    send_request(&mut owner, Routes::START as i32, json!({})).await;
    let below_limit = recv_until(&mut owner, "two-player start response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::START as i64)
    })
    .await;
    assert_eq!(
        below_limit["code"],
        json!(WsResponseCode::NOT_IN_RANGE as i32)
    );

    send_request(&mut owner, Routes::ADD_AI as i32, json!({ "count": 1 })).await;
    recv_until(&mut owner, "replacement AI join event", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::JOIN as i64)
            && value["data"]["is_ai"] == json!(true)
    })
    .await;
    recv_until(&mut owner, "replacement AI response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::ADD_AI as i64)
    })
    .await;

    send_request(&mut owner, Routes::START as i32, json!({})).await;
    let started = recv_start_bundle(&mut owner, true).await;
    assert_eq!(started.hand.len(), 5);

    server.abort();
}

struct StartBundle {
    starter_position: i32,
    hand: Vec<Value>,
}

async fn recv_start_bundle(client: &mut Client, expect_response: bool) -> StartBundle {
    let mut starter_position = None;
    let mut hand = None;
    let mut response_received = !expect_response;
    for _ in 0..30 {
        let value = recv_json(client, "round start bundle").await;
        if value.get("code").and_then(Value::as_i64) == Some(DominoesWsCode::ROUND_START as i64) {
            starter_position = value["data"]["starter_position"]
                .as_i64()
                .map(|position| position as i32);
        }
        if value.get("code").and_then(Value::as_i64) == Some(DominoesWsCode::DEAL as i64) {
            hand = value["data"]["hand"].as_array().cloned();
        }
        if value.get("route").and_then(Value::as_i64) == Some(Routes::START as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
        {
            response_received = true;
        }
        if let (Some(starter_position), Some(hand), true) =
            (starter_position, hand.clone(), response_received)
        {
            return StartBundle {
                starter_position,
                hand,
            };
        }
    }
    panic!("did not receive complete start bundle");
}

async fn join(client: &mut Client, name: &str, room: &str) {
    send_request(
        client,
        Routes::JOIN as i32,
        json!({
            "name": name,
            "password": room,
            "game_id": GameId::DOMINOES as i32,
            "avatar_url": ""
        }),
    )
    .await;
    recv_until(client, "join response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::JOIN as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::JOINED as i64)
    })
    .await;
}

async fn connect_client(url: &str) -> Client {
    connect_async(url).await.expect("connect websocket").0
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind free port")
        .local_addr()
        .expect("local address")
        .port()
}

async fn wait_for_server(port: u16) {
    for _ in 0..50 {
        if TokioTcpListener::bind(("127.0.0.1", port)).await.is_err() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("dominoes websocket server did not start");
}

async fn recv_json(client: &mut Client, label: &str) -> Value {
    loop {
        let frame = tokio::time::timeout(Duration::from_secs(5), client.next())
            .await
            .unwrap_or_else(|_| panic!("websocket timeout while waiting for {label}"))
            .expect("websocket frame")
            .expect("websocket frame ok");
        match frame {
            Message::Text(text) => return serde_json::from_str(text.as_ref()).expect("json frame"),
            Message::Ping(_) | Message::Pong(_) => continue,
            other => panic!("unexpected websocket frame: {other:?}"),
        }
    }
}

async fn recv_until<F>(client: &mut Client, label: &str, mut predicate: F) -> Value
where
    F: FnMut(&Value) -> bool,
{
    for _ in 0..40 {
        let value = recv_json(client, label).await;
        if predicate(&value) {
            return value;
        }
    }
    panic!("expected websocket frame not received for {label}");
}

async fn send_request(client: &mut Client, route: i32, data: Value) {
    client
        .send(Message::Text(
            json!({ "route": route, "data": data }).to_string().into(),
        ))
        .await
        .expect("send websocket request");
}
