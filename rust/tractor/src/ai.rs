//! Tractor AI.
//!
//! Following logic is shared with the safe auto-play in [`TractorGameState`]
//! (beat opponents cheaply, feed points to a winning partner, otherwise shed
//! low). The AI adds combo-aware leading: it looks for tractors and pairs, and
//! decides whether to probe with a low card or cash a guaranteed winner.

use crate::{
    combo::{self, ComboKind},
    game_state::{TractorGameState, card_rank, card_suit, is_trump_card, tractor_card_value},
};

pub fn decide(state: &TractorGameState, position: usize) -> Option<Vec<i32>> {
    let hand = state.hands.get(&position)?;
    if hand.is_empty() {
        return None;
    }

    if state.current_trick.is_empty() {
        lead_play(state, position, hand)
    } else {
        state.choose_auto_play(position)
    }
}

/// Choose an opening play. Preference order:
///   1. A tractor (连对) — hardest for opponents to answer.
///   2. A low pair in a plain suit, to probe and draw out trumps.
///   3. A guaranteed-winning high single once the hand is short.
///   4. Otherwise the lowest single, keeping strong cards back.
fn lead_play(state: &TractorGameState, position: usize, hand: &[i32]) -> Option<Vec<i32>> {
    let rules = &state.rules;
    let value = |cards: &[i32]| {
        cards
            .iter()
            .map(|card| tractor_card_value(*card, rules, None))
            .max()
            .unwrap_or_default()
    };

    let leads = combo::enumerate_leads(hand, rules);

    // 1. Prefer the longest tractor; break ties by playing the weaker one.
    if let Some(tractor) = leads
        .iter()
        .filter(|cards| {
            matches!(
                combo::classify(cards, rules).map(|c| c.kind),
                Some(ComboKind::Tractor(_))
            )
        })
        .max_by_key(|cards| (cards.len(), std::cmp::Reverse(value(cards))))
    {
        return Some(tractor.clone());
    }

    // 2. Lead a low plain-suit pair to probe (keep trump pairs in reserve).
    let plain_pairs: Vec<&Vec<i32>> = leads
        .iter()
        .filter(|cards| {
            combo::classify(cards, rules).map(|c| c.kind) == Some(ComboKind::Pair)
                && cards.iter().all(|card| !is_trump_card(*card, rules))
        })
        .collect();
    if let Some(pair) = plain_pairs.iter().min_by_key(|cards| value(cards)) {
        return Some((*pair).clone());
    }

    // 3. Late in the hand, cash a guaranteed winner (a card nothing outranks).
    if hand.len() <= 5
        && let Some(top) = highest_guaranteed_single(state, position, hand)
    {
        return Some(vec![top]);
    }

    // 4. Otherwise sluff the lowest single, holding strength back. Prefer a
    //    short plain suit so we can create a void to trump later.
    lowest_lead_single(hand, rules).map(|card| vec![card])
}

/// The strongest single card in hand that no opponent card can beat, if any.
fn highest_guaranteed_single(
    state: &TractorGameState,
    position: usize,
    hand: &[i32],
) -> Option<i32> {
    let rules = &state.rules;
    let best = *hand
        .iter()
        .max_by_key(|card| tractor_card_value(**card, rules, None))?;
    let best_value = tractor_card_value(best, rules, None);
    let outranked_by_opponent = state
        .hands
        .iter()
        .filter(|(pos, _)| !crate::game_state::same_team(**pos, position))
        .flat_map(|(_, cards)| cards.iter())
        .any(|card| tractor_card_value(*card, rules, None) > best_value);
    (!outranked_by_opponent).then_some(best)
}

/// Lowest single to lead: prefer the lowest card of the shortest plain suit so
/// the AI works toward a void; fall back to the globally lowest card. Ties are
/// broken by rank then card id to stay deterministic.
fn lowest_lead_single(hand: &[i32], rules: &crate::game_state::TractorRules) -> Option<i32> {
    use std::collections::HashMap;

    let mut suit_len: HashMap<i32, usize> = HashMap::new();
    for card in hand {
        if !is_trump_card(*card, rules)
            && let Some(suit) = card_suit(*card)
        {
            *suit_len.entry(suit).or_default() += 1;
        }
    }
    // Among plain cards, minimise (shortest suit, lowest rank, lowest id).
    let plain_best = hand
        .iter()
        .filter(|card| !is_trump_card(**card, rules))
        .min_by_key(|card| {
            let suit = card_suit(**card).unwrap_or(i32::MAX);
            (
                suit_len.get(&suit).copied().unwrap_or(usize::MAX),
                card_rank(**card),
                **card,
            )
        })
        .copied();
    plain_best.or_else(|| {
        hand.iter()
            .min_by_key(|card| (tractor_card_value(**card, rules, None), **card))
            .copied()
    })
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use share_type_public::{TractorPhase, TractorRank, WsTractorPlayedCards};
    use ws_common::CommonGameState;

    use super::*;
    use crate::game_state::{TractorGameState, TractorRules};

    fn test_state(target: TractorRank) -> TractorGameState {
        let mut common = CommonGameState::new();
        for position in 0..4 {
            common.add_player(position, position as u64 + 1, &format!("u{position}"));
        }
        let mut state = TractorGameState::from_common(Arc::new(Mutex::new(common)));
        state.phase = TractorPhase::Play;
        state.rules = TractorRules {
            blood_enabled: true,
            blood_score_per_unit: 40,
            blood_start_score: 80,
            bottom_card_count: 8,
            deck_count: 2,
            final_target_rank: TractorRank::A,
            removed_rank_count: 0,
            target_rank: target,
            trump_suit: None,
        };
        state.current_position = 0;
        state
    }

    #[test]
    fn ai_leads_a_tractor_when_available() {
        let mut state = test_state(TractorRank::TWO);
        // suit-0 rank3 and rank4 pairs form a tractor, plus a loose card.
        state.hands.insert(0, vec![2, 102, 3, 103, 20]);
        let play = decide(&state, 0).expect("lead");
        assert_eq!(
            combo::classify(&play, &state.rules).map(|c| c.kind),
            Some(ComboKind::Tractor(2))
        );
    }

    #[test]
    fn ai_leads_low_plain_pair_over_high_one() {
        let mut state = test_state(TractorRank::TWO);
        // Two plain pairs (rank3 low, rank9 high) and no tractor.
        state.hands.insert(0, vec![2, 102, 8, 108, 20]);
        let play = decide(&state, 0).expect("lead");
        assert_eq!(
            combo::classify(&play, &state.rules).map(|c| c.kind),
            Some(ComboKind::Pair)
        );
        // The lower pair (rank3 = base 2) is chosen.
        assert_eq!(play, vec![2, 102]);
    }

    #[test]
    fn ai_following_uses_smallest_winning_card() {
        let mut state = test_state(TractorRank::A);
        state.current_position = 1;
        state.current_trick.push(WsTractorPlayedCards {
            position: 0,
            name: "u0".to_owned(),
            cards: vec![4],
        });
        state.hands.insert(1, vec![5, 6, 13]);
        assert_eq!(decide(&state, 1), Some(vec![5]));
    }

    #[test]
    fn ai_returns_none_without_cards() {
        let state = test_state(TractorRank::A);
        assert_eq!(decide(&state, 0), None);
    }
}
