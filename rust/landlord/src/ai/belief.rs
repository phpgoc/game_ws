use std::collections::{BTreeMap, HashMap, HashSet};

use crate::core::play::{Combo, ComboKind, can_beat, card_rank, classify};

use super::{
    AiObservation, Relationship,
    candidates::{all_candidates, estimate_turns, estimate_turns_from_candidates},
};

const FIRST_RANK: u8 = 3;
const LAST_RANK: u8 = 17;
const BELIEF_WORLD_COUNT: usize = 24;
const PASS_CAN_BEAT_LIKELIHOOD: f64 = 0.18;
const NON_MINIMAL_RESPONSE_LIKELIHOOD: f64 = 0.58;
const UNNECESSARY_POWER_PLAY_LIKELIHOOD: f64 = 0.16;
const RECENT_BEHAVIOR_EVIDENCE_PER_PLAYER: usize = 12;

#[derive(Clone, Debug)]
struct WeightedHandSample {
    weight: f64,
    rank_counts: [u8; 18],
    combos: Vec<Combo>,
    estimated_turns: usize,
}

#[derive(Clone, Debug)]
pub(super) struct BeliefWorld {
    pub hands: BTreeMap<usize, Vec<i32>>,
    pub weight: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FarmerRunnerEstimate {
    pub position: usize,
    /// 0 表示两名农民几乎无法区分，1 表示所有加权牌局都支持同一个主跑。
    pub confidence: f64,
    pub expected_turns: f64,
}

#[derive(Clone, Debug)]
pub struct OpponentEstimate {
    pub position: usize,
    pub relationship: Relationship,
    pub hand_size: usize,
    pub certain_rank_counts: [u8; 18],
    pub expected_rank_counts: [f64; 18],
    probability_rank_counts: [[f64; 5]; 18],
    samples: Vec<WeightedHandSample>,
}

impl OpponentEstimate {
    pub fn probability_has_at_least(&self, rank: u8, count: usize) -> f64 {
        if !(FIRST_RANK..=LAST_RANK).contains(&rank) || count > 4 {
            return 0.0;
        }
        self.probability_rank_counts[rank as usize][count].clamp(0.0, 1.0)
    }

    pub fn probability_can_beat(&self, combo: &Combo) -> f64 {
        if combo.kind == ComboKind::Rocket {
            return 0.0;
        }
        if !self.samples.is_empty() {
            return self
                .samples
                .iter()
                .filter(|sample| {
                    sample
                        .combos
                        .iter()
                        .any(|candidate| can_beat(candidate, combo))
                })
                .map(|sample| sample.weight)
                .sum::<f64>()
                .clamp(0.0, 1.0);
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
        combine_probabilities(same_kind, bomb_or_rocket).clamp(0.0, 1.0)
    }

    pub fn probability_has_pair(&self) -> f64 {
        if self.samples.is_empty() {
            let mut none = 1.0;
            for rank in FIRST_RANK..=15 {
                none *= 1.0 - self.probability_has_at_least(rank, 2);
            }
            return (1.0 - none).clamp(0.0, 1.0);
        }
        self.probability_has_combo(|combo| combo.kind == ComboKind::Pair)
    }

    pub fn probability_has_straight(&self, minimum_len: usize) -> f64 {
        if minimum_len < 5 {
            return self.probability_has_straight(5);
        }
        let Ok(minimum_len_u8) = u8::try_from(minimum_len) else {
            return 0.0;
        };
        let Some(offset) = minimum_len_u8.checked_sub(1) else {
            return 0.0;
        };
        if self.samples.is_empty() {
            let mut none = 1.0;
            for start in FIRST_RANK..=14 {
                let Some(end) = start.checked_add(offset) else {
                    continue;
                };
                if end > 14 {
                    continue;
                }
                let probability = (start..=end).fold(1.0, |value, rank| {
                    value * self.probability_has_at_least(rank, 1)
                });
                none *= 1.0 - probability;
            }
            return (1.0 - none).clamp(0.0, 1.0);
        }
        self.probability_has_combo(|combo| {
            combo.kind == ComboKind::Straight && combo.sequence_len >= minimum_len
        })
    }

    pub fn probability_has_bomb_or_rocket(&self) -> f64 {
        if self.samples.is_empty() {
            return self.probability_bomb_or_rocket().clamp(0.0, 1.0);
        }
        self.probability_has_combo(|combo| {
            matches!(combo.kind, ComboKind::Bomb | ComboKind::Rocket)
        })
    }

    pub fn expected_turns_to_finish(&self) -> f64 {
        if self.samples.is_empty() {
            return self.hand_size as f64;
        }
        self.samples
            .iter()
            .map(|sample| sample.weight * sample.estimated_turns as f64)
            .sum()
    }

    fn runner_strength(&self) -> f64 {
        let high_card_strength = self.probability_has_at_least(17, 1) * 4.0
            + self.probability_has_at_least(16, 1) * 3.0
            + self.expected_rank_counts[15] * 0.8
            + self.expected_rank_counts[14] * 0.25;
        -self.expected_turns_to_finish() * 12.0 - self.hand_size as f64
            + high_card_strength
            + self.probability_has_bomb_or_rocket() * 3.0
    }

    fn probability_stronger_than(&self, strength: f64, wins_ties: bool) -> f64 {
        self.samples
            .iter()
            .filter(|sample| {
                let sampled = sampled_runner_strength(sample);
                sampled > strength || (wins_ties && sampled.total_cmp(&strength).is_eq())
            })
            .map(|sample| sample.weight)
            .sum::<f64>()
            .clamp(0.0, 1.0)
    }

    fn probability_has_combo(&self, predicate: impl Fn(&Combo) -> bool) -> f64 {
        self.samples
            .iter()
            .filter(|sample| sample.combos.iter().any(&predicate))
            .map(|sample| sample.weight)
            .sum::<f64>()
            .clamp(0.0, 1.0)
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
    pub(super) worlds: Vec<BeliefWorld>,
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
            opponents.insert(
                position,
                OpponentEstimate {
                    position,
                    relationship: observation.relationship_to(position),
                    hand_size,
                    certain_rank_counts,
                    expected_rank_counts,
                    probability_rank_counts,
                    samples: Vec::new(),
                },
            );
        }

        let mut worlds = sample_worlds(observation, &certain_cards, &played_ids);
        let total_weight = worlds.iter().map(|world| world.weight).sum::<f64>();
        if total_weight.is_finite() && total_weight > 0.0 {
            for world in &mut worlds {
                world.weight /= total_weight;
            }
        } else if !worlds.is_empty() {
            let uniform_weight = 1.0 / worlds.len() as f64;
            for world in &mut worlds {
                world.weight = uniform_weight;
            }
        }
        for world in &worlds {
            for (&position, hand) in &world.hands {
                let Some(estimate) = opponents.get_mut(&position) else {
                    continue;
                };
                let mut rank_counts = [0_u8; 18];
                for &card in hand {
                    rank_counts[card_rank(card) as usize] += 1;
                }
                let candidates = all_candidates(hand);
                estimate.samples.push(WeightedHandSample {
                    weight: world.weight,
                    rank_counts,
                    combos: candidates
                        .iter()
                        .map(|candidate| candidate.combo.clone())
                        .collect(),
                    estimated_turns: estimate_turns_from_candidates(hand, &candidates),
                });
            }
        }
        for estimate in opponents.values_mut() {
            if estimate.samples.is_empty() {
                continue;
            }
            estimate.expected_rank_counts = [0.0; 18];
            estimate.probability_rank_counts = [[0.0; 5]; 18];
            for rank in FIRST_RANK..=LAST_RANK {
                estimate.probability_rank_counts[rank as usize][0] = 1.0;
                for sample in &estimate.samples {
                    let count = sample.rank_counts[rank as usize] as usize;
                    estimate.expected_rank_counts[rank as usize] += sample.weight * count as f64;
                    for needed in 1..=4 {
                        if count >= needed {
                            estimate.probability_rank_counts[rank as usize][needed] +=
                                sample.weight;
                        }
                    }
                }
            }
        }

        Self {
            remaining_outside_hand,
            played_rank_counts,
            opponents,
            worlds,
        }
    }

    pub fn probability_enemies_can_beat(&self, combo: &Combo) -> f64 {
        let enemies = self
            .opponents
            .values()
            .filter(|estimate| estimate.relationship == Relationship::Enemy)
            .collect::<Vec<_>>();
        if let Some(sample_count) = enemies.first().map(|estimate| estimate.samples.len())
            && sample_count > 0
            && enemies
                .iter()
                .all(|estimate| estimate.samples.len() == sample_count)
        {
            return (0..sample_count)
                .filter(|sample_index| {
                    enemies.iter().any(|estimate| {
                        estimate.samples[*sample_index]
                            .combos
                            .iter()
                            .any(|candidate| can_beat(candidate, combo))
                    })
                })
                .map(|sample_index| enemies[0].samples[sample_index].weight)
                .sum::<f64>()
                .clamp(0.0, 1.0);
        }

        let mut none = 1.0;
        for estimate in enemies {
            none *= 1.0 - estimate.probability_can_beat(combo);
        }
        (1.0 - none).clamp(0.0, 1.0)
    }

    pub fn probability_enemies_can_finish_over(&self, combo: &Combo) -> f64 {
        let enemies = self
            .opponents
            .values()
            .filter(|estimate| estimate.relationship == Relationship::Enemy)
            .collect::<Vec<_>>();
        if let Some(sample_count) = enemies.first().map(|estimate| estimate.samples.len())
            && sample_count > 0
            && enemies
                .iter()
                .all(|estimate| estimate.samples.len() == sample_count)
        {
            return (0..sample_count)
                .filter(|sample_index| {
                    enemies.iter().any(|estimate| {
                        estimate.samples[*sample_index]
                            .combos
                            .iter()
                            .any(|candidate| {
                                combo_card_count(candidate) == estimate.hand_size
                                    && can_beat(candidate, combo)
                            })
                    })
                })
                .map(|sample_index| enemies[0].samples[sample_index].weight)
                .sum::<f64>()
                .clamp(0.0, 1.0);
        }

        let mut none = 1.0;
        for estimate in enemies {
            let probability = estimate
                .samples
                .iter()
                .filter(|sample| {
                    sample.combos.iter().any(|candidate| {
                        combo_card_count(candidate) == estimate.hand_size
                            && can_beat(candidate, combo)
                    })
                })
                .map(|sample| sample.weight)
                .sum::<f64>()
                .clamp(0.0, 1.0);
            none *= 1.0 - probability;
        }
        (1.0 - none).clamp(0.0, 1.0)
    }

    pub fn rank_is_control(&self, rank: u8) -> bool {
        ((rank + 1)..=LAST_RANK).all(|higher| self.remaining_outside_hand[higher as usize] == 0)
    }

    pub fn farmer_runner(&self, observation: &AiObservation) -> Option<FarmerRunnerEstimate> {
        let landlord = observation.landlord_position?;
        let farmers = observation
            .positions
            .iter()
            .copied()
            .filter(|position| *position != landlord)
            .collect::<Vec<_>>();
        let [left, right] = farmers.as_slice() else {
            return None;
        };

        let probability_left_runs = if observation.position == *left {
            let own_strength = known_runner_strength(&observation.hand);
            let right_estimate = self.opponents.get(right)?;
            1.0 - right_estimate.probability_stronger_than(own_strength, *right < *left)
        } else if observation.position == *right {
            let own_strength = known_runner_strength(&observation.hand);
            self.opponents
                .get(left)?
                .probability_stronger_than(own_strength, *left < *right)
        } else {
            let left_estimate = self.opponents.get(left)?;
            let right_estimate = self.opponents.get(right)?;
            if left_estimate.samples.len() != right_estimate.samples.len()
                || left_estimate.samples.is_empty()
            {
                if left_estimate.runner_strength() >= right_estimate.runner_strength() {
                    1.0
                } else {
                    0.0
                }
            } else {
                left_estimate
                    .samples
                    .iter()
                    .zip(&right_estimate.samples)
                    .filter(|(left_sample, right_sample)| {
                        let left_strength = sampled_runner_strength(left_sample);
                        let right_strength = sampled_runner_strength(right_sample);
                        left_strength > right_strength
                            || (*left < *right && left_strength.total_cmp(&right_strength).is_eq())
                    })
                    .map(|(left_sample, _)| left_sample.weight)
                    .sum::<f64>()
                    .clamp(0.0, 1.0)
            }
        };
        let (position, probability) = if probability_left_runs >= 0.5 {
            (*left, probability_left_runs)
        } else {
            (*right, 1.0 - probability_left_runs)
        };
        let expected_turns = if position == observation.position {
            estimate_turns(&observation.hand) as f64
        } else {
            self.opponents.get(&position)?.expected_turns_to_finish()
        };
        Some(FarmerRunnerEstimate {
            position,
            confidence: ((probability - 0.5) * 2.0).clamp(0.0, 1.0),
            expected_turns,
        })
    }

    pub fn farmer_runner_position(&self, observation: &AiObservation) -> Option<usize> {
        self.farmer_runner(observation)
            .map(|runner| runner.position)
    }
}

fn combo_card_count(combo: &Combo) -> usize {
    match combo.kind {
        ComboKind::Rocket | ComboKind::Pair => 2,
        ComboKind::Bomb | ComboKind::TripleSingle => 4,
        ComboKind::Single => 1,
        ComboKind::Triple => 3,
        ComboKind::TriplePair => 5,
        ComboKind::Straight => combo.sequence_len,
        ComboKind::StraightPairs => combo.sequence_len * 2,
        ComboKind::Plane => combo.sequence_len * 3,
        ComboKind::PlaneWithSingles => combo.sequence_len * 4,
        ComboKind::PlaneWithPairs => combo.sequence_len * 5,
        ComboKind::FourWithTwoSingles => 6,
        ComboKind::FourWithTwoPairs => 8,
    }
}

fn known_runner_strength(hand: &[i32]) -> f64 {
    let mut counts = [0_u8; 18];
    for &card in hand {
        counts[card_rank(card) as usize] += 1;
    }
    let high_card_strength = f64::from(counts[17]) * 4.0
        + f64::from(counts[16]) * 3.0
        + f64::from(counts[15]) * 0.8
        + f64::from(counts[14]) * 0.25;
    let has_rocket = counts[16] == 1 && counts[17] == 1;
    let bomb_count = (FIRST_RANK..=15)
        .filter(|rank| counts[*rank as usize] == 4)
        .count();
    -(estimate_turns(hand) as f64) * 12.0 - hand.len() as f64
        + high_card_strength
        + (bomb_count as f64 + f64::from(has_rocket)) * 3.0
}

fn sampled_runner_strength(sample: &WeightedHandSample) -> f64 {
    let high_card_strength = f64::from(sample.rank_counts[17]) * 4.0
        + f64::from(sample.rank_counts[16]) * 3.0
        + f64::from(sample.rank_counts[15]) * 0.8
        + f64::from(sample.rank_counts[14]) * 0.25;
    let has_rocket = sample.rank_counts[16] == 1 && sample.rank_counts[17] == 1;
    let bomb_count = (FIRST_RANK..=15)
        .filter(|rank| sample.rank_counts[*rank as usize] == 4)
        .count();
    -(sample.estimated_turns as f64) * 12.0
        - sample
            .rank_counts
            .iter()
            .map(|count| *count as usize)
            .sum::<usize>() as f64
        + high_card_strength
        + (bomb_count as f64 + f64::from(has_rocket)) * 3.0
}

fn sample_worlds(
    observation: &AiObservation,
    _certain_cards: &HashMap<usize, Vec<i32>>,
    played_ids: &HashSet<i32>,
) -> Vec<BeliefWorld> {
    let mut own_original = observation.hand.clone();
    own_original.extend(
        observation
            .play_history
            .iter()
            .filter(|record| record.position == observation.position)
            .flat_map(|record| record.cards.iter().copied()),
    );
    own_original.sort_unstable();

    let mut known_original = HashMap::<usize, Vec<i32>>::new();
    for record in &observation.play_history {
        if record.position != observation.position {
            known_original
                .entry(record.position)
                .or_default()
                .extend(&record.cards);
        }
    }
    if let Some(landlord) = observation.landlord_position
        && landlord != observation.position
    {
        known_original
            .entry(landlord)
            .or_default()
            .extend(&observation.hidden_cards);
    }
    for cards in known_original.values_mut() {
        cards.sort_unstable();
        cards.dedup();
    }

    let own_ids = own_original.iter().copied().collect::<HashSet<_>>();
    let certain_ids = known_original
        .values()
        .flatten()
        .copied()
        .collect::<HashSet<_>>();
    let unknown_cards = (1..=54)
        .filter(|card| !own_ids.contains(card) && !certain_ids.contains(card))
        .collect::<Vec<_>>();
    let opponents = observation
        .positions
        .iter()
        .copied()
        .filter(|position| *position != observation.position)
        .collect::<Vec<_>>();
    let original_hand_sizes = opponents
        .iter()
        .map(|position| {
            let played = observation
                .play_history
                .iter()
                .filter(|record| record.position == *position)
                .map(|record| record.cards.len())
                .sum::<usize>();
            (
                *position,
                observation.hand_sizes.get(position).copied().unwrap_or(0) + played,
            )
        })
        .collect::<HashMap<_, _>>();
    let required_unknown = opponents
        .iter()
        .map(|position| {
            original_hand_sizes[position]
                .saturating_sub(known_original.get(position).map(Vec::len).unwrap_or(0))
        })
        .sum::<usize>();
    if required_unknown > unknown_cards.len() {
        return Vec::new();
    }

    let seed = observation_seed(observation);
    let mut worlds = Vec::with_capacity(BELIEF_WORLD_COUNT);
    let mut combo_cache = HashMap::<u64, Vec<Combo>>::new();
    for sample_index in 0..BELIEF_WORLD_COUNT {
        let sample_seed = seed ^ (sample_index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let Some(hands) = sample_conditioned_hands(
            observation,
            &known_original,
            &original_hand_sizes,
            &unknown_cards,
            &opponents,
            played_ids,
            sample_seed,
        ) else {
            continue;
        };
        let weight = world_likelihood(observation, &hands, &mut combo_cache);
        worlds.push(BeliefWorld { hands, weight });
    }
    worlds
}

fn sample_conditioned_hands(
    observation: &AiObservation,
    known_original: &HashMap<usize, Vec<i32>>,
    original_hand_sizes: &HashMap<usize, usize>,
    unknown_cards: &[i32],
    opponents: &[usize],
    played_ids: &HashSet<i32>,
    seed: u64,
) -> Option<BTreeMap<usize, Vec<i32>>> {
    let mut pool = unknown_cards.to_vec();
    shuffle_cards(&mut pool, seed);

    let mut hands = BTreeMap::new();
    for &position in opponents {
        let mut hand = known_original.get(&position).cloned().unwrap_or_default();
        let original_size = original_hand_sizes.get(&position).copied().unwrap_or(0);
        let unknown_slots = original_size.saturating_sub(hand.len());
        if unknown_slots > pool.len() {
            return None;
        }
        hand.extend(pool.drain(..unknown_slots));
        hand.retain(|card| !played_ids.contains(card));
        hand.sort_unstable();
        if hand.len() != observation.hand_sizes.get(&position).copied().unwrap_or(0) {
            return None;
        }
        hands.insert(position, hand);
    }
    Some(hands)
}

fn world_likelihood(
    observation: &AiObservation,
    hands: &BTreeMap<usize, Vec<i32>>,
    combo_cache: &mut HashMap<u64, Vec<Combo>>,
) -> f64 {
    let mut weight = bidding_likelihood(observation, hands);
    let mut reconstructed_hands = hands.clone();
    let mut evidence_counts = HashMap::<usize, usize>::new();
    for record in observation.play_history.iter().rev() {
        let Some(hand_at_action) = reconstructed_hands.get_mut(&record.position) else {
            continue;
        };
        hand_at_action.extend(&record.cards);
        let evidence_count = evidence_counts.entry(record.position).or_default();
        if *evidence_count >= RECENT_BEHAVIOR_EVIDENCE_PER_PLAYER {
            continue;
        }
        *evidence_count += 1;
        let key = rank_count_key(hand_at_action);
        let combos = combo_cache.entry(key).or_insert_with(|| {
            all_candidates(hand_at_action)
                .into_iter()
                .map(|candidate| candidate.combo)
                .collect()
        });
        weight *= play_choice_likelihood(record, hand_at_action, combos);
    }
    weight
}

fn bidding_likelihood(observation: &AiObservation, hands: &BTreeMap<usize, Vec<i32>>) -> f64 {
    let mut weight = 1.0;
    let mut running_score = 0_u8;
    for (call_index, &(position, actual)) in observation.call_history.iter().enumerate() {
        let pressure = observation.call_history[..call_index]
            .iter()
            .filter(|(other, _)| *other != position)
            .map(|(_, score)| *score)
            .max()
            .unwrap_or(0) as f64
            * 0.35;
        if let Some(current_hand) = hands.get(&position) {
            let mut original_hand = current_hand.clone();
            original_hand.extend(
                observation
                    .play_history
                    .iter()
                    .filter(|record| record.position == position)
                    .flat_map(|record| record.cards.iter().copied()),
            );
            if observation.landlord_position == Some(position) {
                for bottom in &observation.hidden_cards {
                    if let Some(index) = original_hand.iter().position(|card| card == bottom) {
                        original_hand.remove(index);
                    }
                }
            }
            let desired = super::bidding::approximate_desired_bid(&original_hand, pressure);
            let predicted = if desired > running_score { desired } else { 0 };
            weight *= match predicted.abs_diff(actual) {
                0 => 1.0,
                1 => 0.58,
                2 => 0.24,
                _ => 0.10,
            };
        }
        running_score = running_score.max(actual);
    }
    weight
}

fn play_choice_likelihood(
    record: &crate::game_state::LandlordPlayRecord,
    hand_at_action: &[i32],
    combos: &[Combo],
) -> f64 {
    let benchmark = classify(&record.benchmark);
    if record.cards.is_empty() {
        return if benchmark.as_ref().is_some_and(|benchmark| {
            combos
                .iter()
                .any(|candidate| can_beat(candidate, benchmark))
        }) {
            PASS_CAN_BEAT_LIKELIHOOD
        } else {
            1.0
        };
    }

    let Some(played) = classify(&record.cards) else {
        return 1.0;
    };
    let mut likelihood = 1.0;
    let legal_responses = combos
        .iter()
        .filter(|candidate| {
            benchmark
                .as_ref()
                .is_none_or(|benchmark| can_beat(candidate, benchmark))
        })
        .collect::<Vec<_>>();
    let played_is_power = matches!(played.kind, ComboKind::Bomb | ComboKind::Rocket);
    if played_is_power
        && record.cards.len() < hand_at_action.len()
        && legal_responses
            .iter()
            .any(|candidate| !matches!(candidate.kind, ComboKind::Bomb | ComboKind::Rocket))
    {
        likelihood *= UNNECESSARY_POWER_PLAY_LIKELIHOOD;
    }
    if let Some(minimum_rank) = legal_responses
        .iter()
        .filter(|candidate| {
            candidate.kind == played.kind && candidate.sequence_len == played.sequence_len
        })
        .map(|candidate| candidate.main_rank)
        .min()
        && played.main_rank > minimum_rank
    {
        likelihood *=
            NON_MINIMAL_RESPONSE_LIKELIHOOD.powi(i32::from(played.main_rank - minimum_rank).min(4));
    }

    if played.kind == ComboKind::TripleSingle {
        let mut played_counts = [0_u8; 18];
        for &card in &record.cards {
            played_counts[card_rank(card) as usize] += 1;
        }
        let body_rank = played.main_rank;
        let kicker_rank = (FIRST_RANK..=LAST_RANK).find(|rank| played_counts[*rank as usize] == 1);
        let lowest_available = hand_at_action
            .iter()
            .map(|card| card_rank(*card))
            .filter(|rank| *rank != body_rank)
            .min();
        if let (Some(kicker), Some(lowest)) = (kicker_rank, lowest_available)
            && lowest < kicker
        {
            // 这是行为偏好，不是牌面事实；真人或更深搜索都可能有意保留小牌。
            likelihood *= NON_MINIMAL_RESPONSE_LIKELIHOOD;
        }
    }
    likelihood
}

fn rank_count_key(hand: &[i32]) -> u64 {
    let mut counts = [0_u8; 18];
    for &card in hand {
        counts[card_rank(card) as usize] += 1;
    }
    (FIRST_RANK..=LAST_RANK).fold(0_u64, |key, rank| {
        key * 5 + u64::from(counts[rank as usize])
    })
}

fn observation_seed(observation: &AiObservation) -> u64 {
    let mut seed = 0xD1B5_4A32_D192_ED03_u64;
    let mut mix = |value: u64| {
        seed ^= value
            .wrapping_add(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(seed << 6)
            .wrapping_add(seed >> 2);
    };
    mix(observation.position as u64);
    let mut original_hand = observation.hand.clone();
    original_hand.extend(
        observation
            .play_history
            .iter()
            .filter(|record| record.position == observation.position)
            .flat_map(|record| record.cards.iter().copied()),
    );
    original_hand.sort_unstable();
    for card in original_hand {
        mix(card as u64);
    }
    for &position in &observation.positions {
        let played = observation
            .play_history
            .iter()
            .filter(|record| record.position == position)
            .map(|record| record.cards.len())
            .sum::<usize>();
        let original_size = observation.hand_sizes.get(&position).copied().unwrap_or(0) + played;
        mix(((position as u64) << 32) | original_size as u64);
    }
    for &card in &observation.hidden_cards {
        mix(card as u64);
    }
    for &(position, score) in &observation.call_history {
        mix(((position as u64) << 8) | u64::from(score));
    }
    seed
}

fn shuffle_cards(cards: &mut [i32], state: u64) {
    cards.sort_unstable_by_key(|card| {
        let mut value = state ^ (*card as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        (value ^ (value >> 31), *card)
    });
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

    fn sampled_estimate(samples: &[(Vec<i32>, f64)]) -> OpponentEstimate {
        let samples = samples
            .iter()
            .map(|(hand, weight)| {
                let mut rank_counts = [0_u8; 18];
                for &card in hand {
                    rank_counts[card_rank(card) as usize] += 1;
                }
                WeightedHandSample {
                    weight: *weight,
                    rank_counts,
                    combos: all_candidates(hand)
                        .into_iter()
                        .map(|candidate| candidate.combo)
                        .collect(),
                    estimated_turns: estimate_turns(hand),
                }
            })
            .collect();
        OpponentEstimate {
            position: 1,
            relationship: Relationship::Enemy,
            hand_size: 5,
            certain_rank_counts: [0; 18],
            expected_rank_counts: [0.0; 18],
            probability_rank_counts: [[0.0; 5]; 18],
            samples,
        }
    }

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

    #[test]
    fn joint_samples_detect_pairs_and_straights() {
        let estimate = sampled_estimate(&[(vec![1, 14], 0.4), (vec![1, 2, 3, 4, 5], 0.6)]);

        assert!((estimate.probability_has_pair() - 0.4).abs() < 1e-10);
        assert!((estimate.probability_has_straight(5) - 0.6).abs() < 1e-10);
    }

    #[test]
    fn response_probability_requires_a_complete_legal_attachment() {
        let estimate = sampled_estimate(&[(vec![2, 15, 28], 0.45), (vec![2, 15, 28, 3], 0.55)]);
        let triple_single = classify(&[1, 14, 27, 2]).expect("triple with single");

        assert!((estimate.probability_can_beat(&triple_single) - 0.55).abs() < 1e-10);
    }

    #[test]
    fn triple_single_kicker_is_soft_behavior_evidence_not_a_hard_fact() {
        let record = LandlordPlayRecord {
            position: 1,
            cards: vec![1, 14, 27, 6], // 333 带 8
            benchmark: Vec::new(),
        };
        let with_lower = vec![1, 14, 27, 6, 2];
        let without_lower = vec![1, 14, 27, 6, 9];
        let with_lower_combos = all_candidates(&with_lower)
            .into_iter()
            .map(|candidate| candidate.combo)
            .collect::<Vec<_>>();
        let without_lower_combos = all_candidates(&without_lower)
            .into_iter()
            .map(|candidate| candidate.combo)
            .collect::<Vec<_>>();

        let lower_likelihood = play_choice_likelihood(&record, &with_lower, &with_lower_combos);
        let clean_likelihood =
            play_choice_likelihood(&record, &without_lower, &without_lower_combos);
        assert!(lower_likelihood > 0.0);
        assert!(lower_likelihood < clean_likelihood);
    }

    #[test]
    fn a_three_point_bid_weights_strong_hidden_hands_more_highly() {
        let mut state = state_with_hands(&[
            (0, vec![1]),
            (1, vec![2]),
            (
                2,
                vec![3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 20, 21, 22, 23, 24, 25, 26],
            ),
        ]);
        state.phase = LandlordPhase::Play;
        state.landlord_position = Some(0);
        state.call_history = vec![(0, 3), (1, 0), (2, 0)];
        let observation = AiObservation::from_state(&state, 2).expect("observation");
        let strong = vec![
            53, 54, 13, 26, 39, 52, 1, 14, 27, 40, 2, 15, 3, 16, 4, 17, 5,
        ];
        let weak = vec![1, 2, 3, 4, 5, 6, 14, 16, 18, 20, 28, 30, 32, 34, 42, 44, 46];
        let filler = vec![
            7, 8, 9, 10, 11, 12, 19, 21, 22, 23, 24, 25, 29, 31, 33, 35, 37,
        ];

        let strong_world = BTreeMap::from([(0, strong), (1, filler.clone())]);
        let weak_world = BTreeMap::from([(0, weak), (1, filler)]);
        assert!(
            bidding_likelihood(&observation, &strong_world)
                > bidding_likelihood(&observation, &weak_world)
        );
    }

    #[test]
    fn identical_public_observations_produce_identical_worlds() {
        let mut state =
            state_with_hands(&[(0, vec![1, 2, 3]), (1, vec![4, 5, 6]), (2, vec![7, 8, 9])]);
        state.phase = LandlordPhase::Play;
        state.landlord_position = Some(0);
        let observation = AiObservation::from_state(&state, 0).expect("observation");
        let left = CardBelief::from_observation(&observation);
        let right = CardBelief::from_observation(&observation);

        assert_eq!(left.worlds.len(), right.worlds.len());
        for (left, right) in left.worlds.iter().zip(&right.worlds) {
            assert_eq!(left.hands, right.hands);
            assert!((left.weight - right.weight).abs() < f64::EPSILON);
        }
    }

    #[test]
    fn farmers_estimate_their_teammate_and_agree_on_clear_runner_cases() {
        let mut self_runs = state_with_hands(&[
            (0, vec![1, 2, 3, 4, 5]),
            (1, vec![53]),
            (2, vec![6, 7, 8, 9, 10, 11, 12, 13]),
        ]);
        self_runs.phase = LandlordPhase::Play;
        self_runs.landlord_position = Some(0);
        let observation = AiObservation::from_state(&self_runs, 1).expect("observation");
        let belief = CardBelief::from_observation(&observation);
        assert_eq!(belief.opponents[&2].relationship, Relationship::Ally);
        assert_eq!(belief.farmer_runner_position(&observation), Some(1));

        let mut ally_runs = state_with_hands(&[
            (0, vec![1, 2, 3, 4, 5]),
            (1, vec![6, 7, 8, 9, 10, 11, 12, 13]),
            (2, vec![53]),
        ]);
        ally_runs.phase = LandlordPhase::Play;
        ally_runs.landlord_position = Some(0);
        let observation = AiObservation::from_state(&ally_runs, 1).expect("observation");
        let belief = CardBelief::from_observation(&observation);
        assert_eq!(belief.farmer_runner_position(&observation), Some(2));
    }

    #[test]
    fn landlord_also_identifies_the_more_urgent_farmer_runner() {
        let mut state = state_with_hands(&[
            (0, vec![1, 2, 3, 4, 5, 6]),
            (1, vec![53]),
            (2, vec![7, 8, 9, 10, 11, 12, 13, 14]),
        ]);
        state.phase = LandlordPhase::Play;
        state.landlord_position = Some(0);
        let observation = AiObservation::from_state(&state, 0).expect("observation");
        let runner = CardBelief::from_observation(&observation)
            .farmer_runner(&observation)
            .expect("runner estimate");

        assert_eq!(runner.position, 1);
        assert!(runner.confidence > 0.9);
        assert!((runner.expected_turns - 1.0).abs() < 1e-9);
    }
}
