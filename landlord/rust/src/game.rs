use std::collections::HashSet;

use serde_json::Value;
use share_type_public::{
    Routes, WsCode,
    games::{GameParam, RoomPlayerLimit, landlord::LandlordRoomSettings},
    ws::WsStartEvent,
};
use ws_common::{ClientRequest, Dispatch, GameHandler, RoomService, SessionId};

#[derive(Default)]
pub struct LandlordGameHandler {
    started_rooms: HashSet<String>,
}

pub fn build_room_settings(_room_key: &str) -> Value {
    serde_json::to_value(LandlordRoomSettings {
        limits: RoomPlayerLimit {
            min_players: 3,
            max_players: 3,
        },
        round_time: GameParam {
            default: 30,
            min: 20,
            max: 40,
        },
        away_time: GameParam {
            default: 5,
            min: 2,
            max: 5,
        },
        play_time: GameParam {
            default: 300,
            min: 100,
            max: 500,
        },
        deal_time: GameParam {
            default: 3000,
            min: 500,
            max: 4000,
        },
    })
    .unwrap_or(Value::Null)
}

impl GameHandler for LandlordGameHandler {
    fn build_room_settings(&self, room_key: &str) -> Value {
        build_room_settings(room_key)
    }

    fn handle_game_request(
        &mut self,
        room_service: &mut RoomService,
        session_id: SessionId,
        request: ClientRequest,
    ) -> Dispatch {
        match request.route {
            Routes::START => {
                let mut dispatch = Dispatch::default();
                if !room_service.ensure_in_room(session_id, Routes::START, &mut dispatch) {
                    return dispatch;
                }
                if !room_service.room_ready_to_start(session_id) {
                    return room_service.unsupported_response(session_id, Routes::START);
                }

                if let Some(room_key) = room_service.room_key_of(session_id) {
                    self.started_rooms.insert(room_key);
                }

                let actor = room_service.session_name(session_id);
                room_service.send_all(
                    session_id,
                    WsCode::START,
                    WsStartEvent { name: actor.clone() },
                    &mut dispatch,
                );

                let _ = room_service.send_other(
                    session_id,
                    WsCode::CHANGE_ROUND,
                    serde_json::json!({ "started_by": actor }),
                    &mut dispatch,
                );
                let _ = room_service.send_one_by_position(
                    session_id,
                    0,
                    WsCode::CHANGE_ROUND,
                    serde_json::json!({ "turn_position": 0 }),
                    &mut dispatch,
                );
                let _ = room_service.send_one_by_name(
                    session_id,
                    &room_service.session_name(session_id),
                    WsCode::CHANGE_ROUND,
                    serde_json::json!({ "self_confirm": true }),
                    &mut dispatch,
                );

                room_service.push_ok_response(&mut dispatch, session_id, Routes::START);
                dispatch
            }
            _ => room_service.unsupported_response(session_id, request.route),
        }
    }
}
