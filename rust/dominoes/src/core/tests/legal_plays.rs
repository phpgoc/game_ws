use std::{hint::black_box, time::Instant};

use share_type_public::WsDominoesLegalPlay;

use super::*;

// 保留逐候选重建桌面的旧路径，独立核对优化后的完整候选集合、分数与顺序。
fn reference_legal_plays(state: &DominoesRoundState, position: usize) -> Vec<WsDominoesLegalPlay> {
    let Some(hand) = state.hands.get(&position) else {
        return Vec::new();
    };
    let mut plays = Vec::new();
    for tile in hand {
        if state.endpoints.is_empty() {
            plays.push(WsDominoesLegalPlay {
                tile_id: tile.id,
                endpoint_id: None,
                score: state.preview_five_up_score(*tile, None),
            });
        } else {
            for endpoint in &state.endpoints {
                if tile.matches(endpoint.pip) {
                    plays.push(WsDominoesLegalPlay {
                        tile_id: tile.id,
                        endpoint_id: Some(endpoint.endpoint_id),
                        score: state.preview_five_up_score(*tile, Some(endpoint.endpoint_id)),
                    });
                }
            }
        }
    }
    plays.sort_unstable_by_key(|play| (play.tile_id, play.endpoint_id.unwrap_or(-1)));
    plays
}

fn visit_seeded_states(mut visit: impl FnMut(&DominoesRoundState)) {
    for players in [3, 4] {
        for rule in [DominoesRule::Simple, DominoesRule::FiveUp] {
            for draw in [
                DominoesNoPlayableTiles::PassWithoutDraw,
                DominoesNoPlayableTiles::DrawOne,
                DominoesNoPlayableTiles::KeepDrawing,
            ] {
                for seed in 1..=16 {
                    let mut state = state_with_players(players);
                    state.rule = rule;
                    state.no_playable_tiles = draw;
                    state.rng_state = seed;
                    state.start_new_game().unwrap();
                    for turn in 0..256 {
                        visit(&state);
                        if state.phase != DominoesPhase::Play {
                            break;
                        }
                        let position = state.current_position;
                        let legal = reference_legal_plays(&state, position);
                        if !legal.is_empty() {
                            let play = legal[(seed as usize + turn) % legal.len()];
                            state
                                .play_tile(position, play.tile_id, play.endpoint_id)
                                .unwrap();
                        } else if state.hand_state(position).can_draw {
                            state.draw_tile(position).unwrap();
                        } else {
                            state.pass(position).unwrap();
                        }
                    }
                    assert_ne!(state.phase, DominoesPhase::Play, "round must finish");
                }
            }
        }
    }
}

#[test]
fn legal_candidates_keep_the_same_scores_and_order_for_every_seat() {
    let mut checked = 0;
    let mut reserve_states = 0;
    visit_seeded_states(|state| {
        for &position in &state.positions {
            assert_eq!(
                state.legal_plays(position),
                reference_legal_plays(state, position)
            );
            checked += 1;
        }
        assert!(state.legal_plays(usize::MAX).is_empty());
        reserve_states += usize::from(state.boneyard.len() == 2);
    });
    assert!(checked > 10_000);
    assert!(reserve_states > 0);
    eprintln!("legal candidates checked for {checked} seats; reserve states={reserve_states}");
}

#[test]
#[ignore = "手动 release 比较完整候选枚举耗时；不替代官方 AI 入口门禁"]
fn release_legal_candidate_enumeration_timing() {
    let mut states = Vec::new();
    visit_seeded_states(|state| {
        if state.rule == DominoesRule::FiveUp && state.placements.len() >= 8 {
            states.push(state.clone());
        }
    });
    assert!(!states.is_empty());
    for state in &states {
        assert_eq!(
            state.legal_plays(state.current_position),
            reference_legal_plays(state, state.current_position)
        );
    }
    let mut reference_samples = Vec::new();
    let mut reused_samples = Vec::new();
    let repetitions = 32;
    for run in 0..12 {
        // 交替顺序，计时只含枚举；发牌、局面复制和核对在计时外。
        for reference in if run % 2 == 0 {
            [true, false]
        } else {
            [false, true]
        } {
            let started = Instant::now();
            for _ in 0..repetitions {
                for state in &states {
                    if reference {
                        black_box(reference_legal_plays(
                            black_box(state),
                            state.current_position,
                        ));
                    } else {
                        black_box(state.legal_plays(black_box(state.current_position)));
                    }
                }
            }
            let per_call_ns = started.elapsed().as_secs_f64() * 1_000_000_000.0
                / (states.len() * repetitions) as f64;
            if reference {
                reference_samples.push(per_call_ns);
            } else {
                reused_samples.push(per_call_ns);
            }
        }
    }
    eprintln!(
        "legal candidate timing {}",
        serde_json::json!({
            "states": states.len(), "repetitions": repetitions, "reference_ns_per_call": reference_samples, "reused_ns_per_call": reused_samples,
        })
    );
}
