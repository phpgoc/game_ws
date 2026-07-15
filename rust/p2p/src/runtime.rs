use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use share_type_public::{
    CommonEvent, P2pRoutes, P2pSignalKind, P2pWsCode, WsP2pJoinRequest, WsP2pJoinResponse,
    WsP2pPeer, WsP2pPeerLeftEvent, WsP2pPeerStateEvent, WsP2pSignalEvent, WsP2pSignalRequest,
    WsRequest, WsResponse, WsResponseCode, WsWithoutDataResponse,
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{Mutex, mpsc},
};
use tokio_tungstenite::{accept_async, tungstenite::Message};

use crate::config::IceServiceConfig;

type SessionId = u64;
type Sender = mpsc::UnboundedSender<Message>;

const MAX_SDP_BYTES: usize = 256 * 1024;
const MAX_CANDIDATE_BYTES: usize = 16 * 1024;
const MAX_GAME_BYTES: usize = 64;
const MAX_ROOM_BYTES: usize = 128;
const MAX_NAME_BYTES: usize = 48;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RoomKey {
    game: String,
    room: String,
}

#[derive(Clone)]
struct Peer {
    session_id: SessionId,
    name: String,
    sender: Sender,
}

struct Room {
    peers: [Option<Peer>; 2],
}

#[derive(Clone)]
struct Membership {
    key: RoomKey,
    position: usize,
}

#[derive(Default)]
struct SignalingState {
    rooms: HashMap<RoomKey, Room>,
    memberships: HashMap<SessionId, Membership>,
}

struct Delivery {
    sender: Sender,
    message: Message,
}

impl Room {
    fn new() -> Self {
        Self {
            peers: [None, None],
        }
    }
}

pub async fn run_p2p_listener(
    listener: TcpListener,
    ice_config: IceServiceConfig,
    idle_timeout: Duration,
    heartbeat_interval: Duration,
) -> anyhow::Result<()> {
    let state = Arc::new(Mutex::new(SignalingState::default()));
    let ice_config = Arc::new(ice_config);
    let session_sequence = Arc::new(AtomicU64::new(1));
    loop {
        let (stream, peer) = listener.accept().await?;
        let session_id = session_sequence.fetch_add(1, Ordering::Relaxed);
        let state = Arc::clone(&state);
        let ice_config = Arc::clone(&ice_config);
        tokio::spawn(async move {
            if let Err(error) = handle_connection(
                stream,
                peer,
                session_id,
                state,
                ice_config,
                idle_timeout,
                heartbeat_interval,
            )
            .await
            {
                eprintln!("p2p connection {session_id} ({peer}) failed: {error:#}");
            }
        });
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_connection(
    stream: TcpStream,
    _peer: SocketAddr,
    session_id: SessionId,
    state: Arc<Mutex<SignalingState>>,
    ice_config: Arc<IceServiceConfig>,
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
        let deliveries =
            handle_request(&state, &ice_config, session_id, sender.clone(), request).await;
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
            join_room(state, ice_config, session_id, sender, join).await
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
    session_id: SessionId,
    sender: Sender,
    join: WsP2pJoinRequest,
) -> Vec<Delivery> {
    if !valid_identifier(&join.game, MAX_GAME_BYTES)
        || !valid_room(&join.room)
        || !valid_name(&join.name)
    {
        return vec![response_delivery(
            sender,
            P2pRoutes::JOIN as i32,
            WsResponseCode::ERROR_FORMAT,
        )];
    }

    let key = RoomKey {
        game: join.game,
        room: join.room,
    };
    let (position, peer, ice_targets, peer_state_targets) = {
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
        state
            .memberships
            .insert(session_id, Membership { key, position });
        (position, peer, ice_targets, peer_state_targets)
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
        match ice_config.issue_event(target_session_id, target_position) {
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
    deliveries
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

async fn leave_room(state: &Arc<Mutex<SignalingState>>, session_id: SessionId) -> Vec<Delivery> {
    let mut state = state.lock().await;
    let Some(membership) = state.memberships.remove(&session_id) else {
        return Vec::new();
    };
    let mut peer_delivery = None;
    let mut remove_room = false;
    if let Some(room) = state.rooms.get_mut(&membership.key) {
        room.peers[membership.position] = None;
        if let Some(peer) = room.peers[1 - membership.position].as_ref() {
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

fn valid_room(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= MAX_ROOM_BYTES
        && !value.chars().any(char::is_control)
}

fn valid_name(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= MAX_NAME_BYTES
        && !value.chars().any(char::is_control)
}

fn response_delivery(sender: Sender, route: i32, code: WsResponseCode) -> Delivery {
    serialized_delivery(sender, &WsWithoutDataResponse { route, code })
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

fn serialized_delivery<T: serde::Serialize>(sender: Sender, payload: &T) -> Delivery {
    let text = serde_json::to_string(payload).unwrap_or_else(|_| "{}".into());
    Delivery {
        sender,
        message: Message::Text(text.into()),
    }
}

fn send_serialized<T: serde::Serialize>(sender: &Sender, payload: &T) {
    let text = serde_json::to_string(payload).unwrap_or_else(|_| "{}".into());
    let _ = sender.send(Message::Text(text.into()));
}

fn deliver(deliveries: Vec<Delivery>) {
    for delivery in deliveries {
        let _ = delivery.sender.send(delivery.message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ice_config() -> IceServiceConfig {
        IceServiceConfig::new(
            vec!["stun:stun.example.test:3478".into()],
            vec!["turn:turn.example.test:3478?transport=udp".into()],
            "runtime-test-secret".into(),
            600,
        )
        .expect("ICE config")
    }

    fn channel() -> (Sender, mpsc::UnboundedReceiver<Message>) {
        mpsc::unbounded_channel()
    }

    #[tokio::test]
    async fn two_peers_receive_roles_and_signals() {
        let state = Arc::new(Mutex::new(SignalingState::default()));
        let (red_sender, mut red_rx) = channel();
        let (black_sender, mut black_rx) = channel();
        let red = WsP2pJoinRequest {
            game: "game_a".into(),
            room: "room".into(),
            name: "red".into(),
        };
        deliver(join_room(&state, &ice_config(), 1, red_sender.clone(), red).await);
        let black = WsP2pJoinRequest {
            game: "game_a".into(),
            room: "room".into(),
            name: "black".into(),
        };
        deliver(join_room(&state, &ice_config(), 2, black_sender.clone(), black).await);

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
            "the waiting peer receives fresh TURN credentials when paired"
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

    #[tokio::test]
    async fn room_is_limited_to_two_and_disconnect_notifies_peer() {
        let state = Arc::new(Mutex::new(SignalingState::default()));
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
                    session,
                    sender,
                    WsP2pJoinRequest {
                        game: "game_a".into(),
                        room: "room".into(),
                        name: name.into(),
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
}
