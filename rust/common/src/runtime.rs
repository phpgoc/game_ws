use std::{
    collections::HashMap,
    future::Future,
    net::SocketAddr,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::SyncSender,
    },
    time::{Duration, Instant},
};

use anyhow::Context;
use futures_util::{SinkExt, StreamExt};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{Mutex, OwnedSemaphorePermit, Semaphore, mpsc, watch},
};
use tokio_tungstenite::{
    accept_async_with_config,
    tungstenite::{
        Message,
        protocol::{CloseFrame, WebSocketConfig, frame::coding::CloseCode},
    },
};
use tracing::{error, info, warn};

use crate::{
    ClientRequest, Dispatch, RoomService, SessionId, SettingsBuilderResult,
    cli::parse_bind_cli,
    from_message,
    net::{resolve_host, resolve_port},
    to_text_message,
};

const MAX_CONNECTIONS: usize = 4_096;
const OUTBOUND_QUEUE_CAPACITY: usize = 256;
const INBOUND_MESSAGES_PER_SECOND: f64 = 30.0;
const INBOUND_MESSAGE_BURST: f64 = 60.0;

struct ConnectionContext<H> {
    idle_timeout: Duration,
    heartbeat_interval: Duration,
    senders: SessionSenders,
    room_service: Arc<Mutex<RoomService>>,
    game_handler: Arc<Mutex<H>>,
    stop_signal: StopSignal,
    connection_permit: OwnedSemaphorePermit,
}

struct MessageRateLimiter {
    tokens: f64,
    last_refill: Instant,
}

impl MessageRateLimiter {
    fn new() -> Self {
        Self {
            tokens: INBOUND_MESSAGE_BURST,
            last_refill: Instant::now(),
        }
    }

    fn allow(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.last_refill = now;
        self.tokens =
            (self.tokens + elapsed * INBOUND_MESSAGES_PER_SECOND).min(INBOUND_MESSAGE_BURST);
        if self.tokens < 1.0 {
            return false;
        }
        self.tokens -= 1.0;
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JoinAuthorization {
    pub can_create_room: bool,
    pub has_active_membership: bool,
}

impl JoinAuthorization {
    pub const ALLOW_NONMEMBER: Self = Self {
        can_create_room: true,
        has_active_membership: false,
    };
}

pub type JoinAuthorizationFuture =
    Pin<Box<dyn Future<Output = JoinAuthorization> + Send + 'static>>;

pub trait GameHandler: Send + 'static {
    fn accepts_game_id(&self, game_id: share_type_public::GameId) -> bool {
        game_id == self.game_id()
    }

    fn after_common_request(
        &mut self,
        _room_service: &mut RoomService,
        _session_id: SessionId,
        _request: &ClientRequest,
        _dispatch: &mut Dispatch,
    ) {
        // Optional: override in games that need to enrich common responses/events.
    }

    fn authorize_join(&self, _join: &share_type_public::WsJoinRequest) -> JoinAuthorizationFuture {
        Box::pin(async { JoinAuthorization::ALLOW_NONMEMBER })
    }

    fn supports_ai_players(&self) -> bool {
        false
    }
    /// 创建游戏状态。
    /// 在首个 JOIN 建房成功后立即调用，并将当前成员 populate 进去。
    fn build_game_state(&self) -> Box<dyn crate::game_state::GameState>;

    fn build_room_settings(&self) -> SettingsBuilderResult;

    fn game_id(&self) -> share_type_public::GameId;
    fn handle_game_request(
        &mut self,
        room_service: &mut RoomService,
        session_id: SessionId,
        request: ClientRequest,
    ) -> Dispatch;
    fn set_context(&mut self, _senders: SessionSenders, _room_service: Arc<Mutex<RoomService>>) {
        // Optional: override in games that need access to senders/room_service for event loops
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub service_name: &'static str,
    pub listen_addr: String,
    pub idle_timeout: Duration,
    pub heartbeat_interval: Duration,
}

#[derive(Clone)]
pub struct RuntimeStats {
    room_service: Arc<Mutex<RoomService>>,
    senders: SessionSenders,
    listen_addr: SocketAddr,
}

pub struct RuntimeStopHandle {
    tx: watch::Sender<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionSendError {
    Full,
    Closed,
}

#[derive(Clone)]
pub struct SessionSender {
    tx: mpsc::Sender<Message>,
    disconnect: watch::Sender<bool>,
}

impl SessionSender {
    /// 用有界发送通道和断开通知通道创建单个会话的发送器。
    pub fn new(tx: mpsc::Sender<Message>, disconnect: watch::Sender<bool>) -> Self {
        Self { tx, disconnect }
    }

    /// 尝试将帧写入会话发送队列；队列满时通知连接主动断开。
    pub fn send(&self, frame: Message) -> Result<(), SessionSendError> {
        match self.tx.try_send(frame) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => {
                let _ = self.disconnect.send(true);
                Err(SessionSendError::Full)
            }
            Err(mpsc::error::TrySendError::Closed(_)) => Err(SessionSendError::Closed),
        }
    }
}

/// 创建有界会话发送器及其消息、断开通知接收器。
///
/// 游戏 crate 可用它在不打开真实 socket 的情况下测试消息分发路径。
pub fn session_sender_channel(
    capacity: usize,
) -> (
    SessionSender,
    mpsc::Receiver<Message>,
    watch::Receiver<bool>,
) {
    let (tx, rx) = mpsc::channel(capacity);
    let (disconnect, disconnected) = watch::channel(false);
    (SessionSender { tx, disconnect }, rx, disconnected)
}

pub type SessionSenders = Arc<Mutex<HashMap<SessionId, SessionSender>>>;

#[derive(Clone)]
pub struct StopSignal {
    rx: watch::Receiver<bool>,
}

async fn deliver(dispatch: Dispatch, senders: &SessionSenders) -> anyhow::Result<()> {
    let mut encoded = Vec::with_capacity(dispatch.messages.len());
    for message in dispatch.messages {
        encoded.push((message.recipient, to_text_message(&message.payload)?));
    }

    let senders = senders.lock().await;
    for (recipient, frame) in encoded {
        if let Some(tx) = senders.get(&recipient) {
            if let Err(err) = tx.send(frame) {
                warn!(recipient, ?err, "outbound queue rejected frame");
            }
        }
    }
    Ok(())
}

async fn handle_connection<H>(
    stream: TcpStream,
    peer: SocketAddr,
    session_id: SessionId,
    context: ConnectionContext<H>,
) -> anyhow::Result<()>
where
    H: GameHandler,
{
    let ConnectionContext {
        idle_timeout,
        heartbeat_interval,
        senders,
        room_service,
        game_handler,
        mut stop_signal,
        connection_permit: _connection_permit,
    } = context;
    let mut websocket_config = WebSocketConfig::default();
    websocket_config.max_message_size = Some(64 * 1024);
    websocket_config.max_frame_size = Some(16 * 1024);
    let ws = accept_async_with_config(stream, Some(websocket_config)).await?;
    let (mut sink, mut source) = ws.split();
    let (session_sender, mut rx, mut disconnect_rx) =
        session_sender_channel(OUTBOUND_QUEUE_CAPACITY);
    let heartbeat_tx = session_sender.clone();

    senders
        .lock()
        .await
        .insert(session_id, session_sender.clone());
    room_service.lock().await.connect(session_id);

    let writer = tokio::spawn(async move {
        while let Some(frame) = rx.recv().await {
            if sink.send(frame).await.is_err() {
                break;
            }
        }
    });
    let heartbeat = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(heartbeat_interval);
        loop {
            ticker.tick().await;
            if heartbeat_tx.send(Message::Ping(Vec::new().into())).is_err() {
                break;
            }
        }
    });
    let mut rate_limiter = MessageRateLimiter::new();

    loop {
        let frame = tokio::select! {
            _ = stop_signal.stopped() => break,
            changed = disconnect_rx.changed() => {
                if changed.is_ok() && *disconnect_rx.borrow() {
                    warn!(session_id, peer = %peer, "slow client exceeded outbound queue");
                }
                break;
            },
            result = tokio::time::timeout(idle_timeout, source.next()) => {
                match result {
                    Ok(Some(Ok(frame))) => frame,
                    Ok(Some(Err(err))) => {
                        info!(session_id, peer = %peer, ?err, "connection reset, treating as disconnect");
                        break;
                    }
                    Ok(None) => break,
                    Err(_) => {
                        warn!(session_id, peer = %peer, "idle timeout, closing connection");
                        break;
                    }
                }
            }
        };

        if !rate_limiter.allow() {
            warn!(session_id, peer = %peer, "inbound message rate limit exceeded");
            let _ = session_sender.send(Message::Close(Some(CloseFrame {
                code: CloseCode::Policy,
                reason: "message rate limit exceeded".into(),
            })));
            break;
        }

        let request = match from_message::<ClientRequest>(frame) {
            Ok(Some(request)) => request,
            Ok(None) => continue,
            Err(err) => {
                warn!(session_id, peer = %peer, ?err, "invalid ws frame, ignored");
                continue;
            }
        };

        let parsed_join = (request.route == share_type_public::Routes::JOIN as i32)
            .then(|| {
                serde_json::from_value::<share_type_public::WsJoinRequest>(request.data.clone())
                    .ok()
            })
            .flatten();
        let valid_join = if let Some(join) = parsed_join.as_ref() {
            let handler = game_handler.lock().await;
            !join.name.is_empty()
                && !join.password.is_empty()
                && handler.accepts_game_id(join.game_id)
        } else {
            false
        };
        let join_authorization = if valid_join {
            let join = parsed_join.as_ref().expect("membership join parsed");
            Some(game_handler.lock().await.authorize_join(join).await)
        } else {
            None
        };

        let dispatch = {
            let mut room = room_service.lock().await;
            let mut handler = game_handler.lock().await;
            let creates_room_on_join = parsed_join
                .as_ref()
                .is_some_and(|join| !room.room_exists(&join.password));
            let common_dispatch = if creates_room_on_join
                && join_authorization.is_some_and(|authorization| !authorization.can_create_room)
            {
                Some(room.error_response(
                    session_id,
                    share_type_public::Routes::JOIN as i32,
                    share_type_public::WsResponseCode::NO_PERMISSION,
                ))
            } else {
                room.handle_common_request_with_game_acceptance(
                    session_id,
                    &request,
                    |game_id| handler.accepts_game_id(game_id),
                    || handler.build_room_settings(),
                )
            };
            if let Some(mut dispatch) = common_dispatch {
                // 首个 JOIN 建房成功后，挂载游戏态，确保后续逻辑走具体游戏状态。
                if creates_room_on_join
                    && let Some(join) = parsed_join.as_ref()
                    && room.room_key_of(session_id).as_deref() == Some(join.password.as_str())
                {
                    let room_key = join.password.clone();
                    let mut gs = handler.build_game_state();
                    for (sid, name, pos, avatar) in room.room_members(&room_key) {
                        gs.add_player(pos, sid, &name);
                        gs.set_avatar(pos, &avatar);
                    }
                    room.set_room_game_state(&room_key, gs);
                }
                if request.route == share_type_public::Routes::JOIN as i32
                    && let (Some(join), Some(authorization)) =
                        (parsed_join.as_ref(), join_authorization)
                    && room.room_key_of(session_id).as_deref() == Some(join.password.as_str())
                {
                    room.set_session_active_membership(
                        session_id,
                        authorization.has_active_membership,
                    );
                }
                handler.after_common_request(&mut room, session_id, &request, &mut dispatch);
                dispatch
            } else {
                handler.handle_game_request(&mut room, session_id, request)
            }
        };

        deliver(dispatch, &senders).await?;
    }

    let disconnect_dispatch = room_service.lock().await.disconnect(session_id);
    senders.lock().await.remove(&session_id);
    deliver(disconnect_dispatch, &senders).await?;
    heartbeat.abort();
    writer.abort();
    Ok(())
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .try_init();
}

async fn run_game_server<H>(
    service_name: &'static str,
    host: Option<String>,
    port: Option<u16>,
    idle_timeout: Duration,
    handler: H,
) -> anyhow::Result<()>
where
    H: GameHandler,
{
    let host = resolve_host(host)?;
    let port = resolve_port(host, port)?;

    run_room_runtime(
        RuntimeConfig {
            service_name,
            listen_addr: format!("{host}:{port}"),
            idle_timeout,
            heartbeat_interval: Duration::from_secs(20),
        },
        handler,
    )
    .await
}

/// 解析通用命令行地址参数，并启动指定游戏的 WebSocket 服务。
pub async fn run_game_server_with_cli<H>(
    service_name: &'static str,
    idle_timeout: Duration,
    handler: H,
) -> anyhow::Result<()>
where
    H: GameHandler,
{
    let cli = parse_bind_cli();
    run_game_server(service_name, cli.host, cli.port, idle_timeout, handler).await
}

/// 以给定配置启动房间运行时，并一直运行到进程终止。
pub async fn run_room_runtime<H>(config: RuntimeConfig, handler: H) -> anyhow::Result<()>
where
    H: GameHandler,
{
    let (_stop_handle, stop_signal) = runtime_stop_channel();
    run_room_runtime_until_stopped(config, handler, stop_signal)
        .await
        .map(|_| ())
}

/// 以给定停止信号运行房间服务，并在停止后返回运行时统计信息。
pub async fn run_room_runtime_until_stopped<H>(
    config: RuntimeConfig,
    handler: H,
    stop_signal: StopSignal,
) -> anyhow::Result<RuntimeStats>
where
    H: GameHandler,
{
    run_room_runtime_until_stopped_inner(config, handler, stop_signal, None).await
}

async fn run_room_runtime_until_stopped_inner<H>(
    config: RuntimeConfig,
    handler: H,
    mut stop_signal: StopSignal,
    ready: Option<SyncSender<RuntimeStats>>,
) -> anyhow::Result<RuntimeStats>
where
    H: GameHandler,
{
    init_tracing();

    let listener = TcpListener::bind(&config.listen_addr)
        .await
        .with_context(|| format!("bind {} failed", config.listen_addr))?;
    let listen_addr = listener
        .local_addr()
        .context("read websocket listen address")?;
    let ai_players_enabled = handler.supports_ai_players();
    info!(
        service = config.service_name,
        listen = %format!(" ws://{listen_addr}"),
        ai_players_enabled,
        "ws server started"
    );

    let senders: SessionSenders = Arc::new(Mutex::new(HashMap::new()));
    let room_service = Arc::new(Mutex::new(RoomService::with_ai_players_enabled(
        ai_players_enabled,
    )));
    let stats = RuntimeStats {
        room_service: Arc::clone(&room_service),
        senders: Arc::clone(&senders),
        listen_addr,
    };
    let game_handler = Arc::new(Mutex::new(handler));

    // Set context for game-specific initialization
    {
        let mut h = game_handler.lock().await;
        h.set_context(Arc::clone(&senders), Arc::clone(&room_service));
    }

    if let Some(ready) = ready {
        let _ = ready.send(stats.clone());
    }

    let next_session = Arc::new(AtomicU64::new(1));
    let connection_slots = Arc::new(Semaphore::new(MAX_CONNECTIONS));

    loop {
        let (stream, peer) = tokio::select! {
            _ = stop_signal.stopped() => break,
            result = listener.accept() => result?,
        };
        let Ok(connection_permit) = Arc::clone(&connection_slots).try_acquire_owned() else {
            warn!(peer = %peer, max_connections = MAX_CONNECTIONS, "connection limit reached");
            drop(stream);
            continue;
        };
        let session_id = next_session.fetch_add(1, Ordering::Relaxed);
        let context = ConnectionContext {
            idle_timeout: config.idle_timeout,
            heartbeat_interval: config.heartbeat_interval,
            senders: Arc::clone(&senders),
            room_service: Arc::clone(&room_service),
            game_handler: Arc::clone(&game_handler),
            stop_signal: stop_signal.clone(),
            connection_permit,
        };

        tokio::spawn(async move {
            if let Err(err) = handle_connection(stream, peer, session_id, context).await {
                error!(session_id, peer = %peer, ?err, "connection ended with error");
            } else {
                info!(session_id, peer = %peer, "connection closed");
            }
        });
    }

    Ok(stats)
}

/// 运行房间服务，在监听就绪后经同步通道报告统计句柄，供集成测试使用。
pub async fn run_room_runtime_until_stopped_with_ready<H>(
    config: RuntimeConfig,
    handler: H,
    stop_signal: StopSignal,
    ready: SyncSender<RuntimeStats>,
) -> anyhow::Result<RuntimeStats>
where
    H: GameHandler,
{
    run_room_runtime_until_stopped_inner(config, handler, stop_signal, Some(ready)).await
}

/// 创建一对运行时停止控制器和等待信号。
pub fn runtime_stop_channel() -> (RuntimeStopHandle, StopSignal) {
    let (tx, rx) = watch::channel(false);
    (RuntimeStopHandle { tx }, StopSignal::new(rx))
}

impl RuntimeStats {
    /// 返回服务实际绑定的监听地址。
    pub fn listen_addr(&self) -> SocketAddr {
        self.listen_addr
    }

    /// 异步统计当前仍连接的 WebSocket 客户端数量。
    pub async fn client_count(&self) -> usize {
        self.senders.lock().await.len()
    }

    /// 异步统计当前房间数量。
    pub async fn room_count(&self) -> usize {
        self.room_service.lock().await.room_count()
    }

    /// 判断某房间指定座位是否正由 AI 临时接管。
    pub async fn room_position_is_ai_takeover(&self, room_key: &str, position: usize) -> bool {
        self.room_service
            .lock()
            .await
            .room_position_is_ai_takeover(room_key, position)
    }
}

impl RuntimeStopHandle {
    /// 发出停止信号，使运行时停止接受新连接并退出主循环。
    pub fn stop(&self) {
        let _ = self.tx.send(true);
    }
}

impl StopSignal {
    /// 取出底层停止信号接收器，供需要直接监听 `watch` 的调用方使用。
    pub fn into_receiver(self) -> watch::Receiver<bool> {
        self.rx
    }

    fn is_stopped(&self) -> bool {
        *self.rx.borrow()
    }

    fn new(rx: watch::Receiver<bool>) -> Self {
        Self { rx }
    }

    /// 等待停止信号；若已停止则立即返回。
    pub async fn stopped(&mut self) {
        if self.is_stopped() {
            return;
        }
        let _ = self.rx.changed().await;
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::mpsc::sync_channel, time::Duration};

    use share_type_public::GameId;
    use tokio_tungstenite::tungstenite::Message;

    use crate::{
        ClientRequest, Dispatch, GameSettings, RoomService, SessionId, game_state::SharedGameState,
    };

    use super::{
        GameHandler, INBOUND_MESSAGE_BURST, MessageRateLimiter, RuntimeConfig, SessionSendError,
        run_room_runtime_until_stopped_with_ready, runtime_stop_channel, session_sender_channel,
    };

    pub(super) struct TestHandler;

    #[test]
    fn inbound_rate_limiter_rejects_messages_past_burst() {
        let mut limiter = MessageRateLimiter::new();
        for _ in 0..INBOUND_MESSAGE_BURST as usize {
            assert!(limiter.allow());
        }
        assert!(!limiter.allow());
    }

    #[tokio::test]
    async fn full_outbound_queue_signals_disconnect() {
        let (sender, _rx, mut disconnected) = session_sender_channel(1);

        assert!(sender.send(Message::Text("first".into())).is_ok());
        assert_eq!(
            sender.send(Message::Text("second".into())),
            Err(SessionSendError::Full)
        );
        disconnected.changed().await.unwrap();
        assert!(*disconnected.borrow());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn runtime_reports_ready_and_stops_cleanly() {
        let (stop_handle, stop_signal) = runtime_stop_channel();
        let (ready_tx, ready_rx) = sync_channel(1);
        let runtime = tokio::spawn(run_room_runtime_until_stopped_with_ready(
            RuntimeConfig {
                service_name: "test",
                listen_addr: "127.0.0.1:0".to_string(),
                idle_timeout: Duration::from_secs(1),
                heartbeat_interval: Duration::from_secs(1),
            },
            TestHandler,
            stop_signal,
            ready_tx,
        ));

        let stats = ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(stats.client_count().await, 0);
        assert_eq!(stats.room_count().await, 0);

        stop_handle.stop();
        let stopped_stats = runtime.await.unwrap().unwrap();
        assert_eq!(stopped_stats.client_count().await, 0);
        assert_eq!(stopped_stats.room_count().await, 0);
    }

    impl GameHandler for TestHandler {
        fn build_game_state(&self) -> Box<dyn crate::game_state::GameState> {
            Box::new(SharedGameState::new())
        }

        fn build_room_settings(&self) -> crate::SettingsBuilderResult {
            (GameSettings::new(1, 4), HashMap::new())
        }

        fn game_id(&self) -> GameId {
            GameId::ALL
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
}

#[cfg(test)]
#[path = "runtime/tests.rs"]
mod external_tests;
