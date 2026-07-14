//! Trump-selection and first-round declaration evaluation.

use std::collections::HashMap;

use share_type_public::TractorSuit;

use crate::{
    combo::{self, ComboKind},
    game_state::{
        TractorGameState, TractorRules, base_card, build_tractor_deck_with_removed_ranks,
        card_rank, card_suit, is_trump_card,
    },
};

const SUITS: [TractorSuit; 4] = [
    TractorSuit::SPADE,
    TractorSuit::HEART,
    TractorSuit::CLUB,
    TractorSuit::DIAMOND,
];

#[derive(Debug, Clone)]
pub(crate) struct TrumpAssessment {
    pub(crate) suit: TractorSuit,
    pub(crate) score: i32,
    pub(crate) success_probability: f64,
    pub(crate) trump_count: usize,
    pub(crate) expected_trump_count: f64,
    pub(crate) trump_pairs: usize,
    pub(crate) longest_trump_tractor: usize,
    pub(crate) longest_plain_suit: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct DeclarationDecision {
    pub(crate) cards: Vec<i32>,
    pub(crate) assessment: TrumpAssessment,
}

pub(crate) fn assess_trump(
    state: &TractorGameState,
    position: usize,
    suit: TractorSuit,
) -> TrumpAssessment {
    let hand = state
        .hands
        .get(&position)
        .map(Vec::as_slice)
        .unwrap_or_default();
    assess_hand(hand, &state.rules, suit)
}

pub(crate) fn best_trump_suit(state: &TractorGameState, position: usize) -> TractorSuit {
    SUITS
        .into_iter()
        .map(|suit| assess_trump(state, position, suit))
        .max_by(|left, right| {
            left.success_probability
                .total_cmp(&right.success_probability)
                .then_with(|| left.score.cmp(&right.score))
                .then_with(|| (right.suit as i32).cmp(&(left.suit as i32)))
        })
        .map(|assessment| assessment.suit)
        .unwrap_or(TractorSuit::SPADE)
}

pub(crate) fn declaration_decision(
    state: &TractorGameState,
    position: usize,
    current_strength: i32,
    forced: bool,
) -> Option<DeclarationDecision> {
    let hand = state.hands.get(&position)?;
    let mut by_base: HashMap<i32, Vec<i32>> = HashMap::new();
    for card in hand
        .iter()
        .filter(|card| card_rank(**card) == state.rules.target_rank as i32)
    {
        by_base.entry(base_card(*card)).or_default().push(*card);
    }

    by_base
        .into_values()
        .filter(|cards| cards.len() as i32 > current_strength)
        .filter_map(|mut cards| {
            cards.sort_unstable();
            let suit = card_suit(cards[0]).and_then(suit_from_index)?;
            let assessment = assess_trump(state, position, suit);
            // The end-of-deal fallback may be slightly more adventurous, but
            // it must not turn into "show any lone level card". Passing and
            // playing no-suit trump is preferable to volunteering a clearly
            // weak contract.
            let fallback_discount = if forced { 0.04 } else { 0.0 };
            let threshold =
                declaration_threshold(cards.len(), current_strength > 0) - fallback_discount;
            (assessment.success_probability >= threshold)
                .then_some(DeclarationDecision { cards, assessment })
        })
        .max_by(|left, right| {
            left.cards
                .len()
                .cmp(&right.cards.len())
                .then_with(|| {
                    left.assessment
                        .success_probability
                        .total_cmp(&right.assessment.success_probability)
                })
                .then_with(|| left.assessment.score.cmp(&right.assessment.score))
        })
}

fn assess_hand(hand: &[i32], rules: &TractorRules, suit: TractorSuit) -> TrumpAssessment {
    let mut hypothetical = rules.clone();
    hypothetical.trump_suit = Some(suit);
    let trump_cards: Vec<_> = hand
        .iter()
        .copied()
        .filter(|card| is_trump_card(*card, &hypothetical))
        .collect();
    let mut identity_counts: HashMap<i32, usize> = HashMap::new();
    for card in &trump_cards {
        *identity_counts.entry(base_card(*card)).or_default() += 1;
    }
    let trump_pairs = identity_counts
        .values()
        .map(|count| count / 2)
        .sum::<usize>();
    let longest_trump_tractor = combo::enumerate_leads(&trump_cards, &hypothetical)
        .into_iter()
        .filter_map(|cards| match combo::classify(&cards, &hypothetical)?.kind {
            ComboKind::Tractor(pairs) => Some(pairs),
            _ => None,
        })
        .max()
        .unwrap_or_default();

    let mut plain_lengths = [0usize; 4];
    let mut side_aces = 0usize;
    let mut side_pairs = 0usize;
    let mut plain_identities: HashMap<(i32, i32), usize> = HashMap::new();
    for card in hand
        .iter()
        .copied()
        .filter(|card| !is_trump_card(*card, &hypothetical))
    {
        let natural_suit = card_suit(card).unwrap_or_default();
        plain_lengths[natural_suit as usize] += 1;
        side_aces += usize::from(card_rank(card) == 14);
        *plain_identities
            .entry((natural_suit, base_card(card)))
            .or_default() += 1;
    }
    for count in plain_identities.values() {
        side_pairs += count / 2;
    }
    let longest_plain_suit = plain_lengths.into_iter().max().unwrap_or_default();
    let short_plain_suits = plain_lengths
        .into_iter()
        .filter(|length| *length <= 2)
        .count();

    let top_trump_weight = trump_cards
        .iter()
        .map(|card| {
            let base = base_card(*card);
            if base == 54 {
                8
            } else if base == 53 {
                6
            } else if card_rank(*card) == rules.target_rank as i32 {
                if card_suit(*card) == Some(suit as i32) {
                    6
                } else {
                    4
                }
            } else {
                match card_rank(*card) {
                    14 => 3,
                    13 => 2,
                    _ => 0,
                }
            }
        })
        .sum::<i32>();

    let deck = build_tractor_deck_with_removed_ranks(rules.deck_count, rules.removed_rank_count);
    let total_trumps = deck
        .iter()
        .filter(|card| is_trump_card(**card, &hypothetical))
        .count();
    let expected_trump_count = if deck.is_empty() {
        0.0
    } else {
        total_trumps as f64 * hand.len() as f64 / deck.len() as f64
    };
    let trump_surplus = trump_cards.len() as f64 - expected_trump_count;
    let score = trump_cards.len() as i32 * 8
        + top_trump_weight * 3
        + trump_pairs as i32 * 9
        + longest_trump_tractor as i32 * 8
        + side_aces as i32 * 3
        + side_pairs as i32 * 2
        + longest_plain_suit as i32
        + short_plain_suits as i32 * 2;
    let success_probability = (0.44
        + trump_surplus * 0.035
        + top_trump_weight as f64 * 0.008
        + trump_pairs as f64 * 0.025
        + longest_trump_tractor as f64 * 0.025
        + side_aces as f64 * 0.008
        + side_pairs as f64 * 0.006
        + short_plain_suits as f64 * 0.008)
        .clamp(0.08, 0.94);

    TrumpAssessment {
        suit,
        score,
        success_probability,
        trump_count: trump_cards.len(),
        expected_trump_count,
        trump_pairs,
        longest_trump_tractor,
        longest_plain_suit,
    }
}

fn declaration_threshold(strength: usize, countering: bool) -> f64 {
    let base = match strength {
        0 | 1 => 0.60,
        2 => 0.51,
        3 => 0.45,
        _ => 0.40,
    };
    base + if countering { 0.025 } else { 0.0 }
}

fn suit_from_index(index: i32) -> Option<TractorSuit> {
    match index {
        0 => Some(TractorSuit::SPADE),
        1 => Some(TractorSuit::HEART),
        2 => Some(TractorSuit::CLUB),
        3 => Some(TractorSuit::DIAMOND),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use share_type_public::{TractorPhase, TractorRank};
    use ws_common::CommonGameState;

    use super::*;

    fn state(hand: Vec<i32>) -> TractorGameState {
        let mut common = CommonGameState::new();
        for position in 0..4 {
            common.add_player(position, position as u64 + 1, &format!("u{position}"));
        }
        let mut state = TractorGameState::from_common(Arc::new(Mutex::new(common)));
        state.phase = TractorPhase::Deal;
        state.rules.target_rank = TractorRank::TWO;
        state.rules.deck_count = 3;
        state.hands.insert(0, hand);
        state
    }

    #[test]
    fn a_lone_level_card_does_not_force_a_weak_declaration() {
        let state = state(vec![1, 15, 16, 17, 18, 29, 30, 31, 32, 42, 43, 44, 45, 46]);
        assert!(declaration_decision(&state, 0, 0, false).is_none());
        assert!(declaration_decision(&state, 0, 0, true).is_none());
    }

    #[test]
    fn paired_level_cards_and_trump_support_justify_countering() {
        let state = state(vec![
            1, 101, 53, 153, 54, 154, 13, 113, 12, 112, 11, 111, 20, 21,
        ]);
        let decision = declaration_decision(&state, 0, 1, false).expect("strong counter");
        assert_eq!(decision.cards, vec![1, 101]);
        assert!(decision.assessment.success_probability >= 0.525);
    }

    #[test]
    fn later_trump_selection_values_control_above_raw_suit_length() {
        let state = state(vec![
            // Spades: slightly shorter, but joker/level support combines with
            // an A-K-Q tractor and two identity pairs.
            1, 101, 11, 111, 12, 112, 13, 113, 53, 153, // Hearts: longer loose low cards.
            15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
        ]);
        assert_eq!(best_trump_suit(&state, 0), TractorSuit::SPADE);
    }
}
