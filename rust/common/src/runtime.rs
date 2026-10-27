use std::{
    collections::HashMap,
    future::Future,
    net::SocketAddr,
    panic::AssertUnwindSafe,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::SyncSender,
    },
    time::{Duration, Instant},
};

use anyhow::Context;
use futures_util::{FutureExt, SinkExt, StreamExt};
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
use tracing::{debug, error, warn};

use crate::{
    ClientRequest, Dispatch, OutboundPayload, RequestResponse, RoomService, SessionId,
    SettingsBuilderResult,
    cli::parse_bind_cli,
    from_message,
    net::{resolve_host, resolve_port},
    room::RoomCleanupToken,
    to_text_message,
};

const MAX_CONNECTIONS: usize = 4_096;
const OUTBOUND_QUEUE_CAPACITY: usize = 256;
const INBOUND_MESSAGES_PER_SECOND: f64 = 30.0;
const INBOUND_MESSAGE_BURST: f64 = 60.0;
const LAST_HUMAN_DISCONNECT_GRACE: Duration = Duration::from_secs(30);

struct ConnectionContext {
    idle_timeout: Duration,
    heartbeat_interval: Duration,
    senders: SessionSenders,
    registry: Arc<GameRegistry>,
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

    fn normalize_common_request(&self, _request: &mut ClientRequest) {
        // Optional: override when a game needs to retire a formerly public common setting
        // without breaking older clients that still send it.
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

struct GameSlot {
    handler: Arc<Mutex<Box<dyn GameHandler>>>,
    room_service: Arc<Mutex<RoomService>>,
}

/// 按 JOIN 中的 GameId 将连接分发到各自游戏运行时的注册表。
#[derive(Default)]
pub struct GameRegistry {
    slots_by_game_id: HashMap<i32, Arc<GameSlot>>,
    slots: Vec<Arc<GameSlot>>,
    allow_unbound_single_game_requests: bool,
}

impl GameRegistry {
    /// 创建空注册表。
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册游戏处理器，并为其接受的所有 GameId 创建独立房间服务。
    pub fn register<H>(&mut self, handler: H) -> anyhow::Result<()>
    where
        H: GameHandler,
    {
        const GAME_IDS: [share_type_public::GameId; 13] = [
            share_type_public::GameId::ALL,
            share_type_public::GameId::LANDLORD,
            share_type_public::GameId::SHENYANG_MAHJONG,
            share_type_public::GameId::TEXAS_HOLD_EM,
            share_type_public::GameId::TRACTOR,
            share_type_public::GameId::OPEN_HOLD_EM,
            share_type_public::GameId::SHORT_DECK_HOLD_EM,
            share_type_public::GameId::OMAHA_HOLD_EM,
            share_type_public::GameId::P2P,
            share_type_public::GameId::UPGRADE,
            share_type_public::GameId::DOMINOES,
            share_type_public::GameId::WHO_IS_LANDLORD,
            share_type_public::GameId::NIANGNIANG,
        ];

        let accepted_ids = GAME_IDS
            .into_iter()
            .filter(|game_id| handler.accepts_game_id(*game_id))
            .collect::<Vec<_>>();
        anyhow::ensure!(
            !accepted_ids.is_empty(),
            "game handler accepts no registered GameId"
        );
        for game_id in &accepted_ids {
            anyhow::ensure!(
                !self.slots_by_game_id.contains_key(&i32::from(*game_id)),
                "GameId {game_id:?} is already registered"
            );
        }

        let room_service = Arc::new(Mutex::new(RoomService::with_ai_players_enabled(
            handler.supports_ai_players(),
        )));
        let handler: Arc<Mutex<Box<dyn GameHandler>>> = Arc::new(Mutex::new(Box::new(handler)));
        let slot = Arc::new(GameSlot {
            handler: Arc::clone(&handler),
            room_service: Arc::clone(&room_service),
        });
        for game_id in accepted_ids {
            self.slots_by_game_id
                .insert(i32::from(game_id), Arc::clone(&slot));
        }
        self.slots.push(Arc::clone(&slot));

        Ok(())
    }

    async fn set_context(&self, senders: SessionSenders) {
        for slot in &self.slots {
            slot.handler
                .lock()
                .await
                .set_context(Arc::clone(&senders), Arc::clone(&slot.room_service));
        }
    }

    fn slot_for_game_id(&self, game_id: share_type_public::GameId) -> Option<Arc<GameSlot>> {
        self.slots_by_game_id.get(&i32::from(game_id)).cloned()
    }

    fn first_slot(&self) -> Option<Arc<GameSlot>> {
        self.slots.first().cloned()
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
    room_services: Vec<Arc<Mutex<RoomService>>>,
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
        if let Some(tx) = senders.get(&recipient)
            && let Err(err) = tx.send(frame)
        {
            warn!(recipient, ?err, "outbound queue rejected frame");
        }
    }
    Ok(())
}

async fn cleanup_abandoned_room_after(
    room_service: Arc<Mutex<RoomService>>,
    senders: SessionSenders,
    cleanup: RoomCleanupToken,
    grace: Duration,
) {
    tokio::time::sleep(grace).await;
    let dispatch = room_service
        .lock()
        .await
        .cleanup_abandoned_room_with_dispatch(cleanup);
    if let Some(dispatch) = dispatch
        && let Err(error) = deliver(dispatch, &senders).await
    {
        warn!(?error, "failed to deliver disconnected-room cleanup event");
    }
}

async fn handle_connection(
    stream: TcpStream,
    peer: SocketAddr,
    session_id: SessionId,
    context: ConnectionContext,
) -> anyhow::Result<()> {
    let ConnectionContext {
        idle_timeout,
        heartbeat_interval,
        senders,
        registry,
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
    let mut bound_slot = if registry.allow_unbound_single_game_requests {
        let slot = registry
            .first_slot()
            .expect("game registry must contain at least one handler");
        slot.room_service.lock().await.connect(session_id);
        Some(slot)
    } else {
        None
    };
    let mut bound_game_id: Option<share_type_public::GameId> = None;

    loop {
        let frame = tokio::select! {
            _ = stop_signal.stopped() => break,
            changed = disconnect_rx.changed() => {
                if changed.is_ok() && *disconnect_rx.borrow() {
                    warn!(
                        target: "lan_game_security",
                        session_id,
                        peer = %peer,
                        "slow client exceeded outbound queue"
                    );
                }
                break;
            },
            result = tokio::time::timeout(idle_timeout, source.next()) => {
                match result {
                    Ok(Some(Ok(frame))) => frame,
                    Ok(Some(Err(err))) => {
                        debug!(session_id, peer = %peer, ?err, "connection reset, treating as disconnect");
                        break;
                    }
                    Ok(None) => break,
                    Err(_) => {
                        debug!(session_id, peer = %peer, "idle timeout, closing connection");
                        break;
                    }
                }
            }
        };

        if !rate_limiter.allow() {
            warn!(
                target: "lan_game_security",
                session_id,
                peer = %peer,
                "inbound message rate limit exceeded"
            );
            let _ = session_sender.send(Message::Close(Some(CloseFrame {
                code: CloseCode::Policy,
                reason: "message rate limit exceeded".into(),
            })));
            break;
        }

        let mut request = match from_message::<ClientRequest>(frame) {
            Ok(Some(request)) => request,
            Ok(None) => continue,
            Err(err) => {
                warn!(
                    target: "lan_game_security",
                    session_id,
                    peer = %peer,
                    ?err,
                    "invalid ws frame, ignored"
                );
                continue;
            }
        };

        let request_route = request.route;
        let request_data = request.data.clone();

        let parsed_join = (request.route == share_type_public::Routes::JOIN as i32)
            .then(|| {
                serde_json::from_value::<share_type_public::WsJoinRequest>(request.data.clone())
                    .ok()
            })
            .flatten();
        let selected_slot = if request.route == share_type_public::Routes::JOIN as i32 {
            parsed_join
                .as_ref()
                .and_then(|join| registry.slot_for_game_id(join.game_id))
                .or_else(|| bound_slot.clone())
        } else {
            bound_slot.clone().or_else(|| {
                registry
                    .allow_unbound_single_game_requests
                    .then(|| registry.first_slot())
                    .flatten()
            })
        };

        let Some(slot) = selected_slot else {
            let fallback = registry
                .first_slot()
                .expect("game registry must contain at least one handler");
            let code = if request.route == share_type_public::Routes::JOIN as i32
                && parsed_join.is_some()
            {
                share_type_public::WsResponseCode::WRONG_GAME
            } else {
                share_type_public::WsResponseCode::NOT_LOGIN
            };
            let dispatch =
                fallback
                    .room_service
                    .lock()
                    .await
                    .error_response(session_id, request.route, code);
            let mut dispatch = dispatch;
            annotate_request_id(&mut dispatch, session_id, request_route, &request_data);
            deliver(dispatch, &senders).await?;
            continue;
        };

        if let Some(join) = parsed_join.as_ref()
            && let Some(previous_game_id) = bound_game_id
            && previous_game_id != join.game_id
        {
            let mut dispatch = slot.room_service.lock().await.error_response(
                session_id,
                request.route,
                share_type_public::WsResponseCode::WRONG_GAME,
            );
            annotate_request_id(&mut dispatch, session_id, request_route, &request_data);
            deliver(dispatch, &senders).await?;
            continue;
        }

        slot.handler
            .lock()
            .await
            .normalize_common_request(&mut request);
        let valid_join = if let Some(join) = parsed_join.as_ref() {
            let handler = slot.handler.lock().await;
            !join.name.is_empty()
                && !join.password.is_empty()
                && handler.accepts_game_id(join.game_id)
        } else {
            false
        };
        let join_authorization = if valid_join {
            let join = parsed_join.as_ref().expect("membership join parsed");
            // 授权 future 不借用 handler；数据库/缓存查询期间释放全服处理器锁。
            let authorization = slot.handler.lock().await.authorize_join(join);
            Some(authorization.await)
        } else {
            None
        };

        let provisional_slot =
            if request.route == share_type_public::Routes::JOIN as i32 && bound_slot.is_none() {
                slot.room_service.lock().await.connect(session_id);
                Some(Arc::clone(&slot))
            } else {
                None
            };

        // 房间状态与由它生成的消息按同一顺序提交。这里只写有界发送队列，
        // 不等待网络；后台游戏循环也须持有房间锁直到消息全部入队。
        let mut room = slot.room_service.lock().await;
        let dispatch = {
            let mut handler = slot.handler.lock().await;
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
            } else if room
                .room_key_of(session_id)
                .is_some_and(|room_key| room.is_room_paused(&room_key))
            {
                room.error_response(
                    session_id,
                    request.route,
                    share_type_public::WsResponseCode::NO_PERMISSION,
                )
            } else {
                handler.handle_game_request(&mut room, session_id, request)
            }
        };

        let joined_room = request_route == share_type_public::Routes::JOIN as i32
            && room.room_key_of(session_id).is_some();
        let mut dispatch = dispatch;
        annotate_request_id(&mut dispatch, session_id, request_route, &request_data);
        deliver(dispatch, &senders).await?;
        drop(room);

        if joined_room && bound_slot.is_none() {
            bound_slot = Some(Arc::clone(&slot));
            bound_game_id = parsed_join.as_ref().map(|join| join.game_id);
        } else if provisional_slot.is_some() && !joined_room {
            let provisional = provisional_slot.expect("checked above");
            let (cleanup_dispatch, cleanup) = provisional
                .room_service
                .lock()
                .await
                .disconnect_with_cleanup_grace(session_id);
            deliver(cleanup_dispatch, &senders).await?;
            if let Some(cleanup) = cleanup {
                tokio::spawn(cleanup_abandoned_room_after(
                    Arc::clone(&provisional.room_service),
                    Arc::clone(&senders),
                    cleanup,
                    LAST_HUMAN_DISCONNECT_GRACE,
                ));
            }
        }
    }

    if let Some(slot) = bound_slot {
        let mut room = slot.room_service.lock().await;
        let (disconnect_dispatch, cleanup) = room.disconnect_with_cleanup_grace(session_id);
        senders.lock().await.remove(&session_id);
        deliver(disconnect_dispatch, &senders).await?;
        drop(room);
        if let Some(cleanup) = cleanup {
            tokio::spawn(cleanup_abandoned_room_after(
                Arc::clone(&slot.room_service),
                Arc::clone(&senders),
                cleanup,
                LAST_HUMAN_DISCONNECT_GRACE,
            ));
        }
    } else {
        senders.lock().await.remove(&session_id);
    }
    heartbeat.abort();
    writer.abort();
    Ok(())
}

/// 将请求 payload 中的可选 request_id 回显到同一路由的私有响应，覆盖
/// common handler 和具体游戏 handler 的成功、失败分支。
fn annotate_request_id(
    dispatch: &mut Dispatch,
    session_id: SessionId,
    route: i32,
    data: &serde_json::Value,
) {
    let Some(request_id) = data
        .get("request_id")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 64 && value.is_ascii())
    else {
        return;
    };
    for delivery in &mut dispatch.messages {
        if delivery.recipient != session_id {
            continue;
        }
        match &mut delivery.payload {
            OutboundPayload::Response(RequestResponse::WithoutData(response))
                if response.route == route =>
            {
                response.request_id = Some(request_id.to_owned());
            }
            OutboundPayload::Response(RequestResponse::WithData(response))
                if response.route == route =>
            {
                response.request_id = Some(request_id.to_owned());
            }
            _ => {}
        }
    }
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

/// 与通用命令行入口相同，但在未指定 `--port` 时使用游戏自己的默认端口。
pub async fn run_game_server_with_cli_default_port<H>(
    service_name: &'static str,
    default_port: u16,
    idle_timeout: Duration,
    handler: H,
) -> anyhow::Result<()>
where
    H: GameHandler,
{
    let cli = parse_bind_cli();
    run_game_server(
        service_name,
        cli.host,
        cli.port.or(Some(default_port)),
        idle_timeout,
        handler,
    )
    .await
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
    let mut registry = GameRegistry::new();
    registry.register(handler)?;
    registry.allow_unbound_single_game_requests = true;
    run_game_registry_runtime_until_stopped_inner(config, registry, stop_signal, None).await
}

/// 运行注册表中的多个游戏，共用 WebSocket 监听端口。
pub async fn run_multi_game_runtime(
    config: RuntimeConfig,
    registry: GameRegistry,
) -> anyhow::Result<()> {
    let (_stop_handle, stop_signal) = runtime_stop_channel();
    run_multi_game_runtime_until_stopped(config, registry, stop_signal)
        .await
        .map(|_| ())
}

/// 运行多游戏房间服务，并在停止后返回聚合统计。
pub async fn run_multi_game_runtime_until_stopped(
    config: RuntimeConfig,
    registry: GameRegistry,
    stop_signal: StopSignal,
) -> anyhow::Result<RuntimeStats> {
    run_game_registry_runtime_until_stopped_inner(config, registry, stop_signal, None).await
}

/// 多游戏运行时在监听就绪后报告统计句柄，供集成测试使用。
pub async fn run_multi_game_runtime_until_stopped_with_ready(
    config: RuntimeConfig,
    registry: GameRegistry,
    stop_signal: StopSignal,
    ready: SyncSender<RuntimeStats>,
) -> anyhow::Result<RuntimeStats> {
    run_game_registry_runtime_until_stopped_inner(config, registry, stop_signal, Some(ready)).await
}

async fn run_game_registry_runtime_until_stopped_inner(
    config: RuntimeConfig,
    registry: GameRegistry,
    mut stop_signal: StopSignal,
    ready: Option<SyncSender<RuntimeStats>>,
) -> anyhow::Result<RuntimeStats> {
    anyhow::ensure!(!registry.slots.is_empty(), "game registry is empty");
    let listener = TcpListener::bind(&config.listen_addr)
        .await
        .with_context(|| format!("bind {} failed", config.listen_addr))?;
    let listen_addr = listener
        .local_addr()
        .context("read websocket listen address")?;
    debug!(
        service = config.service_name,
        listen = %format!(" ws://{listen_addr}"),
        registered_games = registry.slots_by_game_id.len(),
        "ws server started"
    );

    let senders: SessionSenders = Arc::new(Mutex::new(HashMap::new()));
    registry.set_context(Arc::clone(&senders)).await;
    let stats = RuntimeStats {
        room_services: registry
            .slots
            .iter()
            .map(|slot| Arc::clone(&slot.room_service))
            .collect(),
        senders: Arc::clone(&senders),
        listen_addr,
    };
    let registry = Arc::new(registry);

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
            warn!(
                target: "lan_game_security",
                peer = %peer,
                max_connections = MAX_CONNECTIONS,
                "connection limit reached"
            );
            drop(stream);
            continue;
        };
        let session_id = next_session.fetch_add(1, Ordering::Relaxed);
        let context = ConnectionContext {
            idle_timeout: config.idle_timeout,
            heartbeat_interval: config.heartbeat_interval,
            senders: Arc::clone(&senders),
            registry: Arc::clone(&registry),
            stop_signal: stop_signal.clone(),
            connection_permit,
        };

        tokio::spawn(async move {
            match AssertUnwindSafe(handle_connection(stream, peer, session_id, context))
                .catch_unwind()
                .await
            {
                Ok(Err(err)) => {
                    debug!(session_id, peer = %peer, ?err, "connection ended with transport error");
                }
                Ok(Ok(())) => debug!(session_id, peer = %peer, "connection closed"),
                Err(_) => {
                    error!(session_id, peer = %peer, "connection task panicked");
                }
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
    let mut registry = GameRegistry::new();
    registry.register(handler)?;
    registry.allow_unbound_single_game_requests = true;
    run_game_registry_runtime_until_stopped_inner(config, registry, stop_signal, Some(ready)).await
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
        let mut total = 0;
        for room_service in &self.room_services {
            total += room_service.lock().await.room_count();
        }
        total
    }

    /// 判断某房间指定座位是否正由 AI 临时接管。
    pub async fn room_position_is_ai_takeover(&self, room_key: &str, position: usize) -> bool {
        for room_service in &self.room_services {
            if room_service
                .lock()
                .await
                .room_position_is_ai_takeover(room_key, position)
            {
                return true;
            }
        }
        false
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
    use share_type_public::{WsResponseCode, WsWithoutDataResponse};
    use tokio_tungstenite::tungstenite::Message;

    use crate::{
        ClientRequest, Dispatch, GameSettings, RoomService, SessionId, game_state::SharedGameState,
    };

    use super::{
        GameHandler, INBOUND_MESSAGE_BURST, MessageRateLimiter, RuntimeConfig, SessionSendError,
        annotate_request_id, run_room_runtime_until_stopped_with_ready, runtime_stop_channel,
        session_sender_channel,
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

    #[test]
    fn request_id_is_echoed_on_private_response_only() {
        let mut dispatch = Dispatch {
            messages: vec![crate::Delivery {
                recipient: 7,
                payload: crate::OutboundPayload::Response(crate::RequestResponse::WithoutData(
                    WsWithoutDataResponse {
                        request_id: None,
                        route: 10,
                        code: WsResponseCode::NO_PERMISSION,
                    },
                )),
            }],
        };
        annotate_request_id(
            &mut dispatch,
            7,
            10,
            &serde_json::json!({"request_id": "req-1"}),
        );
        let crate::OutboundPayload::Response(crate::RequestResponse::WithoutData(response)) =
            &dispatch.messages[0].payload
        else {
            panic!("expected response");
        };
        assert_eq!(response.request_id.as_deref(), Some("req-1"));
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
