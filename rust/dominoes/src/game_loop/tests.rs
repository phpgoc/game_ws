use std::sync::{Arc, Mutex};

use share_type_public::{DominoesActionSource, DominoesNoPlayableTiles, DominoesRule};
use ws_common::CommonGameState;

use super::*;

#[path = "tests/ordering.rs"]
mod ordering;

fn step(state: &mut DominoesRoundState, tick: Option<i32>) -> LoopStep {
    advance_loop(
        state,
        &RoomService::default(),
        "test",
        tick,
        &mut Dispatch::default(),
    )
}

#[test]
fn pause_freezes_countdown_and_automatic_actions() {
    let mut state = state();
    state.remaining_seconds = 0;
    let revision = state.turn_revision;
    state.base.lock().unwrap().paused = true;
    assert_eq!(step(&mut state, Some(revision)), LoopStep::Poll);
    assert!(state.placements.is_empty());
    assert_eq!(state.remaining_seconds, 0);
    state.base.lock().unwrap().paused = false;
    assert_eq!(step(&mut state, None), LoopStep::Again);
    assert_eq!(state.placements.len(), 1);
}

#[test]
fn old_countdown_does_not_consume_time_from_a_new_turn() {
    let initial = state();
    let revision = initial.turn_revision;
    let mut state = initial
        .legal_plays(initial.current_position)
        .into_iter()
        .find_map(|play| {
            let mut next = initial.clone();
            next.play_tile(next.current_position, play.tile_id, play.endpoint_id)
                .unwrap();
            (!next.legal_plays(next.current_position).is_empty()).then_some(next)
        })
        .expect("真实发牌中下家有牌可出的开局");
    let new_revision = state.turn_revision;
    assert_ne!(revision, new_revision);
    assert_eq!(
        step(&mut state, Some(revision)),
        LoopStep::Countdown(new_revision)
    );
    assert_eq!(state.remaining_seconds, 30);
    assert_eq!(
        step(&mut state, Some(new_revision)),
        LoopStep::Countdown(new_revision)
    );
    assert_eq!(state.remaining_seconds, 29);
}

#[test]
fn ai_waits_one_tick_then_uses_the_same_action_pipeline() {
    let mut state = state();
    state
        .base
        .lock()
        .unwrap()
        .mark_ai_position(state.current_position);
    let revision = state.turn_revision;
    assert_eq!(step(&mut state, None), LoopStep::Countdown(revision));
    assert_eq!(state.remaining_seconds, 1);
    assert!(state.placements.is_empty());
    assert_eq!(step(&mut state, Some(revision)), LoopStep::Again);
    assert_eq!(state.placements.len(), 1);
}

#[test]
fn a_forced_playable_draw_restores_the_full_human_turn() {
    let mut state = (1..=64)
        .find_map(|seed| {
            let initial = state_with_seed(seed);
            initial
                .legal_plays(initial.current_position)
                .into_iter()
                .find_map(|play| {
                    let mut next = initial.clone();
                    next.play_tile(next.current_position, play.tile_id, None)
                        .unwrap();
                    (next.legal_plays(next.current_position).is_empty()
                        && next
                            .boneyard
                            .iter()
                            .any(|tile| next.endpoints.iter().any(|end| tile.matches(end.pip))))
                    .then_some(next)
                })
        })
        .expect("真实发牌中下家须摸牌且牌库有可接牌的开局");
    state.no_playable_tiles = DominoesNoPlayableTiles::KeepDrawing;
    state.remaining_seconds = 1;
    let position = state.current_position;
    let original_hand = state.hands[&position].clone();
    let original_boneyard_count = state.boneyard.len();
    let revision = state.turn_revision;
    assert_eq!(step(&mut state, Some(revision)), LoopStep::Again);
    assert_eq!(state.current_position, position);
    assert!(
        original_hand
            .iter()
            .all(|tile| state.hands[&position].contains(tile))
    );
    assert!(state.hands[&position].len() > original_hand.len());
    assert_eq!(
        state.hands[&position].len() - original_hand.len(),
        original_boneyard_count - state.boneyard.len()
    );
    assert!(!state.legal_plays(position).is_empty());
    assert_eq!(state.remaining_seconds, 30);
    assert_eq!(state.placements.len(), 1);
}

fn state() -> DominoesRoundState {
    state_with_seed(11)
}

fn state_with_seed(seed: u64) -> DominoesRoundState {
    let mut common = CommonGameState::new();
    for position in 0..3 {
        common.add_player(position, position as u64 + 1, &format!("P{position}"));
    }
    let mut state = DominoesRoundState::new_with_seed(
        Arc::new(Mutex::new(common)),
        DominoesRule::Simple,
        DominoesNoPlayableTiles::PassWithoutDraw,
        35,
        seed,
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
fn disconnect_keeps_the_loop_but_quit_stops_it() {
    let state = state();
    state.base.lock().expect("common").mark_disconnected(0);
    assert!(
        !state_should_stop(&state),
        "a transport disconnect keeps the roster and event loop alive"
    );

    {
        let mut common = state.base.lock().expect("common");
        common.remove_player(0);
        common.request_stop();
    }
    assert!(
        state_should_stop(&state),
        "QUIT removes the seat and requests the event loop to stop"
    );
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

#[test]
fn game_over_state_stops_the_background_loop() {
    let mut state = state();
    state.phase = DominoesPhase::GameOver;
    assert!(state_should_stop(&state));
}

#[test]
fn game_over_state_stays_registered_for_terminal_rejoin() {
    let mut state = state();
    state.phase = DominoesPhase::GameOver;
    let state = Arc::new(Mutex::new(state));
    let registry = Arc::new(Mutex::new(std::collections::HashMap::from([(
        "terminal-room".to_owned(),
        Arc::clone(&state),
    )])));

    remove_registry_state("terminal-room", &state, &registry);

    assert!(
        registry
            .lock()
            .expect("registry")
            .contains_key("terminal-room")
    );
}
