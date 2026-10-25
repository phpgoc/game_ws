use super::*;
use rand::{SeedableRng, rngs::StdRng};
#[cfg(feature = "official")]
mod ai;
fn room(decks: i32) -> Room {
    Room::new(
        "test".into(),
        std::array::from_fn(|p| format!("p{p}")),
        decks,
        &mut StdRng::seed_from_u64(7),
    )
}
fn fixture(hands: [Vec<i32>; 4]) -> Room {
    let mut r = room(2);
    r.level = 9;
    for (p, cards) in hands.into_iter().enumerate() {
        r.players[p].hand = cards;
    }
    r
}
fn play(r: &mut Room, p: usize, cards: &[i32]) -> bool {
    r.apply(
        p,
        NiangniangAction::Play {
            cards: cards.to_vec(),
        },
    )
}
#[test]
fn deals_all_cards_with_unique_physical_ids_and_private_views() {
    for decks in 1..=2 {
        let r = room(decks);
        let cards = r
            .players
            .iter()
            .flat_map(|p| p.hand.iter().copied())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(cards, (1..=54 * decks).collect());
        let mut counts: Vec<_> = r.players.iter().map(|p| p.hand.len()).collect();
        counts.sort();
        assert_eq!(
            counts,
            if decks == 1 {
                vec![13, 13, 14, 14]
            } else {
                vec![27; 4]
            }
        );
        for p in 0..4 {
            let state = r.state_for(p);
            assert_eq!(state.hand, r.players[p].hand);
            let json = serde_json::to_value(state).unwrap();
            assert!(
                json["players"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|v| v.get("hand").is_none())
            );
        }
    }
}
#[test]
fn equal_count_single_rank_no_bombs_and_invalid_moves_are_atomic() {
    let mut r = fixture([
        vec![1, 14, 27, 40, 55, 68, 2],
        vec![13, 26, 39, 52, 67, 3],
        vec![4],
        vec![5],
    ]);
    assert!(!play(&mut r, 1, &[13]));
    assert!(!play(&mut r, 0, &[1, 2]));
    assert!(!play(&mut r, 0, &[1, 1]));
    assert!(!play(&mut r, 0, &[109]));
    assert!(!r.apply(0, NiangniangAction::Pass));
    assert_eq!(r.players[0].hand.len(), 7);
    assert!(play(&mut r, 0, &[1, 14, 27, 40, 55, 68]));
    let before = r.players[1].hand.clone();
    assert!(!play(&mut r, 1, &[13, 26, 39, 52, 67])); // 五张 2 压不了六张 3
    assert_eq!(r.players[1].hand, before);
}
#[test]
fn level_below_jokers_above_two_and_same_rank_never_beats_itself() {
    assert!(strength(1, 3) > strength(13, 3));
    assert!(strength(53, 3) > strength(1, 3));
    assert!(strength(54, 3) > strength(53, 3));
    assert_eq!(strength(55, 3), strength(1, 3));
    let mut r = fixture([vec![1, 2], vec![14, 3], vec![4], vec![5]]);
    assert!(play(&mut r, 0, &[1]));
    assert!(!play(&mut r, 1, &[14]));
}
#[test]
fn finished_leader_partner_takes_over_after_all_pass_and_finished_seats_are_skipped() {
    let mut r = fixture([vec![1], vec![2, 3], vec![4, 5], vec![6, 7]]);
    assert!(play(&mut r, 0, &[1]));
    for p in 1..=3 {
        assert!(r.apply(p, NiangniangAction::Pass));
    }
    assert_eq!(r.current, 2);
    assert!(r.last.is_none());
    assert!(play(&mut r, 2, &[4]));
    assert!(r.apply(3, NiangniangAction::Pass));
    assert_eq!(r.current, 1);
}
#[test]
fn beating_a_finished_player_keeps_the_new_winners_lead() {
    let mut r = fixture([vec![1], vec![2, 3], vec![4, 5], vec![6, 7]]);
    assert!(play(&mut r, 0, &[1]));
    assert!(play(&mut r, 1, &[2]));
    for p in [2, 3] {
        assert!(r.apply(p, NiangniangAction::Pass));
    }
    assert_eq!(r.current, 1);
    assert!(r.last.is_none());
}
#[test]
fn all_placements_dealer_changes_levels_and_tributes() {
    for dealer in 0..4 {
        for dealer_won in [true, false] {
            for (place, count) in [(2, 2), (3, 1), (4, 0)] {
                let mut r = room(1);
                r.dealer = dealer;
                r.levels = [6, 8];
                r.level = r.levels[dealer % 2];
                let first = if dealer_won { dealer } else { (dealer + 1) % 4 };
                let partner = (first + 2) % 4;
                let other = (first + 1) % 4;
                let other2 = (first + 3) % 4;
                r.finished = match place {
                    2 => vec![first, partner],
                    3 => vec![first, other, partner],
                    _ => vec![first, other, other2],
                };
                r.finish_round();
                let result = r.result.clone().unwrap();
                assert_eq!(result.advanced, if dealer_won { count } else { 0 });
                assert_eq!(result.tribute_count, count);
                assert_eq!(
                    result.next_dealer_position,
                    ((dealer + if dealer_won { 2 } else { 1 }) % 4) as i32
                );
                assert_eq!(
                    r.levels[first % 2],
                    if dealer_won {
                        (if first % 2 == 0 { 6 } else { 8 }) + count
                    } else {
                        if first % 2 == 0 { 6 } else { 8 }
                    }
                );
                let sizes: Vec<_> = r.players.iter().map(|p| p.hand.len()).collect();
                assert!(r.next_round(&mut StdRng::seed_from_u64(99)));
                assert_eq!(r.tributes.len(), count as usize);
                for t in &r.tributes {
                    assert_eq!((t.to_position + 3) % 4, t.from_position);
                    assert!(
                        r.players[t.to_position as usize]
                            .hand
                            .contains(&t.tribute_card)
                    );
                    assert!(
                        r.players[t.from_position as usize]
                            .hand
                            .contains(&t.return_card)
                    );
                    assert!(
                        r.players[t.to_position as usize]
                            .hand
                            .iter()
                            .filter(|c| **c != t.tribute_card)
                            .all(|c| strength(*c, r.level) >= strength(t.return_card, r.level))
                    );
                    assert!(
                        r.players[t.from_position as usize]
                            .hand
                            .iter()
                            .filter(|c| **c != t.return_card)
                            .all(|c| strength(*c, r.level) <= strength(t.tribute_card, r.level))
                    );
                }
                let cards: std::collections::BTreeSet<_> =
                    r.players.iter().flat_map(|p| p.hand.clone()).collect();
                assert_eq!(cards, (1..=54).collect());
                assert_eq!(
                    r.players.iter().map(|p| p.hand.len()).sum::<usize>(),
                    sizes.iter().sum::<usize>()
                );
            }
        }
    }
}
#[test]
fn must_play_and_win_at_ace_and_non_dealer_can_never_clear_ace() {
    let mut r = room(1);
    r.level = 13;
    r.levels = [13, 14];
    r.finished = vec![0, 2];
    r.finish_round();
    assert_eq!(r.levels, [14, 14]);
    assert_eq!(r.phase, NiangniangPhase::ROUND_OVER);
    r.level = 14;
    r.finished = vec![0, 1, 3];
    r.finish_round();
    assert_eq!(r.phase, NiangniangPhase::ROUND_OVER);
    assert_eq!(r.winner, None);
    r.finished = vec![1, 3];
    r.finish_round();
    assert_eq!(r.phase, NiangniangPhase::ROUND_OVER);
    assert_eq!(r.levels, [14, 14]);
    r.finished = vec![0, 1, 2];
    r.finish_round();
    assert_eq!(r.phase, NiangniangPhase::MATCH_OVER);
    assert_eq!(r.winner, Some(0));
    assert!(!r.next_round(&mut StdRng::seed_from_u64(1)));
}
#[test]
fn normal_auto_always_covers_but_ai_yields_to_partner() {
    let mut r = fixture([vec![1, 3], vec![4, 5], vec![6, 7], vec![8, 9]]);
    assert!(play(&mut r, 0, &[1]));
    assert!(r.apply(1, NiangniangAction::Pass));
    assert!(matches!(r.choose(2, false), NiangniangAction::Play { .. }));
    assert_eq!(r.choose(2, true), NiangniangAction::Pass);
}
#[test]
fn complete_seeded_matches_terminate_without_losing_cards_or_illegal_actions() {
    for decks in 1..=2 {
        for seed in 0..6 {
            let mut rng = StdRng::seed_from_u64(seed);
            let mut r = Room::new(
                "sim".into(),
                std::array::from_fn(|p| format!("{p}")),
                decks,
                &mut rng,
            );
            let mut moves = 0;
            while r.phase != NiangniangPhase::MATCH_OVER && moves < 30000 {
                if r.phase == NiangniangPhase::ROUND_OVER {
                    assert!(r.next_round(&mut rng));
                } else {
                    let p = r.current;
                    let action = r.choose(p, seed % 2 == 0);
                    let before = r.players.iter().map(|p| p.hand.len()).sum::<usize>();
                    let played = match &action {
                        NiangniangAction::Play { cards } => cards.len(),
                        _ => 0,
                    };
                    assert!(r.apply_turn(p, action));
                    assert_eq!(
                        r.players.iter().map(|p| p.hand.len()).sum::<usize>(),
                        before - played
                    );
                }
                moves += 1;
            }
            assert_eq!(
                r.phase,
                NiangniangPhase::MATCH_OVER,
                "seed={seed}, decks={decks}"
            );
        }
    }
}
