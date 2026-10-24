use std::collections::HashSet;

use share_type_public::{DominoesNoPlayableTiles, DominoesRule};

use super::super::{DominoesRoundState, TILE_COUNT, Tile, half_extents};
use super::state_with_players;

#[test]
fn seeded_real_rounds_preserve_tile_and_layout_invariants() {
    let draw_rules = [
        DominoesNoPlayableTiles::KeepDrawing,
        DominoesNoPlayableTiles::DrawOne,
        DominoesNoPlayableTiles::PassWithoutDraw,
    ];

    for player_count in [3, 4] {
        for game_rule in [DominoesRule::Simple, DominoesRule::FiveUp] {
            for draw_rule in draw_rules {
                for seed in 1..=48 {
                    let mut state = state_with_players(player_count);
                    state.rule = game_rule;
                    state.no_playable_tiles = draw_rule;
                    state.rng_state = seed;
                    state.start_new_game().expect("start seeded round");

                    play_seeded_round(&mut state, seed.wrapping_mul(7919));
                    assert_ne!(state.phase, share_type_public::DominoesPhase::Play);
                }
            }
        }
    }
}

fn play_seeded_round(state: &mut DominoesRoundState, mut choice_seed: u64) {
    for _ in 0..256 {
        assert_round_conservation_and_layout(state);
        if state.phase != share_type_public::DominoesPhase::Play {
            return;
        }

        let position = state.current_position;
        let legal_plays = state.legal_plays(position);
        if !legal_plays.is_empty() {
            let choice = next_random(&mut choice_seed) as usize % legal_plays.len();
            let selected = legal_plays[choice];
            let (_, _, result) = state
                .play_tile(position, selected.tile_id, selected.endpoint_id)
                .expect("selected play is legal");
            assert_round_conservation_and_layout(state);
            if result.is_some() {
                return;
            }
            continue;
        }

        let hand = state.hand_state(position);
        let result = if hand.can_draw {
            state
                .draw_tile(position)
                .expect("draw is allowed")
                .round_result
        } else if hand.can_pass {
            state.pass(position).expect("pass is allowed")
        } else {
            panic!("current player has no legal action: {hand:?}");
        };
        assert_round_conservation_and_layout(state);
        if result.is_some() {
            return;
        }
    }

    panic!("seeded round exceeded its bounded action count");
}

fn assert_round_conservation_and_layout(state: &DominoesRoundState) {
    let mut tile_ids = HashSet::with_capacity(TILE_COUNT);
    let mut remember = |tile: Tile| {
        assert!(tile_ids.insert(tile.id), "tile {} appears twice", tile.id);
    };

    for placement in &state.placements {
        remember(placement.tile);
    }
    for hand in state.hands.values() {
        for tile in hand {
            remember(*tile);
        }
    }
    for tile in &state.boneyard {
        remember(*tile);
    }
    assert_eq!(tile_ids.len(), TILE_COUNT);

    for (index, placement) in state.placements.iter().enumerate() {
        let (width, height) = half_extents(placement.orientation);
        for other in state.placements.iter().skip(index + 1) {
            let (other_width, other_height) = half_extents(other.orientation);
            assert!(
                !((placement.center_x - other.center_x).abs() < width + other_width
                    && (placement.center_y - other.center_y).abs() < height + other_height),
                "{:?}: placements {} and {} overlap",
                state.rule,
                placement.placement_id,
                other.placement_id,
            );
        }
    }

}

fn next_random(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}
