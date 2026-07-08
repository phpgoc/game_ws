use rand::Rng;
use share_type_public::{
    TexasHoldEmAction, TexasHoldEmPhase,
    games::texas_hold_em::WsTexasHoldEmPlayRequest,
};

use crate::game_state::HoldemGameState;
use crate::hand_evaluator::{card_rank, card_suit, evaluate_best};

pub fn decide(state: &HoldemGameState, position: usize) -> WsTexasHoldEmPlayRequest {
    let Some(hole_cards) = state.hands.get(&position) else {
        return default_action(state, position);
    };

    match state.phase {
        TexasHoldEmPhase::PreFlop => preflop_decision(state, position, hole_cards),
        _ => postflop_decision(state, hole_cards),
    }
}

fn preflop_decision(
    state: &HoldemGameState,
    position: usize,
    hole_cards: &[i32],
) -> WsTexasHoldEmPlayRequest {
    let r_high = hole_cards
        .iter()
        .map(|c| card_rank(*c))
        .max()
        .unwrap_or(2);
    let r_low = hole_cards
        .iter()
        .map(|c| card_rank(*c))
        .min()
        .unwrap_or(2);
    let is_pair = r_high == r_low;
    let is_suited = card_suit(hole_cards[0]) == card_suit(hole_cards[1]);
    let gap = r_high - r_low;

    let strength = if is_pair {
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
    };

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

fn postflop_decision(
    state: &HoldemGameState,
    hole_cards: &[i32],
) -> WsTexasHoldEmPlayRequest {
    let position = state
        .hands
        .iter()
        .find(|(_, v)| *v == hole_cards)
        .map(|(k, _)| *k)
        .unwrap_or(0);

    let mut cards = hole_cards.to_vec();
    cards.extend(state.public_cards.iter().copied());

    let hand = evaluate_best(&cards);
    let call_amount = state.call_amount(position);
    let chips = state.chip_count(position);
    let pot = state.pot;
    let big_blind = state.big_blind;

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
            return WsTexasHoldEmPlayRequest {
                action: TexasHoldEmAction::BET,
                amount: (pot / 2).max(big_blind * 2).min(chips),
            };
        }
        if chips <= pot * 2 {
            return WsTexasHoldEmPlayRequest {
                action: TexasHoldEmAction::ALL_IN,
                amount: 0,
            };
        }
        let raise = pot.max(call_amount * 2).min(chips - call_amount);
        if raise > 0 {
            return WsTexasHoldEmPlayRequest {
                action: TexasHoldEmAction::RAISE,
                amount: raise,
            };
        }
        return WsTexasHoldEmPlayRequest {
            action: TexasHoldEmAction::CALL,
            amount: 0,
        };
    }

    if category >= 5 {
        if call_amount == 0 {
            return WsTexasHoldEmPlayRequest {
                action: TexasHoldEmAction::BET,
                amount: (pot / 3).max(big_blind).min(chips),
            };
        }
        if call_amount <= pot {
            return WsTexasHoldEmPlayRequest {
                action: TexasHoldEmAction::CALL,
                amount: 0,
            };
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

    if call_amount == 0 {
        return WsTexasHoldEmPlayRequest {
            action: TexasHoldEmAction::CHECK,
            amount: 0,
        };
    }
    if call_amount <= big_blind && pot > big_blind * 10 && rand::rng().random::<f64>() < 0.1 {
        return WsTexasHoldEmPlayRequest {
            action: TexasHoldEmAction::CALL,
            amount: 0,
        };
    }
    WsTexasHoldEmPlayRequest {
        action: TexasHoldEmAction::FOLD,
        amount: 0,
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

fn aggressive_action(
    state: &HoldemGameState,
    position: usize,
    desired_amount: i32,
) -> WsTexasHoldEmPlayRequest {
    let call_amount = state.call_amount(position);
    let chips = state.chip_count(position);
    if chips <= call_amount {
        return WsTexasHoldEmPlayRequest {
            action: TexasHoldEmAction::ALL_IN,
            amount: 0,
        };
    }

    let wager = desired_amount.max(state.min_raise).min(chips - call_amount);
    if call_amount == 0 && state.current_bet == 0 {
        return WsTexasHoldEmPlayRequest {
            action: TexasHoldEmAction::BET,
            amount: wager,
        };
    }
    if wager >= state.min_raise {
        return WsTexasHoldEmPlayRequest {
            action: TexasHoldEmAction::RAISE,
            amount: wager,
        };
    }
    WsTexasHoldEmPlayRequest {
        action: TexasHoldEmAction::CALL,
        amount: 0,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use share_type_public::{TexasHoldEmAction, TexasHoldEmPhase};
    use ws_common::game_state::CommonGameState;

    use super::*;
    use crate::poker_variant::STANDARD_TEXAS;

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
    fn preflop_marginal_hand_checks_when_call_is_free() {
        let mut state = test_state();
        state.current_bet = 10;
        state.round_bets.insert(0, 10);
        state.hands.insert(0, vec![10, 24]);

        let payload = decide(&state, 0);

        assert_eq!(payload.action, TexasHoldEmAction::CHECK);
    }

    #[test]
    fn preflop_weak_hand_folds_to_raise() {
        let mut state = test_state();
        state.hands.insert(0, vec![1, 8]);

        let payload = decide(&state, 0);

        assert_eq!(payload.action, TexasHoldEmAction::FOLD);
    }

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
}
