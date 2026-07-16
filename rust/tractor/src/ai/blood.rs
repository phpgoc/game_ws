//! Blood-scoring attack route selection.
//!
//! When blood scoring becomes urgent the AI chooses one main route (trump or a
//! plain suit) and releases the largest useful pair/tractor in that route. It
//! does not split a long tractor into repeated one-card probes.

use std::collections::HashMap;

use crate::{
    ai::knowledge::PublicKnowledge,
    combo::{self, ComboKind},
    game_state::{TractorGameState, card_score, same_team},
};

#[derive(Debug, Clone)]
pub(crate) struct BloodPlan {
    pub(crate) route: BloodRoute,
    pub(crate) cards: Vec<i32>,
    pub(crate) control_probability: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BloodRoute {
    Trump,
    Plain(i32),
}

pub(crate) fn lead_plan(
    state: &TractorGameState,
    position: usize,
    hand: &[i32],
    knowledge: &PublicKnowledge,
) -> Option<BloodPlan> {
    if !state.rules.blood_enabled || hand.len() > 18 {
        return None;
    }
    let attacking_score = state.attacking_score();
    let threshold = state.rules.blood_start_score.max(1);
    let attacking = !same_team(position, state.dealer_position);
    let urgency = attacking_score * 100 / threshold;
    // Start committing to a route once the attacking side is close to blood or
    // when either side is in a short-hand endgame.
    if hand.len() > 10 && urgency < 55 {
        return None;
    }

    let leads = combo::enumerate_leads(hand, &state.rules);
    let mut longest_by_route: HashMap<Option<i32>, usize> = HashMap::new();
    for cards in &leads {
        let Some(classified) = combo::classify(cards, &state.rules) else {
            continue;
        };
        if !matches!(
            classified.kind,
            ComboKind::Pair | ComboKind::Tractor(_) | ComboKind::Throw { .. }
        ) {
            continue;
        }
        longest_by_route
            .entry(classified.suit)
            .and_modify(|length| *length = (*length).max(cards.len()))
            .or_insert(cards.len());
    }

    leads
        .into_iter()
        .filter_map(|cards| {
            let classified = combo::classify(&cards, &state.rules)?;
            if !matches!(
                classified.kind,
                ComboKind::Pair | ComboKind::Tractor(_) | ComboKind::Throw { .. }
            ) {
                return None;
            }
            // "一次性全部": only consider the largest available structure in
            // the selected route, never a shorter sub-tractor from the same run.
            if longest_by_route.get(&classified.suit).copied() != Some(cards.len()) {
                return None;
            }
            let probability = if matches!(classified.kind, ComboKind::Throw { .. }) {
                knowledge.throw_success_probability(state, &cards)
            } else {
                knowledge.lead_control_probability(state, &cards)
            };
            let points = cards.iter().map(|card| card_score(*card)).sum::<i32>();
            let route = classified
                .suit
                .map(BloodRoute::Plain)
                .unwrap_or(BloodRoute::Trump);
            let trump_endgame_bonus =
                i32::from(!attacking && hand.len() <= 8 && matches!(route, BloodRoute::Trump)) * 90;
            let uncertain_point_penalty = if points > 0 && probability < 0.78 {
                points * 8
            } else {
                0
            };
            let score =
                cards.len() as i32 * 45 + (probability * 120.0) as i32 + trump_endgame_bonus
                    - uncertain_point_penalty;
            Some((
                score,
                BloodPlan {
                    route,
                    cards,
                    control_probability: probability,
                },
            ))
        })
        .filter(|(_, plan)| plan.control_probability >= 0.42 || plan.cards.len() >= 6)
        .max_by_key(|(score, plan)| (*score, plan.cards.len()))
        .map(|(_, plan)| plan)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use share_type_public::{TractorPhase, TractorRank, TractorSuit};
    use ws_common::CommonGameState;

    use super::*;
    use crate::game_state::{TractorGameState, TractorRules};

    #[test]
    fn blood_plan_chooses_dominant_plain_tractor_over_scattered_trump() {
        let state = state(vec![
            53, 1, 13, // scattered trump
            15, 115, 16, 116, 17, 117, 18, 118, // four-pair heart tractor
            31, 32,
        ]);
        let knowledge = PublicKnowledge::from_state(&state, 0);
        let plan = lead_plan(&state, 0, &state.hands[&0], &knowledge).expect("blood plan");
        assert_eq!(plan.route, BloodRoute::Plain(1));
        assert_eq!(plan.cards.len(), 8);
    }

    #[test]
    fn blood_plan_releases_full_trump_tractor_in_one_play() {
        let state = state(vec![
            11, 111, 12, 112, 13, 113, // spade Q-K-A trump tractor
            15, 18, 31, 44,
        ]);
        let knowledge = PublicKnowledge::from_state(&state, 0);
        let plan = lead_plan(&state, 0, &state.hands[&0], &knowledge).expect("blood plan");
        assert_eq!(plan.route, BloodRoute::Trump);
        assert_eq!(plan.cards.len(), 6);
    }

    fn state(hand: Vec<i32>) -> TractorGameState {
        let mut common = CommonGameState::new();
        for position in 0..4 {
            common.add_player(position, position as u64 + 1, &format!("u{position}"));
        }
        let mut state = TractorGameState::from_common(Arc::new(Mutex::new(common)));
        state.phase = TractorPhase::Play;
        state.rules = TractorRules {
            blood_enabled: true,
            blood_score_per_unit: 30,
            blood_start_score: 160,
            bottom_card_count: 10,
            deck_count: 3,
            final_target_rank: TractorRank::J,
            removed_rank_count: 3,
            target_rank: TractorRank::TWO,
            trump_suit: Some(TractorSuit::SPADE),
        };
        state.dealer_position = 1;
        state.current_position = 0;
        state.collected_scores.insert(0, 100);
        state.hands.insert(0, hand);
        for position in 1..4 {
            state.hands.insert(position, vec![30, 31, 32, 33]);
        }
        state
    }
}
