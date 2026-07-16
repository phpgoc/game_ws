//! Tractor AI strategy coordinator.
//!
//! Following logic is shared with the safe auto-play in [`TractorGameState`]
//! (beat opponents cheaply, feed points to a winning partner, otherwise shed
//! low). The AI adds combo-aware leading: it looks for tractors and pairs, and
//! decides whether to probe with a low card or cash a guaranteed winner.

mod bid;
mod blood;
mod bury;
mod knowledge;

use crate::{
    combo::{self, ComboKind},
    game_state::{
        TractorGameState, card_rank, card_score, card_suit, is_trump_card, same_team,
        tractor_card_value,
    },
};

use self::knowledge::PublicKnowledge;

pub(crate) use bid::{best_trump_suit, declaration_decision};
pub(crate) use bury::choose_bury;

fn candidate_would_win(state: &TractorGameState, position: usize, cards: &[i32]) -> bool {
    let mut trick = state.current_trick.clone();
    trick.push(share_type_public::WsTractorPlayedCards {
        position: position as i32,
        name: String::new(),
        cards: cards.to_vec(),
    });
    combo::trick_winner(&trick, &state.rules) == Some(position)
}

pub fn decide(state: &TractorGameState, position: usize) -> Option<Vec<i32>> {
    let hand = state.hands.get(&position)?;
    if hand.is_empty() {
        return None;
    }

    if state.current_trick.is_empty() {
        lead_play(state, position, hand)
    } else {
        follow_play(state, position, hand)
    }
}

fn follow_play(state: &TractorGameState, position: usize, hand: &[i32]) -> Option<Vec<i32>> {
    let lead = state.lead_combo()?;
    let mut candidates = state.legal_follows(position, &lead);
    if candidates.is_empty() {
        return combo::forced_follow(hand, &lead, &state.rules);
    }

    let knowledge = PublicKnowledge::from_state(state, position);
    let current_winner = combo::trick_winner(&state.current_trick, &state.rules)?;
    let partner_winning = same_team(current_winner, position) && current_winner != position;
    let hold_probability = knowledge.current_winner_hold_probability(state);
    let is_last = state.current_trick.len() + 1 >= state.active_positions().len();
    let current_points = combo::trick_points(&state.current_trick);
    let lead_count = lead.kind.card_count();
    let near_bottom = hand.len() <= lead_count.saturating_mul(2);

    if partner_winning {
        let safe_to_feed = is_last || hold_probability >= 0.84;
        if !safe_to_feed && (current_points > 0 || near_bottom) {
            // A partner may be ahead only provisionally. If a later opponent
            // is likely to ruff/overtake, take over with a materially safer
            // card instead of either donating points or passively watching a
            // scoring trick get stolen.
            let mut protective_takeovers: Vec<_> = candidates
                .iter()
                .filter(|cards| candidate_would_win(state, position, cards))
                .filter_map(|cards| {
                    let hold = knowledge.candidate_hold_probability(state, position, cards);
                    (hold >= 0.78 && hold >= hold_probability + 0.12)
                        .then_some((cards.clone(), hold))
                })
                .collect();
            protective_takeovers.sort_by(|(left, left_hold), (right, right_hold)| {
                let left_cost = structure_loss(hand, left, &state.rules) * 100
                    + play_strength(left, state, lead.suit);
                let right_cost = structure_loss(hand, right, &state.rules) * 100
                    + play_strength(right, state, lead.suit);
                right_hold
                    .total_cmp(left_hold)
                    .then_with(|| left_cost.cmp(&right_cost))
            });
            if let Some((cards, _)) = protective_takeovers.into_iter().next() {
                return Some(cards);
            }
        }
        candidates.sort_by_key(|cards| {
            let points = cards.iter().map(|card| card_score(*card)).sum::<i32>();
            let loss = structure_loss(hand, cards, &state.rules);
            let strength = play_strength(cards, state, lead.suit);
            if safe_to_feed {
                // Captured points always help our partnership: attackers move
                // toward the threshold, while defenders keep those points out
                // of the attacking score. Blood scoring changes urgency, not
                // the sign of a safely captured point card.
                let donation_value = points * 1_000 + strength;
                (-donation_value, loss, strength)
            } else {
                (points, loss, strength)
            }
        });
        return candidates.into_iter().next();
    }

    let mut winning: Vec<_> = candidates
        .iter()
        .filter(|cards| candidate_would_win(state, position, cards))
        .cloned()
        .collect();
    if !winning.is_empty() {
        winning.sort_by(|left, right| {
            let left_utility = winning_utility(
                state,
                &knowledge,
                position,
                hand,
                left,
                current_points,
                near_bottom,
                lead.suit,
            );
            let right_utility = winning_utility(
                state,
                &knowledge,
                position,
                hand,
                right,
                current_points,
                near_bottom,
                lead.suit,
            );
            right_utility.total_cmp(&left_utility)
        });
        let best = &winning[0];
        let ruffing =
            lead.suit.is_some() && best.iter().all(|card| is_trump_card(*card, &state.rules));
        let worth_taking = current_points > 0
            || near_bottom
            || knowledge.candidate_hold_probability(state, position, best) >= 0.82;
        if worth_taking || !(ruffing && state.partner_still_to_play(position)) {
            return Some(best.clone());
        }
    }

    // Losing/discarding: avoid donating points, keep pairs and tractors intact,
    // then prefer a play that empties a plain suit for a future ruff.
    candidates.sort_by_key(|cards| {
        let points = cards.iter().map(|card| card_score(*card)).sum::<i32>();
        let loss = structure_loss(hand, cards, &state.rules);
        let creates_void = play_creates_plain_void(hand, cards, state);
        let strength = play_strength(cards, state, lead.suit);
        (points, loss, !creates_void, strength)
    });
    candidates.into_iter().next()
}

fn hand_structure_value(hand: &[i32], rules: &crate::game_state::TractorRules) -> i32 {
    combo::enumerate_leads(hand, rules)
        .into_iter()
        .filter_map(|cards| match combo::classify(&cards, rules)?.kind {
            ComboKind::Pair => Some(20),
            ComboKind::Tractor(pairs) => Some(45 * pairs as i32),
            ComboKind::Throw { pairs, .. } => Some(10 * pairs as i32),
            ComboKind::Single => None,
        })
        .sum()
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

    if let Some(plan) = blood::lead_plan(state, position, hand, &knowledge) {
        return match plan.route {
            blood::BloodRoute::Trump | blood::BloodRoute::Plain(_) => Some(plan.cards),
        };
    }

    // A top pair can be used as an entry to promote a lower pair in the same
    // suit. This covers the common A-pair/Q-pair judgement: cash the A pair when
    // its hold probability is high instead of blindly probing with Q into an
    // unseen K pair.
    let pair_leads: Vec<&Vec<i32>> = leads
        .iter()
        .filter(|cards| {
            combo::classify(cards, rules).map(|combo| combo.kind) == Some(ComboKind::Pair)
        })
        .collect();
    for suit in [Some(0), Some(1), Some(2), Some(3), None] {
        let mut suited_pairs: Vec<_> = pair_leads
            .iter()
            .copied()
            .filter(|cards| combo::classify(cards, rules).is_some_and(|combo| combo.suit == suit))
            .collect();
        suited_pairs.sort_by_key(|cards| std::cmp::Reverse(value(cards)));
        if suited_pairs.len() >= 2 {
            let top = suited_pairs[0];
            let next = suited_pairs[1];
            let top_probability = knowledge.lead_control_probability(state, top);
            let next_probability = knowledge.lead_control_probability(state, next);
            if top_probability >= 0.82 && next_probability + 0.18 < top_probability {
                return Some(top.clone());
            }
        }
    }

    // A composite throw is a probability decision, never an omniscient peek at
    // hidden hands. In a short hand or near the blood threshold the AI accepts
    // a controlled gamble; otherwise it requires a substantially safer throw.
    let attacking_score = state.attacking_score();
    let near_blood = attacking_score * 5 >= state.rules.blood_start_score.max(1) * 3;
    let throw_threshold = if hand.len() <= 8 || near_blood {
        0.46
    } else {
        0.64
    };
    if let Some(throw) = leads
        .iter()
        .filter(|cards| {
            matches!(
                combo::classify(cards, rules).map(|combo| combo.kind),
                Some(ComboKind::Throw { .. })
            )
        })
        .filter_map(|cards| {
            let probability = knowledge.throw_success_probability(state, cards);
            // Do not empty most of a healthy early hand merely because two
            // independent tractors can technically be concatenated into a
            // throw. Outside an urgent score race, throws are reserved for a
            // short-hand exit or a compact A/Q-style promotion attempt.
            let components = combo::throw_components(cards, rules)?;
            let compact_promotion =
                components.len() == 2 && components.iter().all(|component| component.len() <= 2);
            let short_exit = hand.len() <= 8 && cards.len() + 1 >= hand.len();
            (probability >= throw_threshold && (near_blood || compact_promotion || short_exit))
                .then_some((cards, probability))
        })
        .max_by(
            |(left_cards, left_probability), (right_cards, right_probability)| {
                left_cards
                    .len()
                    .cmp(&right_cards.len())
                    .then_with(|| left_probability.total_cmp(right_probability))
            },
        )
        .map(|(cards, _)| cards)
    {
        return Some(throw.clone());
    }

    // If the partner is known void in a plain suit while at least one opponent
    // still follows it, lead a cheap non-point single as a ruff signal.
    let partner = (position + 2) % 4;
    if let Some(signal) = leads
        .iter()
        .filter(|cards| cards.len() == 1 && card_score(cards[0]) == 0)
        .filter(|cards| {
            let suit = combo::classify(cards, rules).and_then(|combo| combo.suit);
            suit.is_some()
                && knowledge.known_void(partner, suit)
                && state
                    .active_positions()
                    .into_iter()
                    .filter(|other| !same_team(*other, position))
                    .any(|enemy| !knowledge.known_void(enemy, suit))
        })
        .min_by_key(|cards| value(cards))
    {
        return Some(signal.clone());
    }

    // 1. Prefer a controlling tractor, then length. A vulnerable tractor is
    // played low to strip opponents' pairs without burning the team's control.
    let defending_bottom = same_team(position, state.dealer_position);
    let preserve_trump_control = defending_bottom
        && hand.len() > 12
        && leads.iter().any(|cards| {
            combo::classify(cards, rules).is_some_and(|classified| {
                matches!(classified.kind, ComboKind::Tractor(_)) && classified.suit.is_some()
            })
        });
    if let Some(tractor) = leads
        .iter()
        .filter(|cards| {
            combo::classify(cards, rules).is_some_and(|classified| {
                matches!(classified.kind, ComboKind::Tractor(_))
                    && (!preserve_trump_control || classified.suit.is_some())
            })
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
    let pairs = pair_leads;
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

fn play_creates_plain_void(hand: &[i32], cards: &[i32], state: &TractorGameState) -> bool {
    let Some(suit) = cards
        .iter()
        .find(|card| !is_trump_card(**card, &state.rules))
        .and_then(|card| card_suit(*card))
    else {
        return false;
    };
    hand.iter()
        .filter(|card| !is_trump_card(**card, &state.rules) && card_suit(**card) == Some(suit))
        .count()
        == cards
            .iter()
            .filter(|card| !is_trump_card(**card, &state.rules) && card_suit(**card) == Some(suit))
            .count()
}

fn play_strength(cards: &[i32], state: &TractorGameState, lead_suit: Option<i32>) -> i32 {
    cards
        .iter()
        .map(|card| tractor_card_value(*card, &state.rules, lead_suit))
        .max()
        .unwrap_or_default()
}

fn structure_loss(hand: &[i32], cards: &[i32], rules: &crate::game_state::TractorRules) -> i32 {
    let before = hand_structure_value(hand, rules);
    let mut after = hand.to_vec();
    for card in cards {
        if let Some(index) = after.iter().position(|current| current == card) {
            after.remove(index);
        }
    }
    (before - hand_structure_value(&after, rules)).max(0)
}

#[allow(clippy::too_many_arguments)]
fn winning_utility(
    state: &TractorGameState,
    knowledge: &PublicKnowledge,
    position: usize,
    hand: &[i32],
    cards: &[i32],
    current_points: i32,
    near_bottom: bool,
    lead_suit: Option<i32>,
) -> f64 {
    let added_points = cards.iter().map(|card| card_score(*card)).sum::<i32>();
    let hold = knowledge.candidate_hold_probability(state, position, cards);
    let loss = structure_loss(hand, cards, &state.rules);
    let strength = play_strength(cards, state, lead_suit);
    let defending = same_team(position, state.dealer_position);
    let threshold = state.rules.blood_start_score.max(1);
    let attacking_score = state.attacking_score();
    let threshold_pressure = (attacking_score as f64 / threshold as f64).clamp(0.0, 1.5);
    let wins_contract = !defending
        && attacking_score < threshold
        && attacking_score + current_points + added_points >= threshold;
    let contract_swing = if wins_contract { 180.0 } else { 0.0 };
    let bottom_value = if near_bottom { 70.0 } else { 0.0 };
    (current_points + added_points) as f64 * (2.0 + threshold_pressure)
        + hold * 120.0
        + contract_swing
        + bottom_value
        - loss as f64 * 0.7
        - strength as f64 * 0.015
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use share_type_public::{TractorPhase, TractorRank, TractorSuit, WsTractorPlayedCards};
    use ws_common::CommonGameState;

    use super::*;
    use crate::game_state::{TractorGameState, TractorRules};

    #[test]
    fn ai_avoids_points_when_following_a_pair_with_singles() {
        let mut state = test_state(TractorRank::TWO);
        state.current_position = 1;
        state.current_trick.push(WsTractorPlayedCards {
            position: 0,
            name: "u0".to_owned(),
            cards: vec![8, 108],
        });
        // No pair is available, so any two spade singles are legal. Base 4 is
        // the five; a one-candidate fallback would unnecessarily donate it.
        state.hands.insert(1, vec![4, 5, 6]);

        assert_eq!(decide(&state, 1), Some(vec![5, 6]));
    }

    #[test]
    fn ai_cashes_a_controlling_tractor_over_a_vulnerable_one() {
        let mut state = test_state(TractorRank::TWO);
        state.rules.blood_enabled = false;
        state
            .hands
            .insert(0, vec![2, 102, 3, 103, 11, 111, 12, 112, 20]);
        // Opponent cards are populated only to provide public hand counts. The
        // probability model must not inspect those hidden values.
        state.hands.insert(1, vec![4; 20]);
        state.hands.insert(2, vec![19; 20]);
        state.hands.insert(3, vec![21; 20]);
        state.completed_tricks = vec![vec![WsTractorPlayedCards {
            position: 1,
            name: "u1".to_owned(),
            cards: vec![13, 113],
        }]];

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
    fn ai_cashes_ace_pair_before_risking_queen_pair() {
        let mut state = test_state(TractorRank::TWO);
        state.rules.deck_count = 4;
        state.hands.insert(
            0,
            vec![
                13, 113, // A pair
                11, 111, // Q pair, with unseen K pair risk
                18, 19, 20, 21, 22, 23, 24, 25,
            ],
        );
        for position in 1..4 {
            state.hands.insert(position, vec![30; 40]);
        }

        assert_eq!(decide(&state, 0), Some(vec![13, 113]));
    }

    #[test]
    fn ai_feeds_point_king_to_a_safe_winning_partner() {
        let mut state = test_state(TractorRank::TWO);
        state.current_position = 0;
        state.current_trick = vec![
            WsTractorPlayedCards {
                position: 1,
                name: "u1".to_owned(),
                cards: vec![4],
            },
            WsTractorPlayedCards {
                position: 2,
                name: "u2".to_owned(),
                cards: vec![13],
            },
            WsTractorPlayedCards {
                position: 3,
                name: "u3".to_owned(),
                cards: vec![5],
            },
        ];
        state.hands.insert(0, vec![6, 10, 11, 12]);
        state.hands.insert(1, vec![20, 21, 22]);
        state.hands.insert(2, vec![34, 35, 36]);
        state.hands.insert(3, vec![30, 31, 32, 33]);

        assert_eq!(decide(&state, 0), Some(vec![12]));
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
    fn ai_gambles_ace_queen_throw_and_referee_falls_back_to_queen_pair() {
        let mut state = test_state(TractorRank::TWO);
        state
            .hands
            .insert(0, vec![13, 113, 11, 111, 18, 19, 20, 21, 22, 23]);
        state.hands.insert(1, vec![12, 112, 30, 31]);
        state.hands.insert(2, vec![32, 33, 34, 35]);
        state.hands.insert(3, vec![42, 43, 44, 45]);

        let play = decide(&state, 0).expect("probability-backed throw");
        assert_eq!(
            combo::classify(&play, &state.rules).map(|combo| combo.kind),
            Some(ComboKind::Throw { cards: 4, pairs: 2 })
        );
        let played = state
            .play_cards(0, "u0".to_owned(), play)
            .expect("referee resolves failed throw");
        assert_eq!(played.cards, vec![11, 111]);
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
        // Two plain pairs (rank3 low, rank9 high) in an early-round-sized hand
        // and no tractor. The AI probes low instead of cashing control early.
        state.hands.insert(
            0,
            vec![2, 102, 8, 108, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23],
        );
        for position in 1..4 {
            state.hands.insert(position, vec![30; 20]);
        }
        let play = decide(&state, 0).expect("lead");
        assert_eq!(
            combo::classify(&play, &state.rules).map(|c| c.kind),
            Some(ComboKind::Pair)
        );
        // The lower pair (rank3 = base 2) is chosen.
        assert_eq!(play, vec![2, 102]);
    }

    #[test]
    fn ai_overtakes_an_unsafe_partner_to_protect_a_point_trick() {
        let mut state = test_state(TractorRank::TWO);
        state.current_position = 3;
        state.current_trick = vec![
            WsTractorPlayedCards {
                position: 1,
                name: "u1".to_owned(),
                cards: vec![4], // partner currently wins a five-point trick
            },
            WsTractorPlayedCards {
                position: 2,
                name: "u2".to_owned(),
                cards: vec![2],
            },
        ];
        // Position 0 previously failed to follow spades and is still to act.
        state.completed_tricks = vec![vec![
            WsTractorPlayedCards {
                position: 1,
                name: "u1".to_owned(),
                cards: vec![6],
            },
            WsTractorPlayedCards {
                position: 0,
                name: "u0".to_owned(),
                cards: vec![19],
            },
        ]];
        state.hands.insert(0, vec![30, 31, 32, 33]);
        state.hands.insert(1, vec![20, 21, 22]);
        state.hands.insert(2, vec![34, 35, 36]);
        // Position 3 is void in the led suit and can make the trick certain
        // with the unbeatable big joker.
        state.hands.insert(3, vec![18, 53, 54]);

        assert_eq!(decide(&state, 3), Some(vec![54]));
    }

    #[test]
    fn ai_releases_long_tractor_when_every_other_player_is_void() {
        let mut state = test_state(TractorRank::TWO);
        state.hands.insert(0, vec![2, 102, 3, 103, 20, 21]);
        for position in 1..4 {
            state.hands.insert(position, vec![30, 31, 32, 33]);
        }
        state.completed_tricks = vec![vec![
            WsTractorPlayedCards {
                position: 0,
                name: "u0".to_owned(),
                cards: vec![5],
            },
            WsTractorPlayedCards {
                position: 1,
                name: "u1".to_owned(),
                cards: vec![18],
            },
            WsTractorPlayedCards {
                position: 2,
                name: "u2".to_owned(),
                cards: vec![31],
            },
            WsTractorPlayedCards {
                position: 3,
                name: "u3".to_owned(),
                cards: vec![44],
            },
        ]];

        let play = decide(&state, 0).expect("lead long tractor");
        assert_eq!(
            combo::classify(&play, &state.rules).map(|combo| combo.kind),
            Some(ComboKind::Tractor(2))
        );
    }

    #[test]
    fn ai_returns_none_without_cards() {
        let state = test_state(TractorRank::A);
        assert_eq!(decide(&state, 0), None);
    }

    #[test]
    fn ai_withholds_points_when_future_enemy_is_known_void_and_can_ruff() {
        let mut state = test_state(TractorRank::TWO);
        state.current_position = 3;
        state.current_trick = vec![
            WsTractorPlayedCards {
                position: 1,
                name: "u1".to_owned(),
                cards: vec![13],
            },
            WsTractorPlayedCards {
                position: 2,
                name: "u2".to_owned(),
                cards: vec![4],
            },
        ];
        state.completed_tricks = vec![vec![
            WsTractorPlayedCards {
                position: 1,
                name: "u1".to_owned(),
                cards: vec![6],
            },
            WsTractorPlayedCards {
                position: 0,
                name: "u0".to_owned(),
                cards: vec![19],
            },
        ]];
        state.hands.insert(0, vec![30, 31, 32, 33]);
        state.hands.insert(1, vec![20, 21, 22]);
        state.hands.insert(2, vec![34, 35, 36]);
        state.hands.insert(3, vec![5, 12]);

        assert_eq!(decide(&state, 3), Some(vec![5]));
    }

    #[test]
    fn complete_ai_rounds_never_stall_or_produce_an_illegal_play() {
        for deck_count in [2, 3, 4] {
            let mut state = test_state(TractorRank::TWO);
            let mut rules = state.rules.clone();
            rules.deck_count = deck_count;
            rules.bottom_card_count = if deck_count == 3 { 10 } else { 8 };
            state.phase = TractorPhase::Start;
            state
                .deal_new_round(rules)
                .expect("prepare randomized AI round");
            while state.phase == TractorPhase::Deal {
                state.deal_next_card().expect("deal card");
            }
            let bury = state.choose_auto_bury().expect("AI bury plan");
            state
                .bury_bottom(state.dealer_position, bury)
                .expect("AI bury is legal");

            let initial_cards = state.hands.values().map(Vec::len).sum::<usize>();
            let mut actions = 0usize;
            while state.phase == TractorPhase::Play {
                let position = state.current_position;
                let cards = decide(&state, position).expect("AI always has a decision");
                state
                    .play_cards(position, format!("u{position}"), cards)
                    .expect("AI decision is accepted by the referee");
                actions += 1;
                assert!(actions <= initial_cards, "AI round did not make progress");
            }
            assert_eq!(state.phase, TractorPhase::Settlement);
            assert!(state.hands.values().all(Vec::is_empty));
        }
    }

    #[test]
    fn dealer_team_preserves_trump_tractor_when_plain_route_is_available_early() {
        let mut state = test_state(TractorRank::TWO);
        state.rules.blood_enabled = false;
        state.rules.trump_suit = Some(TractorSuit::SPADE);
        state.dealer_position = 0;
        state.hands.insert(
            0,
            vec![
                11, 111, 12, 112, // high trump tractor reserved for bottom control
                15, 115, 16, 116, // plain heart tractor to establish first
                30, 31, 32, 33, 42, 44,
            ],
        );
        for position in 1..4 {
            state.hands.insert(position, vec![35; 20]);
        }

        let play = decide(&state, 0).expect("lead");
        let classified = combo::classify(&play, &state.rules).expect("tractor");
        assert_eq!(classified.kind, ComboKind::Tractor(2));
        assert_eq!(classified.suit, Some(1));
    }

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
}
