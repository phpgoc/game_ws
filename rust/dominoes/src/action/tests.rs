use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use share_type_public::{
    DominoesActionSource, DominoesNoPlayableTiles, DominoesPhase, DominoesPort, DominoesRule,
};
use ws_common::CommonGameState;

use super::{ActionEvent, forced_no_playable_turn};
use crate::core::{DEFAULT_TURN_SECONDS, DominoesRoundState, Endpoint, Tile};

fn blocked_state(rule: DominoesNoPlayableTiles) -> DominoesRoundState {
    let mut common = CommonGameState::new();
    for position in 0..3 {
        common.add_player(position, position as u64 + 1, &format!("P{position}"));
    }
    let mut state = DominoesRoundState::new_with_seed(
        Arc::new(Mutex::new(common)),
        DominoesRule::Simple,
        rule,
        35,
        7,
    )
    .expect("state");
    state.phase = DominoesPhase::Play;
    state.current_position = 0;
    state.remaining_seconds = 3;
    state.endpoints = vec![Endpoint {
        endpoint_id: 1,
        placement_id: 0,
        pip: 6,
        port: DominoesPort::Right,
        anchor_x: 2,
        anchor_y: 0,
        direction: DominoesPort::Right,
    }];
    state.hands = HashMap::from([
        (0, vec![Tile::from_id(0).expect("0-0")]),
        (1, vec![Tile::from_id(8).expect("1-1")]),
        (2, vec![Tile::from_id(15).expect("2-2")]),
    ]);
    state
}

#[test]
fn keep_drawing_is_forced_until_the_current_player_can_play() {
    let mut state = blocked_state(DominoesNoPlayableTiles::KeepDrawing);
    state.boneyard = vec![
        Tile::from_id(6).expect("0-6"),
        Tile::from_id(1).expect("0-1"),
    ];
    let previous_revision = state.turn_revision;

    let outcome = forced_no_playable_turn(&mut state)
        .expect("forced action")
        .expect("draw outcome");

    assert_eq!(state.current_position, 0);
    assert_eq!(state.hands[&0].len(), 3);
    assert!(!state.legal_plays(0).is_empty());
    assert_eq!(state.remaining_seconds, DEFAULT_TURN_SECONDS);
    assert_ne!(state.turn_revision, previous_revision);
    assert_eq!(
        outcome
            .events
            .iter()
            .filter(|event| matches!(event, ActionEvent::Draw { .. }))
            .count(),
        2
    );
    assert!(outcome.events.iter().all(|event| match event {
        ActionEvent::Draw { source, .. } | ActionEvent::Pass { source, .. } => {
            *source == DominoesActionSource::Forced
        }
        ActionEvent::Play { .. } => false,
    }));
}

#[test]
fn draw_one_is_forced_once_and_then_passes_when_still_blocked() {
    let mut state = blocked_state(DominoesNoPlayableTiles::DrawOne);
    state.boneyard = vec![Tile::from_id(1).expect("0-1")];

    let outcome = forced_no_playable_turn(&mut state)
        .expect("forced action")
        .expect("draw outcome");

    assert_eq!(state.current_position, 1);
    assert!(matches!(
        outcome.events.as_slice(),
        [ActionEvent::Draw { .. }, ActionEvent::Pass { .. }]
    ));
}

#[test]
fn pass_without_draw_is_forced_without_touching_the_boneyard() {
    let mut state = blocked_state(DominoesNoPlayableTiles::PassWithoutDraw);
    state.boneyard = vec![Tile::from_id(6).expect("0-6")];

    let outcome = forced_no_playable_turn(&mut state)
        .expect("forced action")
        .expect("pass outcome");

    assert_eq!(state.current_position, 1);
    assert_eq!(state.boneyard.len(), 1);
    assert!(matches!(
        outcome.events.as_slice(),
        [ActionEvent::Pass {
            source: DominoesActionSource::Forced,
            ..
        }]
    ));
}

#[test]
fn an_empty_boneyard_passes_without_emitting_a_fake_draw() {
    let mut state = blocked_state(DominoesNoPlayableTiles::KeepDrawing);
    state.boneyard.clear();

    let outcome = forced_no_playable_turn(&mut state)
        .expect("forced action")
        .expect("pass outcome");

    assert_eq!(state.current_position, 1);
    assert!(matches!(
        outcome.events.as_slice(),
        [ActionEvent::Pass {
            source: DominoesActionSource::Forced,
            ..
        }]
    ));
}
