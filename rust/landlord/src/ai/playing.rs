use share_type_public::LandlordPhase;

use crate::core::play::{ComboKind, can_beat, card_rank, classify};

use super::{
    AiObservation, CardBelief, FarmerRunnerEstimate, Relationship,
    candidates::{Candidate, all_candidates, estimate_turns},
    search::choose_endgame_play,
};

pub(super) fn choose_play(observation: &AiObservation) -> Vec<i32> {
    choose_play_with_search(observation, true)
}

#[cfg(test)]
pub(super) fn choose_heuristic_play(observation: &AiObservation) -> Vec<i32> {
    choose_play_with_search(observation, false)
}

fn choose_play_with_search(observation: &AiObservation, use_search: bool) -> Vec<i32> {
    if observation.phase != LandlordPhase::Play
        || observation.current_position != observation.position
        || observation.hand.is_empty()
    {
        return Vec::new();
    }

    let leading =
        observation.last_play.is_empty() || observation.last_play_position == observation.position;
    let previous_combo = (!leading)
        .then(|| classify(&observation.last_play))
        .flatten();
    let mut candidates = all_candidates(&observation.hand)
        .into_iter()
        .filter(|candidate| {
            leading
                || previous_combo
                    .as_ref()
                    .is_some_and(|previous| can_beat(&candidate.combo, previous))
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Vec::new();
    }

    if let Some(finisher) = candidates
        .iter()
        .find(|candidate| candidate.cards.len() == observation.hand.len())
    {
        return finisher.cards.clone();
    }

    let belief = CardBelief::from_observation(observation);
    let previous_relationship = observation.relationship_to(observation.last_play_position);
    let farmer_runner = belief.farmer_runner(observation);
    if !leading && previous_relationship == Relationship::Ally {
        // 残局中“是否压队友”可能取决于数轮之后的牌权。先让搜索同时比较过牌和
        // 全部合法压牌；搜索未覆盖的中盘再使用下面的公开信息风险规则。
        if use_search
            && let Some(cards) = choose_endgame_play(observation, &belief, &candidates, false)
        {
            return cards;
        }
        let tactical =
            choose_over_ally_if_required(observation, &belief, &candidates, farmer_runner);
        let Some(tactical) = tactical else {
            return Vec::new();
        };
        return tactical.cards.clone();
    }
    if use_search
        && let Some(cards) = choose_endgame_play(observation, &belief, &candidates, leading)
    {
        return cards;
    }
    if leading {
        return choose_lead(observation, &belief, candidates, farmer_runner);
    }

    let enemy_cards = observation
        .hand_sizes
        .get(&observation.last_play_position)
        .copied()
        .unwrap_or(usize::MAX);
    let urgent = enemy_cards <= 2
        || observation.hand_sizes.iter().any(|(position, count)| {
            observation.relationship_to(*position) == Relationship::Enemy && *count <= 1
        })
        || farmer_runner.is_some_and(|runner| {
            runner.position == observation.last_play_position
                && runner.confidence >= 0.25
                && runner.expected_turns <= 2.25
        });
    let has_non_bomb = candidates
        .iter()
        .any(|candidate| !is_power_combo(candidate));
    if has_non_bomb {
        candidates.retain(|candidate| !is_power_combo(candidate));
    } else if !urgent
        && candidates.iter().all(|candidate| {
            let mut remaining = observation.hand.clone();
            remove_cards(&mut remaining, &candidate.cards);
            estimate_turns(&remaining) > 2
        })
    {
        return Vec::new();
    }

    best_candidate(
        observation,
        &belief,
        &candidates,
        false,
        urgent,
        farmer_runner,
    )
    .map(|candidate| candidate.cards.clone())
    .unwrap_or_default()
}

fn choose_lead(
    observation: &AiObservation,
    belief: &CardBelief,
    mut candidates: Vec<Candidate>,
    farmer_runner: Option<FarmerRunnerEstimate>,
) -> Vec<i32> {
    if let Some(next) = observation.next_position(observation.position) {
        let next_cards = observation
            .hand_sizes
            .get(&next)
            .copied()
            .unwrap_or(usize::MAX);
        match observation.relationship_to(next) {
            Relationship::Ally if next_cards == 1 => {
                if let Some(single) = candidates
                    .iter()
                    .filter(|candidate| candidate.combo.kind == ComboKind::Single)
                    .min_by_key(|candidate| candidate.combo.main_rank)
                {
                    return single.cards.clone();
                }
            }
            Relationship::Enemy if next_cards == 1 => {
                let has_non_single = candidates
                    .iter()
                    .any(|candidate| candidate.combo.kind != ComboKind::Single);
                if has_non_single {
                    candidates.retain(|candidate| candidate.combo.kind != ComboKind::Single);
                } else if let Some(single) = candidates.iter().max_by(|left, right| {
                    lead_single_score(belief, left).total_cmp(&lead_single_score(belief, right))
                }) {
                    return single.cards.clone();
                }
            }
            _ => {}
        }
    }

    best_candidate(observation, belief, &candidates, true, false, farmer_runner)
        .map(|candidate| candidate.cards.clone())
        .unwrap_or_default()
}

fn choose_over_ally_if_required<'a>(
    observation: &AiObservation,
    belief: &CardBelief,
    candidates: &'a [Candidate],
    farmer_runner: Option<FarmerRunnerEstimate>,
) -> Option<&'a Candidate> {
    let next = observation.next_position(observation.position)?;
    if observation.relationship_to(next) != Relationship::Enemy {
        return None;
    }
    let next_cards = observation
        .hand_sizes
        .get(&next)
        .copied()
        .unwrap_or(usize::MAX);
    let previous = classify(&observation.last_play)?;
    let previous_risk = belief.probability_enemies_can_beat(&previous);
    let tactical_threat = next_cards <= 2
        || (next_cards <= 5 && previous_risk >= 0.72)
        || farmer_runner.is_some_and(|runner| {
            runner.position == next && runner.confidence >= 0.45 && runner.expected_turns <= 2.25
        });
    if !tactical_threat {
        return None;
    }
    let candidate = best_candidate(observation, belief, candidates, false, true, farmer_runner)?;
    let candidate_risk = belief.probability_enemies_can_beat(&candidate.combo);
    let improvement_required = if farmer_runner.is_some_and(|runner| {
        runner.position == observation.last_play_position && runner.confidence >= 0.35
    }) {
        0.28
    } else if farmer_runner
        .is_some_and(|runner| runner.position == observation.position && runner.confidence >= 0.35)
    {
        0.08
    } else if next_cards > 2 {
        0.22
    } else {
        0.15
    };
    (candidate_risk + improvement_required < previous_risk).then_some(candidate)
}

fn best_candidate<'a>(
    observation: &AiObservation,
    belief: &CardBelief,
    candidates: &'a [Candidate],
    leading: bool,
    urgent: bool,
    farmer_runner: Option<FarmerRunnerEstimate>,
) -> Option<&'a Candidate> {
    candidates
        .iter()
        .map(|candidate| {
            (
                candidate,
                candidate_score(
                    observation,
                    belief,
                    candidate,
                    leading,
                    urgent,
                    farmer_runner,
                ),
            )
        })
        .max_by(|(_, left_score), (_, right_score)| left_score.total_cmp(right_score))
        .map(|(candidate, _)| candidate)
}

fn candidate_score(
    observation: &AiObservation,
    belief: &CardBelief,
    candidate: &Candidate,
    leading: bool,
    urgent: bool,
    farmer_runner: Option<FarmerRunnerEstimate>,
) -> f64 {
    let mut remaining = observation.hand.clone();
    remove_cards(&mut remaining, &candidate.cards);
    let turns = estimate_turns(&remaining);
    let risk = belief.probability_enemies_can_beat(&candidate.combo);
    let mut score = candidate.cards.len() as f64 * 7.0 - turns as f64 * 15.0 - risk * 18.0;

    score += match candidate.combo.kind {
        ComboKind::Straight | ComboKind::StraightPairs => 10.0,
        ComboKind::Plane | ComboKind::PlaneWithSingles | ComboKind::PlaneWithPairs => 14.0,
        ComboKind::TripleSingle | ComboKind::TriplePair => 5.0,
        ComboKind::FourWithTwoSingles | ComboKind::FourWithTwoPairs => 2.0,
        ComboKind::Bomb => {
            if urgent {
                -12.0
            } else {
                -70.0
            }
        }
        ComboKind::Rocket => {
            if urgent {
                -15.0
            } else {
                -85.0
            }
        }
        _ => 0.0,
    };

    let high_cards_spent = candidate
        .cards
        .iter()
        .map(|card| match card_rank(*card) {
            17 => 8.0,
            16 => 6.0,
            15 => 3.0,
            14 => 1.0,
            _ => 0.0,
        })
        .sum::<f64>();
    score -= if urgent {
        high_cards_spent * 0.35
    } else {
        high_cards_spent
    };
    score -= attachment_cost(candidate) * if urgent { 0.08 } else { 0.28 };
    score -= power_structure_cost(&observation.hand, candidate) * if urgent { 0.28 } else { 1.0 };

    if !leading {
        // 管牌时同牌型优先用最小代价，给后续回合保留控制牌。
        score -= candidate.combo.main_rank as f64 * 0.8;
    }
    if candidate.combo.kind == ComboKind::Single
        && belief.rank_is_control(candidate.combo.main_rank)
    {
        score += if urgent { 18.0 } else { 7.0 };
    }
    if risk < 0.05 {
        score += 8.0;
    }
    if let Some(runner) = farmer_runner
        && observation.landlord_position != Some(observation.position)
    {
        let role_weight = runner.confidence.max(0.15);
        if runner.position == observation.position {
            // 主跑农民优先减少后续手数，支援农民则保留大牌和炸弹负责夺回牌权。
            score += (candidate.cards.len() as f64 * 1.5 - turns as f64 * 2.5) * role_weight;
        } else {
            let control_cards_spent = candidate
                .cards
                .iter()
                .filter(|card| card_rank(**card) >= 15 || belief.rank_is_control(card_rank(**card)))
                .count() as f64;
            score -= if urgent {
                control_cards_spent * 1.5 * role_weight
            } else {
                control_cards_spent * 6.0 * role_weight
            };
            if leading
                && observation.next_position(observation.position) == Some(runner.position)
                && matches!(candidate.combo.kind, ComboKind::Single | ComboKind::Pair)
                && let Some(ally) = belief.opponents.get(&runner.position)
            {
                score += ally.probability_can_beat(&candidate.combo) * 18.0 * role_weight;
                score -= candidate.combo.main_rank as f64 * 0.35;
            }
        }
    } else if let Some(runner) = farmer_runner
        && observation.landlord_position == Some(observation.position)
        && runner.confidence >= 0.25
        && runner.expected_turns <= 2.5
    {
        // 地主应围绕最可能先走完的农民防守，而不是只看当前出牌者的张数。
        score -= risk * 14.0 * runner.confidence;
        if !leading && observation.last_play_position == runner.position {
            score += candidate.cards.len() as f64 * 1.4 * runner.confidence;
        }
    }
    if remaining.is_empty() {
        score += 10_000.0;
    }
    score
}

fn lead_single_score(belief: &CardBelief, candidate: &Candidate) -> f64 {
    let control = if belief.rank_is_control(candidate.combo.main_rank) {
        100.0
    } else {
        0.0
    };
    control + candidate.combo.main_rank as f64
}

fn attachment_cost(candidate: &Candidate) -> f64 {
    let body_start = candidate
        .combo
        .main_rank
        .saturating_sub(candidate.combo.sequence_len.saturating_sub(1) as u8);
    candidate
        .cards
        .iter()
        .map(|card| card_rank(*card))
        .filter(|rank| match candidate.combo.kind {
            ComboKind::TripleSingle | ComboKind::TriplePair => *rank != candidate.combo.main_rank,
            ComboKind::PlaneWithSingles | ComboKind::PlaneWithPairs => {
                *rank < body_start || *rank > candidate.combo.main_rank
            }
            ComboKind::FourWithTwoSingles | ComboKind::FourWithTwoPairs => {
                *rank != candidate.combo.main_rank
            }
            _ => false,
        })
        .map(f64::from)
        .sum()
}

fn power_structure_cost(hand: &[i32], candidate: &Candidate) -> f64 {
    let mut held = [0_u8; 18];
    let mut used = [0_u8; 18];
    for &card in hand {
        held[card_rank(card) as usize] += 1;
    }
    for &card in &candidate.cards {
        used[card_rank(card) as usize] += 1;
    }

    let mut cost = 0.0;
    for rank in 3..=15 {
        if held[rank] != 4 || used[rank] == 0 {
            continue;
        }
        cost += if candidate.combo.kind == ComboKind::Bomb && used[rank] == 4 {
            0.0
        } else if used[rank] == 4 {
            18.0
        } else {
            24.0 + f64::from(used[rank]) * 2.0
        };
    }
    if held[16] == 1
        && held[17] == 1
        && candidate.combo.kind != ComboKind::Rocket
        && (used[16] == 1 || used[17] == 1)
    {
        cost += 22.0;
    }
    cost
}

fn is_power_combo(candidate: &Candidate) -> bool {
    matches!(candidate.combo.kind, ComboKind::Bomb | ComboKind::Rocket)
}

fn remove_cards(hand: &mut Vec<i32>, cards: &[i32]) {
    for card in cards {
        if let Some(index) = hand.iter().position(|candidate| candidate == card) {
            hand.remove(index);
        }
    }
}

#[cfg(test)]
mod tests {
    use share_type_public::LandlordPhase;

    use crate::ai::{AiObservation, tests::state_with_hands};
    use crate::game_state::LandlordPlayRecord;

    use crate::core::play::ComboKind;

    use super::choose_play;

    fn play_observation(
        position: usize,
        landlord: usize,
        hands: &[(usize, Vec<i32>)],
        last_position: usize,
        last_play: Vec<i32>,
    ) -> AiObservation {
        let mut state = state_with_hands(hands);
        state.phase = LandlordPhase::Play;
        state.current_position = position;
        state.landlord_position = Some(landlord);
        state.last_play_position = last_position;
        state.last_play = last_play;
        AiObservation::from_state(&state, position).expect("observation")
    }

    #[test]
    fn farmer_search_overtakes_teammate_before_a_dangerous_landlord() {
        let observation = play_observation(
            2,
            0,
            &[(0, vec![8, 9, 10]), (1, vec![5, 18]), (2, vec![6, 19, 32])],
            1,
            vec![5],
        );
        assert!(!choose_play(&observation).is_empty());
    }

    #[test]
    fn farmer_does_not_overtake_after_landlord_has_already_passed() {
        let observation = play_observation(
            1,
            0,
            &[(0, vec![8, 9, 10]), (1, vec![6, 19, 32]), (2, vec![5, 18])],
            2,
            vec![5],
        );
        assert!(choose_play(&observation).is_empty());
    }

    #[test]
    fn farmer_blocks_landlord_with_one_card() {
        let observation = play_observation(
            1,
            0,
            &[(0, vec![12]), (1, vec![7, 20, 33]), (2, vec![1, 2, 3])],
            0,
            vec![6],
        );
        assert_eq!(choose_play(&observation), vec![7]);
    }

    #[test]
    fn leader_avoids_single_against_one_card_enemy() {
        let observation = play_observation(
            0,
            0,
            &[(0, vec![1, 14, 2]), (1, vec![54]), (2, vec![3, 4, 5])],
            0,
            Vec::new(),
        );
        assert_eq!(choose_play(&observation), vec![1, 14]);
    }

    #[test]
    fn farmer_feeds_one_card_teammate() {
        let observation = play_observation(
            1,
            0,
            &[(0, vec![8, 9, 10]), (1, vec![1, 2]), (2, vec![11])],
            1,
            Vec::new(),
        );
        assert_eq!(choose_play(&observation), vec![1]);
    }

    #[test]
    fn ordinary_triple_single_prefers_the_cheapest_kicker() {
        let observation = play_observation(
            0,
            0,
            &[
                (0, vec![1, 3, 16, 29, 7]), // 单 3、三张 5、单 9
                (
                    1,
                    vec![2, 4, 5, 6, 8, 9, 10, 11, 12, 13, 14, 15, 17, 18, 19],
                ),
                (
                    2,
                    vec![20, 21, 22, 23, 24, 25, 26, 27, 28, 30, 31, 32, 33, 34, 35],
                ),
            ],
            0,
            Vec::new(),
        );

        let cards = choose_play(&observation);
        assert_eq!(
            ComboKind::TripleSingle,
            crate::core::play::classify(&cards).unwrap().kind
        );
        assert!(
            cards.contains(&1),
            "AI should shed the 3 kicker first: {cards:?}"
        );
    }

    #[test]
    fn leading_sequence_does_not_split_a_bomb_for_one_extra_card() {
        let observation = play_observation(
            0,
            0,
            &[
                (0, vec![1, 14, 27, 40, 2, 3, 4, 5, 6]), // 3333 + 45678
                (1, vec![7, 8, 9, 10, 11, 12, 13, 15, 16, 17, 18]),
                (2, vec![19, 20, 21, 22, 23, 24, 25, 26, 28, 29, 30]),
            ],
            0,
            Vec::new(),
        );

        let cards = choose_play(&observation);
        assert_eq!(cards, vec![2, 3, 4, 5, 6]);
    }

    #[test]
    fn farmer_avoids_leading_a_known_winning_pair_to_two_card_landlord() {
        let mut state = state_with_hands(&[
            (0, vec![2, 15]), // 地主只剩一对 4
            (1, vec![5, 6, 7, 8]),
            (2, vec![1, 14, 3, 4]), // 农民有一对 3 和更安全的单牌
        ]);
        state.phase = LandlordPhase::Play;
        state.landlord_position = Some(0);
        state.current_position = 2;
        state.last_play_position = 2;
        state.hidden_cards = vec![2, 15, 54];
        state.play_history.push(LandlordPlayRecord {
            position: 0,
            cards: vec![54],
            benchmark: Vec::new(),
        });
        let observation = AiObservation::from_state(&state, 2).expect("observation");

        let cards = choose_play(&observation);
        assert_ne!(
            crate::core::play::classify(&cards).unwrap().kind,
            ComboKind::Pair
        );
    }

    #[test]
    fn landlord_uses_a_bomb_instead_of_letting_one_card_runner_regain_lead() {
        let observation = play_observation(
            0,
            0,
            &[
                (0, vec![1, 14, 27, 40, 2]),
                (1, vec![3, 4, 5, 6]),
                (2, vec![7]),
            ],
            2,
            vec![12], // 农民主跑刚出 A，地主没有普通牌能压
        );

        assert_eq!(choose_play(&observation), vec![1, 14, 27, 40]);
    }

    #[test]
    fn support_farmer_bombs_teammate_only_to_stop_a_certain_landlord_finish() {
        let mut state = state_with_hands(&[
            (0, vec![13]),
            (1, vec![3, 4, 5]),
            (2, vec![1, 14, 27, 40, 2]),
        ]);
        state.phase = LandlordPhase::Play;
        state.landlord_position = Some(0);
        state.current_position = 2;
        state.last_play_position = 1;
        state.last_play = vec![11]; // 队友出 K，地主已知最后一张是 2
        state.hidden_cards = vec![13, 53, 54];
        state.play_history.extend([
            LandlordPlayRecord {
                position: 0,
                cards: vec![53, 54],
                benchmark: Vec::new(),
            },
            LandlordPlayRecord {
                position: 1,
                cards: vec![11],
                benchmark: Vec::new(),
            },
        ]);
        let observation = AiObservation::from_state(&state, 2).expect("observation");

        assert_eq!(choose_play(&observation), vec![1, 14, 27, 40]);
    }

    #[test]
    fn ordinary_lead_keeps_the_rocket_together() {
        let observation = play_observation(
            0,
            0,
            &[
                (0, vec![53, 54, 1, 2, 3, 4, 5, 8]),
                (1, vec![6, 7, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18]),
                (2, vec![19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30]),
            ],
            0,
            Vec::new(),
        );

        let cards = choose_play(&observation);
        assert!(
            !cards.contains(&53) && !cards.contains(&54),
            "split rocket: {cards:?}"
        );
    }
}
