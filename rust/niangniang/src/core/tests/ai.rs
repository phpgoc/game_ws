use super::*;
use crate::ai_input::PublicTurn;
use std::time::Instant;

// 2026-10-23 改进前的策略副本，不调用被评估的新智能入口。
fn legacy(r: &Room, p: usize) -> NiangniangAction {
    let mut groups = BTreeMap::<i32, Vec<i32>>::new();
    for card in &r.players[p].hand {
        groups
            .entry(strength(*card, r.level))
            .or_default()
            .push(*card);
    }
    if let Some((owner, previous)) = &r.last {
        if owner % 2 == p % 2 && !r.finished.contains(owner) {
            if let Some(cards) = groups.values().find(|g| {
                g.len() == r.players[p].hand.len()
                    && g.len() == previous.len()
                    && strength(g[0], r.level) > strength(previous[0], r.level)
            }) {
                return NiangniangAction::Play {
                    cards: cards.clone(),
                };
            }
            return NiangniangAction::Pass;
        }
        if let Some((_, cards)) = groups
            .iter()
            .filter(|(rank, g)| {
                **rank > strength(previous[0], r.level) && g.len() >= previous.len()
            })
            .min_by_key(|(rank, g)| (g.len() != previous.len(), **rank))
        {
            return NiangniangAction::Play {
                cards: cards[..previous.len()].to_vec(),
            };
        }
        NiangniangAction::Pass
    } else {
        NiangniangAction::Play {
            cards: groups
                .iter()
                .min_by_key(|(rank, g)| (-(g.len() as i32), **rank))
                .unwrap()
                .1
                .clone(),
        }
    }
}

fn high_cover(r: &Room, p: usize) -> NiangniangAction {
    let mut groups = BTreeMap::<i32, Vec<i32>>::new();
    for card in &r.players[p].hand {
        groups
            .entry(strength(*card, r.level))
            .or_default()
            .push(*card);
    }
    if let Some((_, previous)) = &r.last {
        if let Some((_, cards)) = groups.iter().rev().find(|(rank, cards)| {
            **rank > strength(previous[0], r.level) && cards.len() >= previous.len()
        }) {
            return NiangniangAction::Play {
                cards: cards[..previous.len()].to_vec(),
            };
        }
        NiangniangAction::Pass
    } else {
        NiangniangAction::Play {
            cards: groups.first_key_value().unwrap().1.clone(),
        }
    }
}

#[test]
fn public_ledger_is_atomic_survives_takeover_and_does_not_read_enemy_hands() {
    let mut r = room(2);
    for step in 0..100 {
        if r.phase != NiangniangPhase::PLAY {
            break;
        }
        let p = r.current;
        let observation = r.ai_observation(p);
        let before = crate::ai::choose(&observation).expect("valid public ledger at every turn");
        assert_eq!(before, r.choose(p, true));
        let a = (p + 1) % 4;
        let b = (p + 3) % 4;
        if !r.players[a].hand.is_empty() && !r.players[b].hand.is_empty() {
            let card = r.players[a].hand[0];
            r.players[a].hand[0] = r.players[b].hand[0];
            r.players[b].hand[0] = card;
            assert_eq!(
                before,
                r.choose(p, true),
                "hidden ownership cannot affect AI"
            );
            let card = r.players[a].hand[0];
            r.players[a].hand[0] = r.players[b].hand[0];
            r.players[b].hand[0] = card;
        }
        let history = r.history.clone();
        for viewer in 0..4 {
            let _ = r.state_for(viewer);
        }
        let card = r.players[p].hand[0];
        assert!(!r.apply_turn(
            p,
            NiangniangAction::Play {
                cards: vec![card, card]
            }
        ));
        assert_eq!(history, r.history);
        // 中途才开智能模式，仍使用开局就累计的公共记录。
        let action = if step < 20 { legacy(&r, p) } else { before };
        assert!(r.apply_turn(p, action.clone()));
        assert_eq!(r.history.len(), history.len() + 1);
        match action {
            NiangniangAction::Play { cards } => assert_eq!(
                r.history.last(),
                Some(&PublicTurn::Play { position: p, cards })
            ),
            NiangniangAction::Pass => {
                assert_eq!(r.history.last(), Some(&PublicTurn::Pass { position: p }))
            }
            _ => unreachable!(),
        }
    }
}

fn normalized(action: NiangniangAction) -> Option<(i32, usize)> {
    match action {
        NiangniangAction::Play { cards } => Some((rank(cards[0]), cards.len())),
        NiangniangAction::Pass => None,
        _ => unreachable!(),
    }
}

#[test]
fn public_strategy_is_invariant_to_seat_rotation_and_equivalent_suits() {
    let mut r = room(2);
    for _ in 0..100 {
        if r.phase != NiangniangPhase::PLAY {
            break;
        }
        let p = r.current;
        let expected = normalized(r.choose(p, true));
        for offset in 1..4 {
            let rotate = |p: i32| (p + offset) % 4;
            let mut state = r.state_for(p);
            state.self_position = rotate(state.self_position);
            state.current_position = rotate(state.current_position);
            state.dealer_position = rotate(state.dealer_position);
            if offset % 2 == 1 {
                state.team_levels.swap(0, 1);
            }
            for player in &mut state.players {
                player.position = rotate(player.position);
            }
            state.last_play_position = state.last_play_position.map(rotate);
            for position in &mut state.passed_positions {
                *position = rotate(*position);
            }
            for position in &mut state.finish_order {
                *position = rotate(*position);
            }
            for tribute in &mut state.tributes {
                tribute.from_position = rotate(tribute.from_position);
                tribute.to_position = rotate(tribute.to_position);
            }
            let history = r
                .history
                .iter()
                .map(|turn| match turn {
                    PublicTurn::Play { position, cards } => PublicTurn::Play {
                        position: rotate(*position as i32) as usize,
                        cards: cards.clone(),
                    },
                    PublicTurn::Pass { position } => PublicTurn::Pass {
                        position: rotate(*position as i32) as usize,
                    },
                })
                .collect::<Vec<_>>();
            assert_eq!(
                expected,
                normalized(
                    crate::ai::choose(&crate::ai_input::AiObservation {
                        state,
                        history: &history
                    })
                    .unwrap()
                )
            );
        }
        let suit = |card: i32| {
            let base = (card - 1) / 54 * 54;
            let index = (card - 1) % 54;
            if index >= 52 {
                card
            } else {
                base + (index + 13) % 52 + 1
            }
        };
        let mut state = r.state_for(p);
        for card in &mut state.hand {
            *card = suit(*card);
        }
        for card in &mut state.last_play_cards {
            *card = suit(*card);
        }
        for tribute in &mut state.tributes {
            tribute.tribute_card = suit(tribute.tribute_card);
            tribute.return_card = suit(tribute.return_card);
        }
        let history = r
            .history
            .iter()
            .map(|turn| match turn {
                PublicTurn::Play { position, cards } => PublicTurn::Play {
                    position: *position,
                    cards: cards.iter().copied().map(suit).collect(),
                },
                turn => turn.clone(),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            expected,
            normalized(
                crate::ai::choose(&crate::ai_input::AiObservation {
                    state,
                    history: &history
                })
                .unwrap()
            )
        );
        assert!(r.apply_turn(p, r.choose(p, true)));
    }
}

#[test]
fn automatic_dispatch_keeps_normal_covering_and_uses_intelligent_policy() {
    let mut r = room(2);
    for step in 0..100 {
        if r.phase != NiangniangPhase::PLAY {
            break;
        }
        let p = r.current;
        let intelligent = step % 2 == 1;
        let expected = if intelligent {
            crate::ai::choose(&r.ai_observation(p)).unwrap()
        } else {
            let action = r.choose(p, false);
            if let Some((_, previous)) = &r.last {
                let mut counts = BTreeMap::<i32, usize>::new();
                for card in &r.players[p].hand {
                    *counts.entry(strength(*card, r.level)).or_default() += 1;
                }
                let minimum = counts.iter().find(|(rank, count)| {
                    **rank > strength(previous[0], r.level) && **count >= previous.len()
                });
                match (&action, minimum) {
                    (NiangniangAction::Play { cards }, Some((minimum, _))) => {
                        assert_eq!(strength(cards[0], r.level), *minimum);
                        assert_eq!(cards.len(), previous.len());
                    }
                    (NiangniangAction::Pass, None) => (),
                    _ => panic!("normal automation must cover whenever legal"),
                }
            }
            action
        };
        r.players[p].away = true;
        r.players[p].intelligent = intelligent;
        assert!(r.automatic_step());
        let expected = match expected {
            NiangniangAction::Play { cards } => PublicTurn::Play { position: p, cards },
            NiangniangAction::Pass => PublicTurn::Pass { position: p },
            _ => unreachable!(),
        };
        assert_eq!(r.history.last(), Some(&expected));
        r.players[p].away = false;
        r.players[p].intelligent = false;
    }
}

#[test]
fn legal_prefix_uses_a_sufficient_level_single_before_spending_the_big_joker() {
    let seed = 10003;
    let mut rng = StdRng::seed_from_u64(seed);
    let mut r = Room::new(
        "conserve-control".into(),
        std::array::from_fn(|p| format!("p{p}")),
        1,
        &mut rng,
    );
    r.dealer = seed as usize % 4;
    r.levels = [3 + (seed % 12) as i32, 3 + (seed / 12 % 12) as i32];
    r.deal(&mut rng);
    for _ in 0..3 {
        let p = r.current;
        let action = if p.is_multiple_of(2) {
            legacy(&r, p)
        } else {
            r.choose(p, false)
        };
        assert!(r.apply_turn(p, action));
    }
    assert_eq!(r.current, 2);
    assert_eq!(normalized(r.choose(2, true)), Some((8, 1)));
}

#[test]
fn full_matches_keep_public_inventory_valid_through_tributes_and_reset() {
    for decks in 1..=2 {
        for seed in 800..812 {
            let mut rng = StdRng::seed_from_u64(seed);
            let mut r = Room::new(
                "full-ai".into(),
                std::array::from_fn(|p| format!("p{p}")),
                decks,
                &mut rng,
            );
            let mut actions = 0;
            let mut rounds_with_tribute = 0;
            while r.phase != NiangniangPhase::MATCH_OVER {
                assert!(actions < 30000, "match failed to terminate seed={seed}");
                if r.phase == NiangniangPhase::ROUND_OVER {
                    assert!(r.next_round(&mut rng));
                    assert!(r.history.is_empty());
                    rounds_with_tribute += usize::from(!r.tributes.is_empty());
                } else {
                    let p = r.current;
                    let action = crate::ai::choose(&r.ai_observation(p))
                        .expect("no invalid ledger fallback");
                    assert_eq!(action, r.choose(p, true));
                    assert!(r.apply_turn(p, action));
                    actions += 1;
                }
            }
            assert!(rounds_with_tribute > 0);
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Opponent {
    Basic,
    Legacy,
    High,
}

#[test]
#[ignore = "开发诊断：查看配对退步的首个分歧，只打印当事人可见信息"]
fn audit_first_changed_losing_prefixes() {
    let mut shown = 0;
    for seed in 10000..10128 {
        let (old, _) = round(seed, 1, 0, seed as usize % 4, Opponent::Basic, false, false);
        let (new, _) = round(seed, 1, 0, seed as usize % 4, Opponent::Basic, false, true);
        if old.winning_team != 0 || new.winning_team == 0 {
            continue;
        }
        let mut rng = StdRng::seed_from_u64(seed);
        let mut r = Room::new(
            "audit".into(),
            std::array::from_fn(|p| format!("p{p}")),
            1,
            &mut rng,
        );
        r.dealer = seed as usize % 4;
        r.levels = [3 + (seed % 12) as i32, 3 + (seed / 12 % 12) as i32];
        r.deal(&mut rng);
        for step in 0..1000 {
            if r.phase != NiangniangPhase::PLAY {
                break;
            }
            let p = r.current;
            let old = if p.is_multiple_of(2) {
                legacy(&r, p)
            } else {
                r.choose(p, false)
            };
            if p.is_multiple_of(2) {
                let new = r.choose(p, true);
                if old != new {
                    println!(
                        "seed={seed} step={step} seat={p} level={} hand={:?} counts={:?} last={:?} finished={:?} old={:?} new={:?}",
                        r.level,
                        r.players[p]
                            .hand
                            .iter()
                            .copied()
                            .map(rank)
                            .collect::<Vec<_>>(),
                        r.players.iter().map(|p| p.hand.len()).collect::<Vec<_>>(),
                        r.last.as_ref().map(|(p, c)| (p, rank(c[0]), c.len())),
                        r.finished,
                        normalized(old),
                        normalized(new)
                    );
                    break;
                }
            }
            assert!(r.apply_turn(p, old));
        }
        shown += 1;
        if shown == 8 {
            break;
        }
    }
}

fn round(
    seed: u64,
    decks: i32,
    team: usize,
    dealer: usize,
    opponent: Opponent,
    mixed: bool,
    improved: bool,
) -> (NiangniangRoundResult, Vec<u128>) {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut r = Room::new(
        "evaluation".into(),
        std::array::from_fn(|p| format!("p{p}")),
        decks,
        &mut rng,
    );
    r.dealer = dealer;
    r.levels = [3 + (seed % 12) as i32, 3 + (seed / 12 % 12) as i32];
    r.deal(&mut rng);
    let mut latency = Vec::new();
    for _ in 0..1000 {
        if r.phase != NiangniangPhase::PLAY {
            return (r.result.unwrap(), latency);
        }
        let p = r.current;
        let action = if p % 2 == team {
            if mixed && p == team + 2 {
                r.choose(p, false)
            } else if improved {
                let start = Instant::now();
                let action = crate::ai::choose(&r.ai_observation(p))
                    .expect("evaluation cannot use silent fallback");
                latency.push(start.elapsed().as_nanos());
                action
            } else {
                legacy(&r, p)
            }
        } else {
            match opponent {
                Opponent::Basic => r.choose(p, false),
                Opponent::Legacy => legacy(&r, p),
                Opponent::High => high_cover(&r, p),
            }
        };
        assert!(
            r.apply_turn(p, action),
            "illegal action seed={seed}, decks={decks}, improved={improved}"
        );
    }
    panic!("round failed to terminate seed={seed}");
}

#[test]
#[ignore = "release 下运行完整对照，开发与留出使用不同种子区间"]
fn evaluate_public_counting_strategy() {
    let first: u64 = std::env::var("NIANGNIANG_AI_FIRST_SEED")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(10000);
    let count: u64 = std::env::var("NIANGNIANG_AI_SEEDS")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(128);
    let mut rows = Vec::new();
    let mut latency = Vec::new();
    for decks in 1..=2 {
        for opponent in [Opponent::Basic, Opponent::Legacy, Opponent::High] {
            for mixed in [false, true] {
                let mut wins = [0; 2];
                let mut placements = [[0; 3]; 2];
                let mut flips = [0; 2];
                let mut upgrades = [0; 2];
                for seed in first..first + count {
                    for team in 0..2 {
                        // 固定发牌轮换主打人；同种子及座位属于配对样本。
                        let dealer = (seed as usize + team) % 4;
                        let (old, _) = round(seed, decks, team, dealer, opponent, mixed, false);
                        let (new, times) = round(seed, decks, team, dealer, opponent, mixed, true);
                        latency.extend(times);
                        let old_win = old.winning_team as usize == team;
                        let new_win = new.winning_team as usize == team;
                        flips[0] += usize::from(!old_win && new_win);
                        flips[1] += usize::from(old_win && !new_win);
                        for (version, result) in [old, new].iter().enumerate() {
                            if result.winning_team as usize == team {
                                wins[version] += 1;
                                placements[version][(2 - result.tribute_count) as usize] += 1;
                                upgrades[version] += result.advanced;
                            }
                        }
                    }
                }
                rows.push(serde_json::json!({"decks":decks,"opponent":format!("{opponent:?}"),"mixed_basic_partner":mixed,"rounds":count*2,"old_new_wins":wins,"old_new_12_13_14":placements,"old_new_upgrades":upgrades,"loss_to_win_win_to_loss":flips}));
            }
        }
    }
    latency.sort_unstable();
    let report = serde_json::json!({"first_seed":first,"independent_deal_seeds":count,"rows":rows,"decision_count":latency.len(),"mean_us":latency.iter().sum::<u128>() as f64 / latency.len() as f64 / 1000.0,"p95_us":latency[latency.len()*95/100] as f64/1000.0,"p99_us":latency[latency.len()*99/100] as f64/1000.0,"max_us":latency.last().unwrap().to_owned() as f64/1000.0});
    println!("NIANGNIANG_AI_REPORT={report}");
}

#[test]
#[ignore = "release 下验证含贡还牌、换主打及过 A 的整场对照"]
fn evaluate_public_counting_matches() {
    let first: u64 = std::env::var("NIANGNIANG_AI_FIRST_SEED")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(30000);
    let count: u64 = std::env::var("NIANGNIANG_AI_SEEDS")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(32);
    let mut rows = Vec::new();
    for decks in 1..=2 {
        for opponent in [Opponent::Basic, Opponent::Legacy] {
            for mixed in [false, true] {
                let mut wins = [0; 2];
                let mut flips = [0; 2];
                for seed in first..first + count {
                    for team in 0..2 {
                        let mut outcomes = [false; 2];
                        for (version, outcome) in outcomes.iter_mut().enumerate() {
                            let mut rng = StdRng::seed_from_u64(seed);
                            let mut r = Room::new(
                                "match-evaluation".into(),
                                std::array::from_fn(|p| format!("p{p}")),
                                decks,
                                &mut rng,
                            );
                            let mut actions = 0;
                            while r.phase != NiangniangPhase::MATCH_OVER {
                                assert!(actions < 30000, "nonterminating match seed={seed}");
                                if r.phase == NiangniangPhase::ROUND_OVER {
                                    assert!(r.next_round(&mut rng));
                                    continue;
                                }
                                let p = r.current;
                                let action = if p % 2 == team {
                                    if mixed && p == team + 2 {
                                        r.choose(p, false)
                                    } else if version == 1 {
                                        crate::ai::choose(&r.ai_observation(p)).unwrap()
                                    } else {
                                        legacy(&r, p)
                                    }
                                } else {
                                    match opponent {
                                        Opponent::Basic => r.choose(p, false),
                                        Opponent::Legacy => legacy(&r, p),
                                        Opponent::High => unreachable!(),
                                    }
                                };
                                assert!(r.apply_turn(p, action));
                                actions += 1;
                            }
                            *outcome = r.winner == Some(team);
                            wins[version] += usize::from(*outcome);
                        }
                        flips[0] += usize::from(!outcomes[0] && outcomes[1]);
                        flips[1] += usize::from(outcomes[0] && !outcomes[1]);
                    }
                }
                rows.push(serde_json::json!({"decks":decks,"opponent":format!("{opponent:?}"),"mixed_basic_partner":mixed,"matches":count*2,"old_new_wins":wins,"loss_to_win_win_to_loss":flips}));
            }
        }
    }
    println!(
        "NIANGNIANG_AI_MATCH_REPORT={}",
        serde_json::json!({"first_seed":first,"independent_shuffle_seeds":count,"rows":rows})
    );
}
