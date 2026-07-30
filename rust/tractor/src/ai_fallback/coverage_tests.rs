use std::sync::{Arc, Mutex};

use share_type_public::{TractorPhase, TractorSuit};
use ws_common::CommonGameState;

use super::{best_trump_suit, choose_bury, decide, declaration_decision};
use crate::game_state::{TractorGameState, TractorRules};

fn state() -> TractorGameState {
    let common = Arc::new(Mutex::new(CommonGameState::new()));
    common.lock().unwrap().add_player(0, 1, "p0");
    TractorGameState::from_common(common)
}

#[test]
fn fallback_declaration_is_conservative_and_trump_is_spade() {
    let state = state();
    assert!(declaration_decision(&state, 0, 0, false).is_none());
    assert_eq!(best_trump_suit(&state, 0), TractorSuit::SPADE);
}

#[test]
fn fallback_bury_uses_timeout_selection_only_during_bury_phase() {
    let mut state = state();
    assert!(choose_bury(&state).is_none());
    state.phase = TractorPhase::Bury;
    state.dealer_position = 0;
    state.rules = TractorRules {
        bottom_card_count: 2,
        ..state.rules.clone()
    };
    state.hands.insert(0, vec![1, 14, 27]);
    let bottom = choose_bury(&state).expect("fallback should choose a legal bottom");
    assert_eq!(bottom.len(), 2);
    assert_eq!(bottom, state.choose_timeout_bury().unwrap());
}

#[test]
fn decide_delegates_to_the_rules_correct_auto_play() {
    let mut state = state();
    state.phase = TractorPhase::Play;
    state.hands.insert(0, vec![14, 1]);
    assert_eq!(decide(&state, 0), Some(vec![14]));
    state.hands.insert(0, Vec::new());
    assert_eq!(decide(&state, 0), None);
}
