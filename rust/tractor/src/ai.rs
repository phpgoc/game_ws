use std::collections::HashMap;

use crate::game_state::{
    TractorGameState, card_rank, card_suit, is_trump_card, tractor_card_value,
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

fn find_pairs(cards: &[i32]) -> Vec<Vec<i32>> {
    let mut by_rank: HashMap<i32, Vec<i32>> = HashMap::new();
    for card in cards {
        by_rank.entry(card_rank(*card)).or_default().push(*card);
    }
    by_rank
        .into_values()
        .filter(|v| v.len() >= 2)
        .map(|v| v[..2].to_vec())
        .collect()
}

fn lead_play(state: &TractorGameState, _position: usize, hand: &[i32]) -> Option<Vec<i32>> {
    let rules = &state.rules;
    let target_rank = rules.target_rank;

    let trump_cards: Vec<&i32> = hand
        .iter()
        .filter(|c| is_trump_card(**c, target_rank))
        .collect();

    let mut by_suit: HashMap<Option<i32>, Vec<&i32>> = HashMap::new();
    for card in hand {
        if !is_trump_card(*card, target_rank) {
            by_suit.entry(card_suit(*card)).or_default().push(card);
        }
    }

    if trump_cards.len() >= 4 {
        let lowest = trump_cards
            .iter()
            .min_by_key(|c| tractor_card_value(***c, rules, None))?;
        return Some(vec![**lowest]);
    }

    for (_, cards) in by_suit.iter().filter(|(s, _)| s.is_some()) {
        if cards.len() == 1 {
            return Some(vec![*cards[0]]);
        }
        if cards.len() == 2 {
            let lower = cards.iter().min_by_key(|c| card_rank(***c))?;
            return Some(vec![**lower]);
        }
    }

    let mut pair_candidates: Vec<Vec<i32>> = Vec::new();
    for (_, cards) in &by_suit {
        let card_values: Vec<i32> = cards.iter().map(|c| **c).collect();
        pair_candidates.extend(find_pairs(&card_values));
    }
    if !pair_candidates.is_empty() {
        pair_candidates.sort_by_key(|p| card_rank(p[0]));
        return Some(pair_candidates[0].clone());
    }

    if let Some(longest) = by_suit
        .iter()
        .filter(|(s, _)| s.is_some())
        .max_by_key(|(_, cards)| cards.len())
    {
        let lowest = longest.1.iter().min_by_key(|c| card_rank(***c))?;
        return Some(vec![**lowest]);
    }

    if !trump_cards.is_empty() {
        let lowest = trump_cards
            .iter()
            .min_by_key(|c| tractor_card_value(***c, rules, None))?;
        return Some(vec![**lowest]);
    }

    hand.first().map(|c| vec![*c])
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use share_type_public::{TractorPhase, TractorRank, WsTractorPlayedCards};
    use ws_common::CommonGameState;

    use super::*;
    use crate::game_state::{TractorGameState, TractorRules};

    #[test]
    fn ai_following_uses_smallest_winning_card() {
        let mut state = test_state();
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
    fn ai_leads_low_pair_when_no_short_suit_or_trump_plan() {
        let mut state = test_state();
        state
            .hands
            .insert(0, vec![1, 101, 2, 14, 15, 16, 27, 28, 29]);

        assert_eq!(decide(&state, 0), Some(vec![1, 101]));
    }

    #[test]
    fn ai_leads_lowest_trump_when_holding_many_trumps() {
        let mut state = test_state();
        state.hands.insert(0, vec![13, 26, 39, 53, 1, 14]);

        assert_eq!(decide(&state, 0), Some(vec![13]));
    }

    #[test]
    fn ai_leads_singleton_short_suit_before_long_suits() {
        let mut state = test_state();
        state.hands.insert(0, vec![1, 14, 15, 16, 27, 28, 29]);

        assert_eq!(decide(&state, 0), Some(vec![1]));
    }

    #[test]
    fn ai_returns_none_without_cards() {
        let state = test_state();

        assert_eq!(decide(&state, 0), None);
    }

    fn test_state() -> TractorGameState {
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
            removed_rank_mask: 0,
            target_rank: TractorRank::A,
        };
        state.current_position = 0;
        state
    }
}
