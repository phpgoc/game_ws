use std::{
    net::TcpListener,
    path::PathBuf,
    sync::mpsc::sync_channel,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use share_type_public::{Routes, WsCode, WsResponseCode};
use tokio_tungstenite::{WebSocketStream, connect_async, tungstenite::Message};
use ws_common::{RuntimeConfig, run_room_runtime_until_stopped_with_ready, runtime_stop_channel};

type Client = WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind free port")
        .local_addr()
        .expect("read free port")
        .port()
}

fn temporary_database_directory() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "game-{OFFICIAL_SERVICE_NAME}-official-{unique}-{}",
        std::process::id()
    ))
}

async fn create_official_session(name: &str, active_member: bool) -> String {
    let user = data::user_create(data::UserCreateInput {
        name: name.to_owned(),
        account: format!("{OFFICIAL_SERVICE_NAME}-{name}-account"),
        email: None,
        third_platform: 2,
        avatar_url: format!("https://example.com/{name}.png"),
        share_id: 0,
    })
    .await
    .expect("create official test user");
    if active_member {
        data::game_pay_upsert(data::GamePayUpsertInput {
            user_id: user.id,
            game_id: OFFICIAL_GAME_ID,
            duration: 3_600,
        })
        .await
        .expect("activate official test membership");
    }
    data::cache_set_session(user.id)
        .await
        .expect("create official test session")
}

async fn connect_client(url: &str) -> Client {
    connect_async(url).await.expect("connect websocket").0
}

async fn send_request(client: &mut Client, route: i32, data: Value) {
    client
        .send(Message::Text(
            json!({ "route": route, "data": data }).to_string().into(),
        ))
        .await
        .expect("send websocket request");
}

async fn receive_until<F>(client: &mut Client, label: &str, mut predicate: F) -> Value
where
    F: FnMut(&Value) -> bool,
{
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let frame = tokio::time::timeout(remaining, client.next())
            .await
            .unwrap_or_else(|_| panic!("timeout waiting for {label}"))
            .expect("websocket frame")
            .expect("valid websocket frame");
        match frame {
            Message::Text(text) => {
                let value: Value = serde_json::from_str(text.as_ref()).expect("json frame");
                if predicate(&value) {
                    return value;
                }
            }
            Message::Ping(_) | Message::Pong(_) => {}
            other => panic!("unexpected websocket frame while waiting for {label}: {other:?}"),
        }
    }
}

async fn join(client: &mut Client, name: &str, room: &str, session_id: &str) -> Value {
    send_request(
        client,
        Routes::JOIN as i32,
        json!({
            "name": name,
            "password": room,
            "game_id": OFFICIAL_GAME_ID as i32,
            "session_id": session_id,
            "avatar_url": format!("https://example.com/{name}.png")
        }),
    )
    .await;
    receive_until(client, "JOIN response", |value| {
        value.get("route").and_then(Value::as_i64) == Some(Routes::JOIN as i64)
    })
    .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn official_membership_gate_and_ai_takeover_work_over_websocket() {
    let database_directory = temporary_database_directory();
    std::fs::create_dir_all(&database_directory).expect("create temporary database directory");
    let database_path = database_directory.join("official.data");
    data::init_with_config(data::DataConfig::sqlite_file(
        database_path.to_string_lossy().as_ref(),
    ))
    .await
    .expect("initialize official test data");

    let member_session = create_official_session("member", true).await;
    let nonmember_session = create_official_session("nonmember", false).await;
    let port = free_port();
    let listen_addr = format!("127.0.0.1:{port}");
    let url = format!("ws://{listen_addr}");
    let (stop_handle, stop_signal) = runtime_stop_channel();
    let (ready_tx, ready_rx) = sync_channel(1);
    let server = tokio::spawn(run_room_runtime_until_stopped_with_ready(
        RuntimeConfig {
            service_name: OFFICIAL_SERVICE_NAME,
            listen_addr,
            idle_timeout: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(30),
        },
        OfficialGameHandler::default(),
        stop_signal,
        ready_tx,
    ));
    let stats = ready_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("official server ready");

    let room = format!("{OFFICIAL_SERVICE_NAME}-membership-room");
    let mut nonmember = connect_client(&url).await;
    let denied = join(&mut nonmember, "nonmember", &room, &nonmember_session).await;
    assert_eq!(denied["code"], json!(WsResponseCode::NO_PERMISSION as i32));
    assert_eq!(stats.room_count().await, 0);

    let mut member = connect_client(&url).await;
    let joined = join(&mut member, "member", &room, &member_session).await;
    assert_eq!(joined["code"], json!(WsResponseCode::JOINED as i32));
    assert_eq!(joined["data"]["self_position"], json!(0));
    assert_eq!(stats.room_count().await, 1);

    let joined_nonmember = join(&mut nonmember, "nonmember", &room, &nonmember_session).await;
    assert_eq!(
        joined_nonmember["code"],
        json!(WsResponseCode::JOINED as i32)
    );
    assert_eq!(joined_nonmember["data"]["self_position"], json!(1));

    send_request(&mut member, Routes::AWAY as i32, json!({})).await;
    let member_away = receive_until(&mut member, "member AWAY event", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::AWAY as i64)
            && value["data"]["position"] == json!(0)
    })
    .await;
    assert_eq!(member_away["data"]["is_ai_takeover"], json!(true));
    let member_away_seen_by_other =
        receive_until(&mut nonmember, "member AWAY event observed by other", |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::AWAY as i64)
                && value["data"]["position"] == json!(0)
        })
        .await;
    assert_eq!(
        member_away_seen_by_other["data"]["is_ai_takeover"],
        json!(true)
    );
    assert!(stats.room_position_is_ai_takeover(&room, 0).await);

    send_request(&mut member, Routes::BACK as i32, json!({})).await;
    receive_until(&mut member, "member BACK event", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::BACK as i64)
            && value["data"]["position"] == json!(0)
    })
    .await;
    let member_back_seen_by_other =
        receive_until(&mut nonmember, "member BACK event observed by other", |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::BACK as i64)
                && value["data"]["position"] == json!(0)
        })
        .await;
    assert_eq!(
        member_back_seen_by_other["data"]["is_ai_takeover"],
        json!(false)
    );
    assert!(!stats.room_position_is_ai_takeover(&room, 0).await);

    send_request(&mut nonmember, Routes::AWAY as i32, json!({})).await;
    let nonmember_away = receive_until(&mut nonmember, "nonmember AWAY event", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::AWAY as i64)
            && value["data"]["position"] == json!(1)
    })
    .await;
    assert_eq!(nonmember_away["data"]["is_ai_takeover"], json!(false));
    let nonmember_away_seen_by_other =
        receive_until(&mut member, "nonmember AWAY event observed by other", |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::AWAY as i64)
                && value["data"]["position"] == json!(1)
        })
        .await;
    assert_eq!(
        nonmember_away_seen_by_other["data"]["is_ai_takeover"],
        json!(false)
    );
    assert!(!stats.room_position_is_ai_takeover(&room, 1).await);

    send_request(&mut nonmember, Routes::BACK as i32, json!({})).await;
    receive_until(&mut nonmember, "nonmember BACK event", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::BACK as i64)
            && value["data"]["position"] == json!(1)
    })
    .await;
    let nonmember_back_seen_by_other =
        receive_until(&mut member, "nonmember BACK event observed by other", |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::BACK as i64)
                && value["data"]["position"] == json!(1)
        })
        .await;
    assert_eq!(
        nonmember_back_seen_by_other["data"]["is_ai_takeover"],
        json!(false)
    );

    member.close(None).await.expect("disconnect member websocket");
    let member_disconnected = receive_until(&mut nonmember, "member disconnect event", |value| {
        value.get("code").and_then(Value::as_i64) == Some(WsCode::JOIN as i64)
            && value["data"]["position"] == json!(0)
            && value["data"]["is_active"] == json!(false)
    })
    .await;
    assert_eq!(
        member_disconnected["data"]["is_ai_takeover"],
        json!(true)
    );
    assert!(stats.room_position_is_ai_takeover(&room, 0).await);

    let mut member_rejoined = connect_client(&url).await;
    let rejoined = join(
        &mut member_rejoined,
        "member",
        &room,
        &member_session,
    )
    .await;
    assert_eq!(rejoined["code"], json!(WsResponseCode::JOINED as i32));
    assert_eq!(rejoined["data"]["self_position"], json!(0));
    assert!(!stats.room_position_is_ai_takeover(&room, 0).await);

    nonmember
        .close(None)
        .await
        .expect("disconnect nonmember websocket");
    let nonmember_disconnected =
        receive_until(&mut member_rejoined, "nonmember disconnect event", |value| {
            value.get("code").and_then(Value::as_i64) == Some(WsCode::JOIN as i64)
                && value["data"]["position"] == json!(1)
                && value["data"]["is_active"] == json!(false)
        })
        .await;
    assert_eq!(
        nonmember_disconnected["data"]["is_ai_takeover"],
        json!(false)
    );
    assert!(!stats.room_position_is_ai_takeover(&room, 1).await);

    stop_handle.stop();
    server
        .await
        .expect("join official server task")
        .expect("stop official server");
    data::shutdown().await;
    let _ = std::fs::remove_dir_all(database_directory);
}
