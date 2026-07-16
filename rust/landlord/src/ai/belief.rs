use std::collections::{BTreeMap, HashMap, HashSet};

use crate::core::play::{Combo, ComboKind, can_beat, card_rank, classify};

use super::{
    AiObservation, Relationship,
    candidates::{all_candidates, estimate_turns, estimate_turns_from_candidates},
};

const FIRST_RANK: u8 = 3;
const LAST_RANK: u8 = 17;
const BELIEF_WORLD_COUNT: usize = 48;
const PASS_CAN_BEAT_LIKELIHOOD: f64 = 0.18;

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

    pub fn rank_is_control(&self, rank: u8) -> bool {
        ((rank + 1)..=LAST_RANK).all(|higher| self.remaining_outside_hand[higher as usize] == 0)
    }

    pub fn farmer_runner_position(&self, observation: &AiObservation) -> Option<usize> {
        let landlord = observation.landlord_position?;
        if observation.position == landlord {
            return None;
        }
        let ally = observation
            .positions
            .iter()
            .copied()
            .find(|position| *position != landlord && *position != observation.position)?;
        let own_strength = known_runner_strength(&observation.hand);
        let ally_strength = self.opponents.get(&ally)?.runner_strength();
        if own_strength > ally_strength
            || (own_strength.total_cmp(&ally_strength).is_eq() && observation.position < ally)
        {
            Some(observation.position)
        } else {
            Some(ally)
        }
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

fn sample_worlds(
    observation: &AiObservation,
    certain_cards: &HashMap<usize, Vec<i32>>,
    played_ids: &HashSet<i32>,
) -> Vec<BeliefWorld> {
    let own_ids = observation.hand.iter().copied().collect::<HashSet<_>>();
    let certain_ids = certain_cards
        .values()
        .flatten()
        .copied()
        .collect::<HashSet<_>>();
    let unknown_cards = (1..=54)
        .filter(|card| {
            !own_ids.contains(card) && !played_ids.contains(card) && !certain_ids.contains(card)
        })
        .collect::<Vec<_>>();
    let opponents = observation
        .positions
        .iter()
        .copied()
        .filter(|position| *position != observation.position)
        .collect::<Vec<_>>();
    let required_unknown = opponents
        .iter()
        .map(|position| {
            observation
                .hand_sizes
                .get(position)
                .copied()
                .unwrap_or(0)
                .saturating_sub(certain_cards.get(position).map(Vec::len).unwrap_or(0))
        })
        .sum::<usize>();
    if required_unknown > unknown_cards.len() {
        return Vec::new();
    }

    let minimum_rank_evidence = triple_single_minimum_rank_evidence(observation, certain_cards);
    let seed = observation_seed(observation);
    let mut worlds = Vec::with_capacity(BELIEF_WORLD_COUNT);
    let mut combo_cache = HashMap::<u64, Vec<Combo>>::new();
    for sample_index in 0..BELIEF_WORLD_COUNT {
        let sample_seed = seed ^ (sample_index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let Some(hands) = sample_conditioned_hands(
            observation,
            certain_cards,
            &unknown_cards,
            &opponents,
            &minimum_rank_evidence,
            sample_seed,
        ) else {
            continue;
        };
        let weight = world_likelihood(
            observation,
            &hands,
            &minimum_rank_evidence,
            &mut combo_cache,
        );
        worlds.push(BeliefWorld { hands, weight });
    }
    worlds
}

fn sample_conditioned_hands(
    observation: &AiObservation,
    certain_cards: &HashMap<usize, Vec<i32>>,
    unknown_cards: &[i32],
    opponents: &[usize],
    minimum_rank_evidence: &HashMap<usize, u8>,
    seed: u64,
) -> Option<BTreeMap<usize, Vec<i32>>> {
    let mut pool = unknown_cards.to_vec();
    shuffle_cards(&mut pool, seed);

    // 所有下界都是“牌点至少为 N”的嵌套约束。先给下界最高的玩家发牌，
    // 可以在约束可满足时避免普通拒绝采样把整批样本全部浪费掉。
    let mut allocation_order = opponents.to_vec();
    allocation_order.sort_by(|left, right| {
        minimum_rank_evidence
            .get(right)
            .copied()
            .unwrap_or(FIRST_RANK)
            .cmp(
                &minimum_rank_evidence
                    .get(left)
                    .copied()
                    .unwrap_or(FIRST_RANK),
            )
            .then(left.cmp(right))
    });

    let mut hands = BTreeMap::new();
    for position in allocation_order {
        let minimum_rank = minimum_rank_evidence
            .get(&position)
            .copied()
            .unwrap_or(FIRST_RANK);
        let mut hand = certain_cards.get(&position).cloned().unwrap_or_default();
        if hand.iter().any(|card| card_rank(*card) < minimum_rank) {
            return None;
        }
        let hand_size = observation.hand_sizes.get(&position).copied().unwrap_or(0);
        let unknown_slots = hand_size.saturating_sub(hand.len());
        let mut selected = Vec::with_capacity(unknown_slots);
        let mut remaining = Vec::with_capacity(pool.len().saturating_sub(unknown_slots));
        for card in pool {
            if selected.len() < unknown_slots && card_rank(card) >= minimum_rank {
                selected.push(card);
            } else {
                remaining.push(card);
            }
        }
        if selected.len() != unknown_slots {
            return None;
        }
        pool = remaining;
        hand.extend(selected);
        hand.sort_unstable();
        hands.insert(position, hand);
    }
    Some(hands)
}

fn world_likelihood(
    observation: &AiObservation,
    hands: &BTreeMap<usize, Vec<i32>>,
    minimum_rank_evidence: &HashMap<usize, u8>,
    combo_cache: &mut HashMap<u64, Vec<Combo>>,
) -> f64 {
    for (&position, &minimum_rank) in minimum_rank_evidence {
        if hands
            .get(&position)
            .is_some_and(|hand| hand.iter().any(|card| card_rank(*card) < minimum_rank))
        {
            return 0.0;
        }
    }

    let mut weight = 1.0;
    for (record_index, record) in observation.play_history.iter().enumerate() {
        if !record.cards.is_empty() {
            continue;
        }
        let Some(benchmark) = classify(&record.benchmark) else {
            continue;
        };
        let Some(current_hand) = hands.get(&record.position) else {
            continue;
        };
        let mut hand_at_pass = current_hand.clone();
        hand_at_pass.extend(
            observation.play_history[record_index + 1..]
                .iter()
                .filter(|later| later.position == record.position)
                .flat_map(|later| later.cards.iter().copied()),
        );
        let key = rank_count_key(&hand_at_pass);
        let combos = combo_cache.entry(key).or_insert_with(|| {
            all_candidates(&hand_at_pass)
                .into_iter()
                .map(|candidate| candidate.combo)
                .collect()
        });
        if combos
            .iter()
            .any(|candidate| can_beat(candidate, &benchmark))
        {
            weight *= PASS_CAN_BEAT_LIKELIHOOD;
        }
    }
    weight
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

fn triple_single_minimum_rank_evidence(
    observation: &AiObservation,
    certain_cards: &HashMap<usize, Vec<i32>>,
) -> HashMap<usize, u8> {
    let mut evidence = HashMap::<usize, u8>::new();
    for (record_index, record) in observation.play_history.iter().enumerate() {
        if record.cards.is_empty() {
            continue;
        }
        let Some(combo) = classify(&record.cards) else {
            continue;
        };
        if combo.kind != ComboKind::TripleSingle {
            continue;
        }
        let mut counts = HashMap::<u8, usize>::new();
        for &card in &record.cards {
            *counts.entry(card_rank(card)).or_default() += 1;
        }
        let Some(kicker_rank) = counts
            .into_iter()
            .find_map(|(rank, count)| (count == 1).then_some(rank))
        else {
            continue;
        };
        let later_lower_card_was_played = observation.play_history[record_index + 1..]
            .iter()
            .filter(|later| later.position == record.position)
            .flat_map(|later| later.cards.iter())
            .any(|card| card_rank(*card) < kicker_rank);
        let certain_current_lower_card_exists = certain_cards
            .get(&record.position)
            .is_some_and(|cards| cards.iter().any(|card| card_rank(*card) < kicker_rank));
        if later_lower_card_was_played || certain_current_lower_card_exists {
            // 公开事实已经证明该玩家这一次没有遵守“最小牌作带牌”的策略，
            // 不能让一条被反证的行为模型清空全部可能牌局。
            continue;
        }
        evidence
            .entry(record.position)
            .and_modify(|minimum| *minimum = (*minimum).max(kicker_rank))
            .or_insert(kicker_rank);
    }
    evidence
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
    for &card in &observation.hand {
        mix(card as u64);
    }
    for (&position, &size) in &observation.hand_sizes {
        mix(((position as u64) << 32) | size as u64);
    }
    for &card in &observation.hidden_cards {
        mix(card as u64);
    }
    for record in observation
        .play_history
        .iter()
        .filter(|record| !record.cards.is_empty())
    {
        mix(record.position as u64);
        for &card in &record.cards {
            mix(card as u64);
        }
    }
    seed
}

fn shuffle_cards(cards: &mut [i32], mut state: u64) {
    for index in (1..cards.len()).rev() {
        state = state
            .wrapping_add(0x9E37_79B9_7F4A_7C15)
            .wrapping_mul(0xBF58_476D_1CE4_E5B9);
        let offset = (state ^ (state >> 30)) as usize % (index + 1);
        cards.swap(index, offset);
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
    fn triple_single_kicker_is_a_hard_floor_for_remaining_cards() {
        let mut state = state_with_hands(&[
            (0, vec![10, 23, 36]),
            (1, vec![11, 24, 37]),
            (2, vec![12, 25, 38]),
        ]);
        state.phase = LandlordPhase::Play;
        state.landlord_position = Some(0);
        state.play_history.push(LandlordPlayRecord {
            position: 1,
            cards: vec![1, 14, 27, 6], // 333 带 8
            benchmark: Vec::new(),
        });
        let observation = AiObservation::from_state(&state, 0).expect("observation");
        let belief = CardBelief::from_observation(&observation);

        assert!(!belief.worlds.is_empty());
        assert!(
            belief
                .worlds
                .iter()
                .all(|world| { world.hands[&1].iter().all(|card| card_rank(*card) >= 8) })
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
}
