use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use serde_json::json;
use share_type_public::{GameId, LandlordPhase, LandlordRoutes, Routes, WsResponseCode};
use ws_common::{
    ClientRequest, CommonGameState, Dispatch, GameHandler, OutboundPayload, RequestResponse,
    RoomService,
};

use super::{
    LandlordGameHandler, join_succeeded, loop_state_matches_common,
    running_loop_state_matches_common, validate_play_request_inner,
};
use crate::{game_setting::build_landlord_settings, game_state::LandlordLoopState};

fn join_request(name: &str, room_key: &str) -> ClientRequest {
    ClientRequest {
        route: Routes::JOIN as i32,
        data: json!({
            "name": name,
            "password": room_key,
            "game_id": GameId::LANDLORD as i32,
        }),
    }
}

fn room_with_players(room_key: &str, count: usize) -> RoomService {
    let mut room = RoomService::default();
    for session_id in 1..=count as u64 {
        room.connect(session_id);
        room.handle_common_request(
            session_id,
            &join_request(&format!("P{}", session_id - 1), room_key),
            GameId::LANDLORD,
            build_landlord_settings,
        )
        .expect("landlord JOIN should be handled");
    }
    room
}

fn state_for(
    room: &RoomService,
    room_key: &str,
    phase: LandlordPhase,
) -> Arc<Mutex<LandlordLoopState>> {
    let common = room.room_common_state(room_key).expect("room common state");
    let mut state = LandlordLoopState::new(common);
    state.phase = phase;
    state.current_position = 0;
    state.landlord_position = Some(0);
    state.hands = HashMap::from([(0, vec![2]), (1, vec![3]), (2, vec![4])]);
    state.last_play_position = 0;
    state.last_play = vec![2];
    Arc::new(Mutex::new(state))
}

fn has_code(dispatch: &Dispatch, route: i32, code: WsResponseCode) -> bool {
    dispatch
        .messages
        .iter()
        .any(|message| match &message.payload {
            OutboundPayload::Response(RequestResponse::WithoutData(response)) => {
                response.route == route && response.code as i32 == code as i32
            }
            OutboundPayload::Response(RequestResponse::WithData(response)) => {
                response.route == route && response.code as i32 == code as i32
            }
            OutboundPayload::Event(_) => false,
        })
}

#[test]
fn start_rejects_missing_owner_nonready_rooms_and_duplicate_loops() {
    let mut handler = LandlordGameHandler::default();
    let mut empty = RoomService::default();
    assert!(has_code(
        &handler.handle_start(&mut empty, 99),
        Routes::START as i32,
        WsResponseCode::NOT_LOGIN,
    ));

    let mut not_ready = room_with_players("not-ready", 2);
    assert!(has_code(
        &handler.handle_start(&mut not_ready, 1),
        Routes::START as i32,
        WsResponseCode::NOT_IN_RANGE,
    ));

    let mut ready = room_with_players("ready", 3);
    assert!(has_code(
        &handler.handle_start(&mut ready, 2),
        Routes::START as i32,
        WsResponseCode::NO_PERMISSION,
    ));
    let started = handler.handle_start(&mut ready, 1);
    assert!(has_code(&started, Routes::START as i32, WsResponseCode::OK));
    let duplicate = handler.handle_start(&mut ready, 1);
    assert!(has_code(
        &duplicate,
        Routes::START as i32,
        WsResponseCode::NO_PERMISSION,
    ));
}

#[test]
fn start_resets_a_stopped_common_state_for_a_new_game() {
    let mut room = room_with_players("reset", 3);
    let old_common = room.room_common_state("reset").expect("old common");
    old_common.lock().unwrap().request_stop();
    let mut handler = LandlordGameHandler::default();

    let dispatch = handler.handle_start(&mut room, 1);
    assert!(has_code(
        &dispatch,
        Routes::START as i32,
        WsResponseCode::OK
    ));
    let new_common = room.room_common_state("reset").expect("new common");
    assert!(!Arc::ptr_eq(&old_common, &new_common));
}

#[test]
fn call_landlord_covers_parse_phase_turn_and_score_guards() {
    let mut room = room_with_players("call", 3);
    let state = state_for(&room, "call", LandlordPhase::CallLandlord);
    let handler = LandlordGameHandler::default();
    handler
        .loop_states
        .lock()
        .unwrap()
        .insert("call".to_owned(), Arc::clone(&state));

    assert!(has_code(
        &handler.handle_call_landlord(&mut room, 99, json!({"score": 1})),
        LandlordRoutes::CALL_LANDLORD as i32,
        WsResponseCode::NOT_LOGIN,
    ));
    assert!(has_code(
        &handler.handle_call_landlord(&mut room, 1, json!({"score": "bad"})),
        LandlordRoutes::CALL_LANDLORD as i32,
        WsResponseCode::ERROR_FORMAT,
    ));

    state.lock().unwrap().phase = LandlordPhase::Play;
    assert!(has_code(
        &handler.handle_call_landlord(&mut room, 1, json!({"score": 1})),
        LandlordRoutes::CALL_LANDLORD as i32,
        WsResponseCode::NO_PERMISSION,
    ));
    state.lock().unwrap().phase = LandlordPhase::CallLandlord;
    state.lock().unwrap().current_position = 1;
    assert!(has_code(
        &handler.handle_call_landlord(&mut room, 1, json!({"score": 1})),
        LandlordRoutes::CALL_LANDLORD as i32,
        WsResponseCode::NO_PERMISSION,
    ));
    state.lock().unwrap().current_position = 0;
    assert!(has_code(
        &handler.handle_call_landlord(&mut room, 1, json!({"score": 4})),
        LandlordRoutes::CALL_LANDLORD as i32,
        WsResponseCode::NO_PERMISSION,
    ));
    state.lock().unwrap().score = 2;
    assert!(has_code(
        &handler.handle_call_landlord(&mut room, 1, json!({"score": 1})),
        LandlordRoutes::CALL_LANDLORD as i32,
        WsResponseCode::NO_PERMISSION,
    ));

    state.lock().unwrap().score = 0;
    let success = handler.handle_call_landlord(&mut room, 1, json!({"score": 2}));
    assert!(has_code(
        &success,
        LandlordRoutes::CALL_LANDLORD as i32,
        WsResponseCode::OK,
    ));
    assert_eq!(state.lock().unwrap().call_history, vec![(0, 2)]);
}

#[test]
fn play_covers_missing_state_permissions_and_success() {
    let mut room = room_with_players("play", 3);
    let state = state_for(&room, "play", LandlordPhase::Play);
    let handler = LandlordGameHandler::default();
    handler
        .loop_states
        .lock()
        .unwrap()
        .insert("play".to_owned(), Arc::clone(&state));

    assert!(has_code(
        &handler.handle_play(&mut room, 99, json!({"cards": []})),
        Routes::PLAY as i32,
        WsResponseCode::NOT_LOGIN,
    ));
    assert!(has_code(
        &handler.handle_play(&mut room, 1, json!({"cards": "bad"})),
        Routes::PLAY as i32,
        WsResponseCode::ERROR_FORMAT,
    ));
    state.lock().unwrap().current_position = 1;
    assert!(has_code(
        &handler.handle_play(&mut room, 1, json!({"cards": [2]})),
        Routes::PLAY as i32,
        WsResponseCode::NO_PERMISSION,
    ));
    state.lock().unwrap().current_position = 0;
    assert!(has_code(
        &handler.handle_play(&mut room, 1, json!({"cards": [99]})),
        Routes::PLAY as i32,
        WsResponseCode::NO_PERMISSION,
    ));
    let success = handler.handle_play(&mut room, 1, json!({"cards": [2]}));
    assert!(has_code(&success, Routes::PLAY as i32, WsResponseCode::OK));
    assert_eq!(state.lock().unwrap().current_play, vec![2]);
}

#[test]
fn rejoin_payload_is_attached_for_an_active_play_state() {
    let mut room = room_with_players("rejoin", 3);
    let state = state_for(&room, "rejoin", LandlordPhase::Play);
    let mut handler = LandlordGameHandler::default();
    handler
        .loop_states
        .lock()
        .unwrap()
        .insert("rejoin".to_owned(), Arc::clone(&state));
    let common = room.room_common_state("rejoin").unwrap();
    assert!(loop_state_matches_common(&state, &common));
    assert!(running_loop_state_matches_common(&state, &common));

    let request = join_request("P0", "rejoin");
    let mut dispatch = room
        .handle_common_request(1, &request, GameId::LANDLORD, build_landlord_settings)
        .expect("repeat JOIN should be handled");
    handler.after_common_request(&mut room, 1, &request, &mut dispatch);
    let rejoin_data = dispatch.messages.iter().find_map(|message| {
        let OutboundPayload::Response(RequestResponse::WithData(response)) = &message.payload
        else {
            return None;
        };
        (message.recipient == 1
            && response.route == Routes::JOIN as i32
            && response.code as i32 == WsResponseCode::JOINED as i32)
            .then(|| response.data.get("rejoin_data").cloned())
            .flatten()
    });
    let rejoin_data = rejoin_data.expect("active JOIN should include rejoin data");
    assert_eq!(rejoin_data["my_cards"], json!([2]));
    assert_eq!(rejoin_data["now_playing"], json!(0));
    assert_eq!(rejoin_data["hidden_cards"], json!([]));
}

#[test]
fn stale_loop_is_removed_when_the_room_entry_is_gone() {
    let handler = LandlordGameHandler::default();
    let common = Arc::new(Mutex::new(CommonGameState::default()));
    let state = Arc::new(Mutex::new(LandlordLoopState::new(common)));
    handler
        .loop_states
        .lock()
        .unwrap()
        .insert("gone".to_owned(), Arc::clone(&state));
    assert!(
        handler
            .current_loop_state(&RoomService::default(), "gone")
            .is_none()
    );
    assert!(handler.loop_states.lock().unwrap().is_empty());
}

#[test]
fn validate_play_request_requires_the_play_phase_and_current_position() {
    let room = room_with_players("validate", 3);
    let state = state_for(&room, "validate", LandlordPhase::Play);
    state.lock().unwrap().last_play.clear();
    assert!(validate_play_request_inner(&state.lock().unwrap(), 0, &[2]));
    assert!(!validate_play_request_inner(
        &state.lock().unwrap(),
        1,
        &[2]
    ));
    state.lock().unwrap().phase = LandlordPhase::CallLandlord;
    assert!(!validate_play_request_inner(&state.lock().unwrap(), 0, &[]));
}

#[test]
fn join_succeeded_only_accepts_joined_response_for_the_requested_session() {
    let dispatch = Dispatch::default();
    assert!(!join_succeeded(&dispatch, 1));
}

#[test]
fn start_discards_a_loop_state_owned_by_a_previous_room_instance() {
    let mut room = room_with_players("recreated", 3);
    let current_common = room
        .room_common_state("recreated")
        .expect("current common state");
    let stale_common = Arc::new(Mutex::new(CommonGameState::default()));
    let stale_state = Arc::new(Mutex::new(LandlordLoopState::new(stale_common)));
    let mut handler = LandlordGameHandler::default();
    handler
        .loop_states
        .lock()
        .unwrap()
        .insert("recreated".to_owned(), stale_state);

    let dispatch = handler.handle_start(&mut room, 1);
    assert!(has_code(
        &dispatch,
        Routes::START as i32,
        WsResponseCode::OK,
    ));
    let current_loop = handler
        .current_loop_state(&room, "recreated")
        .expect("new loop state");
    assert!(loop_state_matches_common(&current_loop, &current_common));
}

#[test]
fn game_requests_reject_stale_loops_and_unknown_routes() {
    let mut room = room_with_players("stale", 3);
    let mut handler = LandlordGameHandler::default();

    assert!(has_code(
        &handler.handle_call_landlord(&mut room, 1, json!({ "score": 1 })),
        LandlordRoutes::CALL_LANDLORD as i32,
        WsResponseCode::NO_PERMISSION,
    ));
    assert!(has_code(
        &handler.handle_play(&mut room, 1, json!({ "cards": [2] })),
        Routes::PLAY as i32,
        WsResponseCode::NO_PERMISSION,
    ));
    assert!(has_code(
        &handler.handle_game_request(
            &mut room,
            1,
            ClientRequest {
                route: i32::MAX,
                data: serde_json::Value::Null,
            },
        ),
        i32::MAX,
        WsResponseCode::NOT_IN_RANGE,
    ));

    let state = state_for(&room, "stale", LandlordPhase::Play);
    handler
        .loop_states
        .lock()
        .unwrap()
        .insert("stale".to_owned(), state);
    handler.remove_loop_state("stale");
    assert!(handler.loop_states.lock().unwrap().is_empty());
}

#[test]
fn non_join_common_requests_prune_stopped_loop_states() {
    let mut handler = LandlordGameHandler::default();
    let state = Arc::new(Mutex::new(LandlordLoopState::new(Arc::new(Mutex::new(
        CommonGameState::default(),
    )))));
    state.lock().unwrap().request_stop();
    handler
        .loop_states
        .lock()
        .unwrap()
        .insert("stopped".to_owned(), state);

    handler.after_common_request(
        &mut RoomService::default(),
        1,
        &ClientRequest {
            route: Routes::QUIT as i32,
            data: serde_json::Value::Null,
        },
        &mut Dispatch::default(),
    );
    assert!(handler.loop_states.lock().unwrap().is_empty());
}

#[test]
fn rejoin_call_phase_keeps_hidden_cards_private_and_omits_empty_last_play() {
    let mut room = room_with_players("rejoin-call", 3);
    let state = state_for(&room, "rejoin-call", LandlordPhase::CallLandlord);
    {
        let mut state = state.lock().unwrap();
        state.last_play.clear();
        state.hidden_cards = vec![52, 53, 54];
    }
    let mut handler = LandlordGameHandler::default();
    handler
        .loop_states
        .lock()
        .unwrap()
        .insert("rejoin-call".to_owned(), state);

    let request = join_request("P0", "rejoin-call");
    let mut dispatch = room
        .handle_common_request(1, &request, GameId::LANDLORD, build_landlord_settings)
        .expect("repeat JOIN should be handled");
    handler.after_common_request(&mut room, 1, &request, &mut dispatch);
    let rejoin_data = dispatch.messages.iter().find_map(|message| {
        let OutboundPayload::Response(RequestResponse::WithData(response)) = &message.payload
        else {
            return None;
        };
        (message.recipient == 1
            && response.route == Routes::JOIN as i32
            && response.code as i32 == WsResponseCode::JOINED as i32)
            .then(|| response.data.get("rejoin_data").cloned())
            .flatten()
    });
    let rejoin_data = rejoin_data.expect("call phase JOIN should include rejoin data");
    assert_eq!(rejoin_data["hidden_cards"], json!([]));
    assert!(rejoin_data["last_play_position"].is_null());
}
