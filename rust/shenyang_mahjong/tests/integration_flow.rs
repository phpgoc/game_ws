#[cfg(feature = "official")]
use std::time::Instant;
use std::{net::TcpListener, time::Duration};

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
#[cfg(feature = "official")]
use share_type_public::WsCode;
use share_type_public::{GameId, Routes, WsResponseCode};
use shenyang_mahjong::game::ShenyangMahjongGameHandler;
use tokio::net::TcpListener as TokioTcpListener;
use tokio_tungstenite::{WebSocketStream, connect_async, tungstenite::Message};
#[cfg(feature = "official")]
use ws_common::{
    ClientRequest, Dispatch, GameHandler, GameState, MembershipAuthorization, RoomService,
    SessionId, SessionSenders, SettingsBuilderResult,
};
use ws_common::{RuntimeConfig, run_room_runtime};

type Client = WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

#[cfg(feature = "official")]
#[derive(Default)]
struct TestOfficialShenyangMahjongHandler(ShenyangMahjongGameHandler);

#[cfg(feature = "official")]
impl GameHandler for TestOfficialShenyangMahjongHandler {
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

    fn authorize_room_creation(
        &self,
        _join: &share_type_public::WsJoinRequest,
    ) -> MembershipAuthorization {
        Box::pin(async { true })
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

#[cfg(feature = "official")]
async fn close_client(client: &mut Client) {
    client
        .send(Message::Close(None))
        .await
        .expect("send close frame");
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
            "game_id": GameId::SHENYANG_MAHJONG as i32,
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

#[cfg(feature = "official")]
fn my_tiles(event: &Value) -> Vec<i32> {
    event["data"]["my_tiles"]
        .as_array()
        .expect("my tiles")
        .iter()
        .map(|tile| tile.as_i64().expect("tile number") as i32)
        .collect()
}

async fn recv_json(client: &mut Client, label: &str) -> Value {
    loop {
        let frame = tokio::time::timeout(Duration::from_secs(8), client.next())
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
    for _ in 0..100 {
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
#[cfg(not(feature = "official"))]
async fn shenyang_mahjong_nonofficial_rejects_ai_management() {
    let port = free_port();
    let listen_addr = format!("127.0.0.1:{port}");
    let url = format!("ws://{listen_addr}");
    let server = tokio::spawn(run_room_runtime(
        RuntimeConfig {
            service_name: "shenyang-mahjong-no-ai-test",
            listen_addr,
            idle_timeout: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(30),
        },
        ShenyangMahjongGameHandler::default(),
    ));
    for _ in 0..50 {
        if TokioTcpListener::bind(("127.0.0.1", port)).await.is_err() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let mut owner = connect_client(&url).await;
    let _ = join(&mut owner, "owner", "mahjong-no-ai-room").await;
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
async fn shenyang_mahjong_last_human_quit_clears_ai_room() {
    let port = free_port();
    let listen_addr = format!("127.0.0.1:{port}");
    let url = format!("ws://{listen_addr}");
    let server = tokio::spawn(run_room_runtime(
        RuntimeConfig {
            service_name: "shenyang-mahjong-last-human-quit-test",
            listen_addr,
            idle_timeout: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(30),
        },
        TestOfficialShenyangMahjongHandler::default(),
    ));

    for _ in 0..50 {
        if TokioTcpListener::bind(("127.0.0.1", port)).await.is_err() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let mut owner = connect_client(&url).await;
    let room = "shenyang-mahjong-last-human-quit-room";
    let first_join = join(&mut owner, "owner", room).await;
    assert_eq!(first_join["data"]["existing_members"], json!([]));

    send_request(&mut owner, Routes::ADD_AI as i32, json!({ "count": 3 })).await;
    for _ in 0..3 {
        recv_until(&mut owner, "ai join event", |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::JOIN as i64)
                && value
                    .get("data")
                    .and_then(|data| data.get("is_ai"))
                    .and_then(Value::as_bool)
                    == Some(true)
        })
        .await;
    }
    recv_until(&mut owner, "add ai ok", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::ADD_AI as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;

    send_request(&mut owner, Routes::QUIT as i32, json!({})).await;
    recv_until(&mut owner, "quit ok", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::QUIT as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;

    let mut newcomer = connect_client(&url).await;
    let recreated = join(&mut newcomer, "new-owner", room).await;
    assert_eq!(recreated["data"]["self_position"], json!(0));
    assert_eq!(recreated["data"]["existing_members"], json!([]));

    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[cfg(feature = "official")]
async fn shenyang_mahjong_nonofficial_away_owner_uses_timeout_fallback() {
    let port = free_port();
    let listen_addr = format!("127.0.0.1:{port}");
    let url = format!("ws://{listen_addr}");
    let server = tokio::spawn(run_room_runtime(
        RuntimeConfig {
            service_name: "shenyang-mahjong-away-test",
            listen_addr,
            idle_timeout: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(30),
        },
        TestOfficialShenyangMahjongHandler::default(),
    ));

    for _ in 0..50 {
        if TokioTcpListener::bind(("127.0.0.1", port)).await.is_err() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let mut owner = connect_client(&url).await;
    let mut watcher = connect_client(&url).await;
    let room = "shenyang-mahjong-away-room";
    join(&mut owner, "owner", room).await;
    join(&mut watcher, "watcher", room).await;

    send_request(&mut owner, Routes::ADD_AI as i32, json!({ "count": 2 })).await;
    for _ in 0..2 {
        recv_until(&mut owner, "ai join event", |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::JOIN as i64)
                && value
                    .get("data")
                    .and_then(|data| data.get("is_ai"))
                    .and_then(Value::as_bool)
                    == Some(true)
        })
        .await;
    }
    recv_until(&mut owner, "add ai ok", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::ADD_AI as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;

    send_request(
        &mut owner,
        Routes::SETTING as i32,
        json!({
            "current_configs": {
                "play_time": 5,
                "claim_time": 3,
                "settlement_time": 2
            }
        }),
    )
    .await;
    recv_until(&mut owner, "setting ok", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::SETTING as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;

    send_request(&mut owner, Routes::START as i32, json!({})).await;
    recv_until(&mut owner, "start ok", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::START as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;
    recv_until(&mut owner, "mahjong deal", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::DEAL as i64)
    })
    .await;

    send_request(&mut owner, Routes::AWAY as i32, json!({})).await;
    let away_started = Instant::now();
    let away_event = recv_until(&mut owner, "owner away event", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::AWAY as i64)
    })
    .await;
    assert_eq!(away_event["data"]["position"], json!(0));

    close_client(&mut watcher).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let mut watcher = connect_client(&url).await;
    join(&mut watcher, "watcher", room).await;
    let away_snapshot = recv_until(&mut watcher, "away table snapshot", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
    })
    .await;
    let owner_snapshot = away_snapshot["data"]["players"]
        .as_array()
        .expect("snapshot players")
        .iter()
        .find(|player| player["position"] == json!(0))
        .expect("owner snapshot");
    assert_eq!(owner_snapshot["away"], json!(true));
    assert_eq!(owner_snapshot["is_ai"], json!(false));

    let owner_timeout_play = recv_until(&mut owner, "away owner timeout play", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::PLAY as i64)
            && value
                .get("data")
                .and_then(|data| data.get("position"))
                .and_then(Value::as_i64)
                == Some(0)
            && value
                .get("data")
                .and_then(|data| data.get("action"))
                .and_then(Value::as_i64)
                == Some(2)
    })
    .await;
    assert_eq!(owner_timeout_play["data"]["name"], json!("owner"));
    assert!(away_started.elapsed() >= Duration::from_millis(750));

    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[cfg(feature = "official")]
async fn shenyang_mahjong_nonofficial_disconnected_owner_uses_timeout_fallback() {
    let port = free_port();
    let listen_addr = format!("127.0.0.1:{port}");
    let url = format!("ws://{listen_addr}");
    let server = tokio::spawn(run_room_runtime(
        RuntimeConfig {
            service_name: "shenyang-mahjong-disconnected-test",
            listen_addr,
            idle_timeout: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(30),
        },
        TestOfficialShenyangMahjongHandler::default(),
    ));

    for _ in 0..50 {
        if TokioTcpListener::bind(("127.0.0.1", port)).await.is_err() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let mut owner = connect_client(&url).await;
    let mut watcher = connect_client(&url).await;
    let room = "shenyang-mahjong-disconnected-room";
    join(&mut owner, "owner", room).await;
    join(&mut watcher, "watcher", room).await;

    send_request(&mut owner, Routes::ADD_AI as i32, json!({ "count": 2 })).await;
    for _ in 0..2 {
        recv_until(&mut owner, "ai join event", |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::JOIN as i64)
                && value
                    .get("data")
                    .and_then(|data| data.get("is_ai"))
                    .and_then(Value::as_bool)
                    == Some(true)
        })
        .await;
    }
    recv_until(&mut owner, "add ai ok", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::ADD_AI as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;

    send_request(
        &mut owner,
        Routes::SETTING as i32,
        json!({
            "current_configs": {
                "play_time": 5,
                "claim_time": 3,
                "settlement_time": 2
            }
        }),
    )
    .await;
    recv_until(&mut owner, "setting ok", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::SETTING as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;

    send_request(&mut owner, Routes::START as i32, json!({})).await;
    recv_until(&mut owner, "start ok", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::START as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;
    recv_until(&mut owner, "mahjong deal", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::DEAL as i64)
    })
    .await;

    let disconnected_started = Instant::now();
    close_client(&mut owner).await;
    let inactive_owner = recv_until(&mut watcher, "owner inactive event", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::JOIN as i64)
            && value
                .get("data")
                .and_then(|data| data.get("position"))
                .and_then(Value::as_i64)
                == Some(0)
            && value
                .get("data")
                .and_then(|data| data.get("is_active"))
                .and_then(Value::as_bool)
                == Some(false)
    })
    .await;
    assert_eq!(inactive_owner["data"]["name"], json!("owner"));

    let disconnected_owner_timeout_play =
        recv_until(&mut watcher, "disconnected owner timeout play", |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::PLAY as i64)
                && value
                    .get("data")
                    .and_then(|data| data.get("position"))
                    .and_then(Value::as_i64)
                    == Some(0)
                && value
                    .get("data")
                    .and_then(|data| data.get("action"))
                    .and_then(Value::as_i64)
                    == Some(2)
        })
        .await;
    assert_eq!(
        disconnected_owner_timeout_play["data"]["name"],
        json!("owner")
    );
    assert!(disconnected_started.elapsed() >= Duration::from_millis(750));

    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[cfg(feature = "official")]
async fn shenyang_mahjong_owner_can_start_with_ai_and_receive_ai_play() {
    let port = free_port();
    let listen_addr = format!("127.0.0.1:{port}");
    let url = format!("ws://{listen_addr}");
    let server = tokio::spawn(run_room_runtime(
        RuntimeConfig {
            service_name: "shenyang-mahjong-test",
            listen_addr,
            idle_timeout: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(30),
        },
        TestOfficialShenyangMahjongHandler::default(),
    ));

    for _ in 0..50 {
        if TokioTcpListener::bind(("127.0.0.1", port)).await.is_err() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let mut owner = connect_client(&url).await;
    let mut watcher = connect_client(&url).await;
    let room = "shenyang-mahjong-flow-room";
    let join_response = join(&mut owner, "owner", room).await;
    assert_eq!(join_response["data"]["existing_members"], json!([]));

    send_request(&mut owner, Routes::ADD_AI as i32, json!({ "count": 2 })).await;
    let mut joined_ai_positions = Vec::new();
    for _ in 0..2 {
        let join_event = recv_until(&mut owner, "ai join event", |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::JOIN as i64)
                && value
                    .get("data")
                    .and_then(|data| data.get("is_ai"))
                    .and_then(Value::as_bool)
                    == Some(true)
        })
        .await;
        joined_ai_positions.push(
            join_event["data"]["position"]
                .as_i64()
                .expect("ai position") as i32,
        );
    }
    joined_ai_positions.sort_unstable();
    assert_eq!(joined_ai_positions, vec![1, 2]);
    recv_until(&mut owner, "add ai ok", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::ADD_AI as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;
    join(&mut watcher, "watcher", room).await;

    send_request(
        &mut owner,
        Routes::SETTING as i32,
        json!({
            "current_configs": {
                "play_time": 5,
                "claim_time": 3,
                "settlement_time": 2
            }
        }),
    )
    .await;
    recv_until(&mut owner, "setting ok", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::SETTING as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;

    send_request(&mut owner, Routes::START as i32, json!({})).await;
    recv_until(&mut owner, "start ok", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::START as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;

    let deal = recv_until(&mut owner, "mahjong deal", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::DEAL as i64)
    })
    .await;
    let owner_tiles = my_tiles(&deal);
    assert_eq!(owner_tiles.len(), 14);
    assert_eq!(deal["data"]["dealer_position"], json!(0));
    assert_eq!(deal["data"]["current_position"], json!(0));
    assert_eq!(deal["data"]["turn_countdown"], json!(5));

    close_client(&mut owner).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let mut owner = connect_client(&url).await;
    let rejoin_response = join(&mut owner, "owner", room).await;
    let existing_members = rejoin_response["data"]["existing_members"]
        .as_array()
        .expect("existing members");
    assert_eq!(existing_members.len(), 3);
    assert_eq!(
        existing_members
            .iter()
            .filter(|member| member["is_ai"].as_bool() == Some(true))
            .count(),
        2
    );
    assert!(existing_members.iter().any(|member| {
        member["name"] == json!("watcher")
            && member["is_active"].as_bool() == Some(true)
            && member["is_ai"].as_bool() == Some(false)
    }));
    let snapshot = recv_until(&mut owner, "rejoin table snapshot", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
    })
    .await;
    assert_eq!(my_tiles(&snapshot), owner_tiles);
    assert_eq!(
        snapshot["data"]["players"]
            .as_array()
            .expect("snapshot players")
            .len(),
        4
    );
    assert_eq!(snapshot["data"]["current_position"], json!(0));
    assert_eq!(snapshot["data"]["claim_window"], Value::Null);

    close_client(&mut owner).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let mut owner = connect_client(&url).await;
    let replacement_response = join(&mut owner, "replacement", room).await;
    assert_eq!(replacement_response["data"]["self_position"], json!(0));
    assert!(
        replacement_response["data"]["existing_members"]
            .as_array()
            .expect("replacement existing members")
            .iter()
            .all(|member| member["name"] != json!("owner"))
    );
    let replacement_snapshot = recv_until(&mut owner, "replacement table snapshot", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::TABLE_SNAPSHOT as i64)
    })
    .await;
    assert_eq!(my_tiles(&replacement_snapshot), owner_tiles);
    assert_eq!(replacement_snapshot["data"]["current_position"], json!(0));

    let last_drawn_tile = replacement_snapshot["data"]["last_drawn_tile"]
        .as_i64()
        .expect("last drawn tile") as i32;
    assert!(owner_tiles.contains(&last_drawn_tile));

    let discard_tile = owner_tiles[0];
    send_request(
        &mut owner,
        Routes::PLAY as i32,
        json!({
            "action": 2,
            "tiles": [discard_tile],
            "target_tile": discard_tile,
            "from_position": null
        }),
    )
    .await;
    recv_until(&mut owner, "discard ok", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::PLAY as i64)
            && value.get("code").and_then(Value::as_i64) == Some(WsResponseCode::OK as i64)
    })
    .await;

    let ai_play = recv_until(&mut owner, "ai play event", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::PLAY as i64)
            && value
                .get("data")
                .and_then(|data| data.get("position"))
                .and_then(Value::as_i64)
                .is_some_and(|position| position > 0)
            && value
                .get("data")
                .and_then(|data| data.get("action"))
                .and_then(Value::as_i64)
                .is_some()
    })
    .await;
    assert!(ai_play["data"]["wall_count"].as_i64().expect("wall count") > 0);

    server.abort();
}
