use std::sync::{Arc, Mutex};

use share_type_public::LandlordPhase;
use ws_common::CommonGameState;

use crate::game_state::LandlordLoopState;

use super::{choose_bid, choose_play, hand_has_bomb};

fn state() -> LandlordLoopState {
    let common = Arc::new(Mutex::new(CommonGameState::new()));
    common.lock().unwrap().add_player(0, 1, "player");
    LandlordLoopState::new(common)
}

#[test]
fn fallback_bid_is_conservative_and_bombs_include_rocket() {
    let state = state();
    assert_eq!(choose_bid(&state, 0), 0);
    assert!(hand_has_bomb(&[1, 14, 27, 40]));
    assert!(hand_has_bomb(&[53, 54]));
    assert!(!hand_has_bomb(&[1, 14, 27, 2]));
}

#[test]
fn fallback_play_only_leads_or_follows_single_cards_when_legal() {
    let mut state = state();
    state.hands.insert(0, vec![7, 2, 15, 53]);
    assert!(choose_play(&state, 0).is_empty());

    state.phase = LandlordPhase::Play;
    state.current_position = 1;
    assert!(choose_play(&state, 0).is_empty());

    state.current_position = 0;
    assert_eq!(choose_play(&state, 0), vec![7]);

    state.last_play_position = 1;
    state.last_play = vec![1];
    assert_eq!(choose_play(&state, 0), vec![2]);

    state.last_play = vec![1, 14];
    assert!(choose_play(&state, 0).is_empty());

    state.last_play = vec![55];
    assert!(choose_play(&state, 0).is_empty());

    state.last_play = vec![53];
    assert!(choose_play(&state, 0).is_empty());

    state.hands.insert(0, Vec::new());
    state.last_play.clear();
    assert!(choose_play(&state, 0).is_empty());
}
