//! Tractor AI.
//!
//! Following logic is shared with the safe auto-play in [`TractorGameState`]
//! (beat opponents cheaply, feed points to a winning partner, otherwise shed
//! low). The AI adds combo-aware leading: it looks for tractors and pairs, and
//! decides whether to probe with a low card or cash a guaranteed winner.

mod bid;
mod bury;
mod knowledge;

use crate::{
    combo::{self, ComboKind},
    game_state::{
        TractorGameState, card_rank, card_score, card_suit, is_trump_card, tractor_card_value,
    },
};

use self::knowledge::PublicKnowledge;

pub(crate) use bid::{best_trump_suit, declaration_decision};
pub(crate) use bury::choose_bury;

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

/// Choose an opening play. The AI estimates whether every legal opponent reply
/// can beat a candidate, so it can distinguish a controlling tractor from a
/// merely long but vulnerable one. It still uses low pairs/tractors to strip
/// shapes early, then cashes control cards once the hand gets shorter.
fn lead_play(state: &TractorGameState, position: usize, hand: &[i32]) -> Option<Vec<i32>> {
    let rules = &state.rules;
    let knowledge = PublicKnowledge::from_state(state, position);
    let value = |cards: &[i32]| {
        cards
            .iter()
            .map(|card| tractor_card_value(*card, rules, None))
            .max()
            .unwrap_or_default()
    };

    let leads = combo::enumerate_leads(hand, rules);

    // When every other player has publicly shown void in a long plain suit,
    // release the longest tractor together. This forces opponents to spend
    // trump pairs (or discard) instead of wasting repeated single leads.
    if let Some(tractor) = leads
        .iter()
        .filter(|cards| {
            let Some(combo) = combo::classify(cards, rules) else {
                return false;
            };
            matches!(combo.kind, ComboKind::Tractor(_))
                && combo.suit.is_some()
                && knowledge.all_other_players_void(combo.suit)
        })
        .max_by_key(|cards| (cards.len(), value(cards)))
    {
        return Some(tractor.clone());
    }

    // 1. Prefer a controlling tractor, then length. A vulnerable tractor is
    // played low to strip opponents' pairs without burning the team's control.
    if let Some(tractor) = leads
        .iter()
        .filter(|cards| {
            matches!(
                combo::classify(cards, rules).map(|c| c.kind),
                Some(ComboKind::Tractor(_))
            )
        })
        .max_by_key(|cards| {
            let probability = knowledge.lead_control_probability(state, cards);
            let controlled = probability >= 0.82;
            (
                controlled,
                cards.len(),
                if controlled {
                    value(cards)
                } else {
                    -value(cards)
                },
                cards.iter().map(|card| card_score(*card)).sum::<i32>(),
            )
        })
    {
        return Some(tractor.clone());
    }

    // 2. Lead pairs before falling back to singles. Early in a round, probe a
    // short plain suit with a cheap non-point pair. Late, cash the strongest
    // controlled pair. Trump pairs are preserved unless no plain pair remains.
    let pairs: Vec<&Vec<i32>> = leads
        .iter()
        .filter(|cards| combo::classify(cards, rules).map(|c| c.kind) == Some(ComboKind::Pair))
        .collect();
    if hand.len() <= 12
        && let Some(pair) = pairs
            .iter()
            .filter(|cards| knowledge.lead_control_probability(state, cards) >= 0.82)
            .max_by_key(|cards| {
                (
                    cards.iter().map(|card| card_score(*card)).sum::<i32>(),
                    value(cards),
                )
            })
    {
        return Some((**pair).clone());
    }
    let mut suit_len = [0usize; 4];
    for card in hand.iter().filter(|card| !is_trump_card(**card, rules)) {
        if let Some(suit) = card_suit(*card) {
            suit_len[suit as usize] += 1;
        }
    }
    if let Some(pair) = pairs.iter().min_by_key(|cards| {
        let trump = cards.iter().any(|card| is_trump_card(*card, rules));
        let points = cards.iter().map(|card| card_score(*card)).sum::<i32>();
        let suit_size = cards
            .first()
            .and_then(|card| card_suit(*card))
            .map(|suit| suit_len[suit as usize])
            .unwrap_or(usize::MAX);
        (trump, points > 0, suit_size, value(cards))
    }) {
        return Some((**pair).clone());
    }

    // 3. Late in the hand, cash an estimated controlling single.
    if hand.len() <= 5
        && let Some(top) = leads
            .iter()
            .filter(|cards| {
                cards.len() == 1 && knowledge.lead_control_probability(state, cards) >= 0.82
            })
            .max_by_key(|cards| value(cards))
            .and_then(|cards| cards.first())
    {
        return Some(vec![*top]);
    }

    // 4. Otherwise sluff the lowest single, holding strength back. Prefer a
    //    short plain suit so we can create a void to trump later.
    lowest_lead_single(hand, rules).map(|card| vec![card])
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
    fn ai_cashes_a_controlling_tractor_over_a_vulnerable_one() {
        let mut state = test_state(TractorRank::TWO);
        state
            .hands
            .insert(0, vec![2, 102, 3, 103, 11, 111, 12, 112, 20]);
        // Opponent cards are populated only to provide public hand counts. The
        // probability model must not inspect those hidden values.
        state.hands.insert(1, vec![4, 104, 5, 105]);
        state.hands.insert(2, vec![19]);
        state.hands.insert(3, vec![21]);

        let play = decide(&state, 0).expect("lead");
        assert_eq!(
            combo::classify(&play, &state.rules).map(|combo| combo.kind),
            Some(ComboKind::Tractor(2))
        );
        let mut ranks: Vec<_> = play.iter().map(|card| card_rank(*card)).collect();
        ranks.sort_unstable();
        assert_eq!(ranks, vec![12, 12, 13, 13]);
    }

    #[test]
    fn ai_leads_low_plain_pair_over_high_one() {
        let mut state = test_state(TractorRank::TWO);
        // Two plain pairs (rank3 low, rank9 high) in an early-round-sized hand
        // and no tractor. The AI probes low instead of cashing control early.
        state.hands.insert(
            0,
            vec![2, 102, 8, 108, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23],
        );
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
