use super::*;

#[test]
fn next_round_starter_rotates_after_both_empty_hand_and_blocked_rounds() {
    // Switch 实机 sm42780597：01:55/02:46（人→绿）、05:40/06:27（红→蓝），
    // 以及 Simple 的 07:43/08:17（红→蓝）；先手轮换不由上轮赢家决定。
    for positions in [vec![0, 2, 3], vec![0, 1, 2, 3]] {
        for rule in [DominoesRule::Simple, DominoesRule::FiveUp] {
            for blocked in [false, true] {
                for first_index in 0..positions.len() {
                    let mut common = CommonGameState::new();
                    for &position in &positions {
                        common.add_player(position, position as u64 + 1, &format!("P{position}"));
                    }
                    let mut state = DominoesRoundState::new_with_seed(
                        Arc::new(Mutex::new(common)), rule,
                        DominoesNoPlayableTiles::PassWithoutDraw, 61, 1,
                    ).unwrap();
                    state.start_new_game_pending().unwrap();
                    state.starter_pool = vec![tile(0, 1), tile(0, 2), tile(0, 3), tile(0, 4)];
                    state.starter_pool[first_index] = tile(6, 6);
                    for (slot, &position) in positions.iter().enumerate() {
                        state.select_starter_slot(position, slot).unwrap();
                    }

                    // 走过一整圈并越过空座；结算时 current_position 已经不再是先手。
                    let mut round_starter = positions[first_index];
                    for offset in 0..3 {
                        assert_eq!(state.current_position, round_starter);
                        let starter_index = positions.iter().position(|p| *p == round_starter).unwrap();
                        let expected_next = positions[(starter_index + 1) % positions.len()];
                        let winner = positions[(starter_index + 2) % positions.len()];
                        assert_ne!(winner, expected_next);
                        state.current_position = winner;
                        state.hands = positions.iter().enumerate().map(|(index, &position)| {
                            let mut hand = vec![tile(index as i32 + 1, index as i32 + 1)];
                            if position == winner {
                                if !blocked { hand.clear(); }
                                hand.push(tile(0, 0));
                            }
                            (position, hand)
                        }).collect();
                        let (_, _, mut result) = state.play_tile(winner, tile(0, 0).id, None).unwrap();
                        if blocked {
                            assert!(result.is_none());
                            for _ in &positions {
                                result = state.pass(state.current_position).unwrap();
                            }
                        }
                        let result = result.expect("the round must settle");
                        assert_eq!(result.winner_position, winner);
                        assert_eq!(result.blocked, blocked);
                        assert!(!result.game_over);
                        assert_eq!(state.start_next_round().unwrap(), expected_next);
                        round_starter = expected_next;
                        assert_eq!(state.round, offset as i32 + 2);
                        assert!(state.placements.is_empty());
                        assert!(state.hands.values().all(|hand| hand.len() == 5));
                    }
                }
            }
        }
    }
}
