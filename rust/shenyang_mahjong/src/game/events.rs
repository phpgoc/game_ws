//! Allocate sequence numbers only when emitting a table event, never when reading state.
use super::*;
use share_type_public::games::shenyang_mahjong::{
    WsShenyangMahjongEventContext, WsShenyangMahjongPhaseEvent,
};

pub(super) fn is_sequenced_event(code: i32) -> bool {
    [
        WsCode::START,
        WsCode::DEAL,
        WsCode::PLAY,
        WsCode::CLAIM_WINDOW,
        WsCode::CHANGE_PHASE,
        WsCode::GAME_OVER,
        WsCode::TABLE_SNAPSHOT,
    ]
    .iter()
    .any(|value| *value as i32 == code)
}

fn next_context(
    state: &ShenyangMahjongLoopState,
    session_id: SessionId,
) -> WsShenyangMahjongEventContext {
    let players = state.players_snapshot();
    let mut streams = state.event_streams.lock().unwrap();
    streams.retain(|id, _| players.values().any(|(current, _)| id == current));
    let context = streams
        .entry(session_id)
        .or_insert_with(|| WsShenyangMahjongEventContext {
            stream_id: uuid::Uuid::new_v4().simple().to_string(),
            event_seq: 0,
        });
    if let Some(next) = context.event_seq.checked_add(1) {
        context.event_seq = next;
    } else {
        // A different stream requires an explicitly requested baseline on the client.
        context.stream_id = uuid::Uuid::new_v4().simple().to_string();
        context.event_seq = 1;
    }
    context.clone()
}

pub(crate) fn push_direct_event<T: serde::Serialize>(
    state: &ShenyangMahjongLoopState,
    dispatch: &mut Dispatch,
    session_id: SessionId,
    code: i32,
    payload: T,
) {
    let mut data = serde_json::to_value(payload).expect("table event must serialize");
    if is_sequenced_event(code) {
        data.as_object_mut()
            .expect("table event must be an object")
            .insert(
                "event_context".to_owned(),
                serde_json::to_value(next_context(state, session_id)).unwrap(),
            );
    }
    dispatch.messages.push(Delivery {
        recipient: session_id,
        payload: OutboundPayload::Event(share_type_public::CommonEvent { code, data }),
    });
}

pub(crate) fn push_room_event<T: serde::Serialize>(
    room_service: &RoomService,
    room_key: &str,
    state: &ShenyangMahjongLoopState,
    dispatch: &mut Dispatch,
    code: i32,
    payload: T,
) {
    let data = serde_json::to_value(payload).expect("room event must serialize");
    for session_id in room_service.connected_session_ids(room_key) {
        push_direct_event(state, dispatch, session_id, code, &data);
    }
}

pub(crate) fn push_phase_change(
    room_service: &RoomService,
    room_key: &str,
    state: &ShenyangMahjongLoopState,
    dispatch: &mut Dispatch,
    phase: ShenyangMahjongPhase,
    current_position: usize,
    turn_countdown: u32,
) {
    push_room_event(
        room_service,
        room_key,
        state,
        dispatch,
        WsCode::CHANGE_PHASE as i32,
        WsShenyangMahjongPhaseEvent {
            event_context: None,
            phase,
            position: current_position as i32,
            turn_countdown: turn_countdown as i32,
        },
    );
}
