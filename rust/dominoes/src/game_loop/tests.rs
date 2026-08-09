use std::sync::{Arc, Mutex};

use share_type_public::{DominoesActionSource, DominoesNoPlayableTiles, DominoesRule};
use ws_common::CommonGameState;

use super::*;

fn state() -> DominoesRoundState {
    let mut common = CommonGameState::new();
    for position in 0..3 {
        common.add_player(position, position as u64 + 1, &format!("P{position}"));
    }
    let mut state = DominoesRoundState::new_with_seed(
        Arc::new(Mutex::new(common)),
        DominoesRule::Simple,
        DominoesNoPlayableTiles::PassWithoutDraw,
        35,
        11,
    )
    .expect("state");
    state.start_new_game().expect("start");
    state
}

#[test]
fn disconnected_turn_is_capped_without_becoming_native_ai() {
    let mut state = state();
    state.remaining_seconds = 30;
    state
        .base
        .lock()
        .expect("common")
        .mark_disconnected(state.current_position);
    let (source, cap) = automatic_source_and_cap(&state);
    assert_eq!(source, None);
    assert_eq!(cap, DISCONNECTED_TURN_SECONDS);
}

#[test]
fn native_ai_and_takeover_have_distinct_action_sources() {
    let native = state();
    native
        .base
        .lock()
        .expect("common")
        .mark_ai_position(native.current_position);
    assert_eq!(
        automatic_source_and_cap(&native),
        (Some(DominoesActionSource::NativeAi), AI_TURN_SECONDS)
    );

    let takeover = state();
    takeover
        .base
        .lock()
        .expect("common")
        .mark_ai_takeover_position(takeover.current_position);
    assert_eq!(
        automatic_source_and_cap(&takeover),
        (Some(DominoesActionSource::AiTakeover), AI_TURN_SECONDS)
    );
}
