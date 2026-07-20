mod action;
mod claim;
mod claim_resolution;
mod lifecycle;
mod self_draw_and_gang;
mod settlement_fan;
mod settlement_score;
mod snapshot;
mod xi_gang;

use std::sync::{Arc, Mutex as StdMutex};

use ws_common::CommonGameState;

use super::*;

fn has_room_event(dispatch: &Dispatch, code: WsCode) -> bool {
    dispatch.messages.iter().any(
        |item| matches!(&item.payload, OutboundPayload::Event(event) if event.code == code as i32),
    )
}

fn open_peng_meld(tile: i32, from_position: usize) -> WsShenyangMahjongMeld {
    build_meld(
        ShenyangMahjongMeldKind::PENG,
        vec![tile, tile, tile],
        Some(from_position),
    )
}

fn open_chi_meld(start_tile: i32) -> WsShenyangMahjongMeld {
    build_meld(
        ShenyangMahjongMeldKind::CHI,
        vec![start_tile, start_tile + 1, start_tile + 2],
        Some(0),
    )
}

fn play_request(
    action: ShenyangMahjongAction,
    tiles: Vec<i32>,
    target_tile: Option<i32>,
    from_position: Option<usize>,
) -> ClientRequest {
    ClientRequest {
        route: Routes::PLAY as i32,
        data: serde_json::json!({
            "action": action as i32,
            "tiles": tiles,
            "target_tile": target_tile,
            "from_position": from_position,
        }),
    }
}

fn playable_state() -> ShenyangMahjongLoopState {
    let base = Arc::new(StdMutex::new(CommonGameState::default()));
    {
        let mut common = base.lock().unwrap();
        for position in 0..4 {
            common.add_player(position, position as u64 + 1, &format!("P{}", position));
        }
    }
    let mut state = ShenyangMahjongLoopState::new(base);
    state.phase = ShenyangMahjongPhase::Play;
    state.current_position = 0;
    state.dealer_position = 0;
    state
}

fn seven_pairs_ting_hand() -> Vec<i32> {
    vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 12, 21, 21, 31, 32]
}

fn default_configs() -> HashMap<String, i32> {
    HashMap::new()
}

fn response_code(dispatch: &Dispatch, recipient: SessionId, route: Routes) -> Option<i32> {
    dispatch
        .messages
        .iter()
        .find_map(|item| match &item.payload {
            OutboundPayload::Response(RequestResponse::WithData(response))
                if item.recipient == recipient && response.route == route as i32 =>
            {
                Some(response.code as i32)
            }
            OutboundPayload::Response(RequestResponse::WithoutData(response))
                if item.recipient == recipient && response.route == route as i32 =>
            {
                Some(response.code as i32)
            }
            _ => None,
        })
}

fn setup_request_room() -> (
    RoomService,
    ShenyangMahjongGameHandler,
    String,
    LoopStateHandle,
) {
    setup_request_room_with_configs(serde_json::json!({}))
}

fn setup_unstarted_request_room() -> (RoomService, String) {
    let mut room_service = RoomService::default();
    for session_id in 1..=4 {
        room_service.connect(session_id);
        let _ = room_service.handle_common_request(
            session_id,
            &ClientRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({
                    "name": format!("P{}", session_id),
                    "password": "mahjong-request-room",
                    "game_id": GameId::SHENYANG_MAHJONG as i32
                }),
            },
            GameId::SHENYANG_MAHJONG,
            build_shenyang_mahjong_settings,
        );
    }
    let room_key = room_service.room_key_of(1).expect("room key");
    (room_service, room_key)
}

fn setup_request_room_with_configs(
    configs: serde_json::Value,
) -> (
    RoomService,
    ShenyangMahjongGameHandler,
    String,
    LoopStateHandle,
) {
    let (mut room_service, room_key) = setup_unstarted_request_room();
    if configs.as_object().is_some_and(|items| !items.is_empty()) {
        let _ = room_service.handle_common_request(
            1,
            &ClientRequest {
                route: Routes::SETTING as i32,
                data: serde_json::json!({ "current_configs": configs }),
            },
            GameId::SHENYANG_MAHJONG,
            build_shenyang_mahjong_settings,
        );
    }
    let common = room_service
        .room_common_state(&room_key)
        .expect("common state");
    let loop_state = Arc::new(StdMutex::new(ShenyangMahjongLoopState::new(Arc::clone(
        &common,
    ))));
    room_service.set_room_game_state(
        &room_key,
        Box::new(ShenyangMahjongGameState::from_loop_state(Arc::clone(
            &loop_state,
        ))),
    );
    let handler = ShenyangMahjongGameHandler::default();
    handler
        .loop_states
        .lock()
        .unwrap()
        .insert(room_key.clone(), Arc::clone(&loop_state));

    (room_service, handler, room_key, loop_state)
}
