use std::{
    collections::HashMap,
    future::Future,
    net::SocketAddr,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
        mpsc::SyncSender,
    },
    time::Duration,
};

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use share_type_public::{
    CommonEvent, P2pRoutes, P2pSignalKind, P2pStartState, P2pWsCode, WsP2pJoinRequest,
    WsP2pJoinResponse, WsP2pNetworkResultRequest, WsP2pPeer, WsP2pPeerLeftEvent,
    WsP2pPeerStateEvent, WsP2pSignalEvent, WsP2pSignalRequest, WsP2pStartStateEvent, WsRequest,
    WsResponse, WsResponseCode, WsWithoutDataResponse,
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{Mutex, mpsc, watch},
    task::JoinSet,
};
use tokio_tungstenite::{accept_async, tungstenite::Message};

use crate::config::IceServiceConfig;

const MAX_CANDIDATE_BYTES: usize = 16 * 1024;
const MAX_GAME_BYTES: usize = 64;
const MAX_NAME_BYTES: usize = 48;
const MAX_ROOM_BYTES: usize = 128;
const MAX_SESSION_ID_BYTES: usize = 256;

const MAX_SDP_BYTES: usize = 256 * 1024;

struct ClientCountGuard {
    client_count: Arc<AtomicUsize>,
}

struct Delivery {
    sender: Sender,
    message: Message,
}

#[derive(Clone)]
struct Membership {
    key: RoomKey,
    position: usize,
}

#[derive(Clone)]
pub struct P2pRuntimeStats {
    state: Arc<Mutex<SignalingState>>,
    client_count: Arc<AtomicUsize>,
}

#[derive(Clone)]
struct Peer {
    session_id: SessionId,
    name: String,
    sender: Sender,
    turn_allowed: bool,
    direct_result: Option<bool>,
}

struct Room {
    peers: [Option<Peer>; 2],
    turn_offered: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RoomKey {
    game: String,
    room: String,
}
type Sender = mpsc::UnboundedSender<Message>;

type SessionId = u64;

pub type P2pTurnPermissionFuture = Pin<Box<dyn Future<Output = bool> + Send>>;
pub type P2pTurnPermissionChecker =
    Arc<dyn Fn(Option<String>) -> P2pTurnPermissionFuture + Send + Sync>;

#[derive(Clone, Default)]
pub struct P2pRuntimeOptions {
    /// When present, both JOIN session IDs must pass this check before TURN is
    /// offered after a failed direct connection.
    ///
    /// Without a checker, the self-hosted server still tries STUN first and
    /// automatically falls back to its own TURN relay.
    pub turn_permission_checker: Option<P2pTurnPermissionChecker>,
}

impl P2pRuntimeOptions {
    pub fn turn_permission_gated(checker: P2pTurnPermissionChecker) -> Self {
        Self {
            turn_permission_checker: Some(checker),
        }
    }

    fn requires_turn_permission(&self) -> bool {
        self.turn_permission_checker.is_some()
    }
}

#[derive(Default)]
struct SignalingState {
    rooms: HashMap<RoomKey, Room>,
    memberships: HashMap<SessionId, Membership>,
}

fn deliver(deliveries: Vec<Delivery>) {
    for delivery in deliveries {
        let _ = delivery.sender.send(delivery.message);
    }
}

fn event_delivery<T: serde::Serialize>(sender: Sender, code: P2pWsCode, data: &T) -> Delivery {
    serialized_delivery(
        sender,
        &CommonEvent {
            code: code as i32,
            data,
        },
    )
}

fn start_state_delivery(sender: Sender, state: P2pStartState) -> Delivery {
    event_delivery(
        sender,
        P2pWsCode::START_STATE,
        &WsP2pStartStateEvent { state },
    )
}

async fn forward_signal(
    state: &Arc<Mutex<SignalingState>>,
    session_id: SessionId,
    sender: Sender,
    signal: WsP2pSignalRequest,
) -> Vec<Delivery> {
    if !signal_is_valid(&signal) {
        return vec![response_delivery(
            sender,
            P2pRoutes::SIGNAL as i32,
            WsResponseCode::ERROR_FORMAT,
        )];
    }
    let (target, from_position) = {
        let state = state.lock().await;
        let Some(membership) = state.memberships.get(&session_id) else {
            return vec![response_delivery(
                sender,
                P2pRoutes::SIGNAL as i32,
                WsResponseCode::NOT_LOGIN,
            )];
        };
        if signal.target_position == membership.position as i32 {
            return vec![response_delivery(
                sender,
                P2pRoutes::SIGNAL as i32,
                WsResponseCode::ERROR_FORMAT,
            )];
        }
        let target = state
            .rooms
            .get(&membership.key)
            .and_then(|room| room.peers.get(signal.target_position as usize))
            .and_then(Option::as_ref)
            .cloned();
        (target, membership.position as i32)
    };
    let Some(target) = target else {
        return vec![response_delivery(
            sender,
            P2pRoutes::SIGNAL as i32,
            WsResponseCode::NO_PERMISSION,
        )];
    };
    let event = WsP2pSignalEvent {
        from_position,
        kind: signal.kind,
        sdp: signal.sdp,
        candidate: signal.candidate,
        sdp_mid: signal.sdp_mid,
        sdp_m_line_index: signal.sdp_m_line_index,
        username_fragment: signal.username_fragment,
    };
    vec![
        event_delivery(target.sender, P2pWsCode::SIGNAL, &event),
        response_delivery(sender, P2pRoutes::SIGNAL as i32, WsResponseCode::OK),
    ]
}

async fn report_network_result(
    state: &Arc<Mutex<SignalingState>>,
    ice_config: &IceServiceConfig,
    options: &P2pRuntimeOptions,
    session_id: SessionId,
    sender: Sender,
    result: WsP2pNetworkResultRequest,
) -> Vec<Delivery> {
    let (start_state, targets, turn_targets) = {
        let mut state = state.lock().await;
        let Some(membership) = state.memberships.get(&session_id).cloned() else {
            return vec![response_delivery(
                sender,
                P2pRoutes::NETWORK_RESULT as i32,
                WsResponseCode::NOT_LOGIN,
            )];
        };
        let Some(room) = state.rooms.get_mut(&membership.key) else {
            return vec![response_delivery(
                sender,
                P2pRoutes::NETWORK_RESULT as i32,
                WsResponseCode::NOT_LOGIN,
            )];
        };
        let Some(peer) = room.peers[membership.position].as_mut() else {
            return vec![response_delivery(
                sender,
                P2pRoutes::NETWORK_RESULT as i32,
                WsResponseCode::NOT_LOGIN,
            )];
        };
        if peer.session_id != session_id {
            return vec![response_delivery(
                sender,
                P2pRoutes::NETWORK_RESULT as i32,
                WsResponseCode::NOT_LOGIN,
            )];
        }
        peer.direct_result = Some(result.direct);

        let targets: Vec<Sender> = room
            .peers
            .iter()
            .filter_map(|peer| peer.as_ref().map(|peer| peer.sender.clone()))
            .collect();
        if result.direct {
            (Some(P2pStartState::DIRECT), targets, Vec::new())
        } else {
            let both_failed = room.peers.iter().all(|peer| {
                peer.as_ref()
                    .is_some_and(|peer| peer.direct_result == Some(false))
            });
            if !both_failed {
                (Some(P2pStartState::WAITING_NETWORK), targets, Vec::new())
            } else {
                let both_allowed = room
                    .peers
                    .iter()
                    .all(|peer| peer.as_ref().is_some_and(|peer| peer.turn_allowed));
                if !options.requires_turn_permission() || both_allowed {
                    let turn_targets = if room.turn_offered {
                        Vec::new()
                    } else {
                        room.turn_offered = true;
                        room.peers
                            .iter()
                            .enumerate()
                            .filter_map(|(position, peer)| {
                                let peer = peer.as_ref()?;
                                Some((peer.sender.clone(), peer.session_id, position))
                            })
                            .collect()
                    };
                    (Some(P2pStartState::WAITING_TURN), targets, turn_targets)
                } else {
                    (Some(P2pStartState::BLOCKED), targets, Vec::new())
                }
            }
        }
    };

    let mut deliveries = vec![response_delivery(
        sender,
        P2pRoutes::NETWORK_RESULT as i32,
        WsResponseCode::OK,
    )];
    if let Some(start_state) = start_state {
        deliveries.extend(
            targets
                .into_iter()
                .map(|target| start_state_delivery(target, start_state)),
        );
    }
    for (target, target_session_id, position) in turn_targets {
        match ice_config.issue_event(target_session_id, position) {
            Ok(event) => deliveries.push(event_delivery(target, P2pWsCode::ICE_CONFIG, &event)),
            Err(_) => deliveries.push(response_delivery(
                target,
                P2pRoutes::NETWORK_RESULT as i32,
                WsResponseCode::NO_PERMISSION,
            )),
        }
    }
    deliveries
}

#[allow(clippy::too_many_arguments)]
async fn handle_connection(
    stream: TcpStream,
    _peer: SocketAddr,
    session_id: SessionId,
    state: Arc<Mutex<SignalingState>>,
    ice_config: Arc<IceServiceConfig>,
    options: Arc<P2pRuntimeOptions>,
    idle_timeout: Duration,
    heartbeat_interval: Duration,
) -> anyhow::Result<()> {
    let socket = accept_async(stream).await?;
    let (mut sink, mut source) = socket.split();
    let (sender, mut receiver) = mpsc::unbounded_channel();
    let heartbeat_sender = sender.clone();

    let writer = tokio::spawn(async move {
        while let Some(message) = receiver.recv().await {
            if sink.send(message).await.is_err() {
                break;
            }
        }
    });
    let heartbeat = tokio::spawn(async move {
        let mut interval = tokio::time::interval(heartbeat_interval);
        loop {
            interval.tick().await;
            if heartbeat_sender
                .send(Message::Ping(Vec::new().into()))
                .is_err()
            {
                break;
            }
        }
    });

    loop {
        let frame = match tokio::time::timeout(idle_timeout, source.next()).await {
            Ok(Some(Ok(frame))) => frame,
            Ok(Some(Err(error))) => return Err(error.into()),
            Ok(None) | Err(_) => break,
        };
        if frame.is_close() {
            break;
        }
        let Message::Text(text) = frame else {
            continue;
        };
        let request = match serde_json::from_str::<WsRequest<Value>>(&text) {
            Ok(request) => request,
            Err(_) => {
                send_serialized(
                    &sender,
                    &WsWithoutDataResponse {
                        route: 0,
                        code: WsResponseCode::ERROR_FORMAT,
                    },
                );
                continue;
            }
        };
        let deliveries = handle_request(
            &state,
            &ice_config,
            &options,
            session_id,
            sender.clone(),
            request,
        )
        .await;
        deliver(deliveries);
    }

    deliver(leave_room(&state, session_id).await);
    heartbeat.abort();
    writer.abort();
    Ok(())
}

async fn handle_request(
    state: &Arc<Mutex<SignalingState>>,
    ice_config: &IceServiceConfig,
    options: &Arc<P2pRuntimeOptions>,
    session_id: SessionId,
    sender: Sender,
    request: WsRequest<Value>,
) -> Vec<Delivery> {
    match request.route {
        route if route == P2pRoutes::JOIN as i32 => {
            let Ok(join) = serde_json::from_value::<WsP2pJoinRequest>(request.data) else {
                return vec![response_delivery(
                    sender,
                    route,
                    WsResponseCode::ERROR_FORMAT,
                )];
            };
            join_room(state, ice_config, options, session_id, sender, join).await
        }
        route if route == P2pRoutes::SIGNAL as i32 => {
            let Ok(signal) = serde_json::from_value::<WsP2pSignalRequest>(request.data) else {
                return vec![response_delivery(
                    sender,
                    route,
                    WsResponseCode::ERROR_FORMAT,
                )];
            };
            forward_signal(state, session_id, sender, signal).await
        }
        route if route == P2pRoutes::LEAVE as i32 => {
            let mut deliveries = leave_room(state, session_id).await;
            deliveries.push(response_delivery(sender, route, WsResponseCode::OK));
            deliveries
        }
        route if route == P2pRoutes::NETWORK_RESULT as i32 => {
            let Ok(result) = serde_json::from_value::<WsP2pNetworkResultRequest>(request.data)
            else {
                return vec![response_delivery(
                    sender,
                    route,
                    WsResponseCode::ERROR_FORMAT,
                )];
            };
            report_network_result(state, ice_config, options, session_id, sender, result).await
        }
        route => vec![response_delivery(
            sender,
            route,
            WsResponseCode::NOT_IN_RANGE,
        )],
    }
}

async fn join_room(
    state: &Arc<Mutex<SignalingState>>,
    ice_config: &IceServiceConfig,
    options: &Arc<P2pRuntimeOptions>,
    session_id: SessionId,
    sender: Sender,
    join: WsP2pJoinRequest,
) -> Vec<Delivery> {
    if !valid_identifier(&join.game, MAX_GAME_BYTES)
        || !valid_room(&join.room)
        || !valid_name(&join.name)
        || join.session_id.as_deref().is_some_and(|session_id| {
            session_id.is_empty()
                || session_id.len() > MAX_SESSION_ID_BYTES
                || session_id.chars().any(char::is_control)
        })
    {
        return vec![response_delivery(
            sender,
            P2pRoutes::JOIN as i32,
            WsResponseCode::ERROR_FORMAT,
        )];
    }

    let turn_allowed = if let Some(checker) = &options.turn_permission_checker {
        checker(join.session_id.clone()).await
    } else {
        true
    };

    let key = RoomKey {
        game: join.game,
        room: join.room,
    };
    let (position, peer, ice_targets, peer_state_targets, start_state_targets, room_is_full) = {
        let mut state = state.lock().await;
        if state.memberships.contains_key(&session_id) {
            return vec![response_delivery(
                sender,
                P2pRoutes::JOIN as i32,
                WsResponseCode::NO_PERMISSION,
            )];
        }
        let room = state.rooms.entry(key.clone()).or_insert_with(Room::new);
        let Some(position) = room.peers.iter().position(Option::is_none) else {
            return vec![response_delivery(
                sender,
                P2pRoutes::JOIN as i32,
                WsResponseCode::NO_PERMISSION,
            )];
        };
        room.peers[position] = Some(Peer {
            session_id,
            name: join.name,
            sender: sender.clone(),
            turn_allowed,
            direct_result: None,
        });
        let peer = room.peers[1 - position].as_ref().map(|peer| WsP2pPeer {
            position: (1 - position) as i32,
            name: peer.name.clone(),
        });
        let room_is_full = room.peers.iter().all(Option::is_some);
        let ice_targets = if room_is_full {
            room.peers
                .iter()
                .enumerate()
                .filter_map(|(own_position, own)| {
                    let own = own.as_ref()?;
                    Some((own.sender.clone(), own.session_id, own_position))
                })
                .collect()
        } else {
            vec![(sender.clone(), session_id, position)]
        };
        let peer_state_targets = if room_is_full {
            room.peers
                .iter()
                .enumerate()
                .filter_map(|(own_position, own)| {
                    let own = own.as_ref()?;
                    let other = room.peers[1 - own_position].as_ref()?;
                    Some((
                        own.sender.clone(),
                        WsP2pPeerStateEvent {
                            self_position: own_position as i32,
                            peer_position: (1 - own_position) as i32,
                            peer_name: other.name.clone(),
                            initiator: own_position == 0,
                        },
                    ))
                })
                .collect()
        } else {
            Vec::new()
        };
        let start_state_targets = room
            .peers
            .iter()
            .filter_map(|peer| peer.as_ref().map(|peer| peer.sender.clone()))
            .collect::<Vec<_>>();
        state
            .memberships
            .insert(session_id, Membership { key, position });
        (
            position,
            peer,
            ice_targets,
            peer_state_targets,
            start_state_targets,
            room_is_full,
        )
    };

    let mut deliveries = vec![serialized_delivery(
        sender.clone(),
        &WsResponse {
            route: P2pRoutes::JOIN as i32,
            code: WsResponseCode::JOINED,
            data: WsP2pJoinResponse {
                self_position: position as i32,
                peer,
            },
        },
    )];
    for (target, target_session_id, target_position) in ice_targets {
        match ice_config.issue_event_with_turn(target_session_id, target_position, false) {
            Ok(event) => {
                deliveries.push(event_delivery(target, P2pWsCode::ICE_CONFIG, &event));
            }
            Err(_) => deliveries.push(response_delivery(
                target,
                P2pRoutes::JOIN as i32,
                WsResponseCode::NO_PERMISSION,
            )),
        }
    }
    for (target, event) in peer_state_targets {
        deliveries.push(event_delivery(target, P2pWsCode::PEER_STATE, &event));
    }
    let initial_state = if room_is_full {
        P2pStartState::WAITING_NETWORK
    } else {
        P2pStartState::WAITING_PEER
    };
    for target in start_state_targets {
        deliveries.push(start_state_delivery(target, initial_state));
    }
    deliveries
}

async fn leave_room(state: &Arc<Mutex<SignalingState>>, session_id: SessionId) -> Vec<Delivery> {
    let mut state = state.lock().await;
    let Some(membership) = state.memberships.remove(&session_id) else {
        return Vec::new();
    };
    let mut peer_delivery = None;
    let mut remove_room = false;
    if let Some(room) = state.rooms.get_mut(&membership.key) {
        room.peers[membership.position] = None;
        room.turn_offered = false;
        if let Some(peer) = room.peers[1 - membership.position].as_mut() {
            // A replacement peer needs a new direct-connect attempt and, if
            // necessary, a fresh pair of TURN credentials.
            peer.direct_result = None;
            peer_delivery = Some(event_delivery(
                peer.sender.clone(),
                P2pWsCode::PEER_LEFT,
                &WsP2pPeerLeftEvent {
                    peer_position: membership.position as i32,
                },
            ));
        }
        remove_room = room.peers.iter().all(Option::is_none);
    }
    if remove_room {
        state.rooms.remove(&membership.key);
    }
    peer_delivery.into_iter().collect()
}

fn response_delivery(sender: Sender, route: i32, code: WsResponseCode) -> Delivery {
    serialized_delivery(sender, &WsWithoutDataResponse { route, code })
}

pub async fn run_p2p_listener(
    listener: TcpListener,
    ice_config: IceServiceConfig,
    idle_timeout: Duration,
    heartbeat_interval: Duration,
) -> anyhow::Result<()> {
    run_p2p_listener_with_options(
        listener,
        ice_config,
        idle_timeout,
        heartbeat_interval,
        P2pRuntimeOptions::default(),
    )
    .await
    .map(|_| ())
}

pub async fn run_p2p_listener_with_options(
    listener: TcpListener,
    ice_config: IceServiceConfig,
    idle_timeout: Duration,
    heartbeat_interval: Duration,
    options: P2pRuntimeOptions,
) -> anyhow::Result<()> {
    let (_stop_tx, stop_rx) = watch::channel(false);
    run_p2p_listener_until_stopped_with_options(
        listener,
        ice_config,
        idle_timeout,
        heartbeat_interval,
        stop_rx,
        None,
        options,
    )
    .await
    .map(|_| ())
}

pub async fn run_p2p_listener_until_stopped(
    listener: TcpListener,
    ice_config: IceServiceConfig,
    idle_timeout: Duration,
    heartbeat_interval: Duration,
    stop_signal: watch::Receiver<bool>,
    ready: Option<SyncSender<P2pRuntimeStats>>,
) -> anyhow::Result<P2pRuntimeStats> {
    run_p2p_listener_until_stopped_with_options(
        listener,
        ice_config,
        idle_timeout,
        heartbeat_interval,
        stop_signal,
        ready,
        P2pRuntimeOptions::default(),
    )
    .await
}

pub async fn run_p2p_listener_until_stopped_with_options(
    listener: TcpListener,
    ice_config: IceServiceConfig,
    idle_timeout: Duration,
    heartbeat_interval: Duration,
    mut stop_signal: watch::Receiver<bool>,
    ready: Option<SyncSender<P2pRuntimeStats>>,
    options: P2pRuntimeOptions,
) -> anyhow::Result<P2pRuntimeStats> {
    let state = Arc::new(Mutex::new(SignalingState::default()));
    let client_count = Arc::new(AtomicUsize::new(0));
    let stats = P2pRuntimeStats {
        state: Arc::clone(&state),
        client_count: Arc::clone(&client_count),
    };
    let ice_config = Arc::new(ice_config);
    let options = Arc::new(options);
    let session_sequence = Arc::new(AtomicU64::new(1));
    let mut connections = JoinSet::new();
    if let Some(ready) = ready {
        let _ = ready.send(stats.clone());
    }

    loop {
        tokio::select! {
            _ = wait_for_stop(&mut stop_signal) => break,
            accepted = listener.accept() => {
                let (stream, peer) = accepted?;
                let session_id = session_sequence.fetch_add(1, Ordering::Relaxed);
                let state = Arc::clone(&state);
                let ice_config = Arc::clone(&ice_config);
                let options = Arc::clone(&options);
                let count_guard = ClientCountGuard::new(Arc::clone(&client_count));
                connections.spawn(async move {
                    let _count_guard = count_guard;
                    if let Err(error) = handle_connection(
                        stream,
                        peer,
                        session_id,
                        state,
                        ice_config,
                        options,
                        idle_timeout,
                        heartbeat_interval,
                    )
                    .await
                    {
                        eprintln!("p2p connection {session_id} ({peer}) failed: {error:#}");
                    }
                });
            }
            Some(result) = connections.join_next(), if !connections.is_empty() => {
                if let Err(error) = result {
                    eprintln!("p2p connection task failed: {error}");
                }
            }
        }
    }

    connections.abort_all();
    while connections.join_next().await.is_some() {}
    Ok(stats)
}

fn send_serialized<T: serde::Serialize>(sender: &Sender, payload: &T) {
    let text = serde_json::to_string(payload).unwrap_or_else(|_| "{}".into());
    let _ = sender.send(Message::Text(text.into()));
}

fn serialized_delivery<T: serde::Serialize>(sender: Sender, payload: &T) -> Delivery {
    let text = serde_json::to_string(payload).unwrap_or_else(|_| "{}".into());
    Delivery {
        sender,
        message: Message::Text(text.into()),
    }
}

fn signal_is_valid(signal: &WsP2pSignalRequest) -> bool {
    if !(0..=1).contains(&signal.target_position) {
        return false;
    }
    match signal.kind {
        P2pSignalKind::OFFER | P2pSignalKind::ANSWER => signal
            .sdp
            .as_deref()
            .is_some_and(|sdp| !sdp.is_empty() && sdp.len() <= MAX_SDP_BYTES),
        P2pSignalKind::ICE_CANDIDATE => {
            signal.sdp.is_none()
                && signal
                    .candidate
                    .as_deref()
                    .is_none_or(|candidate| candidate.len() <= MAX_CANDIDATE_BYTES)
                && signal
                    .sdp_m_line_index
                    .is_none_or(|index| (0..=65_535).contains(&index))
        }
    }
}

fn valid_identifier(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

fn valid_name(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= MAX_NAME_BYTES
        && !value.chars().any(char::is_control)
}

fn valid_room(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= MAX_ROOM_BYTES
        && !value.chars().any(char::is_control)
}

async fn wait_for_stop(stop_signal: &mut watch::Receiver<bool>) {
    if *stop_signal.borrow() {
        return;
    }
    let _ = stop_signal.changed().await;
}

impl ClientCountGuard {
    fn new(client_count: Arc<AtomicUsize>) -> Self {
        client_count.fetch_add(1, Ordering::Relaxed);
        Self { client_count }
    }
}

impl Drop for ClientCountGuard {
    fn drop(&mut self) {
        self.client_count.fetch_sub(1, Ordering::Relaxed);
    }
}

impl P2pRuntimeStats {
    pub fn client_count(&self) -> usize {
        self.client_count.load(Ordering::Relaxed)
    }

    pub async fn room_count(&self) -> usize {
        self.state.lock().await.rooms.len()
    }
}

impl Room {
    fn new() -> Self {
        Self {
            peers: [None, None],
            turn_offered: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel() -> (Sender, mpsc::UnboundedReceiver<Message>) {
        mpsc::unbounded_channel()
    }

    fn ice_config() -> IceServiceConfig {
        IceServiceConfig::new(
            vec!["stun:stun.example.test:3478".into()],
            vec!["turn:turn.example.test:3478?transport=udp".into()],
            "runtime-test-secret".into(),
            600,
        )
        .expect("ICE config")
    }

    #[tokio::test]
    async fn room_is_limited_to_two_and_disconnect_notifies_peer() {
        let state = Arc::new(Mutex::new(SignalingState::default()));
        let options = Arc::new(P2pRuntimeOptions::default());
        let (red_sender, mut red_rx) = channel();
        let (black_sender, _black_rx) = channel();
        let (third_sender, mut third_rx) = channel();
        for (session, sender, name) in [
            (1, red_sender, "red"),
            (2, black_sender, "black"),
            (3, third_sender, "third"),
        ] {
            deliver(
                join_room(
                    &state,
                    &ice_config(),
                    &options,
                    session,
                    sender,
                    WsP2pJoinRequest {
                        game: "game_a".into(),
                        room: "room".into(),
                        name: name.into(),
                        session_id: None,
                    },
                )
                .await,
            );
        }
        let third_response = third_rx.recv().await.expect("third response");
        assert!(
            third_response
                .into_text()
                .expect("text")
                .contains("\"code\":403")
        );

        while red_rx.try_recv().is_ok() {}
        deliver(leave_room(&state, 2).await);
        let peer_left = red_rx.recv().await.expect("peer left");
        assert!(
            peer_left
                .into_text()
                .expect("text")
                .contains("\"code\":5004")
        );
    }

    #[tokio::test]
    async fn two_peers_receive_roles_and_signals() {
        let state = Arc::new(Mutex::new(SignalingState::default()));
        let options = Arc::new(P2pRuntimeOptions::default());
        let (red_sender, mut red_rx) = channel();
        let (black_sender, mut black_rx) = channel();
        let red = WsP2pJoinRequest {
            game: "game_a".into(),
            room: "room".into(),
            name: "red".into(),
            session_id: None,
        };
        deliver(join_room(&state, &ice_config(), &options, 1, red_sender.clone(), red).await);
        let black = WsP2pJoinRequest {
            game: "game_a".into(),
            room: "room".into(),
            name: "black".into(),
            session_id: None,
        };
        deliver(
            join_room(
                &state,
                &ice_config(),
                &options,
                2,
                black_sender.clone(),
                black,
            )
            .await,
        );

        let mut red_messages = Vec::new();
        while let Ok(message) = red_rx.try_recv() {
            red_messages.push(message.into_text().expect("text").to_string());
        }
        assert_eq!(
            red_messages
                .iter()
                .filter(|message| message.contains("\"code\":5001"))
                .count(),
            2,
            "the waiting peer receives a fresh STUN configuration when paired"
        );
        assert!(
            red_messages
                .iter()
                .filter(|message| message.contains("\"code\":5001"))
                .all(|message| message.contains("\"turn_enabled\":false"))
        );
        assert!(
            red_messages
                .iter()
                .any(|message| message.contains("\"code\":5002"))
        );

        let signal = WsP2pSignalRequest {
            target_position: 1,
            kind: P2pSignalKind::OFFER,
            sdp: Some("v=0".into()),
            candidate: None,
            sdp_mid: None,
            sdp_m_line_index: None,
            username_fragment: None,
        };
        deliver(forward_signal(&state, 1, red_sender, signal).await);
        let mut black_messages = Vec::new();
        while let Ok(message) = black_rx.try_recv() {
            black_messages.push(message.into_text().expect("text").to_string());
        }
        assert!(
            black_messages
                .iter()
                .any(|message| message.contains("\"code\":5003"))
        );
        assert!(
            black_messages
                .iter()
                .any(|message| message.contains("\"sdp\":\"v=0\""))
        );
    }
}
