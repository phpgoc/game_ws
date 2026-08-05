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

#[test]
fn owner_can_start_the_next_round_after_settlement() {
    let handler = UpgradeGameHandler::default();
    let mut room = RoomService::with_ai_players_enabled(false);
    for session_id in 1..=4 {
        let request = ClientRequest {
            route: Routes::JOIN as i32,
            data: json!({
                "name": format!("player-{session_id}"),
                "password": "upgrade-room",
                "game_id": GameId::UPGRADE as i32,
                "avatar_url": ""
            }),
        };
        room.handle_common_request(session_id, &request, handler.game_id(), || {
            handler.build_room_settings()
        });
    }
    let started = handler.handle_start(&mut room, 1);
    assert_eq!(response_code(&started), Some(WsResponseCode::OK as i32));
    let state = handler.state("upgrade-room").unwrap();
    assert_eq!(
        state.lock().unwrap().base.lock().unwrap().turn_countdown,
        90
    );
    let bottom = state.lock().unwrap().bottom_cards.clone();
    let _ = handler.handle_bury_bottom(&mut room, 1, json!({ "cards": bottom }));
    assert_eq!(
        state.lock().unwrap().base.lock().unwrap().turn_countdown,
        30
    );
    {
        let mut state = state.lock().unwrap();
        state.phase = share_type_public::UpgradePhase::Settlement;
        state.collected_scores.insert(1, 5);
    }

    let next = handler.handle_start(&mut room, 1);

    assert_eq!(response_code(&next), Some(WsResponseCode::OK as i32));
    {
        let state = state.lock().unwrap();
        assert_eq!(state.round_index, 1);
        assert_eq!(state.phase, share_type_public::UpgradePhase::Bury);
        assert_eq!(state.rules.target_rank, upgrade_common::Rank::Five);
        assert_eq!(state.base.lock().unwrap().turn_countdown, 90);
    }
    let bottom = state.lock().unwrap().bottom_cards.clone();
    let _ = handler.handle_select_trump(
        &mut room,
        1,
        json!({ "trump_suit": share_type_public::UpgradeSuit::HEART as i8 }),
    );
    assert_eq!(
        state.lock().unwrap().phase,
        share_type_public::UpgradePhase::Bury
    );
    assert_eq!(
        state.lock().unwrap().base.lock().unwrap().turn_countdown,
        90
    );
    let _ = handler.handle_bury_bottom(&mut room, 1, json!({ "cards": bottom }));
    assert_eq!(
        state.lock().unwrap().phase,
        share_type_public::UpgradePhase::Play
    );
    assert_eq!(
        state.lock().unwrap().base.lock().unwrap().turn_countdown,
        30
    );
}
