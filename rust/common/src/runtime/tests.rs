use std::{collections::HashMap, sync::Arc};

use serde_json::Value;
use share_type_public::{GameId, WsJoinRequest, WsResponseCode, WsWithoutDataResponse};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{Mutex, mpsc, watch},
};
use tokio_tungstenite::tungstenite::Message;

use crate::{
    ClientRequest, Delivery, Dispatch, GameSettings, OutboundPayload, RequestResponse, RoomService,
    SessionId, SettingsBuilderResult, SharedGameState,
};

use super::{
    GameHandler, JoinAuthorization, RuntimeConfig, RuntimeStats, SessionSendError, SessionSender,
    cleanup_abandoned_room_after, deliver, run_game_server, run_room_runtime,
    run_room_runtime_until_stopped, runtime_stop_channel, session_sender_channel,
    tests::TestHandler,
};

struct DefaultHookHandler;

impl GameHandler for DefaultHookHandler {
    fn build_game_state(&self) -> Box<dyn crate::GameState> {
        Box::new(SharedGameState::new())
    }

    fn build_room_settings(&self) -> SettingsBuilderResult {
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

#[tokio::test]
async fn session_sender_reports_closed_receivers_without_disconnect_signal() {
    let (tx, mut rx) = mpsc::channel(1);
    let (disconnect, mut disconnected) = watch::channel(false);
    let sender = SessionSender::new(tx, disconnect);

    sender
        .send(Message::Text("delivered".into()))
        .expect("send to open receiver");
    assert_eq!(rx.recv().await, Some(Message::Text("delivered".into())));
    drop(rx);
    assert_eq!(
        sender.send(Message::Text("closed".into())),
        Err(SessionSendError::Closed)
    );
    assert!(!*disconnected.borrow_and_update());
}

#[tokio::test]
async fn stop_signal_supports_waiting_and_already_stopped_consumers() {
    let (stop_handle, mut waiting_signal) = runtime_stop_channel();
    let waiter = tokio::spawn(async move {
        waiting_signal.stopped().await;
        waiting_signal.is_stopped()
    });
    tokio::task::yield_now().await;
    stop_handle.stop();
    assert!(waiter.await.expect("stop waiter task"));

    let (immediate_handle, mut immediate_signal) = runtime_stop_channel();
    immediate_handle.stop();
    immediate_signal.stopped().await;
    assert!(immediate_signal.is_stopped());
    let receiver = immediate_signal.into_receiver();
    assert!(*receiver.borrow());
}

#[tokio::test]
async fn default_game_handler_hooks_keep_nonmember_rooms_enabled() {
    let mut handler = DefaultHookHandler;
    assert!(handler.accepts_game_id(GameId::LANDLORD));
    assert!(!handler.accepts_game_id(GameId::TRACTOR));
    assert!(!handler.supports_ai_players());

    let authorization = handler
        .authorize_join(&WsJoinRequest {
            name: "player".to_owned(),
            password: "room".to_owned(),
            game_id: GameId::LANDLORD,
            session_id: String::new(),
            avatar_url: String::new(),
        })
        .await;
    assert_eq!(authorization, JoinAuthorization::ALLOW_NONMEMBER);

    let senders = Arc::new(Mutex::new(HashMap::new()));
    let room_service = Arc::new(Mutex::new(RoomService::default()));
    handler.set_context(Arc::clone(&senders), Arc::clone(&room_service));
    let mut service = RoomService::default();
    let mut dispatch = Dispatch::default();
    handler.after_common_request(
        &mut service,
        1,
        &ClientRequest {
            route: 0,
            data: Value::Null,
        },
        &mut dispatch,
    );
    assert!(dispatch.messages.is_empty());
    assert_eq!(handler.build_room_settings().0.max_players, 4);
    assert_eq!(handler.build_game_state().players().len(), 0);
    assert!(
        handler
            .handle_game_request(
                &mut service,
                1,
                ClientRequest {
                    route: 0,
                    data: Value::Null,
                },
            )
            .messages
            .is_empty()
    );
}

#[tokio::test]
async fn delivery_ignores_unknown_or_closed_session_queues() {
    let senders = Arc::new(Mutex::new(HashMap::new()));
    let (closed_sender, closed_receiver, _) = session_sender_channel(1);
    drop(closed_receiver);
    senders.lock().await.insert(2, closed_sender);
    let dispatch = Dispatch {
        messages: vec![
            Delivery {
                recipient: 1,
                payload: OutboundPayload::Response(RequestResponse::WithoutData(
                    WsWithoutDataResponse {
                        route: 2,
                        code: WsResponseCode::OK,
                    },
                )),
            },
            Delivery {
                recipient: 2,
                payload: OutboundPayload::Response(RequestResponse::WithoutData(
                    WsWithoutDataResponse {
                        route: 2,
                        code: WsResponseCode::OK,
                    },
                )),
            },
        ],
    };

    deliver(dispatch, &senders)
        .await
        .expect("delivery failure handling must not abort the runtime");
}

#[tokio::test]
async fn delayed_cleanup_helper_removes_an_abandoned_room_after_its_deadline() {
    let mut service = RoomService::default();
    service.connect(1);
    service
        .handle_common_request(
            1,
            &share_type_public::WsRequest {
                route: share_type_public::Routes::JOIN as i32,
                data: serde_json::json!({
                    "name": "owner",
                    "password": "delayed-cleanup-room",
                    "game_id": GameId::LANDLORD as i32
                }),
            },
            GameId::LANDLORD,
            || (GameSettings::new(1, 4), HashMap::new()),
        )
        .expect("join is a common request");
    let (_, cleanup) = service.disconnect_with_cleanup_grace(1);
    let service = Arc::new(Mutex::new(service));

    cleanup_abandoned_room_after(
        Arc::clone(&service),
        cleanup.expect("cleanup token"),
        std::time::Duration::ZERO,
    )
    .await;

    assert_eq!(service.lock().await.room_count(), 0);
}

#[tokio::test]
async fn runtime_stats_expose_empty_room_and_takeover_state() {
    let stats = RuntimeStats {
        room_service: Arc::new(Mutex::new(RoomService::default())),
        senders: Arc::new(Mutex::new(HashMap::new())),
        listen_addr: "127.0.0.1:12345".parse().expect("parse test address"),
    };

    assert_eq!(stats.listen_addr().port(), 12345);
    assert_eq!(stats.client_count().await, 0);
    assert_eq!(stats.room_count().await, 0);
    assert!(!stats.room_position_is_ai_takeover("missing-room", 0).await);
}

#[tokio::test]
async fn runtime_startup_helpers_validate_addresses_and_stop_cleanly() {
    let invalid_host = run_game_server(
        "test",
        Some("invalid-host".to_owned()),
        Some(9001),
        std::time::Duration::from_secs(1),
        DefaultHookHandler,
    )
    .await
    .expect_err("invalid hosts must not start a runtime");
    assert!(invalid_host.to_string().contains("invalid host"));

    let reserved_port = run_game_server(
        "test",
        Some("127.0.0.1".to_owned()),
        Some(9000),
        std::time::Duration::from_secs(1),
        DefaultHookHandler,
    )
    .await
    .expect_err("reserved ports must not start a runtime");
    assert!(reserved_port.to_string().contains("port must be > 9000"));

    let (stop_handle, stop_signal) = runtime_stop_channel();
    stop_handle.stop();
    let stats = run_room_runtime_until_stopped(
        RuntimeConfig {
            service_name: "test",
            listen_addr: "127.0.0.1:0".to_owned(),
            idle_timeout: std::time::Duration::from_secs(1),
            heartbeat_interval: std::time::Duration::from_secs(1),
        },
        DefaultHookHandler,
        stop_signal,
    )
    .await
    .expect("an already-stopped runtime should return cleanly");
    assert_eq!(stats.client_count().await, 0);
}

#[tokio::test]
async fn runtime_startup_helpers_report_listener_conflicts() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("reserve a local test port");
    let port = listener.local_addr().expect("read local address").port();

    let helper_error = run_game_server(
        "test",
        Some("127.0.0.1".to_owned()),
        Some(port),
        std::time::Duration::from_secs(1),
        DefaultHookHandler,
    )
    .await
    .expect_err("an occupied port cannot start through the helper");
    assert!(helper_error.to_string().contains("port is not bindable"));

    let runtime_error = run_room_runtime(
        RuntimeConfig {
            service_name: "test",
            listen_addr: format!("127.0.0.1:{port}"),
            idle_timeout: std::time::Duration::from_secs(1),
            heartbeat_interval: std::time::Duration::from_secs(1),
        },
        DefaultHookHandler,
    )
    .await
    .expect_err("an occupied port cannot start the runtime");
    assert!(!runtime_error.to_string().is_empty());

    drop(listener);
}

#[tokio::test]
async fn game_server_helper_starts_on_a_bindable_port() {
    let reserved = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("reserve a local test port");
    let port = reserved
        .local_addr()
        .expect("read reserved local address")
        .port();
    drop(reserved);

    let server = tokio::spawn(run_game_server(
        "test",
        Some("127.0.0.1".to_owned()),
        Some(port),
        std::time::Duration::from_secs(1),
        DefaultHookHandler,
    ));
    let client = tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            match TcpStream::connect(("127.0.0.1", port)).await {
                Ok(stream) => break stream,
                Err(_) => tokio::time::sleep(std::time::Duration::from_millis(10)).await,
            }
        }
    })
    .await
    .expect("game server starts listening");
    drop(client);

    server.abort();
    assert!(
        server
            .await
            .expect_err("server task is cancelled")
            .is_cancelled()
    );
}

#[test]
fn test_handler_exposes_the_expected_game_contract() {
    let mut handler = TestHandler;
    assert_eq!(handler.game_id(), GameId::ALL);
    assert_eq!(handler.build_room_settings().0.max_players, 4);
    assert!(handler.build_game_state().players().is_empty());
    assert!(
        handler
            .handle_game_request(
                &mut RoomService::default(),
                1,
                ClientRequest {
                    route: 0,
                    data: Value::Null,
                },
            )
            .messages
            .is_empty()
    );
}
