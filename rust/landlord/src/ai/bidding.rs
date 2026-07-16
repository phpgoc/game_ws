use std::collections::BTreeMap;

use share_type_public::LandlordPhase;

use crate::core::play::card_rank;

use super::{AiObservation, candidates::estimate_turns};

pub(super) fn choose_bid(observation: &AiObservation) -> u8 {
    if observation.phase != LandlordPhase::CallLandlord
        || observation.current_position != observation.position
    {
        return 0;
    }
    let desired = desired_bid(&observation.hand);
    if desired > observation.current_score {
        desired
    } else {
        0
    }
}

fn desired_bid(hand: &[i32]) -> u8 {
    let grouped = rank_counts(hand);
    let rocket = grouped.get(&16) == Some(&1) && grouped.get(&17) == Some(&1);
    let bombs = grouped.values().filter(|count| **count == 4).count();
    let turns = estimate_turns(hand).max(1);

    let mut strength = 0.0;
    if rocket {
        strength += 7.0;
    } else {
        strength += grouped.get(&17).copied().unwrap_or(0) as f64 * 3.0;
        strength += grouped.get(&16).copied().unwrap_or(0) as f64 * 2.3;
    }
    strength += bombs as f64 * 5.0;
    strength += grouped.get(&15).copied().unwrap_or(0) as f64 * 1.55;
    strength += grouped.get(&14).copied().unwrap_or(0) as f64 * 0.65;
    strength += (8_usize.saturating_sub(turns)) as f64 * 1.15;

    // 手牌越整齐、控制牌越多，拿三张底牌后的收益越高。
    match strength {
        value if value >= 15.0 => 3,
        value if value >= 10.0 => 2,
        value if value >= 6.5 => 1,
        _ => 0,
    }
}

fn rank_counts(cards: &[i32]) -> BTreeMap<u8, usize> {
    let mut counts = BTreeMap::new();
    for &card in cards {
        *counts.entry(card_rank(card)).or_insert(0) += 1;
    }
    counts
}

#[cfg(test)]
mod tests {
    use share_type_public::LandlordPhase;

    use crate::ai::{AiObservation, tests::state_with_hands};

    use super::choose_bid;

    fn observation(hand: Vec<i32>, current_score: u8) -> AiObservation {
        let mut state = state_with_hands(&[(0, hand), (1, vec![2, 3, 4]), (2, vec![5, 6, 7])]);
        state.phase = LandlordPhase::CallLandlord;
        state.current_position = 0;
        state.score = current_score as u32;
        AiObservation::from_state(&state, 0).expect("observation")
    }

    #[test]
    fn rocket_bomb_and_twos_call_three() {
        let strong = vec![
            53, 54, 13, 26, 39, 52, 1, 14, 27, 40, 2, 15, 3, 16, 4, 17, 5,
        ];
        assert_eq!(choose_bid(&observation(strong, 0)), 3);
    }

    #[test]
    fn scattered_low_cards_pass() {
        let weak = vec![1, 2, 3, 4, 5, 6, 14, 16, 18, 20, 28, 30, 32, 34, 42, 44, 46];
        assert_eq!(choose_bid(&observation(weak, 0)), 0);
    }

    #[test]
    fn never_repeats_or_underbids_current_score() {
        let medium = vec![54, 13, 26, 1, 14, 27, 2, 15, 28, 3, 16, 4, 17, 5, 18, 6, 19];
        let first = choose_bid(&observation(medium.clone(), 0));
        assert!(first > 0);
        assert_eq!(choose_bid(&observation(medium, first)), 0);
    }
}
