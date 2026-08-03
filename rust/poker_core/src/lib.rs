use std::cmp::Ordering;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluatedHand {
    pub category: i32,
    pub ranks: Vec<i32>,
    pub name: &'static str,
}

pub fn card_rank(card: i32) -> i32 {
    ((card - 1) % 13) + 2
}

pub fn card_suit(card: i32) -> i32 {
    (card - 1) / 13
}

pub fn evaluate_best(cards: &[i32]) -> Option<EvaluatedHand> {
    evaluate_best_with_rules(cards, false)
}

pub fn evaluate_short_deck_best(cards: &[i32]) -> Option<EvaluatedHand> {
    evaluate_best_with_rules(cards, true)
}

fn evaluate_best_with_rules(cards: &[i32], short_deck: bool) -> Option<EvaluatedHand> {
    if cards.len() < 5 {
        return None;
    }
    let mut best: Option<EvaluatedHand> = None;
    for a in 0..cards.len() - 4 {
        for b in a + 1..cards.len() - 3 {
            for c in b + 1..cards.len() - 2 {
                for d in c + 1..cards.len() - 1 {
                    for e in d + 1..cards.len() {
                        let hand = evaluate_five_with_rules(
                            &[cards[a], cards[b], cards[c], cards[d], cards[e]],
                            short_deck,
                        );
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

pub fn evaluate_five(cards: &[i32; 5]) -> EvaluatedHand {
    evaluate_five_with_rules(cards, false)
}

fn evaluate_five_with_rules(cards: &[i32; 5], short_deck: bool) -> EvaluatedHand {
    let mut ranks = cards.map(card_rank);
    ranks.sort_unstable_by(|a, b| b.cmp(a));

    let mut counts = [0_u8; 15];
    for rank in ranks {
        counts[rank as usize] += 1;
    }
    let flush = cards
        .iter()
        .all(|card| card_suit(*card) == card_suit(cards[0]));
    let straight = straight_high(&counts, short_deck);

    if flush && let Some(high) = straight {
        return EvaluatedHand {
            category: 8,
            ranks: vec![high],
            name: "straight_flush",
        };
    }

    let mut four = 0;
    let mut trip = 0;
    let mut pairs = Vec::with_capacity(2);
    let mut singles = Vec::with_capacity(5);
    for rank in (2..=14).rev() {
        match counts[rank as usize] {
            4 => four = rank,
            3 => trip = rank,
            2 => pairs.push(rank),
            1 => singles.push(rank),
            _ => {}
        }
    }

    if four > 0 {
        return EvaluatedHand {
            category: 7,
            ranks: vec![four, singles[0]],
            name: "four_of_a_kind",
        };
    }

    if trip > 0 && !pairs.is_empty() {
        return EvaluatedHand {
            category: if short_deck { 5 } else { 6 },
            ranks: vec![trip, pairs[0]],
            name: "full_house",
        };
    }

    if flush {
        return EvaluatedHand {
            category: if short_deck { 6 } else { 5 },
            ranks: ranks.to_vec(),
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

    if trip > 0 {
        let mut tie_breakers = vec![trip];
        tie_breakers.extend(singles);
        return EvaluatedHand {
            category: 3,
            ranks: tie_breakers,
            name: "three_of_a_kind",
        };
    }

    if pairs.len() >= 2 {
        return EvaluatedHand {
            category: 2,
            ranks: vec![pairs[0], pairs[1], singles[0]],
            name: "two_pair",
        };
    }

    if pairs.len() == 1 {
        let mut tie_breakers = vec![pairs[0]];
        tie_breakers.extend(singles);
        return EvaluatedHand {
            category: 1,
            ranks: tie_breakers,
            name: "one_pair",
        };
    }

    EvaluatedHand {
        category: 0,
        ranks: ranks.to_vec(),
        name: "high_card",
    }
}

pub fn evaluate_omaha(hole_cards: &[i32], public_cards: &[i32]) -> Option<EvaluatedHand> {
    if hole_cards.len() < 2 || public_cards.len() < 3 {
        return None;
    }
    let mut best: Option<EvaluatedHand> = None;
    for a in 0..hole_cards.len() - 1 {
        for b in a + 1..hole_cards.len() {
            for c in 0..public_cards.len() - 2 {
                for d in c + 1..public_cards.len() - 1 {
                    for e in d + 1..public_cards.len() {
                        let hand = evaluate_five(&[
                            hole_cards[a],
                            hole_cards[b],
                            public_cards[c],
                            public_cards[d],
                            public_cards[e],
                        ]);
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

fn straight_high(counts: &[u8; 15], short_deck: bool) -> Option<i32> {
    if short_deck && counts[14] > 0 && (6..=9).all(|rank| counts[rank] > 0) {
        return Some(9);
    }
    for high in (6..=14).rev() {
        if (high - 4..=high).all(|rank| counts[rank as usize] > 0) {
            return Some(high);
        }
    }
    (counts[14] > 0 && (2..=5).all(|rank| counts[rank] > 0)).then_some(5)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn card(rank: i32, suit: i32) -> i32 {
        suit * 13 + rank - 1
    }

    fn hand(cards: [(i32, i32); 5]) -> [i32; 5] {
        cards.map(|(rank, suit)| card(rank, suit))
    }

    #[test]
    fn standard_categories_have_the_expected_order() {
        let hands = [
            hand([(14, 0), (13, 1), (12, 2), (11, 3), (9, 0)]),
            hand([(14, 0), (14, 1), (12, 2), (11, 3), (9, 0)]),
            hand([(14, 0), (14, 1), (12, 2), (12, 3), (9, 0)]),
            hand([(14, 0), (14, 1), (14, 2), (11, 3), (9, 0)]),
            hand([(9, 0), (8, 1), (7, 2), (6, 3), (5, 0)]),
            hand([(14, 0), (12, 0), (10, 0), (7, 0), (3, 0)]),
            hand([(14, 0), (14, 1), (14, 2), (12, 0), (12, 1)]),
            hand([(14, 0), (14, 1), (14, 2), (14, 3), (12, 0)]),
            hand([(9, 0), (8, 0), (7, 0), (6, 0), (5, 0)]),
        ];
        let evaluated = hands.map(|cards| evaluate_five(&cards));
        assert_eq!(
            evaluated.each_ref().map(|value| value.category),
            [0, 1, 2, 3, 4, 5, 6, 7, 8]
        );
        assert!(evaluated.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn every_category_uses_all_required_tie_breakers() {
        let stronger = [
            hand([(14, 0), (13, 1), (12, 2), (11, 3), (9, 0)]),
            hand([(14, 0), (14, 1), (13, 2), (11, 3), (9, 0)]),
            hand([(14, 0), (14, 1), (12, 2), (12, 3), (10, 0)]),
            hand([(14, 0), (14, 1), (14, 2), (13, 3), (9, 0)]),
            hand([(10, 0), (9, 1), (8, 2), (7, 3), (6, 0)]),
            hand([(14, 0), (13, 0), (11, 0), (8, 0), (4, 0)]),
            hand([(14, 0), (14, 1), (14, 2), (13, 0), (13, 1)]),
            hand([(14, 0), (14, 1), (14, 2), (14, 3), (13, 0)]),
            hand([(10, 0), (9, 0), (8, 0), (7, 0), (6, 0)]),
        ];
        let weaker = [
            hand([(14, 1), (13, 2), (12, 3), (11, 0), (8, 1)]),
            hand([(14, 2), (14, 3), (12, 0), (11, 1), (9, 2)]),
            hand([(14, 2), (14, 3), (12, 0), (12, 1), (9, 2)]),
            hand([(14, 0), (14, 1), (14, 2), (12, 3), (11, 0)]),
            hand([(9, 1), (8, 2), (7, 3), (6, 0), (5, 1)]),
            hand([(14, 1), (13, 1), (10, 1), (8, 1), (4, 1)]),
            hand([(13, 0), (13, 1), (13, 2), (14, 0), (14, 1)]),
            hand([(14, 0), (14, 1), (14, 2), (14, 3), (12, 0)]),
            hand([(9, 1), (8, 1), (7, 1), (6, 1), (5, 1)]),
        ];
        for (stronger, weaker) in stronger.into_iter().zip(weaker) {
            assert!(evaluate_five(&stronger) > evaluate_five(&weaker));
        }
    }

    #[test]
    fn equal_ranks_ignore_suits() {
        let left = hand([(14, 0), (13, 1), (12, 2), (11, 3), (9, 0)]);
        let right = hand([(14, 1), (13, 2), (12, 3), (11, 0), (9, 1)]);
        assert_eq!(evaluate_five(&left), evaluate_five(&right));
    }

    #[test]
    fn best_of_seven_checks_all_twenty_one_combinations() {
        let cards = [11, 24, 37, 10, 23, 4, 5];
        let best = evaluate_best(&cards).expect("seven cards produce a hand");
        assert_eq!(best.category, 6);
        assert_eq!(best.ranks, vec![12, 11]);
    }

    #[test]
    fn wheel_and_short_deck_ace_low_straights_are_supported() {
        let wheel = hand([(14, 0), (5, 1), (4, 2), (3, 3), (2, 0)]);
        assert_eq!(evaluate_five(&wheel).ranks, vec![5]);

        let short = hand([(14, 0), (9, 1), (8, 2), (7, 3), (6, 0)]);
        let short = evaluate_short_deck_best(&short).expect("short-deck straight");
        assert_eq!((short.category, short.ranks), (4, vec![9]));
    }

    #[test]
    fn short_deck_flush_beats_full_house() {
        let flush = hand([(14, 0), (12, 0), (10, 0), (8, 0), (6, 0)]);
        let full_house = hand([(14, 0), (14, 1), (14, 2), (12, 0), (12, 1)]);
        assert!(
            evaluate_short_deck_best(&flush).expect("flush")
                > evaluate_short_deck_best(&full_house).expect("full house")
        );
    }

    #[test]
    fn omaha_uses_exactly_two_hole_and_three_board_cards() {
        let royal = evaluate_omaha(
            &[card(14, 0), card(13, 0), card(2, 1), card(3, 2)],
            &[
                card(12, 0),
                card(11, 0),
                card(10, 0),
                card(9, 1),
                card(8, 2),
            ],
        )
        .expect("valid Omaha hand");
        assert_eq!(royal.category, 8);

        let board_royal = evaluate_omaha(
            &[card(2, 1), card(3, 2), card(4, 3), card(5, 1)],
            &[
                card(14, 0),
                card(13, 0),
                card(12, 0),
                card(11, 0),
                card(10, 0),
            ],
        )
        .expect("valid Omaha hand");
        assert_ne!(board_royal.category, 8);
    }

    #[test]
    fn all_standard_five_card_hands_match_known_category_frequencies() {
        let mut counts = [0_u32; 9];
        for a in 1..=48 {
            for b in a + 1..=49 {
                for c in b + 1..=50 {
                    for d in c + 1..=51 {
                        for e in d + 1..=52 {
                            let value = evaluate_five(&[a, b, c, d, e]);
                            counts[value.category as usize] += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(
            counts,
            [
                1_302_540, 1_098_240, 123_552, 54_912, 10_200, 5_108, 3_744, 624, 40
            ]
        );
        assert_eq!(counts.into_iter().sum::<u32>(), 2_598_960);
    }
}
