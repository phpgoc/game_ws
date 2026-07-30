use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::sync_channel,
    },
    time::Duration,
};

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

struct GameRouteRuntimeHandler;

impl GameHandler for GameRouteRuntimeHandler {
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
        room_service: &mut RoomService,
        session_id: SessionId,
        request: ClientRequest,
    ) -> Dispatch {
        room_service.error_response(session_id, request.route, WsResponseCode::OK)
    }
}

struct MembershipRuntimeHandler {
    context_was_set: Arc<AtomicBool>,
    membership_seen_by_hook: Arc<AtomicBool>,
}

impl GameHandler for MembershipRuntimeHandler {
    fn authorize_join(&self, _join: &share_type_public::WsJoinRequest) -> JoinAuthorizationFuture {
        Box::pin(async {
            JoinAuthorization {
                can_create_room: true,
                has_active_membership: true,
            }
        })
    }

    fn after_common_request(
        &mut self,
        room_service: &mut RoomService,
        session_id: SessionId,
        request: &ClientRequest,
        _dispatch: &mut Dispatch,
    ) {
        if request.route != Routes::JOIN as i32 {
            return;
        }
        let has_membership = room_service
            .room_key_of(session_id)
            .is_some_and(|room_key| room_service.room_position_has_active_membership(&room_key, 0));
        self.membership_seen_by_hook
            .store(has_membership, Ordering::SeqCst);
    }

    fn supports_ai_players(&self) -> bool {
        true
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

    fn set_context(
        &mut self,
        _senders: ws_common::SessionSenders,
        _room_service: Arc<tokio::sync::Mutex<RoomService>>,
    ) {
        self.context_was_set.store(true, Ordering::SeqCst);
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
async fn runtime_dispatches_non_common_routes_to_the_game_handler() {
    let (stop, signal) = runtime_stop_channel();
    let (ready_tx, ready_rx) = sync_channel(1);
    let server = tokio::spawn(run_room_runtime_until_stopped_with_ready(
        RuntimeConfig {
            service_name: "runtime-game-route-test",
            listen_addr: "127.0.0.1:0".to_owned(),
            idle_timeout: Duration::from_secs(2),
            heartbeat_interval: Duration::from_secs(60),
        },
        GameRouteRuntimeHandler,
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
            serde_json::json!({"route": 123_456, "data": {"move": "test"}})
                .to_string()
                .into(),
        ))
        .await
        .expect("send game route");
    let response = timeout(Duration::from_secs(1), async {
        loop {
            match client
                .next()
                .await
                .expect("websocket frame")
                .expect("valid frame")
            {
                Message::Text(text) => break text,
                Message::Ping(_) | Message::Pong(_) => continue,
                frame => panic!("unexpected game response frame: {frame:?}"),
            }
        }
    })
    .await
    .expect("game response arrives");
    let response: serde_json::Value = serde_json::from_str(&response).expect("response json");
    assert_eq!(response["route"], 123_456);
    assert_eq!(response["code"], WsResponseCode::OK as i32);

    client.close(None).await.expect("close client");
    wait_for_client_count(&stats, 0).await;
    stop.stop();
    server
        .await
        .expect("runtime task joins")
        .expect("runtime stops");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runtime_propagates_join_membership_before_running_common_hooks() {
    let context_was_set = Arc::new(AtomicBool::new(false));
    let membership_seen_by_hook = Arc::new(AtomicBool::new(false));
    let (stop, signal) = runtime_stop_channel();
    let (ready_tx, ready_rx) = sync_channel(1);
    let server = tokio::spawn(run_room_runtime_until_stopped_with_ready(
        RuntimeConfig {
            service_name: "runtime-membership-test",
            listen_addr: "127.0.0.1:0".to_owned(),
            idle_timeout: Duration::from_secs(2),
            heartbeat_interval: Duration::from_secs(60),
        },
        MembershipRuntimeHandler {
            context_was_set: Arc::clone(&context_was_set),
            membership_seen_by_hook: Arc::clone(&membership_seen_by_hook),
        },
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
    assert!(context_was_set.load(Ordering::SeqCst));

    let (mut client, _) = connect_async(format!("ws://{}", stats.listen_addr()))
        .await
        .expect("connect websocket client");
    client
        .send(Message::Text(
            serde_json::json!({
                "route": Routes::JOIN as i32,
                "data": {
                    "name": "member",
                    "password": "membership-room",
                    "game_id": GameId::LANDLORD as i32,
                }
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("send member join");
    let response = timeout(Duration::from_secs(1), async {
        loop {
            match client
                .next()
                .await
                .expect("websocket frame")
                .expect("valid frame")
            {
                Message::Text(text) => break text,
                Message::Ping(_) | Message::Pong(_) => continue,
                frame => panic!("unexpected membership response frame: {frame:?}"),
            }
        }
    })
    .await
    .expect("join response arrives");
    let response: serde_json::Value = serde_json::from_str(&response).expect("response json");
    assert_eq!(response["code"], WsResponseCode::JOINED as i32);
    assert!(membership_seen_by_hook.load(Ordering::SeqCst));

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
