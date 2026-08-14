use std::sync::Arc;

use serde_json::{Value, json};
use share_type_public::{
    GameId, Routes, UpgradePhase, UpgradeRoutes, UpgradeSuit, UpgradeWsCode, WsJoinRequest,
    WsResponseCode, WsUpgradeSelectTrumpRequest,
};
use upgrade_common::{Rank, Suit};
use ws_common::{
    ClientRequest, Dispatch, GameHandler, GameState, OutboundPayload, RequestResponse, RoomService,
};

use super::{UpgradeGameHandler, UpgradeGameStateHandle, join_succeeded};
use crate::{
    game_setting::{
        KEY_ATTACKING_WIN_SCORE, KEY_DECK_COUNT, KEY_PLAY_TIME, KEY_REMOVED_RANK_COUNT,
        KEY_SCORE_PER_LEVEL, KEY_SHUTOUT_BONUS_LEVELS, build_upgrade_settings,
    },
    state::UpgradeGameState,
};

const ROOM_KEY: &str = "upgrade-handler-coverage";

fn join_request(name: &str) -> ClientRequest {
    ClientRequest {
        route: Routes::JOIN as i32,
        data: serde_json::to_value(WsJoinRequest {
            name: name.to_owned(),
            password: ROOM_KEY.to_owned(),
            game_id: GameId::UPGRADE,
            session_id: String::new(),
            avatar_url: String::new(),
        })
        .expect("serialize upgrade join request"),
    }
}

fn select_spade_request() -> Value {
    serde_json::to_value(WsUpgradeSelectTrumpRequest {
        trump_suit: UpgradeSuit::SPADE,
    })
    .expect("serialize upgrade select-trump request")
}

fn join(room: &mut RoomService, session_id: u64) {
    room.handle_common_request(
        session_id,
        &join_request(&format!("u{session_id}")),
        GameId::UPGRADE,
        build_upgrade_settings,
    )
    .expect("common upgrade join dispatch");
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
            .expect("upgrade request response"),
    )
}

fn ready_room() -> (UpgradeGameHandler, RoomService) {
    let handler = UpgradeGameHandler::default();
    let mut room = RoomService::default();
    for session_id in 1..=4 {
        join(&mut room, session_id);
    }
    (handler, room)
}

#[test]
fn request_handlers_reject_unjoined_and_malformed_messages() {
    let handler = UpgradeGameHandler::default();
    let mut room = RoomService::default();

    assert_eq!(
        response_code(&handler.handle_start(&mut room, 99)),
        WsResponseCode::NOT_LOGIN
    );
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
        response_code(&handler.handle_start(&mut room, 2)),
        WsResponseCode::NO_PERMISSION
    );
    for dispatch in [
        handler.handle_bury_bottom(&mut room, 1, json!({ "cards": [1, 2] })),
        handler.handle_declare_trump(&mut room, 1, json!({ "cards": [1] })),
        handler.handle_play(&mut room, 1, json!({ "cards": [1] })),
        handler.handle_select_trump(&mut room, 1, select_spade_request()),
    ] {
        assert_eq!(response_code(&dispatch), WsResponseCode::NO_PERMISSION);
    }

    assert_eq!(
        response_code(&handler.handle_start(&mut room, 1)),
        WsResponseCode::OK
    );
    let state = handler.state(ROOM_KEY).expect("started upgrade state");
    assert_eq!(state.lock().unwrap().phase, UpgradePhase::Deal);
    assert_eq!(
        response_code(&handler.handle_start(&mut room, 1)),
        WsResponseCode::NO_PERMISSION
    );
    for dispatch in [
        handler.handle_bury_bottom(&mut room, 1, json!({ "cards": [1, 2] })),
        handler.handle_play(&mut room, 1, json!({ "cards": [1] })),
        handler.handle_select_trump(&mut room, 1, select_spade_request()),
    ] {
        assert_eq!(response_code(&dispatch), WsResponseCode::NO_PERMISSION);
    }
}

#[test]
fn away_human_cannot_send_a_manual_play_until_back() {
    let (handler, mut room) = ready_room();
    assert_eq!(
        response_code(&handler.handle_start(&mut room, 1)),
        WsResponseCode::OK
    );
    let state = handler.state(ROOM_KEY).expect("started upgrade state");
    {
        let mut state = state.lock().unwrap();
        state.phase = UpgradePhase::Play;
        state.current_position = 0;
        state.hands = std::collections::HashMap::from([
            (0, vec![1]),
            (1, vec![14]),
            (2, vec![27]),
            (3, vec![40]),
        ]);
        state.set_turn_countdown(30);
        state.base.lock().unwrap().mark_away(0);
    }

    let rejected = handler.handle_play(&mut room, 1, json!({ "cards": [1] }));
    assert_eq!(response_code(&rejected), WsResponseCode::NO_PERMISSION);
    assert_eq!(state.lock().unwrap().private_hand(0), vec![1]);

    let back_request = ClientRequest {
        route: Routes::BACK as i32,
        data: Value::Null,
    };
    room.handle_common_request(1, &back_request, GameId::UPGRADE, build_upgrade_settings);
    let accepted = handler.handle_play(&mut room, 1, json!({ "cards": [1] }));
    assert_eq!(response_code(&accepted), WsResponseCode::OK);
}

#[test]
fn rules_conversion_and_state_adapter_keep_the_upgrade_contract() {
    let configs = std::collections::HashMap::from([
        (KEY_ATTACKING_WIN_SCORE.to_owned(), -20),
        (KEY_SCORE_PER_LEVEL.to_owned(), -1),
        (KEY_SHUTOUT_BONUS_LEVELS.to_owned(), 99),
        (KEY_DECK_COUNT.to_owned(), 99),
        (KEY_REMOVED_RANK_COUNT.to_owned(), 99),
        (KEY_PLAY_TIME.to_owned(), -2),
    ]);
    let rules = UpgradeGameHandler::configs_to_rules(&configs);
    assert_eq!(rules.attacking_win_score, 1);
    assert_eq!(rules.score_per_level, 1);
    assert_eq!(rules.shutout_bonus_levels, 3);
    assert_eq!(rules.deck_count.get(), 6);
    assert_eq!(rules.removed_rank_count, 6);
    assert_eq!(rules.bottom_card_count, 8);
    assert_eq!(rules.target_rank, Rank::Five);
    assert_eq!(rules.final_target_rank, Rank::Ace);
    assert_eq!(rules.trump_suit, None);
    assert_eq!(UpgradeGameHandler::play_time(&configs), 1);

    let common = Arc::new(std::sync::Mutex::new(ws_common::CommonGameState::new()));
    let state = Arc::new(std::sync::Mutex::new(UpgradeGameState::from_common(
        Arc::clone(&common),
    )));
    let adapter = UpgradeGameStateHandle {
        inner: Arc::clone(&state),
    };
    assert!(adapter.can_accept_players());
    assert!(Arc::ptr_eq(&adapter.shared_common_state(), &common));
    state.lock().unwrap().phase = UpgradePhase::Play;
    assert!(!adapter.can_accept_players());

    let mut dispatch = Dispatch::default();
    UpgradeGameHandler::push_private_event(
        &mut dispatch,
        7,
        UpgradeWsCode::HAND_UPDATED,
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

    let mut handler = UpgradeGameHandler::default();
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
    assert_ne!(UpgradeRoutes::BURY_BOTTOM as i32, Routes::PLAY as i32);
}

#[test]
fn successful_action_handlers_update_state_and_broadcast() {
    let (handler, mut room) = ready_room();
    assert_eq!(
        response_code(&handler.handle_start(&mut room, 1)),
        WsResponseCode::OK
    );
    let state = handler.state(ROOM_KEY).expect("running upgrade state");
    {
        let mut state = state.lock().unwrap();
        state.phase = UpgradePhase::Bury;
        state.dealer_position = 0;
        state.rules.trump_suit = Some(Suit::Heart);
        state.rules.bottom_card_count = 2;
        state.hands.insert(0, vec![1, 2, 3]);
        state.set_turn_countdown(90);
    }
    assert_eq!(
        response_code(&handler.handle_bury_bottom(&mut room, 1, json!({ "cards": [1, 2] }))),
        WsResponseCode::OK
    );
    assert_eq!(state.lock().unwrap().phase, UpgradePhase::Play);

    let (handler, mut room) = ready_room();
    assert_eq!(
        response_code(&handler.handle_start(&mut room, 1)),
        WsResponseCode::OK
    );
    let state = handler.state(ROOM_KEY).expect("running upgrade state");
    {
        let mut state = state.lock().unwrap();
        state.phase = UpgradePhase::Deal;
        state.round_index = 0;
        state.rules.target_rank = Rank::Three;
        state.hands.insert(0, vec![2]);
    }
    assert_eq!(
        response_code(&handler.handle_declare_trump(&mut room, 1, json!({ "cards": [2] }))),
        WsResponseCode::OK
    );
    assert_eq!(state.lock().unwrap().rules.trump_suit, Some(Suit::Spade));

    let (handler, mut room) = ready_room();
    assert_eq!(
        response_code(&handler.handle_start(&mut room, 1)),
        WsResponseCode::OK
    );
    let state = handler.state(ROOM_KEY).expect("running upgrade state");
    {
        let mut state = state.lock().unwrap();
        state.phase = UpgradePhase::Bury;
        state.round_index = 1;
        state.dealer_position = 0;
        state.rules.trump_suit = None;
        state.set_turn_countdown(73);
    }
    assert_eq!(
        response_code(&handler.handle_select_trump(&mut room, 1, select_spade_request())),
        WsResponseCode::OK
    );
    assert_eq!(state.lock().unwrap().rules.trump_suit, Some(Suit::Spade));
    assert_eq!(
        state.lock().unwrap().base.lock().unwrap().turn_countdown,
        73
    );

    let (handler, mut room) = ready_room();
    assert_eq!(
        response_code(&handler.handle_start(&mut room, 1)),
        WsResponseCode::OK
    );
    let state = handler.state(ROOM_KEY).expect("running upgrade state");
    {
        let mut state = state.lock().unwrap();
        state.phase = UpgradePhase::Play;
        state.current_position = 0;
        state.rules.trump_suit = Some(Suit::Heart);
        state.hands.insert(0, vec![1]);
        state.set_turn_countdown(30);
    }
    assert_eq!(
        response_code(&handler.handle_play(&mut room, 1, json!({ "cards": [1] }))),
        WsResponseCode::OK
    );
    assert_eq!(state.lock().unwrap().current_trick.len(), 1);
}

#[test]
fn expired_operation_window_rejects_actions_without_mutation() {
    let (handler, mut room) = ready_room();
    assert_eq!(
        response_code(&handler.handle_start(&mut room, 1)),
        WsResponseCode::OK
    );
    let state = handler.state(ROOM_KEY).expect("running upgrade state");

    {
        let mut state = state.lock().unwrap();
        state.phase = UpgradePhase::Bury;
        state.dealer_position = 0;
        state.round_index = 1;
        state.rules.trump_suit = Some(Suit::Heart);
        state.rules.bottom_card_count = 2;
        state.hands.insert(0, vec![1, 2, 3]);
        state.set_turn_countdown(0);
    }
    assert_eq!(
        response_code(&handler.handle_bury_bottom(&mut room, 1, json!({ "cards": [1, 2] }))),
        WsResponseCode::NO_PERMISSION
    );
    assert_eq!(state.lock().unwrap().private_hand(0), vec![1, 2, 3]);

    {
        let mut state = state.lock().unwrap();
        state.phase = UpgradePhase::Bury;
        state.rules.trump_suit = None;
        state.set_turn_countdown(0);
    }
    assert_eq!(
        response_code(&handler.handle_select_trump(&mut room, 1, select_spade_request())),
        WsResponseCode::NO_PERMISSION
    );
    assert_eq!(state.lock().unwrap().rules.trump_suit, None);

    {
        let mut state = state.lock().unwrap();
        state.phase = UpgradePhase::Play;
        state.current_position = 0;
        state.rules.trump_suit = Some(Suit::Heart);
        state.hands.insert(0, vec![1]);
        state.set_turn_countdown(0);
    }
    assert_eq!(
        response_code(&handler.handle_play(&mut room, 1, json!({ "cards": [1] }))),
        WsResponseCode::NO_PERMISSION
    );
    assert_eq!(state.lock().unwrap().private_hand(0), vec![1]);
}

#[test]
fn rejoin_preserves_the_remaining_operation_countdown() {
    let (mut handler, mut room) = ready_room();
    assert_eq!(
        response_code(&handler.handle_start(&mut room, 1)),
        WsResponseCode::OK
    );
    let state = handler.state(ROOM_KEY).expect("running upgrade state");
    let request = join_request("u1");

    for (phase, countdown) in [(UpgradePhase::Bury, 17), (UpgradePhase::Play, 11)] {
        {
            let mut state = state.lock().unwrap();
            state.phase = phase;
            state.dealer_position = 0;
            state.current_position = 0;
            state.set_turn_countdown(countdown);
        }
        let mut dispatch = Dispatch::default();
        dispatch.messages.push(ws_common::Delivery {
            recipient: 1,
            payload: OutboundPayload::Response(RequestResponse::WithData(
                share_type_public::ws::WsResponse {
                    route: Routes::JOIN as i32,
                    code: WsResponseCode::JOINED,
                    data: json!({}),
                },
            )),
        });

        handler.after_common_request(&mut room, 1, &request, &mut dispatch);

        assert_eq!(
            state.lock().unwrap().base.lock().unwrap().turn_countdown,
            countdown
        );
    }
}

#[test]
fn settlement_rejoin_restores_the_game_over_event() {
    let (mut handler, mut room) = ready_room();
    assert_eq!(
        response_code(&handler.handle_start(&mut room, 1)),
        WsResponseCode::OK
    );
    let state = handler.state(ROOM_KEY).expect("running upgrade state");
    state.lock().unwrap().phase = UpgradePhase::Settlement;
    let request = join_request("u1");
    let mut dispatch = Dispatch::default();
    dispatch.messages.push(ws_common::Delivery {
        recipient: 1,
        payload: OutboundPayload::Response(RequestResponse::WithData(
            share_type_public::ws::WsResponse {
                route: Routes::JOIN as i32,
                code: WsResponseCode::JOINED,
                data: json!({}),
            },
        )),
    });

    handler.after_common_request(&mut room, 1, &request, &mut dispatch);

    let settlement = dispatch.messages.iter().find_map(|message| {
        if message.recipient != 1 {
            return None;
        }
        let OutboundPayload::Event(event) = &message.payload else {
            return None;
        };
        (event.code == share_type_public::WsCode::GAME_OVER as i32).then(|| event.data.clone())
    });
    assert!(
        settlement.is_some(),
        "settlement rejoin must restore GAME_OVER"
    );
}
