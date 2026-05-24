use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use anyhow::Context;
use futures_util::{SinkExt, StreamExt};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{Mutex, mpsc},
};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tracing::{error, info, warn};

use crate::{
    ClientRequest, Dispatch, RoomService, SessionId, from_message, parse_bind_cli, resolve_host,
    resolve_port,
    to_text_message,
};
use share_type_public::GameSettings;

type SessionSender = mpsc::UnboundedSender<Message>;
pub type SessionSenders = Arc<Mutex<HashMap<SessionId, SessionSender>>>;

pub trait GameHandler: Send + 'static {
    fn build_room_settings(&self, room_key: &str) -> Box<dyn GameSettings>;
    fn get_player_limits(&self) -> (usize, usize) {
        (1, usize::MAX)
    }
    fn set_context(&mut self, _senders: SessionSenders, _room_service: Arc<Mutex<RoomService>>) {
        // Optional: override in games that need access to senders/room_service for event loops
    }
    fn handle_game_request(
        &mut self,
        room_service: &mut RoomService,
        session_id: SessionId,
        request: ClientRequest,
    ) -> Dispatch;
}

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub service_name: &'static str,
    pub listen_addr: String,
    pub idle_timeout: Duration,
    pub heartbeat_interval: Duration,
}

pub async fn run_room_runtime<H>(config: RuntimeConfig, handler: H) -> anyhow::Result<()>
where
    H: GameHandler,
{
    init_tracing();

    let listener = TcpListener::bind(&config.listen_addr)
        .await
        .with_context(|| format!("bind {} failed", config.listen_addr))?;
    info!(service = config.service_name, listen = %config.listen_addr, "ws server started");

    let senders: SessionSenders = Arc::new(Mutex::new(HashMap::new()));
    let room_service = Arc::new(Mutex::new(RoomService::default()));
    let game_handler = Arc::new(Mutex::new(handler));
    
    // Set context for game-specific initialization
    {
        let mut h = game_handler.lock().await;
        h.set_context(Arc::clone(&senders), Arc::clone(&room_service));
    }
    
    let next_session = Arc::new(AtomicU64::new(1));

    loop {
        let (stream, peer) = listener.accept().await?;
        let session_id = next_session.fetch_add(1, Ordering::Relaxed);
        let senders = Arc::clone(&senders);
        let room_service = Arc::clone(&room_service);
        let game_handler = Arc::clone(&game_handler);
        let idle_timeout = config.idle_timeout;
        let heartbeat_interval = config.heartbeat_interval;

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
            )
            .await
            {
                error!(session_id, peer = %peer, ?err, "connection ended with error");
            } else {
                info!(session_id, peer = %peer, "connection closed");
            }
        });
    }
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

async fn handle_connection<H>(
    stream: TcpStream,
    peer: SocketAddr,
    session_id: SessionId,
    idle_timeout: Duration,
    heartbeat_interval: Duration,
    senders: SessionSenders,
    room_service: Arc<Mutex<RoomService>>,
    game_handler: Arc<Mutex<H>>,
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
        let frame = match tokio::time::timeout(idle_timeout, source.next()).await {
            Ok(Some(frame)) => frame?,
            Ok(None) => break,
            Err(_) => {
                warn!(session_id, peer = %peer, "idle timeout, closing connection");
                break;
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
            if let Some(dispatch) = room.handle_common_request(
                session_id,
                &request,
                |room_key| handler.build_room_settings(room_key),
                || handler.get_player_limits(),
            ) {
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

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .try_init();
}
