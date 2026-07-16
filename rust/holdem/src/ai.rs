use share_type_public::{
    TexasHoldEmAction, TexasHoldEmPhase, games::texas_hold_em::WsTexasHoldEmPlayRequest,
};

use crate::game_state::HoldemGameState;
use crate::hand_evaluator::{card_rank, card_suit};

const ONE_OPPONENT_BLUFF_CHANCE: f64 = 0.15;
const TWO_OPPONENT_BLUFF_CHANCE: f64 = 0.08;

fn aggressive_action(
    state: &HoldemGameState,
    position: usize,
    desired_amount: i32,
) -> WsTexasHoldEmPlayRequest {
    let call_amount = state.call_amount(position);
    let chips = state.chip_count(position);
    if chips <= 0 {
        return default_action(state, position);
    }
    if chips <= call_amount {
        return WsTexasHoldEmPlayRequest {
            action: TexasHoldEmAction::ALL_IN,
            amount: 0,
        };
    }

    if call_amount == 0 && state.current_bet == 0 {
        if chips < state.big_blind {
            return WsTexasHoldEmPlayRequest {
                action: TexasHoldEmAction::ALL_IN,
                amount: 0,
            };
        }
        return WsTexasHoldEmPlayRequest {
            action: TexasHoldEmAction::BET,
            amount: desired_amount.max(state.big_blind).min(chips),
        };
    }

    let wager = desired_amount.max(state.min_raise).min(chips - call_amount);
    if wager >= state.min_raise {
        return WsTexasHoldEmPlayRequest {
            action: TexasHoldEmAction::RAISE,
            amount: wager,
        };
    }
    if call_amount > 0 {
        WsTexasHoldEmPlayRequest {
            action: TexasHoldEmAction::CALL,
            amount: 0,
        }
    } else {
        WsTexasHoldEmPlayRequest {
            action: TexasHoldEmAction::CHECK,
            amount: 0,
        }
    }
}

pub fn decide(state: &HoldemGameState, position: usize) -> WsTexasHoldEmPlayRequest {
    decide_with_bluff_roll(state, position, rand::random::<f64>())
}

fn decide_with_bluff_roll(
    state: &HoldemGameState,
    position: usize,
    bluff_roll: f64,
) -> WsTexasHoldEmPlayRequest {
    let Some(hole_cards) = state.hands.get(&position) else {
        return default_action(state, position);
    };

    let (action, is_weak) = match state.phase {
        TexasHoldEmPhase::PreFlop => {
            let strength = preflop_strength(hole_cards);
            (
                preflop_decision(state, position, strength),
                strength <= 0.45,
            )
        }
        TexasHoldEmPhase::Flop | TexasHoldEmPhase::Turn | TexasHoldEmPhase::River => {
            let hand = state.evaluated_hand(position);
            let is_weak = hand.as_ref().is_some_and(|hand| hand.category == 0);
            (postflop_decision(state, position), is_weak)
        }
        TexasHoldEmPhase::Start | TexasHoldEmPhase::Settlement => {
            return default_action(state, position);
        }
    };

    if action.action == TexasHoldEmAction::CHECK
        && is_weak
        && bluff_roll < bluff_chance(state, position)
    {
        let desired_amount = match state.phase {
            TexasHoldEmPhase::PreFlop => state.big_blind * 2,
            _ => (state.pot / 2).max(state.big_blind),
        };
        aggressive_action(state, position, desired_amount)
    } else {
        action
    }
}

fn bluff_chance(state: &HoldemGameState, position: usize) -> f64 {
    let opponent_count = state
        .active_not_folded_positions()
        .into_iter()
        .filter(|opponent| *opponent != position)
        .count();
    match opponent_count {
        1 => ONE_OPPONENT_BLUFF_CHANCE,
        2 => TWO_OPPONENT_BLUFF_CHANCE,
        _ => 0.0,
    }
}

fn default_action(state: &HoldemGameState, position: usize) -> WsTexasHoldEmPlayRequest {
    let call_amount = state.call_amount(position);
    WsTexasHoldEmPlayRequest {
        action: if call_amount == 0 {
            TexasHoldEmAction::CHECK
        } else {
            TexasHoldEmAction::FOLD
        },
        amount: 0,
    }
}

fn postflop_decision(state: &HoldemGameState, position: usize) -> WsTexasHoldEmPlayRequest {
    let hand = state.evaluated_hand(position);
    let call_amount = state.call_amount(position);
    let chips = state.chip_count(position);
    let pot = state.pot;

    let Some(hand) = hand else {
        return WsTexasHoldEmPlayRequest {
            action: if call_amount == 0 {
                TexasHoldEmAction::CHECK
            } else {
                TexasHoldEmAction::FOLD
            },
            amount: 0,
        };
    };

    let category = hand.category;

    if category >= 7 {
        if call_amount == 0 {
            return aggressive_action(state, position, (pot / 2).max(state.big_blind * 2));
        }
        if chips <= pot * 2 {
            return WsTexasHoldEmPlayRequest {
                action: TexasHoldEmAction::ALL_IN,
                amount: 0,
            };
        }
        return aggressive_action(state, position, pot.max(call_amount * 2));
    }

    if category >= 5 {
        if call_amount == 0 {
            return aggressive_action(state, position, (pot / 3).max(state.big_blind));
        }
        return WsTexasHoldEmPlayRequest {
            action: TexasHoldEmAction::CALL,
            amount: 0,
        };
    }

    if category >= 3 {
        if call_amount == 0 {
            return WsTexasHoldEmPlayRequest {
                action: TexasHoldEmAction::CHECK,
                amount: 0,
            };
        }
        let pot_odds = if pot + call_amount > 0 {
            call_amount as f64 / (pot + call_amount) as f64
        } else {
            0.0
        };
        if pot_odds < 0.35 {
            return WsTexasHoldEmPlayRequest {
                action: TexasHoldEmAction::CALL,
                amount: 0,
            };
        }
        return WsTexasHoldEmPlayRequest {
            action: TexasHoldEmAction::FOLD,
            amount: 0,
        };
    }

    if category >= 1 {
        if call_amount == 0 {
            return WsTexasHoldEmPlayRequest {
                action: TexasHoldEmAction::CHECK,
                amount: 0,
            };
        }
        let pot_odds = if pot + call_amount > 0 {
            call_amount as f64 / (pot + call_amount) as f64
        } else {
            0.0
        };
        let top_rank = hand.ranks.first().copied().unwrap_or(0);
        let is_top_pair = top_rank >= 12;
        let is_overpair = state.phase == TexasHoldEmPhase::Flop && top_rank >= 13;
        if (is_top_pair || is_overpair) && pot_odds < 0.25 {
            return WsTexasHoldEmPlayRequest {
                action: TexasHoldEmAction::CALL,
                amount: 0,
            };
        }
        return WsTexasHoldEmPlayRequest {
            action: TexasHoldEmAction::FOLD,
            amount: 0,
        };
    }

    default_action(state, position)
}

fn preflop_strength(hole_cards: &[i32]) -> f64 {
    let r_high = hole_cards.iter().map(|c| card_rank(*c)).max().unwrap_or(2);
    let r_low = hole_cards.iter().map(|c| card_rank(*c)).min().unwrap_or(2);
    let is_pair = r_high == r_low;
    let is_suited = hole_cards
        .first()
        .zip(hole_cards.get(1))
        .is_some_and(|(first, second)| card_suit(*first) == card_suit(*second));
    let gap = r_high - r_low;

    if is_pair {
        0.30 + (r_high as f64 / 14.0) * 0.65
    } else {
        let mut s = (r_high as f64 / 14.0) * 0.35 + (r_low as f64 / 14.0) * 0.15;
        if is_suited {
            s += 0.06;
        }
        if gap <= 2 {
            s += 0.04;
        }
        if r_high >= 12 && r_low >= 11 {
            s += 0.10;
        }
        s.min(1.0)
    }
}

fn preflop_decision(
    state: &HoldemGameState,
    position: usize,
    strength: f64,
) -> WsTexasHoldEmPlayRequest {
    let call_amount = state.call_amount(position);
    let chips = state.chip_count(position);
    let pot = state.pot;
    let big_blind = state.big_blind;

    if strength > 0.85 {
        if call_amount == 0 {
            return aggressive_action(state, position, (big_blind * 3).max(pot / 2));
        }
        let min_raise = (pot).max(big_blind * 3).max(call_amount * 2);
        if call_amount + min_raise < chips && min_raise > 0 {
            return WsTexasHoldEmPlayRequest {
                action: TexasHoldEmAction::RAISE,
                amount: min_raise,
            };
        }
        if chips <= pot * 2 {
            return WsTexasHoldEmPlayRequest {
                action: TexasHoldEmAction::ALL_IN,
                amount: 0,
            };
        }
        return WsTexasHoldEmPlayRequest {
            action: TexasHoldEmAction::CALL,
            amount: 0,
        };
    }

    if strength > 0.65 {
        if call_amount == 0 {
            return aggressive_action(state, position, big_blind * 3);
        }
        if call_amount <= big_blind * 3 {
            return WsTexasHoldEmPlayRequest {
                action: TexasHoldEmAction::CALL,
                amount: 0,
            };
        }
        return WsTexasHoldEmPlayRequest {
            action: TexasHoldEmAction::FOLD,
            amount: 0,
        };
    }

    if strength > 0.45 {
        if call_amount == 0 {
            return WsTexasHoldEmPlayRequest {
                action: TexasHoldEmAction::CHECK,
                amount: 0,
            };
        }
        if call_amount <= big_blind {
            return WsTexasHoldEmPlayRequest {
                action: TexasHoldEmAction::CALL,
                amount: 0,
            };
        }
        return WsTexasHoldEmPlayRequest {
            action: TexasHoldEmAction::FOLD,
            amount: 0,
        };
    }

    if call_amount == 0 {
        return WsTexasHoldEmPlayRequest {
            action: TexasHoldEmAction::CHECK,
            amount: 0,
        };
    }
    WsTexasHoldEmPlayRequest {
        action: TexasHoldEmAction::FOLD,
        amount: 0,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use share_type_public::{TexasHoldEmAction, TexasHoldEmPhase};
    use ws_common::CommonGameState;

    use super::*;
    use crate::poker_variant::STANDARD_TEXAS;

    #[test]
    fn postflop_made_flush_bets_when_unopened() {
        let mut state = test_state();
        state.phase = TexasHoldEmPhase::Flop;
        state.current_bet = 0;
        state.pot = 90;
        state.hands.insert(0, vec![1, 3]);
        state.public_cards = vec![5, 7, 9, 14, 27];

        let payload = decide(&state, 0);

        assert_eq!(payload.action, TexasHoldEmAction::BET);
        assert!(payload.amount > 0);
    }

    #[test]
    fn postflop_top_pair_folds_to_bad_pot_odds() {
        let mut state = test_state();
        state.phase = TexasHoldEmPhase::Turn;
        state.current_bet = 200;
        state.pot = 50;
        state.hands.insert(0, vec![13, 2]);
        state.public_cards = vec![26, 5, 7, 22, 35];

        let payload = decide(&state, 0);

        assert_eq!(payload.action, TexasHoldEmAction::FOLD);
    }

    #[test]
    fn postflop_top_pair_calls_with_good_pot_odds() {
        let mut state = test_state();
        state.phase = TexasHoldEmPhase::River;
        state.current_bet = 20;
        state.pot = 100;
        state.hands.insert(0, vec![13, 2]);
        state.public_cards = vec![26, 5, 7, 22, 35];

        let payload = decide_with_bluff_roll(&state, 0, 1.0);

        assert_eq!(payload.action, TexasHoldEmAction::CALL);
    }

    #[test]
    fn preflop_marginal_hand_checks_when_call_is_free() {
        let mut state = test_state();
        state.current_bet = 10;
        state.round_bets.insert(0, 10);
        state.hands.insert(0, vec![10, 24]);

        let payload = decide(&state, 0);

        assert_eq!(payload.action, TexasHoldEmAction::CHECK);
    }

    #[test]
    fn preflop_middle_pair_calls_one_big_blind() {
        let mut state = test_state();
        state.current_bet = 10;
        state.hands.insert(0, vec![6, 19]);

        let payload = decide_with_bluff_roll(&state, 0, 1.0);

        assert_eq!(payload.action, TexasHoldEmAction::CALL);
    }

    #[test]
    fn preflop_premium_pair_raises_when_facing_bet() {
        let mut state = test_state();
        state.hands.insert(0, vec![13, 26]);

        let payload = decide(&state, 0);

        assert_eq!(payload.action, TexasHoldEmAction::RAISE);
        assert!(payload.amount >= state.min_raise);
    }

    #[test]
    fn preflop_strong_hand_raises_in_big_blind_option() {
        let mut state = test_state();
        state.current_bet = 10;
        state.round_bets.insert(0, 10);
        state.hands.insert(0, vec![13, 12]);

        let payload = decide(&state, 0);

        assert_eq!(payload.action, TexasHoldEmAction::RAISE);
        assert!(payload.amount >= state.min_raise);
    }

    #[test]
    fn preflop_weak_hand_folds_to_raise() {
        let mut state = test_state();
        state.hands.insert(0, vec![1, 8]);

        let payload = decide(&state, 0);

        assert_eq!(payload.action, TexasHoldEmAction::FOLD);
    }

    #[test]
    fn weak_hand_can_bluff_when_only_one_opponent_remains() {
        let mut state = weak_postflop_state();
        state.folded.extend([2, 3]);

        let payload = decide_with_bluff_roll(&state, 0, ONE_OPPONENT_BLUFF_CHANCE - 0.01);

        assert_eq!(payload.action, TexasHoldEmAction::BET);
        assert_eq!(payload.amount, state.pot / 2);
    }

    #[test]
    fn weak_hand_can_bluff_when_two_opponents_remain() {
        let mut state = weak_postflop_state();
        state.folded.insert(3);

        let payload = decide_with_bluff_roll(&state, 0, TWO_OPPONENT_BLUFF_CHANCE - 0.01);

        assert_eq!(payload.action, TexasHoldEmAction::BET);
    }

    #[test]
    fn weak_hand_does_not_bluff_against_three_opponents() {
        let state = weak_postflop_state();

        let payload = decide_with_bluff_roll(&state, 0, 0.0);

        assert_eq!(payload.action, TexasHoldEmAction::CHECK);
    }

    #[test]
    fn weak_hand_bluff_is_probabilistic() {
        let mut state = weak_postflop_state();
        state.folded.extend([2, 3]);

        let payload = decide_with_bluff_roll(&state, 0, ONE_OPPONENT_BLUFF_CHANCE);

        assert_eq!(payload.action, TexasHoldEmAction::CHECK);
    }

    #[test]
    fn weak_hand_still_folds_to_a_bet_instead_of_bluffing() {
        let mut state = weak_postflop_state();
        state.folded.extend([2, 3]);
        state.current_bet = 50;

        let payload = decide_with_bluff_roll(&state, 0, 0.0);

        assert_eq!(payload.action, TexasHoldEmAction::FOLD);
    }

    fn weak_postflop_state() -> HoldemGameState {
        let mut state = test_state();
        state.phase = TexasHoldEmPhase::River;
        state.current_bet = 0;
        state.pot = 100;
        state.hands.insert(0, vec![1, 21]);
        state.public_cards = vec![3, 18, 33, 48, 12];
        state
    }

    fn test_state() -> HoldemGameState {
        let mut common = CommonGameState::new();
        for position in 0..4 {
            common.add_player(position, position as u64 + 1, &format!("u{position}"));
        }
        let mut state =
            HoldemGameState::from_common_with_variant(Arc::new(Mutex::new(common)), STANDARD_TEXAS);
        state.phase = TexasHoldEmPhase::PreFlop;
        state.big_blind = 10;
        state.small_blind = 5;
        state.min_raise = 10;
        state.current_bet = 20;
        state.pot = 30;
        for position in 0..4 {
            state.chips.insert(position, 1000);
            state.round_bets.insert(position, 0);
        }
        state
    }
}
