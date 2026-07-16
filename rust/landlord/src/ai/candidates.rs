use std::collections::{BTreeMap, HashSet};

use crate::core::play::{Combo, ComboKind, card_rank, classify};

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
    let mut remaining = hand.to_vec();
    let mut turns = 0;
    while !remaining.is_empty() {
        let candidates = all_candidates(&remaining);
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
}
