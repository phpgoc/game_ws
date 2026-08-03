use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use share_type_public::TexasHoldEmPhase;
use ws_common::{CommonGameState, GameState};

use crate::poker_variant::{OMAHA_HOLD_EM, SHORT_DECK_HOLD_EM, STANDARD_TEXAS};

use super::HoldemGameState;

fn common_with_players(player_count: usize) -> Arc<Mutex<CommonGameState>> {
    let common = Arc::new(Mutex::new(CommonGameState::new()));
    let mut locked = common.lock().expect("common state lock");
    for position in 0..player_count {
        locked.add_player(position, position as u64 + 1, &format!("p{position}"));
    }
    drop(locked);
    common
}

fn hand_state() -> HoldemGameState {
    let common = common_with_players(3);
    let mut state = HoldemGameState::from_common_with_variant(common, STANDARD_TEXAS);
    state.hand_players = HashMap::from([
        (0, "p0".to_owned()),
        (1, "p1".to_owned()),
        (2, "p2".to_owned()),
    ]);
    state
}

#[test]
fn constructors_and_shared_state_enforce_the_table_contract() {
    let common = common_with_players(1);
    let mut state = HoldemGameState::from_common(Arc::clone(&common));
    assert!(state.deal_new_hand(100, 5, 10, 0, &HashMap::new()).is_err());
    assert!(!GameState::can_accept_players(&state));
    assert!(Arc::ptr_eq(
        &common,
        &GameState::shared_common_state(&state)
    ));

    state.set_action_received(true);
    state.set_turn_countdown(12);
    assert!(common.lock().expect("common state lock").action_received);
    assert_eq!(state.turn_countdown(), 12);
}

#[test]
fn evaluated_hand_supports_short_deck_and_omaha_rules() {
    let common = common_with_players(2);
    let mut short_deck =
        HoldemGameState::from_common_with_variant(Arc::clone(&common), SHORT_DECK_HOLD_EM);
    short_deck.hands.insert(0, vec![5, 6]);
    short_deck.public_cards = vec![7, 8, 9, 10, 11];
    assert!(short_deck.evaluated_hand(0).is_some());

    let mut omaha = HoldemGameState::from_common_with_variant(common, OMAHA_HOLD_EM);
    omaha.hands.insert(0, vec![1, 2, 3, 4]);
    omaha.public_cards = vec![5, 6, 7, 8, 9];
    assert!(omaha.evaluated_hand(0).is_some());
    assert!(omaha.evaluated_hand(99).is_none());
}

#[test]
fn action_rotation_and_round_completion_skip_unavailable_positions() {
    let mut state = hand_state();
    state.folded.insert(1);
    state.all_in.insert(2);
    assert_eq!(state.next_action_position(0), Some(0));

    state.all_in.insert(0);
    assert_eq!(state.next_action_position(0), None);

    state.current_bet = 10;
    state.round_bets.insert(0, 10);
    state.acted.insert(0);
    assert!(state.is_round_complete());

    state.folded.insert(0);
    assert!(state.is_hand_over_by_folds());
}

#[test]
fn heads_up_button_posts_small_blind_and_acts_first_preflop() {
    let common = common_with_players(2);
    let mut state = HoldemGameState::from_common_with_variant(common, STANDARD_TEXAS);
    let starting_chips = HashMap::from([(0, 1000), (1, 1000)]);

    state
        .deal_new_hand(1000, 5, 10, 0, &starting_chips)
        .expect("deal heads-up hand");

    assert_eq!(state.dealer_position, 0);
    assert_eq!(state.small_blind_position, 0);
    assert_eq!(state.big_blind_position, 1);
    assert_eq!(state.current_position, 0);
    assert_eq!(state.call_amount(0), 5);

    state.phase = TexasHoldEmPhase::PreFlop;
    state.reveal_next_phase();
    assert_eq!(state.current_position, 1);
}

#[test]
fn reveal_next_phase_handles_terminal_and_non_hand_phases() {
    let mut state = hand_state();
    state.phase = TexasHoldEmPhase::River;
    assert_eq!(state.reveal_next_phase(), TexasHoldEmPhase::Settlement);

    state.phase = TexasHoldEmPhase::Start;
    assert_eq!(state.reveal_next_phase(), TexasHoldEmPhase::Start);
}
