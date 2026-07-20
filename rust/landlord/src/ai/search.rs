use std::{cmp::Reverse, collections::HashMap};

use crate::core::play::{Combo, ComboKind, can_beat, card_rank, classify};

use super::{
    AiObservation, CardBelief,
    candidates::{
        Candidate, all_candidates, attachment_cost, estimate_turns, power_structure_cost,
    },
};

const SEARCH_CARD_LIMIT: usize = 14;
const ROLLOUT_CARD_LIMIT: usize = 30;
const SEARCH_WORLD_LIMIT: usize = 4;
const SEARCH_NODE_BUDGET_PER_ACTION: usize = 8_000;

pub(super) fn choose_endgame_play(
    observation: &AiObservation,
    belief: &CardBelief,
    candidates: &[Candidate],
    leading: bool,
) -> Option<Vec<i32>> {
    let total_cards = observation.hand_sizes.values().sum::<usize>();
    if total_cards > ROLLOUT_CARD_LIMIT || belief.worlds.is_empty() {
        return None;
    }

    let benchmark = if leading {
        None
    } else {
        classify(&observation.last_play)
    };
    if !leading && benchmark.is_none() {
        return None;
    }

    let mut actions = candidates.iter().cloned().map(Some).collect::<Vec<_>>();
    if !leading {
        actions.push(None);
    }
    if actions.is_empty() {
        return None;
    }

    let worlds = distinct_search_worlds(observation, belief);
    if worlds.is_empty() {
        return None;
    }

    let root_is_landlord = observation.landlord_position == Some(observation.position);
    let mut action_scores = vec![0.0; actions.len()];
    let total_weight = worlds.iter().map(|world| world.weight).sum::<f64>();
    if total_weight <= 0.0 {
        return None;
    }

    for world in worlds {
        let state = SearchState::from_world(observation, &world.hands, benchmark.clone())?;
        let mut solver = (total_cards <= SEARCH_CARD_LIMIT).then(|| Solver::new(root_is_landlord));
        for (action_index, action) in actions.iter().enumerate() {
            let Some(next) = state.after_action(action.as_ref()) else {
                action_scores[action_index] = f64::NEG_INFINITY;
                continue;
            };
            let value = if let Some(solver) = &mut solver {
                solver.nodes = 0;
                solver.solve(&next, f64::NEG_INFINITY, f64::INFINITY).value
            } else {
                rollout_value(next, root_is_landlord)
            };
            action_scores[action_index] += world.weight * value;
        }
    }

    let action_tiebreaks = actions
        .iter()
        .map(|action| {
            root_action_tiebreak(
                &observation.hand,
                action.as_ref(),
                leading,
                root_is_landlord,
            )
        })
        .collect::<Vec<_>>();
    let best_index = (0..actions.len()).max_by(|left, right| {
        action_scores[*left]
            .total_cmp(&action_scores[*right])
            .then_with(|| action_tiebreaks[*left].cmp(&action_tiebreaks[*right]))
            // 估值和结构代价完全相同时保持候选生成顺序，避免随机抖动。
            .then_with(|| right.cmp(left))
    })?;
    action_scores[best_index].is_finite().then(|| {
        actions[best_index]
            .as_ref()
            .map(|candidate| candidate.cards.clone())
            .unwrap_or_default()
    })
}

type RootActionTiebreak = (
    Reverse<usize>,
    Reverse<u32>,
    Reverse<u8>,
    usize,
    Reverse<u32>,
    u8,
    Reverse<u8>,
);

fn root_action_tiebreak(
    hand: &[i32],
    action: Option<&Candidate>,
    leading: bool,
    root_is_landlord: bool,
) -> RootActionTiebreak {
    let mut remaining = hand.to_vec();
    let Some(candidate) = action else {
        return (
            Reverse(estimate_turns(&remaining)),
            Reverse(0),
            Reverse(0),
            0,
            Reverse(0),
            0,
            Reverse(0),
        );
    };
    for card in &candidate.cards {
        if let Some(index) = remaining.iter().position(|held| held == card) {
            remaining.remove(index);
        }
    }
    let control_cost = candidate
        .cards
        .iter()
        .map(|card| match card_rank(*card) {
            17 => 8,
            16 => 6,
            15 => 3,
            14 => 1,
            _ => 0,
        })
        .sum::<u32>()
        + power_structure_cost(hand, candidate);
    let mut rank_counts = [0_u8; 18];
    for card in hand {
        rank_counts[card_rank(*card) as usize] += 1;
    }
    let singleton_lead = leading
        && root_is_landlord
        && candidate.combo.kind == ComboKind::Single
        && hand.len() == 5
        && rank_counts.iter().all(|count| *count <= 1)
        // K、A、2、王至少三张时，才有足够的控制阶梯支撑顶高首出。
        && rank_counts[13..=17].iter().sum::<u8>() >= 3;
    let power_cost = u8::from(matches!(
        candidate.combo.kind,
        ComboKind::Bomb | ComboKind::Rocket
    ));
    (
        Reverse(estimate_turns(&remaining)),
        Reverse(control_cost),
        Reverse(power_cost),
        candidate.cards.len(),
        Reverse(attachment_cost(candidate)),
        // 地主等值首出顶高一些以迫使农民交控制牌；农民喂牌和跟牌仍优先用低牌。
        if singleton_lead {
            candidate.combo.main_rank
        } else {
            0
        },
        Reverse(if leading {
            0
        } else {
            candidate.combo.main_rank
        }),
    )
}

fn rollout_value(mut state: SearchState, root_is_landlord: bool) -> f64 {
    for _ in 0..120 {
        if let Some(value) = state.terminal_value(root_is_landlord) {
            return value;
        }
        let action = state.rollout_action();
        let Some(next) = state.after_action(action.as_ref()) else {
            break;
        };
        state = next;
    }
    state.heuristic_value(root_is_landlord)
}

#[derive(Clone)]
struct SearchWorld {
    hands: Vec<Vec<i32>>,
    signature: Vec<u64>,
    weight: f64,
}

fn distinct_search_worlds(observation: &AiObservation, belief: &CardBelief) -> Vec<SearchWorld> {
    let mut worlds = Vec::<SearchWorld>::new();
    for world in belief.worlds.iter().filter(|world| world.weight > 0.0) {
        let mut hands = Vec::with_capacity(observation.positions.len());
        let mut valid = true;
        for &position in &observation.positions {
            if position == observation.position {
                hands.push(observation.hand.clone());
            } else if let Some(hand) = world.hands.get(&position) {
                hands.push(hand.clone());
            } else {
                valid = false;
                break;
            }
        }
        if !valid {
            continue;
        }
        let signature = hands
            .iter()
            .map(|hand| encode_hand(hand))
            .collect::<Vec<_>>();
        if let Some(existing) = worlds
            .iter_mut()
            .find(|candidate| candidate.signature == signature)
        {
            existing.weight += world.weight;
        } else {
            worlds.push(SearchWorld {
                hands,
                signature,
                weight: world.weight,
            });
        }
    }
    if worlds.len() <= SEARCH_WORLD_LIMIT {
        return worlds;
    }

    // 直接取概率最高的前几个样本，在开局近似均匀时会退化成“按牌编码取前几个”，
    // 对高低牌分布产生系统偏差。用加权最远点选代表牌局，再把所有样本概率聚合
    // 到最近代表，能同时保留常见牌局和少数但战术差异很大的炸弹/控制牌牌局。
    let total_weight = worlds.iter().map(|world| world.weight).sum::<f64>();
    let first = worlds
        .iter()
        .enumerate()
        .max_by(|(left_index, left), (right_index, right)| {
            left.weight
                .total_cmp(&right.weight)
                .then_with(|| right_index.cmp(left_index))
        })
        .map(|(index, _)| index)
        .unwrap_or(0);
    let mut selected = vec![first];
    while selected.len() < SEARCH_WORLD_LIMIT {
        let Some(next) = worlds
            .iter()
            .enumerate()
            .filter(|(index, _)| !selected.contains(index))
            .max_by(|(left_index, left), (right_index, right)| {
                representative_score(left, &worlds, &selected, total_weight)
                    .total_cmp(&representative_score(
                        right,
                        &worlds,
                        &selected,
                        total_weight,
                    ))
                    .then_with(|| right_index.cmp(left_index))
            })
            .map(|(index, _)| index)
        else {
            break;
        };
        selected.push(next);
    }

    let mut representatives = selected
        .iter()
        .map(|index| {
            let mut representative = worlds[*index].clone();
            representative.weight = 0.0;
            representative
        })
        .collect::<Vec<_>>();
    for world in worlds {
        let nearest = representatives
            .iter()
            .enumerate()
            .min_by_key(|(_, representative)| world_distance(&world, representative))
            .map(|(index, _)| index)
            .unwrap_or(0);
        representatives[nearest].weight += world.weight;
    }
    representatives.sort_by(|left, right| {
        right
            .weight
            .total_cmp(&left.weight)
            .then(left.signature.cmp(&right.signature))
    });
    representatives
}

fn representative_score(
    candidate: &SearchWorld,
    worlds: &[SearchWorld],
    selected: &[usize],
    total_weight: f64,
) -> f64 {
    let minimum_distance = selected
        .iter()
        .map(|index| world_distance(candidate, &worlds[*index]))
        .min()
        .unwrap_or(0) as f64;
    let relative_weight = if total_weight > 0.0 {
        candidate.weight / total_weight
    } else {
        0.0
    };
    minimum_distance * (0.25 + relative_weight)
}

fn world_distance(left: &SearchWorld, right: &SearchWorld) -> usize {
    left.signature
        .iter()
        .zip(&right.signature)
        .map(|(&left_hand, &right_hand)| encoded_hand_distance(left_hand, right_hand))
        .sum()
}

fn encoded_hand_distance(mut left: u64, mut right: u64) -> usize {
    let mut distance = 0;
    for _ in 3..=17 {
        distance += (left % 5).abs_diff(right % 5) as usize;
        left /= 5;
        right /= 5;
    }
    distance
}

#[derive(Clone)]
struct SearchState {
    hands: Vec<Vec<i32>>,
    current: usize,
    landlord: usize,
    trick_owner: usize,
    benchmark: Option<Combo>,
}

impl SearchState {
    fn from_world(
        observation: &AiObservation,
        hands: &[Vec<i32>],
        benchmark: Option<Combo>,
    ) -> Option<Self> {
        let current = observation
            .positions
            .iter()
            .position(|position| *position == observation.position)?;
        let landlord_position = observation.landlord_position?;
        let landlord = observation
            .positions
            .iter()
            .position(|position| *position == landlord_position)?;
        let trick_owner = if benchmark.is_some() {
            observation
                .positions
                .iter()
                .position(|position| *position == observation.last_play_position)?
        } else {
            current
        };
        Some(Self {
            hands: hands.to_vec(),
            current,
            landlord,
            trick_owner,
            benchmark,
        })
    }

    fn after_action(&self, action: Option<&Candidate>) -> Option<Self> {
        let mut next = self.clone();
        if let Some(candidate) = action {
            for card in &candidate.cards {
                let index = next.hands[next.current]
                    .iter()
                    .position(|held| held == card)?;
                next.hands[next.current].remove(index);
            }
            next.benchmark = Some(candidate.combo.clone());
            next.trick_owner = next.current;
        } else if next.benchmark.is_none() {
            return None;
        }

        next.current = (next.current + 1) % next.hands.len();
        if action.is_none() && next.current == next.trick_owner {
            next.benchmark = None;
        }
        Some(next)
    }

    fn actions(&self) -> Vec<Option<Candidate>> {
        let mut actions = all_candidates(&self.hands[self.current])
            .into_iter()
            .filter(|candidate| {
                self.benchmark
                    .as_ref()
                    .is_none_or(|benchmark| can_beat(&candidate.combo, benchmark))
            })
            .map(Some)
            .collect::<Vec<_>>();
        if self.benchmark.is_some() {
            actions.push(None);
        }
        actions
    }

    fn rollout_action(&self) -> Option<Candidate> {
        self.actions()
            .into_iter()
            .map(|action| {
                let score = self.rollout_action_score(action.as_ref());
                (action, score)
            })
            .max_by(|(_, left_score), (_, right_score)| left_score.total_cmp(right_score))
            .and_then(|(action, _)| action)
    }

    fn rollout_action_score(&self, candidate: Option<&Candidate>) -> f64 {
        let previous_is_ally = self.benchmark.is_some()
            && self.current != self.landlord
            && self.trick_owner != self.landlord;
        let enemy_is_urgent = self
            .hands
            .iter()
            .enumerate()
            .filter(|(position, _)| !self.same_team(*position, self.current))
            .any(|(_, hand)| hand.len() <= 2);
        let Some(candidate) = candidate else {
            if previous_is_ally {
                let enemy_can_take_control = self.benchmark.as_ref().is_some_and(|benchmark| {
                    self.hands
                        .iter()
                        .enumerate()
                        .filter(|(position, _)| !self.same_team(*position, self.current))
                        .any(|(_, hand)| {
                            all_candidates(hand)
                                .iter()
                                .any(|response| can_beat(&response.combo, benchmark))
                        })
                });
                if !enemy_can_take_control {
                    return 35.0;
                }
                return if enemy_is_urgent { -30.0 } else { -5.0 };
            }
            return -30.0;
        };

        let mut remaining = self.hands[self.current].clone();
        for card in &candidate.cards {
            if let Some(index) = remaining.iter().position(|held| held == card) {
                remaining.remove(index);
            }
        }
        if remaining.is_empty() {
            return 10_000.0;
        }
        let turns = estimate_turns(&remaining);
        let mut score = candidate.cards.len() as f64 * 7.0 - turns as f64 * 16.0;
        score -= f64::from(attachment_cost(candidate)) * if enemy_is_urgent { 0.08 } else { 0.28 };
        score -= f64::from(power_structure_cost(&self.hands[self.current], candidate))
            * if enemy_is_urgent { 0.28 } else { 1.0 };
        if self.benchmark.is_some() {
            score -= candidate.combo.main_rank as f64 * 0.65;
        }
        if matches!(candidate.combo.kind, ComboKind::Bomb | ComboKind::Rocket) {
            score -= if enemy_is_urgent || turns <= 1 {
                8.0
            } else {
                55.0
            };
        }

        let mut enemy_can_beat = false;
        let mut enemy_can_finish = false;
        for (_, hand) in self
            .hands
            .iter()
            .enumerate()
            .filter(|(position, _)| !self.same_team(*position, self.current))
        {
            for response in all_candidates(hand) {
                if !can_beat(&response.combo, &candidate.combo) {
                    continue;
                }
                enemy_can_beat = true;
                enemy_can_finish |= response.cards.len() == hand.len();
            }
        }
        if enemy_can_beat {
            score -= 12.0;
        } else {
            score += 10.0;
        }
        if enemy_can_finish {
            score -= 100.0;
        }

        let next = (self.current + 1) % self.hands.len();
        if self.benchmark.is_none()
            && self.current != self.landlord
            && next != self.landlord
            && self.hands[next].len() == 1
            && candidate.combo.kind == ComboKind::Single
        {
            score += 60.0 - candidate.combo.main_rank as f64;
        }
        if self.benchmark.is_none()
            && !self.same_team(next, self.current)
            && self.hands[next].len() == 1
            && candidate.combo.kind == ComboKind::Single
        {
            score -= 80.0;
        }

        if self.current != self.landlord {
            let runner = self
                .hands
                .iter()
                .enumerate()
                .filter(|(position, _)| *position != self.landlord)
                .min_by_key(|(_, hand)| (estimate_turns(hand), hand.len()))
                .map(|(position, _)| position);
            if runner == Some(self.current) {
                score += candidate.cards.len() as f64 * 2.0;
            } else {
                score -= candidate
                    .cards
                    .iter()
                    .filter(|card| card_rank(**card) >= 15)
                    .count() as f64
                    * 5.0;
            }
        }
        score
    }

    fn same_team(&self, left: usize, right: usize) -> bool {
        (left == self.landlord) == (right == self.landlord)
    }

    fn terminal_value(&self, root_is_landlord: bool) -> Option<f64> {
        let winner = self.hands.iter().position(Vec::is_empty)?;
        let root_won = (winner == self.landlord) == root_is_landlord;
        let remaining_cards = self.hands.iter().map(Vec::len).sum::<usize>() as f64;
        Some(if root_won {
            1.0 + remaining_cards * 0.001
        } else {
            -1.0 - remaining_cards * 0.001
        })
    }

    fn heuristic_value(&self, root_is_landlord: bool) -> f64 {
        let landlord_turns = estimate_turns(&self.hands[self.landlord]) as f64;
        let farmer_turns = self
            .hands
            .iter()
            .enumerate()
            .filter(|(position, _)| *position != self.landlord)
            .map(|(_, hand)| estimate_turns(hand) as f64)
            .fold(f64::INFINITY, f64::min);
        let landlord_cards = self.hands[self.landlord].len() as f64;
        let farmer_cards = self
            .hands
            .iter()
            .enumerate()
            .filter(|(position, _)| *position != self.landlord)
            .map(|(_, hand)| hand.len() as f64)
            .fold(f64::INFINITY, f64::min);
        let denominator = landlord_turns + farmer_turns + 1.0;
        let landlord_value = ((farmer_turns - landlord_turns) / denominator * 0.65
            + (farmer_cards - landlord_cards) / 20.0 * 0.25)
            .clamp(-0.85, 0.85);
        if root_is_landlord {
            landlord_value
        } else {
            -landlord_value
        }
    }

    fn key(&self) -> SearchKey {
        let (benchmark_kind, benchmark_rank, benchmark_len) = self
            .benchmark
            .as_ref()
            .map(|combo| {
                (
                    combo_kind_code(combo.kind),
                    combo.main_rank,
                    combo.sequence_len as u8,
                )
            })
            .unwrap_or((0, 0, 0));
        SearchKey {
            hands: self.hands.iter().map(|hand| encode_hand(hand)).collect(),
            current: self.current as u8,
            trick_owner: self.trick_owner as u8,
            benchmark_kind,
            benchmark_rank,
            benchmark_len,
        }
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct SearchKey {
    hands: Vec<u64>,
    current: u8,
    trick_owner: u8,
    benchmark_kind: u8,
    benchmark_rank: u8,
    benchmark_len: u8,
}

#[derive(Clone, Copy)]
struct SearchEval {
    value: f64,
    cacheable: bool,
}

struct Solver {
    root_is_landlord: bool,
    nodes: usize,
    cache: HashMap<SearchKey, f64>,
}

impl Solver {
    fn new(root_is_landlord: bool) -> Self {
        Self {
            root_is_landlord,
            nodes: 0,
            cache: HashMap::new(),
        }
    }

    fn solve(&mut self, state: &SearchState, mut alpha: f64, mut beta: f64) -> SearchEval {
        if let Some(value) = state.terminal_value(self.root_is_landlord) {
            return SearchEval {
                value,
                cacheable: true,
            };
        }
        let key = state.key();
        if let Some(&value) = self.cache.get(&key) {
            return SearchEval {
                value,
                cacheable: true,
            };
        }
        self.nodes += 1;
        if self.nodes > SEARCH_NODE_BUDGET_PER_ACTION {
            return SearchEval {
                value: state.heuristic_value(self.root_is_landlord),
                cacheable: false,
            };
        }

        let maximizing = (state.current == state.landlord) == self.root_is_landlord;
        let mut best = if maximizing {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
        let mut cacheable = true;
        let mut cutoff = false;
        for action in state.actions() {
            let Some(next) = state.after_action(action.as_ref()) else {
                continue;
            };
            let child = self.solve(&next, alpha, beta);
            cacheable &= child.cacheable;
            if maximizing {
                best = best.max(child.value);
                alpha = alpha.max(best);
            } else {
                best = best.min(child.value);
                beta = beta.min(best);
            }
            if beta <= alpha {
                cutoff = true;
                break;
            }
        }

        if !best.is_finite() {
            best = state.heuristic_value(self.root_is_landlord);
            cacheable = false;
        }
        if cacheable && !cutoff {
            self.cache.insert(key, best);
        }
        SearchEval {
            value: best,
            cacheable: cacheable && !cutoff,
        }
    }
}

fn encode_hand(hand: &[i32]) -> u64 {
    let mut counts = [0_u8; 18];
    for &card in hand {
        counts[card_rank(card) as usize] += 1;
    }
    (3..=17).fold(0_u64, |key, rank| key * 5 + u64::from(counts[rank]))
}

fn combo_kind_code(kind: ComboKind) -> u8 {
    match kind {
        ComboKind::Rocket => 1,
        ComboKind::Bomb => 2,
        ComboKind::Single => 3,
        ComboKind::Pair => 4,
        ComboKind::Triple => 5,
        ComboKind::TripleSingle => 6,
        ComboKind::TriplePair => 7,
        ComboKind::Straight => 8,
        ComboKind::StraightPairs => 9,
        ComboKind::Plane => 10,
        ComboKind::PlaneWithSingles => 11,
        ComboKind::PlaneWithPairs => 12,
        ComboKind::FourWithTwoSingles => 13,
        ComboKind::FourWithTwoPairs => 14,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use share_type_public::LandlordPhase;

    use crate::ai::belief::BeliefWorld;

    use super::*;

    fn observation_for_runner() -> AiObservation {
        AiObservation {
            position: 1,
            phase: LandlordPhase::Play,
            hand: vec![1, 12], // 3, A
            positions: vec![0, 1, 2],
            hand_sizes: BTreeMap::from([(0, 1), (1, 2), (2, 1)]),
            landlord_position: Some(0),
            current_position: 1,
            current_score: 1,
            call_history: Vec::new(),
            hidden_cards: Vec::new(),
            last_play_position: 1,
            last_play: Vec::new(),
            play_history: Vec::new(),
        }
    }

    fn belief_with_world(landlord_hand: Vec<i32>, ally_hand: Vec<i32>) -> CardBelief {
        CardBelief {
            remaining_outside_hand: [0; 18],
            played_rank_counts: [0; 18],
            opponents: BTreeMap::new(),
            worlds: vec![BeliefWorld {
                hands: BTreeMap::from([(0, landlord_hand), (2, ally_hand)]),
                weight: 1.0,
            }],
        }
    }

    #[test]
    fn farmers_share_the_same_terminal_utility() {
        let farmer_win = SearchState {
            hands: vec![vec![1], Vec::new(), vec![2]],
            current: 2,
            landlord: 0,
            trick_owner: 1,
            benchmark: None,
        };
        let landlord_win = SearchState {
            hands: vec![Vec::new(), vec![1], vec![2]],
            current: 1,
            landlord: 0,
            trick_owner: 0,
            benchmark: None,
        };

        assert!(farmer_win.terminal_value(false).unwrap() > 0.0);
        assert!(landlord_win.terminal_value(false).unwrap() < 0.0);
        assert!(landlord_win.terminal_value(true).unwrap() > 0.0);
    }

    #[test]
    fn two_passes_return_the_lead_to_the_trick_owner() {
        let state = SearchState {
            hands: vec![vec![1], vec![2], vec![3]],
            current: 1,
            landlord: 0,
            trick_owner: 0,
            benchmark: classify(&[4]),
        };
        let after_first_pass = state.after_action(None).expect("first pass");
        assert!(after_first_pass.benchmark.is_some());
        let after_second_pass = after_first_pass.after_action(None).expect("second pass");
        assert_eq!(after_second_pass.current, 0);
        assert!(after_second_pass.benchmark.is_none());
    }

    #[test]
    fn teammate_and_landlord_estimates_change_the_endgame_choice() {
        let observation = observation_for_runner();
        let candidates = all_candidates(&observation.hand);

        let teammate_can_run = belief_with_world(vec![8], vec![2]); // 地主 10，队友 4
        assert_eq!(
            choose_endgame_play(&observation, &teammate_can_run, &candidates, true),
            Some(vec![1])
        );

        let low_lead_loses = belief_with_world(vec![2], vec![14]); // 地主 4，队友 3
        assert_eq!(
            choose_endgame_play(&observation, &low_lead_loses, &candidates, true),
            Some(vec![12])
        );
    }

    #[test]
    fn equal_value_tiebreak_preserves_a_control_kicker() {
        let hand = vec![10, 23, 38, 39, 41, 42, 49]; // QQQ + A、2、4、5
        let candidates = all_candidates(&hand);
        let high_kicker = candidates
            .iter()
            .find(|candidate| {
                candidate.combo.kind == ComboKind::TripleSingle && candidate.cards.contains(&38)
            })
            .expect("triple with ace kicker");
        let low_kicker = candidates
            .iter()
            .find(|candidate| {
                candidate.combo.kind == ComboKind::TripleSingle && candidate.cards.contains(&41)
            })
            .expect("triple with four kicker");

        assert!(
            root_action_tiebreak(&hand, Some(low_kicker), true, true)
                > root_action_tiebreak(&hand, Some(high_kicker), true, true)
        );
    }

    #[test]
    fn equal_value_tiebreak_uses_the_cheaper_ordinary_kicker() {
        let hand = vec![10, 23, 49, 41, 7]; // QQQ + 4、9
        let candidates = all_candidates(&hand);
        let low_kicker = candidates
            .iter()
            .find(|candidate| {
                candidate.combo.kind == ComboKind::TripleSingle && candidate.cards.contains(&41)
            })
            .expect("triple with four kicker");
        let high_kicker = candidates
            .iter()
            .find(|candidate| {
                candidate.combo.kind == ComboKind::TripleSingle && candidate.cards.contains(&7)
            })
            .expect("triple with nine kicker");

        assert!(
            root_action_tiebreak(&hand, Some(low_kicker), true, true)
                > root_action_tiebreak(&hand, Some(high_kicker), true, true)
        );
    }

    #[test]
    fn rollout_uses_the_cheaper_ordinary_kicker() {
        let hand = vec![10, 23, 49, 41, 7]; // QQQ + 4、9
        let candidates = all_candidates(&hand);
        let low_kicker = candidates
            .iter()
            .find(|candidate| {
                candidate.combo.kind == ComboKind::TripleSingle && candidate.cards.contains(&41)
            })
            .expect("triple with four kicker");
        let high_kicker = candidates
            .iter()
            .find(|candidate| {
                candidate.combo.kind == ComboKind::TripleSingle && candidate.cards.contains(&7)
            })
            .expect("triple with nine kicker");
        let state = SearchState {
            hands: vec![hand, vec![1, 2, 3], vec![4, 5, 6]],
            current: 0,
            landlord: 0,
            trick_owner: 0,
            benchmark: None,
        };

        assert!(
            state.rollout_action_score(Some(low_kicker))
                > state.rollout_action_score(Some(high_kicker))
        );
    }

    #[test]
    fn rollout_overtakes_teammate_when_landlord_can_take_control() {
        let state = SearchState {
            hands: vec![
                vec![1, 14, 9, 22],  // 地主：对 3、对 J
                vec![3],             // 主跑农民
                vec![2, 15, 10, 23], // 支援农民：对 4、对 Q
            ],
            current: 2,
            landlord: 0,
            trick_owner: 1,
            benchmark: classify(&[8, 21]), // 队友出对 10
        };

        assert_eq!(
            state.rollout_action().map(|candidate| candidate.cards),
            Some(vec![10, 23])
        );
    }

    #[test]
    fn rollout_passes_when_teammate_already_holds_control() {
        let state = SearchState {
            hands: vec![
                vec![1, 14, 7, 20],  // 地主：对 3、对 9
                vec![3],             // 主跑农民
                vec![2, 15, 10, 23], // 支援农民：对 4、对 Q
            ],
            current: 2,
            landlord: 0,
            trick_owner: 1,
            benchmark: classify(&[8, 21]), // 队友出对 10
        };

        assert!(state.rollout_action().is_none());
    }

    #[test]
    fn rollout_trusts_teammate_pair_against_two_unpaired_enemy_cards() {
        let state = SearchState {
            hands: vec![
                vec![1, 3],          // 地主：单 3、单 5
                vec![6],             // 主跑农民
                vec![2, 15, 10, 23], // 支援农民：对 4、对 Q
            ],
            current: 2,
            landlord: 0,
            trick_owner: 1,
            benchmark: classify(&[8, 21]), // 队友出对 10
        };

        assert!(state.rollout_action().is_none());
    }

    #[test]
    fn rollout_does_not_overtake_for_a_nonfinishing_enemy_bomb() {
        let state = SearchState {
            hands: vec![
                vec![1, 14, 27, 40, 3, 16, 5, 18], // 地主：炸弹 3、对 5、对 7
                vec![4],                           // 主跑农民
                vec![2, 15, 10, 23],               // 支援农民：对 4、对 Q
            ],
            current: 2,
            landlord: 0,
            trick_owner: 1,
            benchmark: classify(&[8, 21]), // 队友出对 10
        };

        assert!(state.rollout_action().is_none());
    }

    #[test]
    fn equal_value_tiebreak_preserves_a_bomb_outside_a_straight() {
        let hand = vec![1, 14, 27, 40, 2, 3, 4, 5, 6]; // 3333 + 45678
        let candidates = all_candidates(&hand);
        let preserving = candidates
            .iter()
            .find(|candidate| candidate.cards == vec![2, 3, 4, 5, 6])
            .expect("straight preserving bomb");
        let splitting = candidates
            .iter()
            .find(|candidate| candidate.cards == vec![1, 2, 3, 4, 5])
            .expect("straight splitting bomb");

        assert!(
            root_action_tiebreak(&hand, Some(preserving), true, true)
                > root_action_tiebreak(&hand, Some(splitting), true, true)
        );
    }

    #[test]
    fn rollout_prefers_an_equivalent_straight_that_preserves_a_bomb() {
        let hand = vec![1, 14, 27, 40, 2, 3, 4, 5, 6]; // 3333 + 45678
        let candidates = all_candidates(&hand);
        let preserving = candidates
            .iter()
            .find(|candidate| candidate.cards == vec![2, 3, 4, 5, 6])
            .expect("straight preserving bomb");
        let splitting = candidates
            .iter()
            .find(|candidate| candidate.cards == vec![1, 2, 3, 4, 5])
            .expect("straight splitting bomb");
        let state = SearchState {
            hands: vec![hand, vec![7, 20, 33], vec![8, 21, 34]],
            current: 0,
            landlord: 0,
            trick_owner: 0,
            benchmark: None,
        };

        assert!(
            state.rollout_action_score(Some(preserving))
                > state.rollout_action_score(Some(splitting))
        );
    }
}
