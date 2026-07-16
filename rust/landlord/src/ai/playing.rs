use share_type_public::LandlordPhase;

use crate::core::play::{ComboKind, can_beat, card_rank, classify};

use super::{
    AiObservation, CardBelief, Relationship,
    candidates::{Candidate, all_candidates, estimate_turns},
};

pub(super) fn choose_play(observation: &AiObservation) -> Vec<i32> {
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
    if leading {
        return choose_lead(observation, &belief, candidates);
    }

    let previous_relationship = observation.relationship_to(observation.last_play_position);
    if previous_relationship == Relationship::Ally {
        return choose_over_ally_if_required(observation, &belief, &candidates)
            .map(|candidate| candidate.cards.clone())
            .unwrap_or_default();
    }

    let enemy_cards = observation
        .hand_sizes
        .get(&observation.last_play_position)
        .copied()
        .unwrap_or(usize::MAX);
    let urgent = enemy_cards <= 2
        || observation.hand_sizes.iter().any(|(position, count)| {
            observation.relationship_to(*position) == Relationship::Enemy && *count <= 1
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

    best_candidate(observation, &belief, &candidates, false, urgent)
        .map(|candidate| candidate.cards.clone())
        .unwrap_or_default()
}

fn choose_lead(
    observation: &AiObservation,
    belief: &CardBelief,
    mut candidates: Vec<Candidate>,
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

    best_candidate(observation, belief, &candidates, true, false)
        .map(|candidate| candidate.cards.clone())
        .unwrap_or_default()
}

fn choose_over_ally_if_required<'a>(
    observation: &AiObservation,
    belief: &CardBelief,
    candidates: &'a [Candidate],
) -> Option<&'a Candidate> {
    let next = observation.next_position(observation.position)?;
    if observation.relationship_to(next) != Relationship::Enemy
        || observation
            .hand_sizes
            .get(&next)
            .copied()
            .unwrap_or(usize::MAX)
            > 2
    {
        return None;
    }
    let previous = classify(&observation.last_play)?;
    let previous_risk = belief.probability_enemies_can_beat(&previous);
    let candidate = best_candidate(observation, belief, candidates, false, true)?;
    let candidate_risk = belief.probability_enemies_can_beat(&candidate.combo);
    (candidate_risk + 0.15 < previous_risk).then_some(candidate)
}

fn best_candidate<'a>(
    observation: &AiObservation,
    belief: &CardBelief,
    candidates: &'a [Candidate],
    leading: bool,
    urgent: bool,
) -> Option<&'a Candidate> {
    candidates.iter().max_by(|left, right| {
        candidate_score(observation, belief, left, leading, urgent).total_cmp(&candidate_score(
            observation,
            belief,
            right,
            leading,
            urgent,
        ))
    })
}

fn candidate_score(
    observation: &AiObservation,
    belief: &CardBelief,
    candidate: &Candidate,
    leading: bool,
    urgent: bool,
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
    fn farmer_does_not_overtake_safe_teammate() {
        let observation = play_observation(
            2,
            0,
            &[(0, vec![8, 9, 10]), (1, vec![5, 18]), (2, vec![6, 19, 32])],
            1,
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
}
