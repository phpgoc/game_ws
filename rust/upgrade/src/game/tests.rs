use serde_json::json;
use share_type_public::{GameId, Routes, WsResponseCode};
use ws_common::{ClientRequest, GameHandler, OutboundPayload, RequestResponse, RoomService};

use super::UpgradeGameHandler;

fn join_request(game_id: GameId) -> ClientRequest {
    ClientRequest {
        route: Routes::JOIN as i32,
        data: json!({
            "name": "owner",
            "password": "upgrade-room",
            "game_id": game_id as i32,
            "avatar_url": ""
        }),
    }
}

fn response_code(dispatch: &ws_common::Dispatch) -> Option<i32> {
    dispatch
        .messages
        .iter()
        .find_map(|delivery| match &delivery.payload {
            OutboundPayload::Response(RequestResponse::WithData(response)) => {
                Some(response.code as i32)
            }
            OutboundPayload::Response(RequestResponse::WithoutData(response)) => {
                Some(response.code as i32)
            }
            OutboundPayload::Event(_) => None,
        })
}

#[test]
fn handler_owns_only_the_upgrade_game_id() {
    let handler = UpgradeGameHandler::default();
    assert_eq!(handler.game_id(), GameId::UPGRADE);
    assert!(handler.accepts_game_id(GameId::UPGRADE));
    assert!(!handler.accepts_game_id(GameId::TRACTOR));
}

#[test]
fn room_join_accepts_upgrade_and_rejects_tractor() {
    let handler = UpgradeGameHandler::default();

    let mut upgrade_room = RoomService::with_ai_players_enabled(false);
    let accepted = upgrade_room
        .handle_common_request(1, &join_request(GameId::UPGRADE), handler.game_id(), || {
            handler.build_room_settings()
        })
        .expect("join is a common request");
    assert_eq!(
        response_code(&accepted),
        Some(WsResponseCode::JOINED as i32)
    );

    let mut tractor_room = RoomService::with_ai_players_enabled(false);
    let rejected = tractor_room
        .handle_common_request(2, &join_request(GameId::TRACTOR), handler.game_id(), || {
            handler.build_room_settings()
        })
        .expect("join is a common request");
    assert_eq!(
        response_code(&rejected),
        Some(WsResponseCode::WRONG_GAME as i32)
    );
}
