use std::{collections::HashMap, sync::mpsc::sync_channel, time::Duration};

use futures_util::{SinkExt, StreamExt};
use share_type_public::{GameId, Routes, WsResponseCode};
use tokio::time::{sleep, timeout};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use ws_common::{
    ClientRequest, Dispatch, GameHandler, GameSettings, JoinAuthorization, JoinAuthorizationFuture,
    RoomService, RuntimeConfig, SessionId, SharedGameState,
    run_room_runtime_until_stopped_with_ready, runtime_stop_channel,
};

struct RuntimeHandler;

impl GameHandler for RuntimeHandler {
    fn build_game_state(&self) -> Box<dyn ws_common::GameState> {
        Box::new(SharedGameState::new())
    }

    fn build_room_settings(&self) -> ws_common::SettingsBuilderResult {
        (GameSettings::new(1, 4), HashMap::new())
    }

    fn game_id(&self) -> GameId {
        GameId::LANDLORD
    }

    fn handle_game_request(
        &mut self,
        _room_service: &mut RoomService,
        _session_id: SessionId,
        _request: ClientRequest,
    ) -> Dispatch {
        Dispatch::default()
    }
}

struct DeniedRuntimeHandler;

impl GameHandler for DeniedRuntimeHandler {
    fn authorize_join(&self, _join: &share_type_public::WsJoinRequest) -> JoinAuthorizationFuture {
        Box::pin(async {
            JoinAuthorization {
                can_create_room: false,
                has_active_membership: false,
            }
        })
    }

    fn build_game_state(&self) -> Box<dyn ws_common::GameState> {
        Box::new(SharedGameState::new())
    }

    fn build_room_settings(&self) -> ws_common::SettingsBuilderResult {
        (GameSettings::new(1, 4), HashMap::new())
    }

    fn game_id(&self) -> GameId {
        GameId::LANDLORD
    }

    fn handle_game_request(
        &mut self,
        _room_service: &mut RoomService,
        _session_id: SessionId,
        _request: ClientRequest,
    ) -> Dispatch {
        Dispatch::default()
    }
}

async fn wait_for_client_count(stats: &ws_common::RuntimeStats, expected: usize) {
    for _ in 0..50 {
        if stats.client_count().await == expected {
            return;
        }
        sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(stats.client_count().await, expected);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runtime_accepts_a_websocket_join_and_cleans_up_on_close() {
    let (stop, signal) = runtime_stop_channel();
    let (ready_tx, ready_rx) = sync_channel(1);
    let server = tokio::spawn(run_room_runtime_until_stopped_with_ready(
        RuntimeConfig {
            service_name: "runtime-integration-test",
            listen_addr: "127.0.0.1:0".to_owned(),
            idle_timeout: Duration::from_secs(2),
            heartbeat_interval: Duration::from_secs(60),
        },
        RuntimeHandler,
        signal,
        ready_tx,
    ));
    let stats = tokio::task::spawn_blocking(move || {
        ready_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("runtime reports listen address")
    })
    .await
    .expect("read runtime stats");

    let (mut client, _) = connect_async(format!("ws://{}", stats.listen_addr()))
        .await
        .expect("connect websocket client");
    wait_for_client_count(&stats, 1).await;
    client
        .send(Message::Ping(Vec::new().into()))
        .await
        .expect("send ping");
    client
        .send(Message::Text(
            serde_json::json!({
                "route": Routes::JOIN as i32,
                "data": {
                    "name": "runtime owner",
                    "password": "runtime-room",
                    "game_id": GameId::LANDLORD as i32,
                }
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("send join");

    let response = timeout(Duration::from_secs(1), async {
        loop {
            match client
                .next()
                .await
                .expect("websocket frame")
                .expect("frame valid")
            {
                Message::Text(text) => break text,
                Message::Pong(_) | Message::Ping(_) => continue,
                frame => panic!("unexpected join response frame: {frame:?}"),
            }
        }
    })
    .await
    .expect("join response arrives");
    let response: serde_json::Value = serde_json::from_str(&response).expect("join response json");
    assert_eq!(response["route"], Routes::JOIN as i32);
    assert_eq!(stats.room_count().await, 1);

    client.close(None).await.expect("close client");
    wait_for_client_count(&stats, 0).await;
    stop.stop();
    let stopped = server
        .await
        .expect("runtime task joins")
        .expect("runtime stops");
    assert_eq!(stopped.room_count().await, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runtime_applies_join_authorization_before_creating_a_room() {
    let (stop, signal) = runtime_stop_channel();
    let (ready_tx, ready_rx) = sync_channel(1);
    let server = tokio::spawn(run_room_runtime_until_stopped_with_ready(
        RuntimeConfig {
            service_name: "runtime-authorization-test",
            listen_addr: "127.0.0.1:0".to_owned(),
            idle_timeout: Duration::from_secs(2),
            heartbeat_interval: Duration::from_secs(60),
        },
        DeniedRuntimeHandler,
        signal,
        ready_tx,
    ));
    let stats = tokio::task::spawn_blocking(move || {
        ready_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("runtime reports listen address")
    })
    .await
    .expect("read runtime stats");
    let (mut client, _) = connect_async(format!("ws://{}", stats.listen_addr()))
        .await
        .expect("connect websocket client");
    client
        .send(Message::Text(
            serde_json::json!({
                "route": Routes::JOIN as i32,
                "data": {
                    "name": "non member",
                    "password": "member-room",
                    "game_id": GameId::LANDLORD as i32,
                }
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("send denied join");
    let response = timeout(Duration::from_secs(1), async {
        loop {
            match client.next().await.expect("frame").expect("valid frame") {
                Message::Text(text) => break text,
                Message::Ping(_) | Message::Pong(_) => continue,
                frame => panic!("unexpected denied-join frame: {frame:?}"),
            }
        }
    })
    .await
    .expect("authorization response arrives");
    let response: serde_json::Value = serde_json::from_str(&response).expect("response json");
    assert_eq!(response["route"], Routes::JOIN as i32);
    assert_eq!(response["code"], WsResponseCode::NO_PERMISSION as i32);
    assert_eq!(stats.room_count().await, 0);

    client.close(None).await.expect("close client");
    wait_for_client_count(&stats, 0).await;
    stop.stop();
    server
        .await
        .expect("runtime task joins")
        .expect("runtime stops");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runtime_times_out_idle_connections_and_rate_limits_control_frames() {
    let (stop, signal) = runtime_stop_channel();
    let (ready_tx, ready_rx) = sync_channel(1);
    let server = tokio::spawn(run_room_runtime_until_stopped_with_ready(
        RuntimeConfig {
            service_name: "runtime-timeout-rate-test",
            listen_addr: "127.0.0.1:0".to_owned(),
            idle_timeout: Duration::from_millis(100),
            heartbeat_interval: Duration::from_secs(60),
        },
        RuntimeHandler,
        signal,
        ready_tx,
    ));
    let stats = tokio::task::spawn_blocking(move || {
        ready_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("runtime reports listen address")
    })
    .await
    .expect("read runtime stats");

    let (mut idle_client, _) = connect_async(format!("ws://{}", stats.listen_addr()))
        .await
        .expect("connect idle client");
    let _ = timeout(Duration::from_secs(1), idle_client.next())
        .await
        .expect("idle connection terminates");
    wait_for_client_count(&stats, 0).await;

    let (mut busy_client, _) = connect_async(format!("ws://{}", stats.listen_addr()))
        .await
        .expect("connect busy client");
    busy_client
        .send(Message::Binary(vec![1, 2].into()))
        .await
        .expect("send ignored binary frame");
    for _ in 0..61 {
        busy_client
            .send(Message::Ping(Vec::new().into()))
            .await
            .expect("send control frame");
    }
    // The runtime intentionally drops its writer after queuing the policy close.
    // Depending on TCP scheduling the peer observes either that close frame or a reset.
    let terminated = timeout(Duration::from_secs(1), async {
        loop {
            match busy_client.next().await {
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => continue,
                outcome => break outcome,
            }
        }
    })
    .await
    .expect("rate-limited connection terminates");
    assert!(matches!(
        terminated,
        Some(Ok(Message::Close(_))) | Some(Err(_)) | None
    ));
    wait_for_client_count(&stats, 0).await;

    stop.stop();
    server
        .await
        .expect("runtime task joins")
        .expect("runtime stops");
}
