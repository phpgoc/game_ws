use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use share_type_public::{
    TexasHoldEmAction, TexasHoldEmPhase, games::texas_hold_em::WsTexasHoldEmPlayRequest,
};
use ws_common::{CommonGameState, RoomService};

use super::{HoldemGameHandler, HoldemGameState, STANDARD_TEXAS, apply_action, settle_hand};

fn new_state() -> HoldemGameState {
    let common = Arc::new(Mutex::new(CommonGameState::new()));
    {
        let mut common = common.lock().unwrap();
        common.add_player(0, 1, "p0");
        common.add_player(1, 2, "p1");
    }
    let mut state = HoldemGameState::from_common_with_variant(common, STANDARD_TEXAS);
    state.hand_players = HashMap::from([(0, "p0".to_string()), (1, "p1".to_string())]);
    state.chips = HashMap::from([(0, 100), (1, 100)]);
    state.big_blind = 10;
    state.min_raise = 10;
    state.current_position = 0;
    state
}

fn request(action: TexasHoldEmAction, amount: i32) -> WsTexasHoldEmPlayRequest {
    WsTexasHoldEmPlayRequest { action, amount }
}

#[test]
fn apply_action_covers_check_call_and_bet_validation() {
    let mut state = new_state();
    let check = apply_action(&mut state, 0, request(TexasHoldEmAction::CHECK, 0))
        .expect("check is legal with no call");
    assert_eq!(check.action, TexasHoldEmAction::CHECK);
    assert!(state.acted.contains(&0));

    state.current_bet = 10;
    assert!(apply_action(&mut state, 0, request(TexasHoldEmAction::CHECK, 0)).is_none());
    let call = apply_action(&mut state, 0, request(TexasHoldEmAction::CALL, 0))
        .expect("call matches the current bet");
    assert_eq!(call.amount, 10);
    assert_eq!(state.pot, 10);
    assert!(apply_action(&mut state, 0, request(TexasHoldEmAction::CALL, 0)).is_none());

    state.current_bet = 0;
    state.acted.clear();
    state.round_bets.clear();
    state.contributions.clear();
    state.chips.insert(0, 100);
    state.pot = 0;
    assert!(apply_action(&mut state, 0, request(TexasHoldEmAction::BET, 9)).is_none());
    let bet = apply_action(&mut state, 0, request(TexasHoldEmAction::BET, 20))
        .expect("minimum opening bet is legal");
    assert_eq!(bet.amount, 20);
    assert_eq!(state.current_bet, 20);
    assert_eq!(state.min_raise, 20);
    assert!(apply_action(&mut state, 0, request(TexasHoldEmAction::BET, 20)).is_none());
}

#[test]
fn apply_action_covers_full_and_under_raise_paths() {
    let mut state = new_state();
    state.current_bet = 10;
    state.round_bets.insert(0, 10);
    state.acted.insert(1);
    assert!(apply_action(&mut state, 0, request(TexasHoldEmAction::RAISE, 9)).is_none());

    let raise = apply_action(&mut state, 0, request(TexasHoldEmAction::RAISE, 20))
        .expect("full raise is legal");
    assert_eq!(raise.amount, 20);
    assert_eq!(state.current_bet, 30);
    assert_eq!(state.min_raise, 20);
    assert_eq!(state.acted, [0].into_iter().collect());

    let mut under_raise = new_state();
    under_raise.current_bet = 10;
    under_raise.min_raise = 10;
    under_raise.chips.insert(0, 15);
    under_raise.acted.insert(1);
    let all_in = apply_action(&mut under_raise, 0, request(TexasHoldEmAction::ALL_IN, 0))
        .expect("short all-in is still a legal action");
    assert!(all_in.all_in);
    assert_eq!(under_raise.current_bet, 15);
    assert!(under_raise.acted.contains(&1));

    let mut full_all_in = new_state();
    full_all_in.current_bet = 10;
    full_all_in.min_raise = 10;
    full_all_in.chips.insert(0, 25);
    full_all_in.acted.insert(1);
    apply_action(&mut full_all_in, 0, request(TexasHoldEmAction::ALL_IN, 0))
        .expect("full all-in is legal");
    assert_eq!(full_all_in.current_bet, 25);
    assert_eq!(full_all_in.min_raise, 15);
    assert_eq!(full_all_in.acted, [0].into_iter().collect());

    let mut empty = new_state();
    empty.chips.insert(0, 0);
    assert!(apply_action(&mut empty, 0, request(TexasHoldEmAction::ALL_IN, 0)).is_none());
}

#[test]
fn actions_reject_unfunded_bets_and_raises_that_do_not_cover_the_call() {
    let mut unfunded_bet = new_state();
    unfunded_bet.chips.insert(0, 0);
    assert!(apply_action(&mut unfunded_bet, 0, request(TexasHoldEmAction::BET, 10)).is_none());

    let mut short_raise = new_state();
    short_raise.current_bet = 10;
    short_raise.chips.insert(0, 5);
    assert!(apply_action(&mut short_raise, 0, request(TexasHoldEmAction::RAISE, 10)).is_none());
}

#[test]
fn settlement_returns_excess_when_every_contributor_folded() {
    let mut state = new_state();
    state.phase = TexasHoldEmPhase::River;
    state.hand_players = HashMap::from([(0, "p0".to_string()), (1, "p1".to_string())]);
    state.contributions = HashMap::from([(0, 6), (1, 5)]);
    state.chips = HashMap::from([(0, 0), (1, 0)]);
    state.folded.extend([0, 1]);
    state.pot = 11;
    let handle = Arc::new(Mutex::new(state));

    let settlement = settle_hand(&handle);
    let state = handle.lock().unwrap();
    assert!(settlement.winners.is_empty());
    assert_eq!(state.phase, TexasHoldEmPhase::Settlement);
    assert_eq!(state.chip_count(0), 6);
    assert_eq!(state.chip_count(1), 5);
}

#[test]
fn settlement_keeps_an_unresolved_pot_without_a_five_card_winner() {
    let mut state = new_state();
    state.contributions = HashMap::from([(0, 10), (1, 10)]);
    state.chips = HashMap::from([(0, 0), (1, 0)]);
    state.pot = 20;
    let handle = Arc::new(Mutex::new(state));

    let settlement = settle_hand(&handle);
    assert!(settlement.winners.is_empty());
    assert_eq!(settlement.pot, 20);
}

#[test]
fn game_request_guards_return_errors_for_unknown_sessions() {
    let handler = HoldemGameHandler::default();
    let mut room = RoomService::default();

    let auto = handler.handle_auto_strategy(&mut room, 99, serde_json::json!(null));
    let play = handler.handle_play(&mut room, 99, serde_json::json!(null));
    let start = handler.handle_start(&mut room, 99);

    assert_eq!(auto.messages.len(), 1);
    assert_eq!(play.messages.len(), 1);
    assert_eq!(start.messages.len(), 1);
    assert!(!room.room_exists("room"));
}

#[test]
fn active_room_state_rejects_new_seats_during_a_hand() {
    let common = Arc::new(Mutex::new(CommonGameState::default()));
    let room = super::ActiveHoldemRoomState {
        common: Arc::clone(&common),
        hand_positions: [0, 1].into_iter().collect(),
    };
    assert!(!ws_common::GameState::can_accept_players(&room));
    assert!(ws_common::GameState::can_join_players(&room));
    assert!(ws_common::GameState::position_reserved_for_join(&room, 1));
    assert!(!ws_common::GameState::position_reserved_for_join(&room, 2));
    assert!(Arc::ptr_eq(
        &common,
        &ws_common::GameState::shared_common_state(&room)
    ));
}
