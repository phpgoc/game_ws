use serde_json::Value;
use share_type_public::{games::{RoomPlayerLimit, landlord::LandlordRoomSettings}};
use ws_common::{ClientRequest, Dispatch, GameHandler, RoomService, SessionId};

#[derive(Default)]
pub struct LandlordGameHandler;

pub fn build_room_settings(room_key: &str) -> Value {
    serde_json::to_value(LandlordRoomSettings {
        name: room_key.to_owned(),
        limits: RoomPlayerLimit {
            min_players: 3,
            max_players: 3,
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
        room_service.unsupported_response(session_id, request.route)
    }
}
