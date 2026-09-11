use super::*;
use share_type_public::games::shenyang_mahjong::{
    ShenyangMahjongRoutes, WsShenyangMahjongSyncRequest,
};

impl ShenyangMahjongGameHandler {
    pub(super) fn handle_sync_table(
        &self,
        room: &RoomService,
        session_id: SessionId,
        data: Value,
    ) -> Dispatch {
        let route = ShenyangMahjongRoutes::SYNC_TABLE as i32;
        let Some(room_key) = room.room_key_of(session_id) else {
            return room.error_response(session_id, route, WsResponseCode::NOT_LOGIN);
        };
        let Some(position) = room.session_position(session_id) else {
            return room.error_response(session_id, route, WsResponseCode::NOT_LOGIN);
        };
        let Ok(request) = serde_json::from_value::<WsShenyangMahjongSyncRequest>(data) else {
            return room.error_response(session_id, route, WsResponseCode::ERROR_FORMAT);
        };
        if request.request_id.is_empty()
            || request.request_id.len() > 64
            || !request.request_id.is_ascii()
        {
            return room.error_response(session_id, route, WsResponseCode::ERROR_FORMAT);
        }
        let Some(shared) = self.current_loop_state(room, &room_key) else {
            return room.error_response(session_id, route, WsResponseCode::NO_PERMISSION);
        };
        let state = shared.lock().unwrap();
        let configs = room.room_configs(&room_key).unwrap_or_default();
        let mut snapshot = build_table_snapshot_event_with_configs(&state, position, &configs);
        snapshot.sync_request_id = Some(request.request_id);
        let mut dispatch = Dispatch::default();
        push_direct_event(
            &state,
            &mut dispatch,
            session_id,
            WsCode::TABLE_SNAPSHOT as i32,
            snapshot,
        );
        room.push_ok_response(&mut dispatch, session_id, route);
        dispatch
    }
}
