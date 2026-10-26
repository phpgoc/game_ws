use serde_json::{Value, json};
use share_type_public::{DominoesPhase, Routes, WsCode, WsRequest, WsResponseCode};
use ws_common::{Dispatch, GameHandler, OutboundPayload, RequestResponse, RoomService};

use super::DominoesGameHandler;

const ROOM: &str = "dominoes-next-match";

fn request(
    handler: &mut DominoesGameHandler,
    rooms: &mut RoomService,
    session: u64,
    route: Routes,
    data: Value,
) -> Dispatch {
    let request = WsRequest {
        route: route as i32,
        data,
    };
    if let Some(mut dispatch) =
        rooms.handle_common_request(session, &request, handler.game_id(), || {
            handler.build_room_settings()
        })
    {
        handler.after_common_request(rooms, session, &request, &mut dispatch);
        dispatch
    } else {
        handler.handle_game_request(rooms, session, request)
    }
}

fn assert_response(dispatch: &Dispatch, route: Routes, code: WsResponseCode) {
    assert!(
        dispatch
            .messages
            .iter()
            .any(|message| match &message.payload {
                OutboundPayload::Response(RequestResponse::WithoutData(reply)) =>
                    reply.route == route as i32 && reply.code as i32 == code as i32,
                OutboundPayload::Response(RequestResponse::WithData(reply)) =>
                    reply.route == route as i32 && reply.code as i32 == code as i32,
                _ => false,
            }),
        "expected {route:?} response {code:?}: {dispatch:?}"
    );
}

fn join(handler: &mut DominoesGameHandler, rooms: &mut RoomService, session: u64) -> Dispatch {
    rooms.connect(session);
    let dispatch = request(
        handler,
        rooms,
        session,
        Routes::JOIN,
        json!({
            "name": format!("P{session}"), "password": ROOM,
            "session_id": format!("lobby-test-{session}"),
            "game_id": handler.game_id() as i32,
        }),
    );
    assert_response(&dispatch, Routes::JOIN, WsResponseCode::JOINED);
    dispatch
}

fn finished_room() -> (DominoesGameHandler, RoomService) {
    let mut handler = DominoesGameHandler::default();
    let mut rooms = RoomService::with_ai_players_enabled(true);
    for session in 1..=3 {
        join(&mut handler, &mut rooms, session);
    }
    let dispatch = request(&mut handler, &mut rooms, 1, Routes::START, json!({}));
    assert_response(&dispatch, Routes::START, WsResponseCode::OK);
    let state = handler.state_for_room(&rooms, ROOM).unwrap();
    let mut state = state.lock().unwrap();
    state.start_new_game().unwrap();
    // 用真实发牌与核心裁判打完整比赛，不直接把 phase 改成 GameOver。
    for _ in 0..10_000 {
        match state.phase {
            DominoesPhase::GameOver => return (handler, rooms),
            DominoesPhase::RoundOver => {
                state.start_next_round().unwrap();
            }
            DominoesPhase::Play => {
                crate::action::automatic_turn(
                    &mut state,
                    share_type_public::DominoesActionSource::Timeout,
                    |_, _, plays| plays[0],
                )
                .unwrap();
            }
            DominoesPhase::StarterSelection => unreachable!(),
        }
    }
    panic!("match did not finish");
}

#[test]
fn final_result_survives_rejoin_and_settings_until_the_roster_changes() {
    let (mut handler, mut rooms) = finished_room();
    let final_state = handler.state_for_room(&rooms, ROOM).unwrap();
    let result = final_state
        .lock()
        .unwrap()
        .last_round_result
        .clone()
        .unwrap();
    let dispatch = join(&mut handler, &mut rooms, 2);
    assert!(dispatch.messages.iter().any(|message| matches!(
        &message.payload,
        OutboundPayload::Event(event) if event.code == share_type_public::DominoesWsCode::GAME_OVER as i32
    )));
    let dispatch = request(
        &mut handler,
        &mut rooms,
        1,
        Routes::SETTING,
        json!({
            "current_configs": { "rule": 1, "target_score": 1 }
        }),
    );
    assert_response(&dispatch, Routes::SETTING, WsResponseCode::OK);
    assert_eq!(
        final_state.lock().unwrap().last_round_result.as_ref(),
        Some(&result)
    );

    let dispatch = join(&mut handler, &mut rooms, 4);
    assert!(handler.state_for_room(&rooms, ROOM).is_none());
    assert!(dispatch.messages.iter().all(|message| {
        match &message.payload {
            OutboundPayload::Response(RequestResponse::WithData(reply)) => reply
                .data
                .get("rejoin_data")
                .and_then(|data| data.get("dominoes"))
                .is_none_or(Value::is_null),
            _ => true,
        }
    }));
    let dispatch = request(&mut handler, &mut rooms, 1, Routes::START, json!({}));
    assert_response(&dispatch, Routes::START, WsResponseCode::OK);
    let next = handler.state_for_room(&rooms, ROOM).unwrap();
    let next = next.lock().unwrap();
    assert_eq!(next.positions, vec![0, 1, 2, 3]);
    assert_eq!(next.rule, share_type_public::DominoesRule::FiveUp);
    assert_eq!(next.target_score, 61);
    assert_eq!(next.phase, DominoesPhase::StarterSelection);
    assert!(next.scores.values().all(|score| *score == 0));
    assert!(next.last_round_result.is_none());
}

#[test]
fn reconnecting_with_a_new_session_preserves_the_completed_match() {
    let (mut handler, mut rooms) = finished_room();
    let previous = handler.state_for_room(&rooms, ROOM).unwrap();
    rooms.disconnect(2);
    rooms.connect(22);
    let game_id = handler.game_id() as i32;
    let dispatch = request(
        &mut handler,
        &mut rooms,
        22,
        Routes::JOIN,
        json!({
            "name": "P2", "password": ROOM, "game_id": game_id,
            "session_id": "lobby-test-22",
        }),
    );
    assert_response(&dispatch, Routes::JOIN, WsResponseCode::JOINED);
    let restored = handler.state_for_room(&rooms, ROOM).unwrap();
    assert!(std::sync::Arc::ptr_eq(&previous, &restored));
    assert_eq!(rooms.session_position(22), Some(1));
}

#[test]
fn only_the_owner_can_start_a_new_match_after_the_final_result() {
    let (mut handler, mut rooms) = finished_room();
    let previous = handler.state_for_room(&rooms, ROOM).unwrap();
    let result = previous.lock().unwrap().last_round_result.clone().unwrap();

    let dispatch = request(&mut handler, &mut rooms, 2, Routes::START, json!({}));
    assert_response(&dispatch, Routes::START, WsResponseCode::NO_PERMISSION);
    let retained = handler.state_for_room(&rooms, ROOM).unwrap();
    assert!(std::sync::Arc::ptr_eq(&previous, &retained));
    assert_eq!(
        retained.lock().unwrap().last_round_result.as_ref(),
        Some(&result)
    );

    let dispatch = request(&mut handler, &mut rooms, 1, Routes::START, json!({}));
    assert_response(&dispatch, Routes::START, WsResponseCode::OK);
    let next = handler.state_for_room(&rooms, ROOM).unwrap();
    assert!(!std::sync::Arc::ptr_eq(&previous, &next));
    let next = next.lock().unwrap();
    assert_eq!(next.phase, DominoesPhase::StarterSelection);
    assert_eq!(next.round, 1);
    assert!(next.scores.values().all(|score| *score == 0));
    assert!(next.last_round_result.is_none());
}

#[test]
fn terminal_room_allows_ai_changes_and_drops_previous_seat_results() {
    let (mut handler, mut rooms) = finished_room();
    let dispatch = request(
        &mut handler,
        &mut rooms,
        1,
        Routes::ADD_AI,
        json!({ "count": 1 }),
    );
    assert_response(&dispatch, Routes::ADD_AI, WsResponseCode::OK);
    assert!(handler.state_for_room(&rooms, ROOM).is_none());
    let dispatch = request(
        &mut handler,
        &mut rooms,
        1,
        Routes::REMOVE_AI,
        json!({ "position": 3 }),
    );
    assert_response(&dispatch, Routes::REMOVE_AI, WsResponseCode::OK);
    let dispatch = request(&mut handler, &mut rooms, 1, Routes::START, json!({}));
    assert_response(&dispatch, Routes::START, WsResponseCode::OK);
}

#[test]
fn terminal_swap_and_quit_clear_results_but_an_invalid_swap_keeps_them() {
    let (mut handler, mut rooms) = finished_room();
    let dispatch = request(
        &mut handler,
        &mut rooms,
        2,
        Routes::SWAP,
        json!({ "a": 0, "b": 1 }),
    );
    assert_response(&dispatch, Routes::SWAP, WsResponseCode::NO_PERMISSION);
    assert!(handler.state_for_room(&rooms, ROOM).is_some());
    let dispatch = request(
        &mut handler,
        &mut rooms,
        1,
        Routes::SWAP,
        json!({ "a": 1, "b": 2 }),
    );
    assert_response(&dispatch, Routes::SWAP, WsResponseCode::OK);
    assert!(handler.state_for_room(&rooms, ROOM).is_none());
    assert_eq!(rooms.session_position(1), Some(0));
    assert_eq!(rooms.session_position(2), Some(2));
    assert_eq!(rooms.session_position(3), Some(1));

    let (mut handler, mut rooms) = finished_room();
    let dispatch = request(&mut handler, &mut rooms, 2, Routes::QUIT, json!({}));
    assert_response(&dispatch, Routes::QUIT, WsResponseCode::OK);
    assert!(handler.state_for_room(&rooms, ROOM).is_none());
    assert!(dispatch.messages.iter().any(|message| matches!(
        &message.payload, OutboundPayload::Event(event) if event.code == WsCode::QUIT as i32
    )));
    join(&mut handler, &mut rooms, 4);
    let dispatch = request(&mut handler, &mut rooms, 1, Routes::START, json!({}));
    assert_response(&dispatch, Routes::START, WsResponseCode::OK);
    let next = handler.state_for_room(&rooms, ROOM).unwrap();
    assert!(!next.lock().unwrap().base.lock().unwrap().stop_requested);
}

#[test]
fn interrupted_match_can_replace_the_departed_player_and_start_with_a_new_loop_identity() {
    let (mut handler, mut rooms) = finished_room();
    let dispatch = request(&mut handler, &mut rooms, 1, Routes::START, json!({}));
    assert_response(&dispatch, Routes::START, WsResponseCode::OK);
    let previous = handler.state_for_room(&rooms, ROOM).unwrap();
    let old_common = std::sync::Arc::clone(&previous.lock().unwrap().base);
    let dispatch = request(&mut handler, &mut rooms, 1, Routes::PAUSE, json!({}));
    assert_response(&dispatch, Routes::PAUSE, WsResponseCode::OK);
    assert!(rooms.is_room_paused(ROOM));
    request(&mut handler, &mut rooms, 2, Routes::QUIT, json!({}));
    assert!(handler.state_for_room(&rooms, ROOM).is_none());
    join(&mut handler, &mut rooms, 4);
    let dispatch = request(&mut handler, &mut rooms, 1, Routes::START, json!({}));
    assert_response(&dispatch, Routes::START, WsResponseCode::OK);
    let next = handler.state_for_room(&rooms, ROOM).unwrap();
    let next = next.lock().unwrap();
    assert!(!std::sync::Arc::ptr_eq(&old_common, &next.base));
    assert!(old_common.lock().unwrap().stop_requested);
    assert!(!next.base.lock().unwrap().stop_requested);
    assert!(!next.base.lock().unwrap().paused);
}

#[test]
fn active_matches_keep_roster_and_settings_locked() {
    let (mut handler, mut rooms) = finished_room();
    let dispatch = request(&mut handler, &mut rooms, 1, Routes::START, json!({}));
    assert_response(&dispatch, Routes::START, WsResponseCode::OK);
    for phase in [
        DominoesPhase::StarterSelection,
        DominoesPhase::Play,
        DominoesPhase::RoundOver,
    ] {
        handler
            .state_for_room(&rooms, ROOM)
            .unwrap()
            .lock()
            .unwrap()
            .phase = phase;
        let dispatch = request(&mut handler, &mut rooms, 1, Routes::START, json!({}));
        assert_response(&dispatch, Routes::START, WsResponseCode::NO_PERMISSION);
        assert_eq!(
            handler
                .state_for_room(&rooms, ROOM)
                .unwrap()
                .lock()
                .unwrap()
                .phase,
            phase,
        );
        let dispatch = request(
            &mut handler,
            &mut rooms,
            1,
            Routes::ADD_AI,
            json!({ "count": 1 }),
        );
        assert_response(&dispatch, Routes::ADD_AI, WsResponseCode::NO_PERMISSION);
        let dispatch = request(
            &mut handler,
            &mut rooms,
            1,
            Routes::SETTING,
            json!({ "current_configs": { "rule": 1 } }),
        );
        assert_response(&dispatch, Routes::SETTING, WsResponseCode::ERROR_FORMAT);
        rooms.connect(4);
        let game_id = handler.game_id() as i32;
        let dispatch = request(
            &mut handler,
            &mut rooms,
            4,
            Routes::JOIN,
            json!({
                "name": "P4", "password": ROOM, "game_id": game_id,
            }),
        );
        assert_response(&dispatch, Routes::JOIN, WsResponseCode::NO_PERMISSION);
    }
}
