use super::*;
use crate::core::CoreError;

fn tile(a: i32, b: i32) -> Tile {
    Tile::all()
        .into_iter()
        .find(|tile| (tile.a, tile.b) == (a, b))
        .unwrap()
}

fn opening() -> DominoesRoundState {
    let mut state = state_with_players(3);
    state.phase = DominoesPhase::Play;
    state.current_position = 0;
    state.no_playable_tiles = DominoesNoPlayableTiles::KeepDrawing;
    state.hands = HashMap::from([
        (0, vec![tile(2, 3), tile(0, 1)]),
        (1, vec![tile(3, 5), tile(0, 0)]),
        (2, vec![tile(2, 2), tile(6, 6)]),
    ]);
    state.boneyard = Tile::all()
        .into_iter()
        .filter(|tile| !state.hands.values().any(|hand| hand.contains(tile)))
        .filter(|item| *item != tile(1, 5))
        .chain([tile(1, 5)])
        .collect();
    state
}

fn play(state: &mut DominoesRoundState, position: usize, a: i32, b: i32, pip: i32) {
    let endpoint = state.endpoints.iter().find(|end| end.pip == pip).unwrap();
    state
        .play_tile(position, tile(a, b).id, Some(endpoint.endpoint_id))
        .unwrap();
}

fn one_rotation() -> DominoesRoundState {
    let mut state = opening();
    state.play_tile(0, tile(2, 3).id, None).unwrap();
    play(&mut state, 1, 3, 5, 3);
    play(&mut state, 2, 2, 2, 2);
    assert_eq!(state.current_position, 0);
    assert!(state.legal_plays(0).is_empty());
    state
}

#[test]
fn evidence_keeps_pre_play_public_context_and_survives_rejected_actions() {
    let mut state = opening();
    state.rule = DominoesRule::FiveUp;
    let boneyard_count = state.boneyard.len();
    state.play_tile(0, tile(2, 3).id, None).unwrap();
    let evidence = state.last_public_play(0).unwrap().clone();
    assert_eq!(evidence.tile, tile(2, 3));
    assert_eq!(evidence.immediate_score, 5);
    assert_eq!(evidence.remaining_tiles, 1);
    assert!(evidence.open_endpoints.is_empty());
    assert_eq!(evidence.context.current_position, 0);
    assert_eq!(
        evidence.context.hand_sizes,
        [(0, Some(2)), (1, Some(2)), (2, Some(2))]
    );
    assert!(evidence.context.played_tile_ids.is_empty());
    assert_eq!(evidence.context.scores, [(0, 0), (1, 0), (2, 0)]);
    assert_eq!(evidence.context.boneyard_count, boneyard_count);
    assert_eq!(state.scores[&0], 5);
    assert_eq!(state.draw_tile(0), Err(CoreError::NotYourTurn));
    assert_eq!(state.draw_tile(1), Err(CoreError::PlayAvailable));
    assert_eq!(state.pass(1), Err(CoreError::PlayAvailable));
    assert_eq!(
        state.play_tile(1, tile(3, 5).id, None),
        Err(CoreError::InvalidEndpoint)
    );
    assert_eq!(state.last_public_play(0), Some(&evidence));
    assert!(state.last_public_play(1).is_none());
}

#[test]
fn actual_draw_invalidates_only_drawers_evidence_and_clones_keep_independent_state() {
    let state = one_rotation();
    let mut sampled = state.clone();
    assert!(std::ptr::eq(
        state.last_public_play(0).unwrap(),
        sampled.last_public_play(0).unwrap(),
    ));
    assert_eq!(sampled.pass(0), Err(CoreError::PassNotAllowed));
    assert!(sampled.last_public_play(0).is_some());
    let drawn = sampled.draw_tile(0).unwrap();
    assert_eq!(drawn.tile, Some(tile(1, 5)));
    assert!(drawn.playable);
    assert!(sampled.last_public_play(0).is_none());
    assert!(sampled.last_public_play(1).is_some());
    assert!(sampled.last_public_play(2).is_some());
    assert!(state.last_public_play(0).is_some());
    assert_eq!(state.hands[&0], [tile(0, 1)]);
    assert_eq!(sampled.hands[&0].len(), 2);
    assert_eq!(sampled.boneyard.len() + 1, state.boneyard.len());
}

#[test]
fn undrawable_reserve_preserves_evidence_and_new_round_or_game_clears_all_public_history() {
    let mut state = one_rotation();
    state.boneyard = vec![tile(2, 5), tile(2, 6)];
    for position in 0..3 {
        let drawn = state.draw_tile(position).unwrap();
        assert_eq!(drawn.tile, None);
        assert!(drawn.passed);
        assert_eq!(state.boneyard, [tile(2, 5), tile(2, 6)]);
        assert!(state.last_public_play(position).is_some());
        assert_eq!(state.inferred_missing_pips(position), (1 << 2) | (1 << 5));
    }
    assert_eq!(state.phase, DominoesPhase::RoundOver);
    let mut new_game = state.clone();
    state.start_next_round().unwrap();
    new_game.start_new_game().unwrap();
    for fresh in [state, new_game] {
        for position in 0..3 {
            assert!(fresh.last_public_play(position).is_none());
            assert_eq!(fresh.inferred_missing_pips(position), 0);
        }
    }
}
