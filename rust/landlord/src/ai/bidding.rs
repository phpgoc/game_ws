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
    let opponent_pressure = observation
        .call_history
        .iter()
        .filter(|(position, _)| *position != observation.position)
        .map(|(_, score)| *score)
        .max()
        .unwrap_or(0) as f64
        * 0.35;
    let desired = desired_bid(&observation.hand, opponent_pressure);
    if desired > observation.current_score {
        desired
    } else {
        0
    }
}

fn desired_bid(hand: &[i32], opponent_pressure: f64) -> u8 {
    bid_for_strength(
        raw_hand_strength(hand) + expected_bottom_gain(hand) * 0.45 - opponent_pressure,
    )
}

/// 对手的叫分只是一条带噪声的公开线索。猜牌阶段会调用这个廉价版本，
/// 避免为每个可能牌局重复模拟底牌，同时仍然保留炸弹、控制牌和牌型整齐度信号。
pub(super) fn approximate_desired_bid(hand: &[i32], opponent_pressure: f64) -> u8 {
    // 普通三张底牌对牌型的平均帮助大约落在 1 分强度附近；这里故意不追求
    // 与本 AI 自己的叫分完全一致，否则会对真人不同的叫分风格过拟合。
    bid_for_strength(raw_hand_strength(hand) + 1.0 - opponent_pressure)
}

fn raw_hand_strength(hand: &[i32]) -> f64 {
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

    strength
}

fn bid_for_strength(strength: f64) -> u8 {
    // 手牌越整齐、控制牌越多，拿三张底牌后的收益越高。
    match strength {
        value if value >= 15.0 => 3,
        value if value >= 10.0 => 2,
        value if value >= 6.5 => 1,
        _ => 0,
    }
}

fn expected_bottom_gain(hand: &[i32]) -> f64 {
    const SAMPLES: usize = 20;

    let held = hand
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    let unknown = (1..=54)
        .filter(|card| !held.contains(card))
        .collect::<Vec<_>>();
    if unknown.len() < 3 {
        return 0.0;
    }
    let base_turns = estimate_turns(hand);
    let seed = hand.iter().fold(0xA076_1D64_78BD_642F_u64, |seed, card| {
        seed.rotate_left(7) ^ (*card as u64).wrapping_mul(0xE703_7ED1_A0B4_28DB)
    });
    let total_gain = (0..SAMPLES)
        .map(|sample_index| {
            let mut cards = unknown.clone();
            shuffle(
                &mut cards,
                seed ^ (sample_index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15),
            );
            let bottom = &cards[..3];
            let mut with_bottom = hand.to_vec();
            with_bottom.extend_from_slice(bottom);
            let combined_turns = estimate_turns(&with_bottom);
            let structure_gain = (base_turns + 3).saturating_sub(combined_turns) as f64 * 1.1;
            let control_gain = bottom
                .iter()
                .map(|card| match card_rank(*card) {
                    17 => 3.0,
                    16 => 2.3,
                    15 => 1.55,
                    14 => 0.65,
                    _ => 0.0,
                })
                .sum::<f64>();
            structure_gain + control_gain
        })
        .sum::<f64>();
    total_gain / SAMPLES as f64
}

fn shuffle(cards: &mut [i32], mut state: u64) {
    for index in (1..cards.len()).rev() {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        cards.swap(index, (state >> 32) as usize % (index + 1));
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

    use super::{choose_bid, expected_bottom_gain};

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

    #[test]
    fn bottom_card_expectation_is_deterministic_and_non_negative() {
        let hand = vec![53, 13, 26, 1, 14, 27, 2, 15, 28, 3, 16, 4, 17, 5, 18, 6, 19];
        let left = expected_bottom_gain(&hand);
        let right = expected_bottom_gain(&hand);

        assert!(left >= 0.0);
        assert!((left - right).abs() < f64::EPSILON);
    }
}
