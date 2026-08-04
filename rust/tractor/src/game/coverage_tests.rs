use std::sync::Arc;

use serde_json::{Value, json};
use share_type_public::{
    GameId, Routes, TractorPhase, TractorRank, TractorRoutes, TractorSuit, WsCode, WsJoinRequest,
    WsResponseCode, games::tractor::WsTractorSelectTrumpRequest,
};
use ws_common::{
    ClientRequest, Dispatch, GameHandler, GameState, OutboundPayload, RequestResponse, RoomService,
};

use super::{TractorGameHandler, TractorGameState, TractorGameStateHandle, join_succeeded};
use crate::game_setting::{
    KEY_ATTACKING_WIN_SCORE, KEY_BOTTOM_CARD_COUNT, KEY_DECK_COUNT, KEY_REMOVED_RANK_COUNT,
    KEY_SCORE_PER_LEVEL, KEY_SHUTOUT_BONUS_LEVELS, KEY_TARGET_RANK, build_tractor_settings,
};

const ROOM_KEY: &str = "handler-coverage";

fn join_request(name: &str) -> ClientRequest {
    ClientRequest {
        route: Routes::JOIN as i32,
        data: serde_json::to_value(WsJoinRequest {
            name: name.to_owned(),
            password: ROOM_KEY.to_owned(),
            game_id: GameId::TRACTOR,
            session_id: String::new(),
            avatar_url: String::new(),
        })
        .expect("serialize join request"),
    }
}

fn select_spade_request() -> Value {
    serde_json::to_value(WsTractorSelectTrumpRequest {
        trump_suit: TractorSuit::SPADE,
    })
    .expect("serialize select trump request")
}

fn join(room: &mut RoomService, session_id: u64) {
    room.handle_common_request(
        session_id,
        &join_request(&format!("u{session_id}")),
        GameId::TRACTOR,
        build_tractor_settings,
    )
    .expect("common join dispatch");
}

#[derive(Debug)]
struct ResponseCode(i32);

impl PartialEq<WsResponseCode> for ResponseCode {
    fn eq(&self, other: &WsResponseCode) -> bool {
        self.0 == *other as i32
    }
}

fn response_code(dispatch: &Dispatch) -> ResponseCode {
    ResponseCode(
        dispatch
            .messages
            .iter()
            .find_map(|message| match &message.payload {
                OutboundPayload::Response(RequestResponse::WithoutData(response)) => {
                    Some(response.code as i32)
                }
                _ => None,
            })
            .expect("request response"),
    )
}

fn ready_room() -> (TractorGameHandler, RoomService) {
    let handler = TractorGameHandler::default();
    let mut room = RoomService::default();
    for session_id in 1..=4 {
        join(&mut room, session_id);
    }
    (handler, room)
}

#[test]
fn request_handlers_reject_unjoined_and_malformed_messages() {
    let handler = TractorGameHandler::default();
    let mut room = RoomService::default();

    assert_eq!(
        response_code(&handler.handle_bury_bottom(&mut room, 99, json!({ "cards": [] }))),
        WsResponseCode::NOT_LOGIN
    );
    assert_eq!(
        response_code(&handler.handle_declare_trump(&mut room, 99, json!({ "cards": [] }))),
        WsResponseCode::NOT_LOGIN
    );
    assert_eq!(
        response_code(&handler.handle_play(&mut room, 99, json!({ "cards": [] }))),
        WsResponseCode::NOT_LOGIN
    );
    assert_eq!(
        response_code(&handler.handle_select_trump(&mut room, 99, select_spade_request())),
        WsResponseCode::NOT_LOGIN
    );

    join(&mut room, 1);
    assert_eq!(
        response_code(&handler.handle_bury_bottom(&mut room, 1, Value::Null)),
        WsResponseCode::ERROR_FORMAT
    );
    assert_eq!(
        response_code(&handler.handle_declare_trump(&mut room, 1, Value::Null)),
        WsResponseCode::ERROR_FORMAT
    );
    assert_eq!(
        response_code(&handler.handle_play(&mut room, 1, Value::Null)),
        WsResponseCode::ERROR_FORMAT
    );
    assert_eq!(
        response_code(&handler.handle_select_trump(&mut room, 1, Value::Null)),
        WsResponseCode::ERROR_FORMAT
    );
}

#[test]
fn request_handlers_require_a_running_game_and_reject_wrong_phase_actions() {
    let (handler, mut room) = ready_room();
    assert_eq!(
        response_code(&handler.handle_bury_bottom(&mut room, 1, json!({ "cards": [1, 2] }))),
        WsResponseCode::NO_PERMISSION
    );
    assert_eq!(
        response_code(&handler.handle_declare_trump(&mut room, 1, json!({ "cards": [1] }))),
        WsResponseCode::NO_PERMISSION
    );
    assert_eq!(
        response_code(&handler.handle_play(&mut room, 1, json!({ "cards": [1] }))),
        WsResponseCode::NO_PERMISSION
    );
    assert_eq!(
        response_code(&handler.handle_select_trump(&mut room, 1, select_spade_request()),),
        WsResponseCode::NO_PERMISSION
    );

    assert_eq!(
        response_code(&handler.handle_start(&mut room, 2)),
        WsResponseCode::NO_PERMISSION
    );
    assert_eq!(
        response_code(&handler.handle_start(&mut room, 1)),
        WsResponseCode::OK
    );
    let state = handler.state(ROOM_KEY).expect("started tractor game");
    assert_eq!(state.lock().unwrap().phase, TractorPhase::Deal);

    assert_eq!(
        response_code(&handler.handle_bury_bottom(&mut room, 1, json!({ "cards": [1, 2] }))),
        WsResponseCode::NO_PERMISSION
    );
    assert_eq!(
        response_code(&handler.handle_declare_trump(&mut room, 1, json!({ "cards": [1] }))),
        WsResponseCode::NO_PERMISSION
    );
    assert_eq!(
        response_code(&handler.handle_play(&mut room, 1, json!({ "cards": [1] }))),
        WsResponseCode::NO_PERMISSION
    );
    assert_eq!(
        response_code(&handler.handle_select_trump(&mut room, 1, select_spade_request()),),
        WsResponseCode::NO_PERMISSION
    );
}

#[test]
fn rules_conversion_clamps_settings_and_state_cleanup_keeps_current_room_identity() {
    let rules = TractorGameHandler::configs_to_rules(&std::collections::HashMap::from([
        (KEY_ATTACKING_WIN_SCORE.to_owned(), -20),
        (KEY_SCORE_PER_LEVEL.to_owned(), -1),
        (KEY_SHUTOUT_BONUS_LEVELS.to_owned(), 99),
        (KEY_BOTTOM_CARD_COUNT.to_owned(), -1),
        (KEY_DECK_COUNT.to_owned(), 99),
        (KEY_REMOVED_RANK_COUNT.to_owned(), 99),
        (KEY_TARGET_RANK.to_owned(), 99),
    ]));
    assert_eq!(rules.attacking_win_score, 1);
    assert_eq!(rules.score_per_level, 1);
    assert_eq!(rules.shutout_bonus_levels, 3);
    assert_eq!(rules.bottom_card_count, 0);
    assert_eq!(rules.deck_count, 3);
    assert_eq!(rules.removed_rank_count, 9);
    assert_eq!(rules.target_rank, TractorRank::THREE);
    assert_eq!(rules.trump_suit, None);

    let (handler, mut room) = ready_room();
    assert_eq!(
        response_code(&handler.handle_start(&mut room, 1)),
        WsResponseCode::OK
    );
    let state = handler.state(ROOM_KEY).expect("started state");
    let common = Arc::clone(&state.lock().unwrap().base);
    common.lock().unwrap().request_stop();
    handler.prune_stopped_states(&mut room);
    assert!(handler.state(ROOM_KEY).is_none());
    assert!(room.room_common_state(ROOM_KEY).is_some());
}

#[test]
fn handler_state_adapter_and_join_response_helpers_follow_the_common_contract() {
    let common = Arc::new(std::sync::Mutex::new(ws_common::CommonGameState::new()));
    let state = Arc::new(std::sync::Mutex::new(TractorGameState::from_common(
        Arc::clone(&common),
    )));
    let adapter = TractorGameStateHandle {
        inner: Arc::clone(&state),
    };
    assert!(adapter.can_accept_players());
    assert!(Arc::ptr_eq(&adapter.shared_common_state(), &common));
    state.lock().unwrap().phase = TractorPhase::Play;
    assert!(!adapter.can_accept_players());

    let mut dispatch = Dispatch::default();
    TractorGameHandler::push_private_event(
        &mut dispatch,
        7,
        share_type_public::TractorWsCode::HAND_UPDATED,
        json!({ "cards": [1] }),
    );
    assert_eq!(dispatch.messages.len(), 1);
    assert!(!join_succeeded(&dispatch, 7));
    dispatch.messages.push(ws_common::Delivery {
        recipient: 7,
        payload: OutboundPayload::Response(RequestResponse::WithData(
            share_type_public::ws::WsResponse {
                route: Routes::JOIN as i32,
                code: WsResponseCode::JOINED,
                data: json!({}),
            },
        )),
    });
    assert!(join_succeeded(&dispatch, 7));

    let mut handler = TractorGameHandler::default();
    assert_eq!(handler.game_id(), GameId::TRACTOR);
    assert_eq!(
        response_code(&handler.handle_game_request(
            &mut RoomService::default(),
            1,
            ClientRequest {
                route: 99_999,
                data: Value::Null,
            },
        )),
        WsResponseCode::NOT_IN_RANGE
    );
    assert_eq!(Routes::PLAY as i32, WsCode::PLAY as i32);
    assert_ne!(TractorRoutes::BURY_BOTTOM as i32, Routes::PLAY as i32);
    assert_eq!(TractorSuit::SPADE as i32, 0);
}

#[test]
fn request_handlers_broadcast_successful_bury_declaration_selection_and_play() {
    let (handler, mut room) = ready_room();
    assert_eq!(
        response_code(&handler.handle_start(&mut room, 1)),
        WsResponseCode::OK
    );
    let state = handler.state(ROOM_KEY).expect("running tractor state");
    {
        let mut state = state.lock().unwrap();
        state.phase = TractorPhase::Bury;
        state.dealer_position = 0;
        state.rules.bottom_card_count = 2;
        state.hands.insert(0, vec![1, 2, 3]);
    }
    assert_eq!(
        response_code(&handler.handle_bury_bottom(&mut room, 1, json!({ "cards": [1, 2] }))),
        WsResponseCode::OK
    );
    assert_eq!(state.lock().unwrap().phase, TractorPhase::Play);

    let (handler, mut room) = ready_room();
    assert_eq!(
        response_code(&handler.handle_start(&mut room, 1)),
        WsResponseCode::OK
    );
    let state = handler.state(ROOM_KEY).expect("running tractor state");
    {
        let mut state = state.lock().unwrap();
        state.phase = TractorPhase::Deal;
        state.round_index = 0;
        state.rules.target_rank = TractorRank::TWO;
        state.hands.insert(0, vec![1]);
    }
    assert_eq!(
        response_code(&handler.handle_declare_trump(&mut room, 1, json!({ "cards": [1] }))),
        WsResponseCode::OK
    );
    assert_eq!(
        state.lock().unwrap().rules.trump_suit,
        Some(TractorSuit::SPADE)
    );

    let (handler, mut room) = ready_room();
    assert_eq!(
        response_code(&handler.handle_start(&mut room, 1)),
        WsResponseCode::OK
    );
    let state = handler.state(ROOM_KEY).expect("running tractor state");
    {
        let mut state = state.lock().unwrap();
        state.phase = TractorPhase::Bury;
        state.round_index = 1;
        state.dealer_position = 0;
        state.set_turn_countdown(73);
    }
    assert_eq!(
        response_code(&handler.handle_select_trump(&mut room, 1, select_spade_request())),
        WsResponseCode::OK
    );
    assert_eq!(
        state.lock().unwrap().rules.trump_suit,
        Some(TractorSuit::SPADE)
    );
    assert_eq!(
        state.lock().unwrap().base.lock().unwrap().turn_countdown,
        73
    );

    let (handler, mut room) = ready_room();
    assert_eq!(
        response_code(&handler.handle_start(&mut room, 1)),
        WsResponseCode::OK
    );
    let state = handler.state(ROOM_KEY).expect("running tractor state");
    {
        let mut state = state.lock().unwrap();
        state.phase = TractorPhase::Play;
        state.current_position = 0;
        state.hands.insert(0, vec![1]);
    }
    assert_eq!(
        response_code(&handler.handle_play(&mut room, 1, json!({ "cards": [1] }))),
        WsResponseCode::OK
    );
    assert_eq!(state.lock().unwrap().current_trick.len(), 1);
}
