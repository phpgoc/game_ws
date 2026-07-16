//! Public-information card memory and probability estimates for the tractor AI.
//!
//! This module deliberately never reads an opponent's card values. It sees the
//! AI's own hand, cards already exposed on the table, public hand counts and (for
//! the dealer only) the cards it buried. That boundary is important: a strong AI
//! should infer and estimate, not use the server's omniscient state as a cheat.

use std::collections::{HashMap, HashSet};

use share_type_public::{TractorPhase, WsTractorPlayedCards};

use crate::{
    combo,
    game_state::{TractorGameState, base_card, build_tractor_deck_with_removed_ranks, same_team},
};

#[derive(Debug, Clone)]
pub(crate) struct PublicKnowledge {
    viewer: usize,
    unseen_cards: Vec<i32>,
    remaining_by_base: HashMap<i32, usize>,
    void_groups: HashMap<usize, HashSet<Option<i32>>>,
    hand_counts: HashMap<usize, usize>,
    active_positions: Vec<usize>,
}

fn hypergeom_exact_zero(population: usize, successes: usize, draws: usize) -> f64 {
    if successes == 0 || draws == 0 {
        return 1.0;
    }
    if draws > population || population.saturating_sub(successes) < draws {
        return 0.0;
    }
    (0..draws).fold(1.0, |probability, index| {
        probability * (population - successes - index) as f64 / (population - index) as f64
    })
}

fn infer_voids(
    trick: &[WsTractorPlayedCards],
    state: &TractorGameState,
    void_groups: &mut HashMap<usize, HashSet<Option<i32>>>,
) {
    let Some(lead) = trick.first() else {
        return;
    };
    let Some(combo) = combo::classify(&lead.cards, &state.rules) else {
        return;
    };
    for played in trick.iter().skip(1) {
        let matching = played
            .cards
            .iter()
            .filter(|card| combo::card_in_group(**card, combo.suit, &state.rules))
            .count();
        // Legal following requires using every available card from the led group
        // until the play is full. Once an off-group card appears, all matching
        // cards have just been exhausted and this player is now publicly void.
        if matching < played.cards.len()
            && let Ok(position) = usize::try_from(played.position)
        {
            void_groups.entry(position).or_default().insert(combo.suit);
        }
    }
}

fn log_choose(n: usize, k: usize) -> f64 {
    if k > n {
        return f64::NEG_INFINITY;
    }
    let k = k.min(n - k);
    (0..k).fold(0.0, |sum, index| {
        sum + ((n - index) as f64).ln() - ((index + 1) as f64).ln()
    })
}

/// Exact probability that one hand contains every requested base-card count.
/// The requested identities are disjoint categories, so a small convolution
/// avoids the independence error caused by multiplying separate marginals.
fn multivariate_hypergeom_at_least(
    population: usize,
    draws: usize,
    requirements: &[(usize, usize)],
) -> f64 {
    if draws > population
        || requirements
            .iter()
            .any(|(available, required)| available < required)
        || requirements
            .iter()
            .map(|(_, required)| required)
            .sum::<usize>()
            > draws
    {
        return 0.0;
    }

    let category_total = requirements
        .iter()
        .map(|(available, _)| *available)
        .sum::<usize>();
    if category_total > population {
        return 0.0;
    }
    let mut ways_by_hits = vec![0.0; draws + 1];
    ways_by_hits[0] = 1.0;
    for (available, required) in requirements.iter().copied() {
        let mut next = vec![0.0; draws + 1];
        for already in 0..=draws {
            if ways_by_hits[already] == 0.0 {
                continue;
            }
            for hits in required..=available.min(draws - already) {
                next[already + hits] += ways_by_hits[already] * log_choose(available, hits).exp();
            }
        }
        ways_by_hits = next;
    }

    let background = population - category_total;
    let favourable = ways_by_hits
        .into_iter()
        .enumerate()
        .filter_map(|(hits, ways)| {
            let misses = draws.checked_sub(hits)?;
            (misses <= background).then_some(ways * log_choose(background, misses).exp())
        })
        .sum::<f64>();
    let total = log_choose(population, draws).exp();
    if total == 0.0 || !total.is_finite() {
        return 0.0;
    }
    (favourable / total).clamp(0.0, 1.0)
}

impl PublicKnowledge {
    pub(crate) fn all_other_players_void(&self, group: Option<i32>) -> bool {
        self.active_positions
            .iter()
            .copied()
            .filter(|position| *position != self.viewer)
            .all(|position| self.known_void(position, group))
    }

    /// Probability that `cards`, played now by `position`, will remain winning
    /// after every seat that has not acted in this trick responds.
    pub(crate) fn candidate_hold_probability(
        &self,
        state: &TractorGameState,
        position: usize,
        cards: &[i32],
    ) -> f64 {
        let Some(lead_play) = state.current_trick.first() else {
            return self.lead_control_probability(state, cards);
        };
        let Some(lead) = combo::classify(&lead_play.cards, &state.rules) else {
            return 0.0;
        };
        let Some(value) = combo::combo_win_value(cards, &lead, &state.rules) else {
            return 0.0;
        };
        let already_played: HashSet<usize> = state
            .current_trick
            .iter()
            .filter_map(|played| usize::try_from(played.position).ok())
            .chain(std::iter::once(position))
            .collect();
        let future_opponents = self
            .active_positions
            .iter()
            .copied()
            .filter(|other| !already_played.contains(other))
            .filter(|other| !same_team(*other, position))
            .collect();
        self.control_probability_against(state, &lead, value, future_opponents)
    }

    fn control_probability_against(
        &self,
        state: &TractorGameState,
        lead: &combo::Combo,
        value_to_beat: i32,
        opponents: Vec<usize>,
    ) -> f64 {
        if opponents.is_empty() {
            return 1.0;
        }

        let mut threat_keys = HashSet::new();
        let threats: Vec<Vec<i32>> = combo::enumerate_leads(&self.unseen_cards, &state.rules)
            .into_iter()
            .filter(|candidate| {
                combo::classify(candidate, &state.rules).map(|combo| combo.kind) == Some(lead.kind)
            })
            .filter(|candidate| {
                combo::combo_win_value(candidate, lead, &state.rules)
                    .is_some_and(|value| value > value_to_beat)
            })
            .filter(|candidate| {
                let mut key: Vec<_> = candidate.iter().map(|card| base_card(*card)).collect();
                key.sort_unstable();
                threat_keys.insert(key)
            })
            .collect();

        let mut no_enemy_threat = 1.0;
        for threat in threats {
            let threat_group = combo::play_suit(&threat, &state.rules);
            let mut no_position_has_threat = 1.0;
            for opponent in opponents.iter().copied() {
                // A remembered void is a hard ownership constraint, not merely
                // a hint about what the player may follow. Do not assign that
                // player a higher card from a group they have exhausted.
                let holds_probability = if self.known_void(opponent, threat_group) {
                    0.0
                } else {
                    self.probability_player_holds(opponent, &threat)
                };
                let can_use_probability = if threat_group.is_none() && lead.suit.is_some() {
                    self.probability_player_void(opponent, lead.suit, state)
                } else {
                    1.0
                };
                no_position_has_threat *=
                    1.0 - (can_use_probability * holds_probability).clamp(0.0, 1.0);
            }
            let any_position_has_threat = 1.0 - no_position_has_threat;
            no_enemy_threat *= 1.0 - any_position_has_threat;
        }
        no_enemy_threat.clamp(0.0, 1.0)
    }

    pub(crate) fn current_winner_hold_probability(&self, state: &TractorGameState) -> f64 {
        let Some(winner) = combo::trick_winner(&state.current_trick, &state.rules) else {
            return 0.0;
        };
        let Some(lead_play) = state.current_trick.first() else {
            return 0.0;
        };
        let Some(lead) = combo::classify(&lead_play.cards, &state.rules) else {
            return 0.0;
        };
        let Some(winning_play) = state
            .current_trick
            .iter()
            .find(|played| played.position == winner as i32)
        else {
            return 0.0;
        };
        let Some(value) = combo::combo_win_value(&winning_play.cards, &lead, &state.rules) else {
            return 0.0;
        };
        let already_played: HashSet<usize> = state
            .current_trick
            .iter()
            .filter_map(|played| usize::try_from(played.position).ok())
            .collect();
        let future_opponents = self
            .active_positions
            .iter()
            .copied()
            .filter(|other| !already_played.contains(other))
            .filter(|other| !same_team(*other, winner))
            .collect();
        self.control_probability_against(state, &lead, value, future_opponents)
    }

    fn enemy_positions(&self) -> impl Iterator<Item = usize> + '_ {
        self.active_positions
            .iter()
            .copied()
            .filter(|position| !same_team(*position, self.viewer))
    }

    pub(crate) fn from_state(state: &TractorGameState, viewer: usize) -> Self {
        let mut seen_by_base: HashMap<i32, usize> = HashMap::new();
        let mut remember = |card: i32| {
            *seen_by_base.entry(base_card(card)).or_default() += 1;
        };

        // Only the viewer's hidden cards are legitimate private information.
        if let Some(hand) = state.hands.get(&viewer) {
            for card in hand {
                remember(*card);
            }
        }
        for trick in &state.completed_tricks {
            for played in trick {
                for card in &played.cards {
                    remember(*card);
                }
            }
        }
        for played in &state.current_trick {
            for card in &played.cards {
                remember(*card);
            }
        }
        // Once burial has completed, the dealer remembers the private bottom.
        // During Bury those cards are still in their hand, so adding them again
        // would double count them.
        if viewer == state.dealer_position
            && matches!(state.phase, TractorPhase::Play | TractorPhase::Settlement)
        {
            for card in &state.bottom_cards {
                remember(*card);
            }
        }

        let mut total_by_base: HashMap<i32, usize> = HashMap::new();
        for card in build_tractor_deck_with_removed_ranks(
            state.rules.deck_count,
            state.rules.removed_rank_count,
        ) {
            *total_by_base.entry(base_card(card)).or_default() += 1;
        }
        let mut remaining_by_base = HashMap::new();
        let mut unseen_cards = Vec::new();
        for (base, total) in total_by_base {
            let remaining = total.saturating_sub(seen_by_base.get(&base).copied().unwrap_or(0));
            if remaining > 0 {
                remaining_by_base.insert(base, remaining);
                unseen_cards.extend(std::iter::repeat_n(base, remaining));
            }
        }

        let mut void_groups: HashMap<usize, HashSet<Option<i32>>> = HashMap::new();
        for trick in state
            .completed_tricks
            .iter()
            .chain(std::iter::once(&state.current_trick))
        {
            infer_voids(trick, state, &mut void_groups);
        }

        // Hand counts are table-visible. Do not retain any opponent card value.
        let hand_counts = state
            .hands
            .iter()
            .map(|(position, hand)| (*position, hand.len()))
            .collect();

        Self {
            viewer,
            unseen_cards,
            remaining_by_base,
            void_groups,
            hand_counts,
            active_positions: state.active_positions(),
        }
    }

    pub(crate) fn known_void(&self, position: usize, group: Option<i32>) -> bool {
        self.void_groups
            .get(&position)
            .is_some_and(|groups| groups.contains(&group))
    }

    /// Estimate the chance that no opponent can legally overtake this lead.
    /// The calculation uses unseen-card hypergeometric probabilities and known
    /// voids. Correlated threats are combined conservatively, so borderline
    /// plays are treated as gambles rather than declared certain winners.
    pub(crate) fn lead_control_probability(&self, state: &TractorGameState, cards: &[i32]) -> f64 {
        let Some(lead) = combo::classify(cards, &state.rules) else {
            return 0.0;
        };
        let Some(lead_value) = combo::combo_win_value(cards, &lead, &state.rules) else {
            return 0.0;
        };

        self.control_probability_against(state, &lead, lead_value, self.enemy_positions().collect())
    }

    fn other_positions(&self) -> impl Iterator<Item = usize> + '_ {
        self.active_positions
            .iter()
            .copied()
            .filter(|position| *position != self.viewer)
    }

    fn probability_player_holds(&self, position: usize, cards: &[i32]) -> f64 {
        let draws = self.hand_counts.get(&position).copied().unwrap_or_default();
        if draws < cards.len() || self.unseen_cards.is_empty() {
            return 0.0;
        }
        let mut requirements: HashMap<i32, usize> = HashMap::new();
        for card in cards {
            *requirements.entry(base_card(*card)).or_default() += 1;
        }
        let requirements: Vec<_> = requirements
            .into_iter()
            .map(|(base, needed)| {
                let available = self.remaining_by_base.get(&base).copied().unwrap_or(0);
                (available, needed)
            })
            .collect();
        multivariate_hypergeom_at_least(self.unseen_cards.len(), draws, &requirements)
    }

    fn probability_player_void(
        &self,
        position: usize,
        group: Option<i32>,
        state: &TractorGameState,
    ) -> f64 {
        if self.known_void(position, group) {
            return 1.0;
        }
        let draws = self.hand_counts.get(&position).copied().unwrap_or_default();
        let group_cards = self
            .unseen_cards
            .iter()
            .filter(|card| combo::card_in_group(**card, group, &state.rules))
            .count();
        hypergeom_exact_zero(self.unseen_cards.len(), group_cards, draws)
    }

    pub(crate) fn throw_success_probability(&self, state: &TractorGameState, cards: &[i32]) -> f64 {
        let Some(components) = combo::throw_components(cards, &state.rules) else {
            return 0.0;
        };
        components.into_iter().fold(1.0, |probability, component| {
            let Some(lead) = combo::classify(&component, &state.rules) else {
                return 0.0;
            };
            let Some(value) = combo::combo_win_value(&component, &lead, &state.rules) else {
                return 0.0;
            };
            // A throw is challenged by every other seat, including our
            // partner. Trick-control estimates intentionally ignore a partner
            // overtaking us, but throw legality cannot do that.
            probability
                * self.control_probability_against(
                    state,
                    &lead,
                    value,
                    self.other_positions().collect(),
                )
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use share_type_public::{TractorPhase, TractorRank, WsTractorPlayedCards};
    use ws_common::CommonGameState;

    use super::*;
    use crate::game_state::{TractorGameState, TractorRules};

    #[test]
    fn exposed_higher_pairs_raise_control_probability() {
        let mut state = state();
        state.hands.insert(0, vec![11, 111, 20, 21]);
        for position in 1..4 {
            state.hands.insert(position, vec![30, 31, 32, 33]);
        }
        let before =
            PublicKnowledge::from_state(&state, 0).lead_control_probability(&state, &[11, 111]);
        state.completed_tricks = vec![vec![WsTractorPlayedCards {
            position: 1,
            name: "u1".to_owned(),
            cards: vec![12, 112],
        }]];
        let after_one =
            PublicKnowledge::from_state(&state, 0).lead_control_probability(&state, &[11, 111]);
        state.completed_tricks.push(vec![WsTractorPlayedCards {
            position: 2,
            name: "u2".to_owned(),
            cards: vec![13, 113],
        }]);
        let after_all =
            PublicKnowledge::from_state(&state, 0).lead_control_probability(&state, &[11, 111]);

        assert!(after_one > before);
        assert!(after_all > after_one);
    }

    #[test]
    fn known_void_player_is_not_assigned_a_higher_card_in_that_suit() {
        let mut state = state();
        state.hands.insert(0, vec![53, 153, 20, 21]);
        for position in 1..4 {
            state.hands.insert(position, vec![30, 31, 32, 33]);
        }
        state.completed_tricks = vec![vec![
            WsTractorPlayedCards {
                position: 0,
                name: "u0".to_owned(),
                cards: vec![18],
            },
            WsTractorPlayedCards {
                position: 1,
                name: "u1".to_owned(),
                cards: vec![1],
            },
        ]];
        let before =
            PublicKnowledge::from_state(&state, 0).lead_control_probability(&state, &[53, 153]);
        // Keep the exact same exposed cards and hand counts, changing only
        // which group was led. Position 1 is now known void in trumps.
        state.completed_tricks = vec![vec![
            WsTractorPlayedCards {
                position: 0,
                name: "u0".to_owned(),
                cards: vec![1],
            },
            WsTractorPlayedCards {
                position: 1,
                name: "u1".to_owned(),
                cards: vec![18],
            },
        ]];
        let after =
            PublicKnowledge::from_state(&state, 0).lead_control_probability(&state, &[53, 153]);

        assert!(after > before, "before={before} after={after}");
    }

    #[test]
    fn multi_card_holding_probability_uses_joint_hypergeometric_math() {
        // Draw two from A,A,B,B. Four of the six equally likely hands contain
        // at least one A and one B.
        let probability = multivariate_hypergeom_at_least(4, 2, &[(2, 1), (2, 1)]);
        assert!((probability - 2.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn off_suit_follow_is_remembered_as_a_void() {
        let mut state = state();
        state.hands.insert(0, vec![5]);
        state.hands.insert(1, vec![18]);
        state.hands.insert(2, vec![6]);
        state.hands.insert(3, vec![7]);
        state.completed_tricks = vec![vec![
            WsTractorPlayedCards {
                position: 0,
                name: "u0".to_owned(),
                cards: vec![4],
            },
            WsTractorPlayedCards {
                position: 1,
                name: "u1".to_owned(),
                cards: vec![18],
            },
        ]];

        let knowledge = PublicKnowledge::from_state(&state, 0);
        assert!(knowledge.known_void(1, Some(0)));
        assert!(!knowledge.known_void(2, Some(0)));
    }

    #[test]
    fn public_estimate_never_changes_when_opponent_hidden_values_change() {
        let mut first = state();
        first.hands.insert(0, vec![11, 111, 20, 21]);
        first.hands.insert(1, vec![12, 112, 13, 113]);
        first.hands.insert(2, vec![30, 31, 32, 33]);
        first.hands.insert(3, vec![40, 41, 42, 43]);
        let mut second = state();
        second.hands.insert(0, vec![11, 111, 20, 21]);
        second.hands.insert(1, vec![2, 3, 4, 5]);
        second.hands.insert(2, vec![14, 15, 16, 17]);
        second.hands.insert(3, vec![27, 28, 29, 30]);

        let first_probability =
            PublicKnowledge::from_state(&first, 0).lead_control_probability(&first, &[11, 111]);
        let second_probability =
            PublicKnowledge::from_state(&second, 0).lead_control_probability(&second, &[11, 111]);

        assert!((first_probability - second_probability).abs() < f64::EPSILON);
    }

    fn state() -> TractorGameState {
        let mut common = CommonGameState::new();
        for position in 0..4 {
            common.add_player(position, position as u64 + 1, &format!("u{position}"));
        }
        let mut state = TractorGameState::from_common(Arc::new(Mutex::new(common)));
        state.phase = TractorPhase::Play;
        state.rules = TractorRules {
            blood_enabled: true,
            blood_score_per_unit: 40,
            blood_start_score: 80,
            bottom_card_count: 8,
            deck_count: 2,
            final_target_rank: TractorRank::A,
            removed_rank_count: 0,
            target_rank: TractorRank::TWO,
            trump_suit: None,
        };
        state
    }
}
