use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap, HashSet},
};

use crate::core::play::{Combo, ComboKind, card_rank, classify};

const EXACT_TURN_CARD_LIMIT: usize = 12;
const EXACT_TURN_CACHE_LIMIT: usize = 50_000;

thread_local! {
    static EXACT_TURN_CACHE: RefCell<HashMap<u64, usize>> = RefCell::new(HashMap::new());
}

#[derive(Clone, Debug)]
pub struct Candidate {
    pub cards: Vec<i32>,
    pub combo: Combo,
}

pub fn all_candidates(hand: &[i32]) -> Vec<Candidate> {
    let grouped = group_by_rank(hand);
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();

    for cards in grouped.values() {
        push(vec![cards[0]], &mut seen, &mut candidates);
        if cards.len() >= 2 {
            push(cards[..2].to_vec(), &mut seen, &mut candidates);
        }
        if cards.len() >= 3 {
            push(cards[..3].to_vec(), &mut seen, &mut candidates);
        }
        if cards.len() == 4 {
            push(cards.clone(), &mut seen, &mut candidates);
        }
    }

    if let (Some(small), Some(big)) = (grouped.get(&16), grouped.get(&17)) {
        push(vec![small[0], big[0]], &mut seen, &mut candidates);
    }

    add_triple_attachments(&grouped, &mut seen, &mut candidates);
    add_sequences(&grouped, 1, 5, &mut seen, &mut candidates);
    add_sequences(&grouped, 2, 3, &mut seen, &mut candidates);
    add_planes(&grouped, &mut seen, &mut candidates);
    add_four_attachments(&grouped, &mut seen, &mut candidates);

    candidates.sort_by(|left, right| {
        right
            .cards
            .len()
            .cmp(&left.cards.len())
            .then(left.combo.main_rank.cmp(&right.combo.main_rank))
            .then(left.cards.cmp(&right.cards))
    });
    candidates
}

/// 用公开牌型做快速的剩余手数估计。它不是穷举求最优解，
/// 但会优先完整牌型并避免过早拆炸弹，适合每个 AI 回合实时调用。
pub fn estimate_turns(hand: &[i32]) -> usize {
    estimate_turns_with_candidates(hand, None)
}

pub fn estimate_turns_from_candidates(hand: &[i32], candidates: &[Candidate]) -> usize {
    estimate_turns_with_candidates(hand, Some(candidates))
}

fn estimate_turns_with_candidates(hand: &[i32], initial_candidates: Option<&[Candidate]>) -> usize {
    if hand.len() <= EXACT_TURN_CARD_LIMIT {
        return exact_turns(hand);
    }
    greedy_turns(hand, initial_candidates)
}

fn greedy_turns(hand: &[i32], initial_candidates: Option<&[Candidate]>) -> usize {
    let mut remaining = hand.to_vec();
    let mut turns = 0;
    while !remaining.is_empty() {
        let generated = if turns == 0 && initial_candidates.is_some() {
            None
        } else {
            Some(all_candidates(&remaining))
        };
        let candidates = if turns == 0 {
            initial_candidates.unwrap_or_else(|| generated.as_deref().unwrap_or_default())
        } else {
            generated.as_deref().unwrap_or_default()
        };
        let Some(best) = candidates.iter().max_by_key(|candidate| {
            let finishes = usize::from(candidate.cards.len() == remaining.len());
            let bomb_penalty = usize::from(matches!(
                candidate.combo.kind,
                ComboKind::Bomb | ComboKind::Rocket
            ));
            (
                finishes,
                candidate.cards.len().saturating_sub(bomb_penalty * 2),
                std::cmp::Reverse(candidate.combo.main_rank),
            )
        }) else {
            return turns + remaining.len();
        };
        for card in &best.cards {
            if let Some(index) = remaining.iter().position(|candidate| candidate == card) {
                remaining.remove(index);
            }
        }
        turns += 1;
    }
    turns
}

fn exact_turns(hand: &[i32]) -> usize {
    if hand.is_empty() {
        return 0;
    }
    let key = rank_count_key(hand);
    if let Some(cached) = EXACT_TURN_CACHE.with(|cache| cache.borrow().get(&key).copied()) {
        return cached;
    }

    let candidates = all_candidates(hand);
    let mut best = hand.len();
    for candidate in candidates {
        if candidate.cards.len() == hand.len() {
            best = 1;
            break;
        }
        let mut remaining = hand.to_vec();
        for card in &candidate.cards {
            if let Some(index) = remaining.iter().position(|held| held == card) {
                remaining.remove(index);
            }
        }
        best = best.min(1 + exact_turns(&remaining));
        if best == 2 {
            // 不是一次能走完已经在上方排除，二手是当前状态的理论下界。
            break;
        }
    }
    EXACT_TURN_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() >= EXACT_TURN_CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(key, best);
    });
    best
}

fn rank_count_key(hand: &[i32]) -> u64 {
    let mut counts = [0_u8; 18];
    for &card in hand {
        counts[card_rank(card) as usize] += 1;
    }
    (3..=17).fold(0_u64, |key, rank| key * 5 + u64::from(counts[rank]))
}

fn add_triple_attachments(
    grouped: &BTreeMap<u8, Vec<i32>>,
    seen: &mut HashSet<Vec<i32>>,
    candidates: &mut Vec<Candidate>,
) {
    let triples = ranks_with_at_least(grouped, 3, false);
    for triple_rank in triples {
        let triple = grouped[&triple_rank][..3].to_vec();
        for (&wing_rank, wing) in grouped {
            if wing_rank == triple_rank {
                continue;
            }
            let mut single = triple.clone();
            single.push(wing[0]);
            push(single, seen, candidates);
            if wing.len() >= 2 {
                let mut pair = triple.clone();
                pair.extend_from_slice(&wing[..2]);
                push(pair, seen, candidates);
            }
        }
    }
}

fn add_sequences(
    grouped: &BTreeMap<u8, Vec<i32>>,
    copies: usize,
    minimum_len: usize,
    seen: &mut HashSet<Vec<i32>>,
    candidates: &mut Vec<Candidate>,
) {
    let ranks = ranks_with_at_least(grouped, copies, true);
    for run in consecutive_runs(&ranks) {
        if run.len() < minimum_len {
            continue;
        }
        for len in minimum_len..=run.len() {
            for start in 0..=run.len() - len {
                let mut cards = Vec::with_capacity(len * copies);
                for rank in &run[start..start + len] {
                    cards.extend_from_slice(&grouped[rank][..copies]);
                }
                push(cards, seen, candidates);
            }
        }
    }
}

fn add_planes(
    grouped: &BTreeMap<u8, Vec<i32>>,
    seen: &mut HashSet<Vec<i32>>,
    candidates: &mut Vec<Candidate>,
) {
    let triple_ranks = ranks_with_at_least(grouped, 3, true);
    for run in consecutive_runs(&triple_ranks) {
        if run.len() < 2 {
            continue;
        }
        for len in 2..=run.len() {
            for start in 0..=run.len() - len {
                let body_ranks = &run[start..start + len];
                let mut body = Vec::with_capacity(len * 3);
                for rank in body_ranks {
                    body.extend_from_slice(&grouped[rank][..3]);
                }
                push(body.clone(), seen, candidates);

                let single_ranks = grouped
                    .keys()
                    .copied()
                    .filter(|rank| !body_ranks.contains(rank))
                    .collect::<Vec<_>>();
                for wings in combinations(&single_ranks, len) {
                    let mut cards = body.clone();
                    for rank in wings {
                        cards.push(grouped[&rank][0]);
                    }
                    push(cards, seen, candidates);
                }

                let pair_ranks = grouped
                    .iter()
                    .filter_map(|(&rank, cards)| {
                        (!body_ranks.contains(&rank) && cards.len() >= 2).then_some(rank)
                    })
                    .collect::<Vec<_>>();
                for wings in combinations(&pair_ranks, len) {
                    let mut cards = body.clone();
                    for rank in wings {
                        cards.extend_from_slice(&grouped[&rank][..2]);
                    }
                    push(cards, seen, candidates);
                }
            }
        }
    }
}

fn add_four_attachments(
    grouped: &BTreeMap<u8, Vec<i32>>,
    seen: &mut HashSet<Vec<i32>>,
    candidates: &mut Vec<Candidate>,
) {
    for (&bomb_rank, bomb) in grouped.iter().filter(|(_, cards)| cards.len() == 4) {
        let wing_ranks = grouped
            .keys()
            .copied()
            .filter(|rank| *rank != bomb_rank)
            .collect::<Vec<_>>();
        for (index, &left) in wing_ranks.iter().enumerate() {
            for &right in &wing_ranks[index..] {
                if left == right && grouped[&left].len() < 2 {
                    continue;
                }
                let mut cards = bomb.clone();
                cards.push(grouped[&left][0]);
                cards.push(grouped[&right][usize::from(left == right)]);
                push(cards, seen, candidates);
            }
        }

        let pair_ranks = wing_ranks
            .into_iter()
            .filter(|rank| grouped[rank].len() >= 2)
            .collect::<Vec<_>>();
        for pairs in combinations(&pair_ranks, 2) {
            let mut cards = bomb.clone();
            for rank in pairs {
                cards.extend_from_slice(&grouped[&rank][..2]);
            }
            push(cards, seen, candidates);
        }
    }
}

fn group_by_rank(hand: &[i32]) -> BTreeMap<u8, Vec<i32>> {
    let mut grouped = BTreeMap::<u8, Vec<i32>>::new();
    for &card in hand {
        grouped.entry(card_rank(card)).or_default().push(card);
    }
    for cards in grouped.values_mut() {
        cards.sort_unstable();
    }
    grouped
}

fn ranks_with_at_least(
    grouped: &BTreeMap<u8, Vec<i32>>,
    copies: usize,
    exclude_two_and_jokers: bool,
) -> Vec<u8> {
    grouped
        .iter()
        .filter_map(|(&rank, cards)| {
            (cards.len() >= copies && (!exclude_two_and_jokers || rank < 15)).then_some(rank)
        })
        .collect()
}

fn consecutive_runs(ranks: &[u8]) -> Vec<&[u8]> {
    if ranks.is_empty() {
        return Vec::new();
    }
    let mut runs = Vec::new();
    let mut start = 0;
    for index in 1..=ranks.len() {
        if index == ranks.len() || ranks[index] != ranks[index - 1] + 1 {
            runs.push(&ranks[start..index]);
            start = index;
        }
    }
    runs
}

fn combinations<T: Copy>(items: &[T], count: usize) -> Vec<Vec<T>> {
    fn visit<T: Copy>(
        items: &[T],
        count: usize,
        start: usize,
        current: &mut Vec<T>,
        output: &mut Vec<Vec<T>>,
    ) {
        if current.len() == count {
            output.push(current.clone());
            return;
        }
        let needed = count - current.len();
        if items.len().saturating_sub(start) < needed {
            return;
        }
        for index in start..=items.len() - needed {
            current.push(items[index]);
            visit(items, count, index + 1, current, output);
            current.pop();
        }
    }

    if count == 0 {
        return vec![Vec::new()];
    }
    if items.len() < count {
        return Vec::new();
    }
    let mut output = Vec::new();
    visit(items, count, 0, &mut Vec::new(), &mut output);
    output
}

fn push(mut cards: Vec<i32>, seen: &mut HashSet<Vec<i32>>, candidates: &mut Vec<Candidate>) {
    cards.sort_unstable();
    if !seen.insert(cards.clone()) {
        return;
    }
    if let Some(combo) = classify(&cards) {
        candidates.push(Candidate { cards, combo });
    }
}

#[cfg(test)]
mod tests {
    use crate::core::play::ComboKind;

    use super::all_candidates;

    #[test]
    fn generates_sequences_planes_and_attachments() {
        let hand = vec![
            1, 14, 27, // 333
            2, 15, 28, // 444
            3, 16, // 55
            4, 17, // 66
            5, 18, // 77
            6, 19, // 88
            7, 20, 33, 46, // 9999
        ];
        let candidates = all_candidates(&hand);

        assert!(
            candidates
                .iter()
                .any(|candidate| candidate.combo.kind == ComboKind::StraightPairs)
        );
        assert!(
            candidates
                .iter()
                .any(|candidate| candidate.combo.kind == ComboKind::PlaneWithPairs)
        );
        assert!(
            candidates
                .iter()
                .any(|candidate| candidate.combo.kind == ComboKind::FourWithTwoPairs)
        );
    }

    #[test]
    fn exact_turn_estimate_finds_a_non_greedy_partition() {
        let hand = vec![1, 14, 15, 29, 33, 40, 45, 50, 53, 54];

        assert_eq!(super::greedy_turns(&hand, None), 7);
        assert_eq!(super::exact_turns(&hand), 6);
        assert_eq!(super::estimate_turns(&hand), 6);
    }

    #[test]
    fn exact_turn_estimate_looks_beyond_the_first_partition() {
        fn one_ply_turns(hand: &[i32]) -> usize {
            let candidates = super::all_candidates(hand);
            let mut best = super::greedy_turns(hand, Some(&candidates));
            for candidate in candidates {
                let mut remaining = hand.to_vec();
                for card in candidate.cards {
                    let index = remaining.iter().position(|held| *held == card).unwrap();
                    remaining.remove(index);
                }
                best = best.min(1 + super::greedy_turns(&remaining, None));
            }
            best
        }

        let hand = vec![2, 7, 8, 10, 14, 23, 34, 36, 44, 47, 53, 54];

        assert_eq!(one_ply_turns(&hand), 6);
        assert_eq!(super::exact_turns(&hand), 5);
        assert_eq!(super::estimate_turns(&hand), 5);
    }
}
