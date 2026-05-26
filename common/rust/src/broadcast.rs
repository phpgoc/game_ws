use std::sync::Arc;

use serde::Serialize;
use share_type_public::{CommonEvent, WsCode};
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;

use crate::{RoomService, SessionId, SessionSenders};

fn build_event_frame<T: Serialize>(code: WsCode, payload: T) -> Option<Message> {
    let data = serde_json::to_value(payload).ok()?;
    let event = CommonEvent { code, data };
    let text = serde_json::to_string(&event).ok()?;
    Some(Message::text(text))
}

async fn send_to_sessions(session_ids: Vec<SessionId>, frame: Message, senders: &SessionSenders) {
    let senders = senders.lock().await;
    for id in session_ids {
        if let Some(tx) = senders.get(&id) {
            let _ = tx.send(frame.clone());
        }
    }
}

/// Send an event to every member in the room.
pub async fn send_all<T: Serialize>(
    room_key: &str,
    code: WsCode,
    payload: T,
    room_service: &Arc<Mutex<RoomService>>,
    senders: &SessionSenders,
) {
    let Some(frame) = build_event_frame(code, payload) else { return };
    let ids: Vec<SessionId> = room_service
        .lock()
        .await
        .get_room_members(room_key)
        .into_iter()
        .map(|(id, _, _)| id)
        .collect();
    send_to_sessions(ids, frame, senders).await;
}

/// Send an event to every member in the room except the given session.
pub async fn send_except_one<T: Serialize>(
    room_key: &str,
    except: SessionId,
    code: WsCode,
    payload: T,
    room_service: &Arc<Mutex<RoomService>>,
    senders: &SessionSenders,
) {
    let Some(frame) = build_event_frame(code, payload) else { return };
    let ids: Vec<SessionId> = room_service
        .lock()
        .await
        .get_room_members(room_key)
        .into_iter()
        .filter_map(|(id, _, _)| if id != except { Some(id) } else { None })
        .collect();
    send_to_sessions(ids, frame, senders).await;
}

/// Send an event to the member with the given display name.
pub async fn send_to_name<T: Serialize>(
    room_key: &str,
    name: &str,
    code: WsCode,
    payload: T,
    room_service: &Arc<Mutex<RoomService>>,
    senders: &SessionSenders,
) {
    let Some(frame) = build_event_frame(code, payload) else { return };
    let ids: Vec<SessionId> = room_service
        .lock()
        .await
        .get_room_members(room_key)
        .into_iter()
        .filter_map(|(id, member_name, _)| if member_name == name { Some(id) } else { None })
        .collect();
    send_to_sessions(ids, frame, senders).await;
}

/// Send an event to the member at the given seat position.
pub async fn send_to_position<T: Serialize>(
    room_key: &str,
    position: usize,
    code: WsCode,
    payload: T,
    room_service: &Arc<Mutex<RoomService>>,
    senders: &SessionSenders,
) {
    let Some(frame) = build_event_frame(code, payload) else { return };
    let ids: Vec<SessionId> = room_service
        .lock()
        .await
        .get_room_members(room_key)
        .into_iter()
        .filter_map(|(id, _, pos)| if pos == position { Some(id) } else { None })
        .collect();
    send_to_sessions(ids, frame, senders).await;
}
