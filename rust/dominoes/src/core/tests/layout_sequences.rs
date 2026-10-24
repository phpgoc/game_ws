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

#[test]
fn seeded_matches_preserve_scores_across_rounds_until_the_highest_score_wins() {
    let mut transitions = 0;
    for count in [3, 4] {
        for rule in [DominoesRule::Simple, DominoesRule::FiveUp] {
            for draw_rule in [
                DominoesNoPlayableTiles::KeepDrawing,
                DominoesNoPlayableTiles::DrawOne,
                DominoesNoPlayableTiles::PassWithoutDraw,
            ] {
                for seed in 1..=8 {
                    let mut state = state_with_players(count);
                    state.rule = rule;
                    state.no_playable_tiles = draw_rule;
                    state.target_score = 61;
                    state.rng_state = seed;
                    state.start_new_game().expect("start match");
                    let mut scores = state.scores.clone();
                    for round in 1..=128 {
                        assert_eq!(state.round, round);
                        play_seeded_round(&mut state, seed * 7919 + round as u64);
                        let result = state.last_round_result.as_ref().expect("round result");
                        for position in &state.positions {
                            assert_eq!(
                                result.scores[position] - scores[position],
                                result.score_changes[position]
                            );
                            assert!(result.score_changes[position] >= 0);
                        }
                        scores = result.scores.clone();
                        if result.game_over {
                            let best = *scores.values().max().unwrap();
                            assert!(best >= 61);
                            let mut winners = scores
                                .iter()
                                .filter_map(|(p, score)| (*score == best).then_some(*p))
                                .collect::<Vec<_>>();
                            winners.sort_unstable();
                            assert_eq!(result.winner_positions, winners);
                            break;
                        }
                        let winner = result.winner_position;
                        assert!(
                            round < 128,
                            "match did not terminate: {count} {rule:?} {draw_rule:?} seed {seed}"
                        );
                        state.start_next_round().expect("start next round");
                        transitions += 1;
                        assert_eq!(state.current_position, winner);
                        assert_eq!(state.scores, scores);
                        assert!(state.last_round_result.is_none());
                        assert!(state.placements.is_empty());
                        assert!(state.endpoints.is_empty());
                        assert!(state.hands.values().all(|hand| hand.len() == 5));
                        assert_eq!(state.boneyard.len(), 28 - count * 5);
                    }
                    assert_eq!(state.phase, share_type_public::DominoesPhase::GameOver);
                }
            }
        }
    }
    assert!(
        transitions > 96,
        "expected multiple rounds across the match matrix"
    );
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
    if state.rule == DominoesRule::Simple {
        assert_eq!(
            state.endpoints.len(),
            if state.placements.is_empty() { 0 } else { 2 }
        );
    }
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
