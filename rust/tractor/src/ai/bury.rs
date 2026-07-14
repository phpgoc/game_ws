//! Dealer bottom-card planning.
//!
//! `ProtectBottom` keeps trump control and removes cheap short-suit cards.
//! `RunLongSuit` accepts more bottom risk to preserve a dominant plain suit and
//! its tractors. The plan is selected from the actual hand instead of applying
//! one fixed "discard the lowest non-trumps" ordering to every deal.

use std::collections::{HashMap, HashSet};

use crate::{
    ai::bid,
    combo::{self, ComboKind},
    game_state::{
        TractorGameState, base_card, card_rank, card_score, card_suit, is_trump_card,
        tractor_card_value,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BuryMode {
    ProtectBottom,
    RunLongSuit(i32),
}

#[derive(Debug, Clone)]
pub(crate) struct BuryDecision {
    pub(crate) cards: Vec<i32>,
    pub(crate) mode: BuryMode,
}

#[derive(Debug, Clone, Copy)]
struct SuitProfile {
    suit: i32,
    count: usize,
    pairs: usize,
    longest_tractor: usize,
}

pub(crate) fn choose_bury(state: &TractorGameState) -> Option<Vec<i32>> {
    bury_decision(state).map(|decision| {
        let BuryDecision { cards, mode } = decision;
        let _runs_long_suit = matches!(mode, BuryMode::RunLongSuit(_));
        cards
    })
}

pub(crate) fn bury_decision(state: &TractorGameState) -> Option<BuryDecision> {
    let position = state.dealer_position;
    let hand = state.hands.get(&position)?;
    if hand.len() < state.rules.bottom_card_count {
        return None;
    }
    let profiles = suit_profiles(hand, state);
    let longest = profiles.iter().max_by_key(|profile| {
        (
            profile.count,
            profile.longest_tractor,
            profile.pairs,
            std::cmp::Reverse(profile.suit),
        )
    });
    let mode = choose_mode(state, position, hand.len(), longest);
    let keep_suit = match mode {
        BuryMode::ProtectBottom => None,
        BuryMode::RunLongSuit(suit) => Some(suit),
    };
    let void_suit = profiles
        .iter()
        .filter(|profile| Some(profile.suit) != keep_suit)
        .filter(|profile| profile.count <= state.rules.bottom_card_count)
        .min_by_key(|profile| (profile.count, profile.pairs, profile.suit))
        .map(|profile| profile.suit);

    let mut identity_total: HashMap<i32, usize> = HashMap::new();
    for card in hand {
        *identity_total.entry(base_card(*card)).or_default() += 1;
    }
    let tractor_bases: HashSet<i32> = combo::enumerate_leads(hand, &state.rules)
        .into_iter()
        .filter(|cards| {
            matches!(
                combo::classify(cards, &state.rules).map(|combo| combo.kind),
                Some(ComboKind::Tractor(_))
            )
        })
        .flatten()
        .map(base_card)
        .collect();
    let suit_totals: HashMap<i32, usize> = profiles
        .iter()
        .map(|profile| (profile.suit, profile.count))
        .collect();

    let mut remaining = hand.clone();
    let mut selected = Vec::with_capacity(state.rules.bottom_card_count);
    let mut selected_by_base: HashMap<i32, usize> = HashMap::new();
    let mut selected_by_suit: HashMap<i32, usize> = HashMap::new();
    while selected.len() < state.rules.bottom_card_count {
        let (index, _) = remaining.iter().enumerate().min_by_key(|(_, card)| {
            marginal_bury_cost(
                **card,
                mode,
                keep_suit,
                void_suit,
                &identity_total,
                &selected_by_base,
                &suit_totals,
                &selected_by_suit,
                &tractor_bases,
                state,
            )
        })?;
        let card = remaining.remove(index);
        *selected_by_base.entry(base_card(card)).or_default() += 1;
        if let Some(suit) = card_suit(card) {
            *selected_by_suit.entry(suit).or_default() += 1;
        }
        selected.push(card);
    }
    selected.sort_by_key(|card| tractor_card_value(*card, &state.rules, None));
    Some(BuryDecision {
        cards: selected,
        mode,
    })
}

fn choose_mode(
    state: &TractorGameState,
    position: usize,
    hand_len: usize,
    longest: Option<&SuitProfile>,
) -> BuryMode {
    let Some(longest) = longest else {
        return BuryMode::ProtectBottom;
    };
    let assessment = state
        .rules
        .trump_suit
        .map(|suit| bid::assess_trump(state, position, suit));
    let secure_trump_control = assessment.as_ref().is_some_and(|assessment| {
        assessment.success_probability >= 0.62
            && assessment.trump_count as f64 >= assessment.expected_trump_count
            && (assessment.trump_pairs >= 2 || assessment.longest_trump_tractor >= 2)
    });
    let dominant_plain_suit = longest.count >= hand_len.div_ceil(3)
        && (longest.pairs >= 2 || longest.longest_tractor >= 2);
    let exceptional_plain_run = longest.count * 5 >= hand_len * 2
        || longest.longest_tractor >= 3
        || assessment
            .as_ref()
            .is_some_and(|assessment| assessment.longest_plain_suit * 5 >= hand_len * 2);

    if dominant_plain_suit && (!secure_trump_control || exceptional_plain_run) {
        BuryMode::RunLongSuit(longest.suit)
    } else {
        BuryMode::ProtectBottom
    }
}

#[allow(clippy::too_many_arguments)]
fn marginal_bury_cost(
    card: i32,
    mode: BuryMode,
    keep_suit: Option<i32>,
    void_suit: Option<i32>,
    identity_total: &HashMap<i32, usize>,
    selected_by_base: &HashMap<i32, usize>,
    suit_totals: &HashMap<i32, usize>,
    selected_by_suit: &HashMap<i32, usize>,
    tractor_bases: &HashSet<i32>,
    state: &TractorGameState,
) -> (i32, i32) {
    let base = base_card(card);
    let natural_suit = card_suit(card);
    let trump_cost = if is_trump_card(card, &state.rules) {
        match mode {
            BuryMode::ProtectBottom => 12_000,
            BuryMode::RunLongSuit(_) => 7_500,
        }
    } else {
        0
    };
    let keep_cost = if natural_suit == keep_suit { 9_000 } else { 0 };
    let point_cost = card_score(card)
        * match mode {
            BuryMode::ProtectBottom => 450,
            BuryMode::RunLongSuit(_) => 60,
        };
    let control_cost = card_rank(card) * 12;

    let total_identity = identity_total.get(&base).copied().unwrap_or(1);
    let already_selected = selected_by_base.get(&base).copied().unwrap_or(0);
    let pairs_before = (total_identity - already_selected) / 2;
    let pairs_after = (total_identity - already_selected - 1) / 2;
    let pair_loss = (pairs_before - pairs_after) as i32
        * if tractor_bases.contains(&base) {
            1_900
        } else {
            1_050
        };

    let void_bonus = if natural_suit == void_suit {
        let suit = natural_suit.unwrap_or_default();
        let total = suit_totals.get(&suit).copied().unwrap_or_default();
        let selected = selected_by_suit.get(&suit).copied().unwrap_or_default();
        if selected + 1 == total { -1_600 } else { -220 }
    } else {
        0
    };

    (
        trump_cost + keep_cost + point_cost + control_cost + pair_loss + void_bonus,
        card,
    )
}

fn suit_profiles(hand: &[i32], state: &TractorGameState) -> Vec<SuitProfile> {
    (0..4)
        .filter_map(|suit| {
            let cards: Vec<_> = hand
                .iter()
                .copied()
                .filter(|card| {
                    !is_trump_card(*card, &state.rules) && card_suit(*card) == Some(suit)
                })
                .collect();
            if cards.is_empty() {
                return None;
            }
            let mut identities: HashMap<i32, usize> = HashMap::new();
            for card in &cards {
                *identities.entry(base_card(*card)).or_default() += 1;
            }
            let pairs = identities.values().map(|count| count / 2).sum();
            let longest_tractor = combo::enumerate_leads(&cards, &state.rules)
                .into_iter()
                .filter_map(|cards| match combo::classify(&cards, &state.rules)?.kind {
                    ComboKind::Tractor(pairs) => Some(pairs),
                    _ => None,
                })
                .max()
                .unwrap_or_default();
            Some(SuitProfile {
                suit,
                count: cards.len(),
                pairs,
                longest_tractor,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use share_type_public::{TractorPhase, TractorRank, TractorSuit};
    use ws_common::CommonGameState;

    use super::*;

    fn state(hand: Vec<i32>, bottom_card_count: usize) -> TractorGameState {
        let mut common = CommonGameState::new();
        for position in 0..4 {
            common.add_player(position, position as u64 + 1, &format!("u{position}"));
        }
        let mut state = TractorGameState::from_common(Arc::new(Mutex::new(common)));
        state.phase = TractorPhase::Bury;
        state.rules.target_rank = TractorRank::TWO;
        state.rules.trump_suit = Some(TractorSuit::SPADE);
        state.rules.deck_count = 3;
        state.rules.bottom_card_count = bottom_card_count;
        state.dealer_position = 0;
        state.hands.insert(0, hand);
        state
    }

    #[test]
    fn protect_bottom_keeps_trumps_pairs_and_avoids_points() {
        let state = state(
            vec![
                53, 153, 1, 101, 13, 113, // trump controls
                15, 16, 17, 18, 19, // loose hearts
                29, 30, 31, 32, // loose clubs
                42, 43, 44, 45, // loose diamonds
            ],
            4,
        );
        let decision = bury_decision(&state).expect("bury decision");
        assert_eq!(decision.mode, BuryMode::ProtectBottom);
        assert!(
            decision
                .cards
                .iter()
                .all(|card| !is_trump_card(*card, &state.rules))
        );
        assert_eq!(
            decision
                .cards
                .iter()
                .map(|card| card_score(*card))
                .sum::<i32>(),
            0
        );
    }

    #[test]
    fn dominant_plain_tractor_selects_run_long_suit_plan() {
        let state = state(
            vec![
                53, 1, 13, // limited trump
                15, 115, 16, 116, 17, 117, 18, 118, 19, 119, // long heart tractor
                31, 32, // short clubs
                43, 44, 45, // short diamonds, includes a point card
            ],
            4,
        );
        let decision = bury_decision(&state).expect("bury decision");
        assert_eq!(decision.mode, BuryMode::RunLongSuit(1));
        assert!(
            decision
                .cards
                .iter()
                .all(|card| card_suit(*card) != Some(1))
        );
    }

    #[test]
    fn three_of_a_kind_can_shed_one_copy_without_destroying_the_pair() {
        let state = state(
            vec![
                53, 1, 13, 15, 115, 215, // three identical hearts
                29, 30, 31, 32, 42, 43, 44,
            ],
            1,
        );
        let decision = bury_decision(&state).expect("bury decision");
        if base_card(decision.cards[0]) == 15 {
            assert_eq!(
                state.hands[&0]
                    .iter()
                    .filter(|card| base_card(**card) == 15)
                    .count()
                    - 1,
                2
            );
        }
    }
}
