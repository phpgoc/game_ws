use std::sync::{Arc, Mutex};

use ws_common::{CommonGameState, GameState};

use super::{LandlordGameState, LandlordLoopState};

#[test]
fn game_state_wrapper_rejects_new_players_and_preserves_the_shared_common_state() {
    let common = Arc::new(Mutex::new(CommonGameState::default()));
    common.lock().unwrap().mark_disconnected(0);
    let loop_state = Arc::new(Mutex::new(LandlordLoopState::new(Arc::clone(&common))));
    let game_state = LandlordGameState::from_loop_state(Arc::clone(&loop_state));

    assert!(!game_state.can_accept_players());
    assert!(Arc::ptr_eq(&game_state.shared_common_state(), &common));
    assert!(loop_state.lock().unwrap().has_disconnected_players());
}

#[test]
fn settlement_without_a_landlord_keeps_scores_unchanged() {
    let common = Arc::new(Mutex::new(CommonGameState::default()));
    let mut state = LandlordLoopState::new(common);
    state.score = 3;

    let summary = state.apply_settlement_scores(true);

    assert_eq!(summary.round_score, 3);
    assert!(state.player_scores.is_empty());
}
