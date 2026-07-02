use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::SyncSender,
    },
    time::Duration,
};

use anyhow::Context;
use futures_util::{SinkExt, StreamExt};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{Mutex, mpsc, watch},
};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tracing::{error, info, warn};

use crate::{
    ClientRequest, Dispatch, RoomService, SessionId, SettingsBuilderResult, from_message,
    parse_bind_cli, resolve_host, resolve_port, to_text_message,
};

pub trait GameHandler: Send + 'static {
    fn after_common_request(
        &mut self,
        _room_service: &mut RoomService,
        _session_id: SessionId,
        _request: &ClientRequest,
        _dispatch: &mut Dispatch,
    ) {
        // Optional: override in games that need to enrich common responses/events.
    }
    /// 创建游戏状态。
    /// 在首个 JOIN 建房成功后立即调用，并将当前成员 populate 进去。
    fn build_game_state(&self) -> Box<dyn crate::game_state::GameState>;

    fn build_room_settings(&self) -> SettingsBuilderResult;

    fn accepts_game_id(&self, game_id: share_type_public::GameId) -> bool {
        game_id == self.game_id()
    }

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
}

pub struct RuntimeStopHandle {
    tx: watch::Sender<bool>,
}

type SessionSender = mpsc::UnboundedSender<Message>;
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
            let _ = tx.send(frame);
        }
    }
    Ok(())
}

async fn handle_connection<H>(
    stream: TcpStream,
    peer: SocketAddr,
    session_id: SessionId,
    idle_timeout: Duration,
    heartbeat_interval: Duration,
    senders: SessionSenders,
    room_service: Arc<Mutex<RoomService>>,
    game_handler: Arc<Mutex<H>>,
    mut stop_signal: StopSignal,
) -> anyhow::Result<()>
where
    H: GameHandler,
{
    let ws = accept_async(stream).await?;
    let (mut sink, mut source) = ws.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    let heartbeat_tx = tx.clone();

    senders.lock().await.insert(session_id, tx);
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

    loop {
        let frame = tokio::select! {
            _ = stop_signal.stopped() => break,
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

        let request = match from_message::<ClientRequest>(frame) {
            Ok(Some(request)) => request,
            Ok(None) => continue,
            Err(err) => {
                warn!(session_id, peer = %peer, ?err, "invalid ws frame, ignored");
                continue;
            }
        };

        let dispatch = {
            let mut room = room_service.lock().await;
            let mut handler = game_handler.lock().await;
            let creates_room_on_join = if request.route == share_type_public::Routes::JOIN as i32 {
                let join_key = serde_json::from_value::<share_type_public::WsJoinRequest>(
                    request.data.clone(),
                )
                .ok()
                .map(|join| join.password);
                join_key
                    .as_ref()
                    .map(|room_key| !room.room_exists(room_key))
                    .unwrap_or(false)
            } else {
                false
            };
            if let Some(mut dispatch) = room.handle_common_request_with_game_acceptance(
                session_id,
                &request,
                |game_id| handler.accepts_game_id(game_id),
                || handler.build_room_settings(),
            ) {
                // 首个 JOIN 建房成功后，挂载游戏态，确保后续逻辑走具体游戏状态。
                if creates_room_on_join {
                    if let Some(room_key) = room.room_key_of(session_id) {
                        let mut gs = handler.build_game_state();
                        for (sid, name, pos, avatar) in room.get_room_members(&room_key) {
                            gs.add_player(pos, sid, &name);
                            gs.set_avatar(pos, &avatar);
                        }
                        room.set_room_game_state(&room_key, gs);
                    }
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

pub async fn run_game_server<H>(
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

pub async fn run_room_runtime<H>(config: RuntimeConfig, handler: H) -> anyhow::Result<()>
where
    H: GameHandler,
{
    let (_stop_handle, stop_signal) = runtime_stop_channel();
    run_room_runtime_until_stopped(config, handler, stop_signal)
        .await
        .map(|_| ())
}

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
    info!(service = config.service_name, listen = %format!(" ws://{}", config.listen_addr), "ws server started");

    let senders: SessionSenders = Arc::new(Mutex::new(HashMap::new()));
    let room_service = Arc::new(Mutex::new(RoomService::default()));
    let stats = RuntimeStats {
        room_service: Arc::clone(&room_service),
        senders: Arc::clone(&senders),
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

    loop {
        let (stream, peer) = tokio::select! {
            _ = stop_signal.stopped() => break,
            result = listener.accept() => result?,
        };
        let session_id = next_session.fetch_add(1, Ordering::Relaxed);
        let senders = Arc::clone(&senders);
        let room_service = Arc::clone(&room_service);
        let game_handler = Arc::clone(&game_handler);
        let idle_timeout = config.idle_timeout;
        let heartbeat_interval = config.heartbeat_interval;
        let stop_signal = stop_signal.clone();

        tokio::spawn(async move {
            if let Err(err) = handle_connection(
                stream,
                peer,
                session_id,
                idle_timeout,
                heartbeat_interval,
                senders,
                room_service,
                game_handler,
                stop_signal,
            )
            .await
            {
                error!(session_id, peer = %peer, ?err, "connection ended with error");
            } else {
                info!(session_id, peer = %peer, "connection closed");
            }
        });
    }

    Ok(stats)
}

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

pub fn runtime_stop_channel() -> (RuntimeStopHandle, StopSignal) {
    let (tx, rx) = watch::channel(false);
    (RuntimeStopHandle { tx }, StopSignal::new(rx))
}

impl RuntimeStats {
    pub async fn client_count(&self) -> usize {
        self.senders.lock().await.len()
    }

    pub async fn room_count(&self) -> usize {
        self.room_service.lock().await.room_count()
    }
}

impl RuntimeStopHandle {
    pub fn stop(&self) {
        let _ = self.tx.send(true);
    }
}

impl StopSignal {
    pub fn is_stopped(&self) -> bool {
        *self.rx.borrow()
    }

    pub fn new(rx: watch::Receiver<bool>) -> Self {
        Self { rx }
    }

    pub async fn stopped(&mut self) {
        if self.is_stopped() {
            return;
        }
        let _ = self.rx.changed().await;
    }
}
