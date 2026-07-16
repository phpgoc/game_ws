use std::collections::{BTreeMap, HashMap, HashSet};

use crate::core::play::{Combo, ComboKind, card_rank, classify};

use super::{AiObservation, Relationship};

const FIRST_RANK: u8 = 3;
const LAST_RANK: u8 = 17;

#[derive(Clone, Debug)]
pub struct OpponentEstimate {
    pub position: usize,
    pub relationship: Relationship,
    pub hand_size: usize,
    pub certain_rank_counts: [u8; 18],
    pub expected_rank_counts: [f64; 18],
    probability_rank_counts: [[f64; 5]; 18],
    pass_constraints: Vec<Combo>,
}

impl OpponentEstimate {
    pub fn probability_has_at_least(&self, rank: u8, count: usize) -> f64 {
        if !(FIRST_RANK..=LAST_RANK).contains(&rank) || count > 4 {
            return 0.0;
        }
        self.probability_rank_counts[rank as usize][count]
    }

    pub fn probability_can_beat(&self, combo: &Combo) -> f64 {
        if combo.kind == ComboKind::Rocket {
            return 0.0;
        }
        let same_kind = match combo.kind {
            ComboKind::Single => self.probability_higher_group(combo.main_rank, 1),
            ComboKind::Pair => self.probability_higher_group(combo.main_rank, 2),
            ComboKind::Triple | ComboKind::TripleSingle | ComboKind::TriplePair => {
                self.probability_higher_group(combo.main_rank, 3)
            }
            ComboKind::Straight => self.probability_higher_sequence(combo, 1),
            ComboKind::StraightPairs => self.probability_higher_sequence(combo, 2),
            ComboKind::Plane | ComboKind::PlaneWithSingles | ComboKind::PlaneWithPairs => {
                self.probability_higher_sequence(combo, 3)
            }
            ComboKind::FourWithTwoSingles | ComboKind::FourWithTwoPairs => {
                self.probability_higher_group(combo.main_rank, 4)
            }
            ComboKind::Bomb => self.probability_higher_group(combo.main_rank, 4),
            ComboKind::Rocket => 0.0,
        };
        let bomb_or_rocket = if combo.kind == ComboKind::Bomb {
            self.probability_rocket()
        } else {
            self.probability_bomb_or_rocket()
        };
        let mut probability = combine_probabilities(same_kind, bomb_or_rocket);
        if self.pass_constraints.iter().any(|constraint| {
            constraint.kind == combo.kind
                && constraint.sequence_len == combo.sequence_len
                && constraint.main_rank <= combo.main_rank
        }) {
            // 不出不等于绝对没有，但它是很强的负面证据。
            probability *= 0.3;
        }
        probability.clamp(0.0, 1.0)
    }

    fn probability_higher_group(&self, main_rank: u8, count: usize) -> f64 {
        let mut none = 1.0;
        for rank in (main_rank + 1)..=LAST_RANK {
            none *= 1.0 - self.probability_has_at_least(rank, count);
        }
        1.0 - none
    }

    fn probability_higher_sequence(&self, combo: &Combo, copies: usize) -> f64 {
        if combo.sequence_len == 0 {
            return 0.0;
        }
        let maximum_main = 14_u8;
        let mut none = 1.0;
        for main_rank in (combo.main_rank + 1)..=maximum_main {
            let Some(start) = main_rank.checked_sub(combo.sequence_len as u8 - 1) else {
                continue;
            };
            if start < FIRST_RANK {
                continue;
            }
            let probability = (start..=main_rank).fold(1.0, |value, rank| {
                value * self.probability_has_at_least(rank, copies)
            });
            none *= 1.0 - probability;
        }
        1.0 - none
    }

    fn probability_bomb_or_rocket(&self) -> f64 {
        let mut none = 1.0;
        for rank in FIRST_RANK..=15 {
            let probability = self.probability_has_at_least(rank, 4);
            none *= 1.0 - probability;
        }
        combine_probabilities(1.0 - none, self.probability_rocket())
    }

    fn probability_rocket(&self) -> f64 {
        self.probability_has_at_least(16, 1) * self.probability_has_at_least(17, 1)
    }
}

#[derive(Clone, Debug)]
pub struct CardBelief {
    pub remaining_outside_hand: [u8; 18],
    pub played_rank_counts: [u8; 18],
    pub opponents: BTreeMap<usize, OpponentEstimate>,
}

impl CardBelief {
    pub fn from_observation(observation: &AiObservation) -> Self {
        let played_cards = observation
            .play_history
            .iter()
            .flat_map(|record| record.cards.iter().copied())
            .collect::<Vec<_>>();
        let played_ids = played_cards.iter().copied().collect::<HashSet<_>>();
        let mut played_rank_counts = [0_u8; 18];
        for &card in &played_cards {
            played_rank_counts[card_rank(card) as usize] += 1;
        }

        let mut remaining_outside_hand = full_rank_counts();
        for &card in observation.hand.iter().chain(played_cards.iter()) {
            remaining_outside_hand[card_rank(card) as usize] =
                remaining_outside_hand[card_rank(card) as usize].saturating_sub(1);
        }

        let mut certain_cards = HashMap::<usize, Vec<i32>>::new();
        if let Some(landlord) = observation.landlord_position
            && landlord != observation.position
        {
            certain_cards.insert(
                landlord,
                observation
                    .hidden_cards
                    .iter()
                    .copied()
                    .filter(|card| !played_ids.contains(card))
                    .collect(),
            );
        }

        let mut unknown_rank_counts = remaining_outside_hand;
        for cards in certain_cards.values() {
            for &card in cards {
                unknown_rank_counts[card_rank(card) as usize] =
                    unknown_rank_counts[card_rank(card) as usize].saturating_sub(1);
            }
        }
        let total_unknown = unknown_rank_counts
            .iter()
            .map(|count| *count as usize)
            .sum();

        let mut opponents = BTreeMap::new();
        for &position in &observation.positions {
            if position == observation.position {
                continue;
            }
            let known = certain_cards.get(&position).cloned().unwrap_or_default();
            let mut certain_rank_counts = [0_u8; 18];
            for card in &known {
                certain_rank_counts[card_rank(*card) as usize] += 1;
            }
            let hand_size = observation.hand_sizes.get(&position).copied().unwrap_or(0);
            let unknown_slots = hand_size.saturating_sub(known.len());
            let mut expected_rank_counts = [0.0_f64; 18];
            let mut probability_rank_counts = [[0.0_f64; 5]; 18];
            for rank in FIRST_RANK..=LAST_RANK {
                let known_count = certain_rank_counts[rank as usize] as usize;
                let available = unknown_rank_counts[rank as usize] as usize;
                expected_rank_counts[rank as usize] = known_count as f64
                    + if total_unknown == 0 {
                        0.0
                    } else {
                        available as f64 * unknown_slots as f64 / total_unknown as f64
                    };
                probability_rank_counts[rank as usize][0] = 1.0;
                for (needed, probability) in probability_rank_counts[rank as usize]
                    .iter_mut()
                    .enumerate()
                    .skip(1)
                {
                    *probability = if known_count >= needed {
                        1.0
                    } else {
                        hypergeometric_at_least(
                            total_unknown,
                            available,
                            unknown_slots,
                            needed - known_count,
                        )
                    };
                }
            }
            let pass_constraints = observation
                .play_history
                .iter()
                .filter(|record| record.position == position && record.cards.is_empty())
                .filter_map(|record| classify(&record.benchmark))
                .collect();
            opponents.insert(
                position,
                OpponentEstimate {
                    position,
                    relationship: observation.relationship_to(position),
                    hand_size,
                    certain_rank_counts,
                    expected_rank_counts,
                    probability_rank_counts,
                    pass_constraints,
                },
            );
        }

        Self {
            remaining_outside_hand,
            played_rank_counts,
            opponents,
        }
    }

    pub fn probability_enemies_can_beat(&self, combo: &Combo) -> f64 {
        let mut none = 1.0;
        for estimate in self
            .opponents
            .values()
            .filter(|estimate| estimate.relationship == Relationship::Enemy)
        {
            none *= 1.0 - estimate.probability_can_beat(combo);
        }
        1.0 - none
    }

    pub fn rank_is_control(&self, rank: u8) -> bool {
        ((rank + 1)..=LAST_RANK).all(|higher| self.remaining_outside_hand[higher as usize] == 0)
    }
}

fn full_rank_counts() -> [u8; 18] {
    let mut counts = [0_u8; 18];
    for rank in FIRST_RANK..=15 {
        counts[rank as usize] = 4;
    }
    counts[16] = 1;
    counts[17] = 1;
    counts
}

fn hypergeometric_at_least(
    population: usize,
    successes: usize,
    draws: usize,
    needed: usize,
) -> f64 {
    if needed == 0 {
        return 1.0;
    }
    if successes < needed || draws < needed || population == 0 {
        return 0.0;
    }
    let maximum = successes.min(draws);
    let denominator = combination(population, draws);
    if denominator == 0.0 {
        return 0.0;
    }
    ((needed..=maximum)
        .map(|hits| {
            combination(successes, hits) * combination(population - successes, draws - hits)
        })
        .sum::<f64>()
        / denominator)
        .clamp(0.0, 1.0)
}

fn combination(total: usize, selected: usize) -> f64 {
    if selected > total {
        return 0.0;
    }
    let selected = selected.min(total - selected);
    (0..selected).fold(1.0, |value, index| {
        value * (total - index) as f64 / (index + 1) as f64
    })
}

fn combine_probabilities(left: f64, right: f64) -> f64 {
    1.0 - (1.0 - left) * (1.0 - right)
}

#[cfg(test)]
mod tests {
    use share_type_public::LandlordPhase;

    use crate::{ai::tests::state_with_hands, game_state::LandlordPlayRecord};

    use super::*;

    #[test]
    fn public_bottom_cards_are_certain_landlord_cards() {
        let mut state =
            state_with_hands(&[(0, vec![1, 2, 3]), (1, vec![4, 5, 6]), (2, vec![7, 8, 9])]);
        state.phase = LandlordPhase::Play;
        state.landlord_position = Some(0);
        state.hidden_cards = vec![53, 54, 13];
        let observation = AiObservation::from_state(&state, 1).expect("observation");
        let belief = CardBelief::from_observation(&observation);
        let landlord = &belief.opponents[&0];

        assert_eq!(landlord.certain_rank_counts[16], 1);
        assert_eq!(landlord.certain_rank_counts[17], 1);
        assert_eq!(landlord.certain_rank_counts[15], 1);
    }

    #[test]
    fn a_public_play_updates_rank_memory() {
        let mut state = state_with_hands(&[(0, vec![1, 2]), (1, vec![3, 4]), (2, vec![5, 6])]);
        state.phase = LandlordPhase::Play;
        state.landlord_position = Some(0);
        state.play_history.push(LandlordPlayRecord {
            position: 0,
            cards: vec![53],
            benchmark: Vec::new(),
        });
        let observation = AiObservation::from_state(&state, 1).expect("observation");
        let belief = CardBelief::from_observation(&observation);

        assert_eq!(belief.played_rank_counts[16], 1);
        assert_eq!(belief.remaining_outside_hand[16], 0);
    }

    #[test]
    fn passing_is_negative_evidence_for_beating_the_same_combo() {
        let mut state =
            state_with_hands(&[(0, vec![1, 2, 3]), (1, vec![4, 5, 6]), (2, vec![7, 8, 9])]);
        state.phase = LandlordPhase::Play;
        state.landlord_position = Some(0);
        let observation_without_pass = AiObservation::from_state(&state, 1).expect("observation");
        let combo = classify(&[8]).expect("single");
        let before = CardBelief::from_observation(&observation_without_pass).opponents[&2]
            .probability_can_beat(&combo);

        state.play_history.push(LandlordPlayRecord {
            position: 2,
            cards: Vec::new(),
            benchmark: vec![8],
        });
        let observation_with_pass = AiObservation::from_state(&state, 1).expect("observation");
        let after = CardBelief::from_observation(&observation_with_pass).opponents[&2]
            .probability_can_beat(&combo);

        assert!(after < before);
    }
}
