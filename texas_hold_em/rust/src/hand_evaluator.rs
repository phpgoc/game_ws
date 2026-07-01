use std::cmp::Ordering;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluatedHand {
    pub category: i32,
    pub ranks: Vec<i32>,
    pub name: &'static str,
}

impl Ord for EvaluatedHand {
    fn cmp(&self, other: &Self) -> Ordering {
        self.category
            .cmp(&other.category)
            .then_with(|| self.ranks.cmp(&other.ranks))
    }
}

impl PartialOrd for EvaluatedHand {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub fn card_rank(card: i32) -> i32 {
    ((card - 1) % 13) + 2
}

fn card_suit(card: i32) -> i32 {
    (card - 1) / 13
}

pub fn evaluate_best(cards: &[i32]) -> Option<EvaluatedHand> {
    if cards.len() < 5 {
        return None;
    }
    let mut best: Option<EvaluatedHand> = None;
    for a in 0..cards.len() - 4 {
        for b in a + 1..cards.len() - 3 {
            for c in b + 1..cards.len() - 2 {
                for d in c + 1..cards.len() - 1 {
                    for e in d + 1..cards.len() {
                        let hand =
                            evaluate_five(&[cards[a], cards[b], cards[c], cards[d], cards[e]]);
                        if best.as_ref().is_none_or(|current| hand > *current) {
                            best = Some(hand);
                        }
                    }
                }
            }
        }
    }
    best
}

fn straight_high(mut ranks: Vec<i32>) -> Option<i32> {
    ranks.sort_unstable();
    ranks.dedup();
    if ranks.contains(&14) {
        ranks.insert(0, 1);
    }
    let mut best = None;
    let mut run = 1;
    for idx in 1..ranks.len() {
        if ranks[idx] == ranks[idx - 1] + 1 {
            run += 1;
            if run >= 5 {
                best = Some(ranks[idx]);
            }
        } else {
            run = 1;
        }
    }
    best
}

pub fn evaluate_five(cards: &[i32; 5]) -> EvaluatedHand {
    let mut ranks: Vec<i32> = cards.iter().map(|card| card_rank(*card)).collect();
    ranks.sort_unstable_by(|a, b| b.cmp(a));
    let flush = cards
        .iter()
        .all(|card| card_suit(*card) == card_suit(cards[0]));
    let straight = straight_high(ranks.clone());

    if flush && straight.is_some() {
        return EvaluatedHand {
            category: 8,
            ranks: vec![straight.unwrap()],
            name: "straight_flush",
        };
    }

    let mut counts: HashMap<i32, usize> = HashMap::new();
    for rank in &ranks {
        *counts.entry(*rank).or_default() += 1;
    }
    let mut groups: Vec<(usize, i32)> = counts
        .into_iter()
        .map(|(rank, count)| (count, rank))
        .collect();
    groups.sort_unstable_by(|a, b| b.cmp(a));

    if groups[0].0 == 4 {
        let quad = groups[0].1;
        let kicker = groups.iter().find(|(_, rank)| *rank != quad).unwrap().1;
        return EvaluatedHand {
            category: 7,
            ranks: vec![quad, kicker],
            name: "four_of_a_kind",
        };
    }

    if groups[0].0 == 3 && groups.get(1).is_some_and(|group| group.0 == 2) {
        return EvaluatedHand {
            category: 6,
            ranks: vec![groups[0].1, groups[1].1],
            name: "full_house",
        };
    }

    if flush {
        return EvaluatedHand {
            category: 5,
            ranks,
            name: "flush",
        };
    }

    if let Some(high) = straight {
        return EvaluatedHand {
            category: 4,
            ranks: vec![high],
            name: "straight",
        };
    }

    if groups[0].0 == 3 {
        let trip = groups[0].1;
        let mut kickers: Vec<i32> = groups
            .iter()
            .filter_map(|(count, rank)| (*count == 1).then_some(*rank))
            .collect();
        kickers.sort_unstable_by(|a, b| b.cmp(a));
        let mut out = vec![trip];
        out.extend(kickers);
        return EvaluatedHand {
            category: 3,
            ranks: out,
            name: "three_of_a_kind",
        };
    }

    let pairs: Vec<i32> = groups
        .iter()
        .filter_map(|(count, rank)| (*count == 2).then_some(*rank))
        .collect();
    if pairs.len() >= 2 {
        let kicker = groups
            .iter()
            .filter_map(|(count, rank)| (*count == 1).then_some(*rank))
            .max()
            .unwrap_or_default();
        return EvaluatedHand {
            category: 2,
            ranks: vec![pairs[0], pairs[1], kicker],
            name: "two_pair",
        };
    }

    if pairs.len() == 1 {
        let pair = pairs[0];
        let mut kickers: Vec<i32> = groups
            .iter()
            .filter_map(|(count, rank)| (*count == 1).then_some(*rank))
            .collect();
        kickers.sort_unstable_by(|a, b| b.cmp(a));
        let mut out = vec![pair];
        out.extend(kickers);
        return EvaluatedHand {
            category: 1,
            ranks: out,
            name: "one_pair",
        };
    }

    EvaluatedHand {
        category: 0,
        ranks,
        name: "high_card",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_straight_flush() {
        let hand = evaluate_five(&[9, 10, 11, 12, 13]);
        assert_eq!(hand.category, 8);
        assert_eq!(hand.ranks, vec![14]);
    }

    #[test]
    fn evaluates_wheel_straight() {
        let hand = evaluate_five(&[1, 2, 3, 4, 26]);
        assert_eq!(hand.category, 4);
        assert_eq!(hand.ranks, vec![5]);
    }

    #[test]
    fn evaluates_four_of_a_kind() {
        let hand = evaluate_five(&[13, 26, 39, 52, 12]);
        assert_eq!(hand.category, 7);
        assert_eq!(hand.ranks, vec![14, 13]);
    }

    #[test]
    fn best_of_seven_prefers_full_house() {
        let hand = evaluate_best(&[11, 24, 37, 10, 23, 4, 5]).unwrap();
        assert_eq!(hand.category, 6);
        assert_eq!(hand.ranks, vec![12, 11]);
    }
}
