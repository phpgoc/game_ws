use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use share_type_public::{
    GameId, Routes, ShenyangMahjongAction, WsJoinRequest, WsResponseCode,
    games::shenyang_mahjong::{ShenyangMahjongPhase, WsShenyangMahjongPlayRequest},
};
use ws_common::{
    ClientRequest, Dispatch, GameHandler, OutboundPayload, RequestResponse, RoomService,
};

use super::{ShenyangMahjongGameHandler, ShenyangMahjongGameState, ShenyangMahjongLoopState};

const ROOM_KEY: &str = "handler-coverage";

fn join_request(name: &str) -> ClientRequest {
    ClientRequest {
        route: Routes::JOIN as i32,
        data: serde_json::to_value(WsJoinRequest {
            name: name.to_owned(),
            password: ROOM_KEY.to_owned(),
            game_id: GameId::SHENYANG_MAHJONG,
            session_id: String::new(),
            avatar_url: String::new(),
        })
        .expect("serialize join request"),
    }
}

fn join(room: &mut RoomService, session_id: u64) {
    room.handle_common_request(
        session_id,
        &join_request(&format!("p{session_id}")),
        GameId::SHENYANG_MAHJONG,
        crate::game_setting::build_shenyang_mahjong_settings,
    )
    .expect("common join dispatch");
}

fn play_request(action: ShenyangMahjongAction) -> Value {
    serde_json::to_value(WsShenyangMahjongPlayRequest {
        action,
        tiles: Vec::new(),
        target_tile: None,
        from_position: None,
        declare_ting: None,
    })
    .expect("serialize play request")
}

fn response_code(dispatch: &Dispatch) -> i32 {
    dispatch
        .messages
        .iter()
        .find_map(|message| match &message.payload {
            OutboundPayload::Response(RequestResponse::WithoutData(response)) => {
                Some(response.code as i32)
            }
            OutboundPayload::Response(RequestResponse::WithData(response)) => {
                Some(response.code as i32)
            }
            OutboundPayload::Event(_) => None,
        })
        .expect("request response")
}

fn ready_room() -> (RoomService, ShenyangMahjongGameHandler) {
    let mut room = RoomService::default();
    for session_id in 1..=4 {
        join(&mut room, session_id);
    }
    (room, ShenyangMahjongGameHandler::default())
}

fn install_loop_state(
    room: &mut RoomService,
    handler: &ShenyangMahjongGameHandler,
) -> Arc<Mutex<ShenyangMahjongLoopState>> {
    let common = room.room_common_state(ROOM_KEY).expect("room common state");
    let state = Arc::new(Mutex::new(ShenyangMahjongLoopState::new(common)));
    room.set_room_game_state(
        ROOM_KEY,
        Box::new(ShenyangMahjongGameState::from_loop_state(Arc::clone(
            &state,
        ))),
    );
    handler
        .loop_states
        .lock()
        .unwrap()
        .insert(ROOM_KEY.to_owned(), Arc::clone(&state));
    state
}

#[test]
fn play_handler_rejects_unjoined_malformed_and_inactive_rooms() {
    let handler = ShenyangMahjongGameHandler::default();
    let mut room = RoomService::default();
    assert_eq!(
        response_code(&handler.handle_play(
            &mut room,
            99,
            play_request(ShenyangMahjongAction::PASS)
        )),
        WsResponseCode::NOT_LOGIN as i32
    );

    join(&mut room, 1);
    assert_eq!(
        response_code(&handler.handle_play(&mut room, 1, Value::Null)),
        WsResponseCode::ERROR_FORMAT as i32
    );
    assert_eq!(
        response_code(&handler.handle_play(
            &mut room,
            1,
            play_request(ShenyangMahjongAction::PASS)
        )),
        WsResponseCode::NO_PERMISSION as i32
    );
}

#[test]
fn play_handler_requires_play_phase_current_position_and_human_control() {
    let (mut room, handler) = ready_room();
    let state = install_loop_state(&mut room, &handler);

    assert!(handler.current_loop_state(&room, ROOM_KEY).is_some());
    assert_eq!(
        response_code(&handler.handle_play(
            &mut room,
            1,
            play_request(ShenyangMahjongAction::PASS)
        )),
        WsResponseCode::NO_PERMISSION as i32
    );

    {
        let mut state = state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 1;
    }
    assert_eq!(
        response_code(&handler.handle_play(
            &mut room,
            1,
            play_request(ShenyangMahjongAction::PASS)
        )),
        WsResponseCode::NO_PERMISSION as i32
    );

    {
        let mut state = state.lock().unwrap();
        state.current_position = 0;
        state.base.lock().unwrap().mark_away(0);
    }
    assert_eq!(
        response_code(&handler.handle_play(
            &mut room,
            1,
            play_request(ShenyangMahjongAction::PASS)
        )),
        WsResponseCode::NO_PERMISSION as i32
    );
}

#[test]
fn start_and_router_require_owner_ready_room_and_current_loop_identity() {
    let mut handler = ShenyangMahjongGameHandler::default();
    let mut room = RoomService::default();
    assert_eq!(
        response_code(&handler.handle_start(&mut room, 1)),
        WsResponseCode::NOT_LOGIN as i32
    );

    join(&mut room, 1);
    assert_eq!(
        response_code(&handler.handle_start(&mut room, 1)),
        WsResponseCode::NOT_IN_RANGE as i32
    );
    for session_id in 2..=4 {
        join(&mut room, session_id);
    }
    assert_eq!(
        response_code(&handler.handle_start(&mut room, 2)),
        WsResponseCode::NO_PERMISSION as i32
    );
    assert_eq!(
        response_code(&handler.handle_start(&mut room, 1)),
        WsResponseCode::OK as i32
    );
    let active = handler.loop_state(ROOM_KEY).expect("started loop state");
    assert!(handler.current_loop_state(&room, ROOM_KEY).is_some());
    assert_eq!(
        response_code(&handler.handle_start(&mut room, 1)),
        WsResponseCode::NO_PERMISSION as i32
    );
    active.lock().unwrap().request_stop();
    assert!(handler.current_loop_state(&room, ROOM_KEY).is_none());
    handler.prune_stopped_loop_states(&mut room);
    assert!(handler.loop_state(ROOM_KEY).is_none());

    assert_eq!(
        response_code(&handler.handle_game_request(
            &mut room,
            1,
            ClientRequest {
                route: 99_999,
                data: json!({}),
            },
        )),
        WsResponseCode::NOT_IN_RANGE as i32
    );
}
