use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use crate::rules::{can_chi, can_gang, can_peng, is_complete_win_with_melds, sort_tiles};

use super::observation::{AiClaimView, AiPublicTable};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiClaimChoice {
    Pass,
    Peng,
    Gang,
    Chi { consume_tiles: Vec<i32> },
    Hu,
}

fn best_chi_option(hand: &[i32], tile: i32) -> Option<Vec<i32>> {
    let mut best: Option<(f64, Vec<i32>)> = None;
    for consume_tiles in [
        [tile - 2, tile - 1],
        [tile - 1, tile + 1],
        [tile + 1, tile + 2],
    ] {
        if !can_chi(hand, tile, &consume_tiles) {
            continue;
        }
        let mut next = hand.to_vec();
        for consume in consume_tiles {
            if let Some(index) = next.iter().position(|item| *item == consume) {
                next.remove(index);
            }
        }
        next.push(tile);
        next.sort_unstable();
        let score = hand_power(&next);
        match &best {
            None => best = Some((score, consume_tiles.to_vec())),
            Some((best_score, best_tiles)) => {
                if score > *best_score
                    || (score == *best_score && consume_tiles.to_vec() < *best_tiles)
                {
                    best = Some((score, consume_tiles.to_vec()));
                }
            }
        }
    }
    best.map(|(_, tiles)| tiles)
}

pub fn choose_claim_from_view(
    hand: &[i32],
    claim: &AiClaimView,
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> Option<AiClaimChoice> {
    if !claim.eligible_positions.contains(&position) {
        return None;
    }
    let tile = claim.tile;
    let mut win_hand = hand.to_vec();
    win_hand.push(tile);
    win_hand.sort_unstable();
    let melds = table
        .seats
        .get(&position)
        .map(|seat| seat.melds.as_slice())
        .unwrap_or(&[]);
    if is_complete_win_with_melds(&win_hand, melds, win_rule) {
        return Some(AiClaimChoice::Hu);
    }

    if can_gang(hand, tile) {
        return Some(AiClaimChoice::Gang);
    }

    if can_peng(hand, tile) {
        let before = hand_power(hand);
        let mut next = hand.to_vec();
        let mut removed = 0;
        next.retain(|item| {
            if *item == tile && removed < 2 {
                removed += 1;
                false
            } else {
                true
            }
        });
        next.push(tile);
        next.push(tile);
        next.push(tile);
        next.sort_unstable();
        let after = hand_power(&next);
        if after >= before + 1.8 {
            return Some(AiClaimChoice::Peng);
        }
    }

    if position == next_position_after(claim.from_position, table) {
        if let Some(consume_tiles) = best_chi_option(hand, tile) {
            let mut next = hand.to_vec();
            for consume in &consume_tiles {
                if let Some(index) = next.iter().position(|item| item == consume) {
                    next.remove(index);
                }
            }
            next.push(tile);
            next.sort_unstable();
            if hand_power(&next) >= hand_power(hand) - 0.4 {
                return Some(AiClaimChoice::Chi { consume_tiles });
            }
        }
    }

    Some(AiClaimChoice::Pass)
}

pub fn choose_discard_from_view(
    hand: &[i32],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> Option<i32> {
    if hand.len() % 3 != 2 {
        return None;
    }
    let melds = table
        .seats
        .get(&position)
        .map(|seat| seat.melds.as_slice())
        .unwrap_or(&[]);
    if is_complete_win_with_melds(hand, melds, win_rule) {
        return None;
    }

    let mut best: Option<(f64, i32)> = None;
    for tile in hand.iter().copied() {
        let mut next = hand.to_vec();
        if let Some(index) = next.iter().position(|item| *item == tile) {
            next.remove(index);
        }
        let score = hand_power(&next);
        let pressure = estimate_pressure_for_tile(table, position, tile);
        let discard_bias = match (
            hand.iter().filter(|&&item| item == tile).count(),
            is_honor(tile),
            tile_is_terminal(tile),
            neighbor_count(hand, tile),
        ) {
            (c, true, _, _) if c == 1 => 6.0,
            (1, _, true, 0) => 4.8,
            (1, _, _, 0) => 4.0,
            (2, _, _, _) => -1.8,
            (c, _, _, neigh) if c >= 3 => -4.5 - neigh as f64,
            _ => 0.0,
        };
        let combined = score + discard_bias + pressure;
        match best {
            None => best = Some((combined, tile)),
            Some((best_score, best_tile)) => {
                let better = combined.partial_cmp(&best_score) == Some(Ordering::Greater);
                if better || (combined == best_score && tile < best_tile) {
                    best = Some((combined, tile));
                }
            }
        }
    }
    best.map(|(_, tile)| tile)
}

fn estimate_pressure_for_tile(table: &AiPublicTable, position: usize, tile: i32) -> f64 {
    let mut pressure = 0.0;
    for (seat_position, seat) in &table.seats {
        if *seat_position == position || seat.is_away || seat.is_ai {
            continue;
        }
        let dist = seat.position.abs_diff(position);
        if seat.discards.contains(&tile) {
            pressure += 2.0;
        }
        if seat.melds.len() >= 2 {
            pressure -= 0.7;
        }
        if tile >= 31 && seat.hand_count >= 10 {
            pressure += 0.5 / (dist as f64 + 1.0);
        }
        if tile_is_terminal(tile) && seat.hand_count >= 8 {
            pressure += 0.8 / (dist as f64 + 1.0);
        }
    }
    if table.wall_count < 30 {
        pressure -= 0.3;
    }
    if table.current_position == position && table.dealer_position != position {
        pressure += 0.1;
    }
    pressure
}

fn hand_power(hand: &[i32]) -> f64 {
    let mut counts: HashMap<i32, usize> = HashMap::new();
    for &tile in hand {
        *counts.entry(tile).or_default() += 1;
    }

    let mut score = 0.0;
    let mut used = HashSet::new();
    for (&tile, &count) in &counts {
        if count >= 3 {
            score += 18.0;
            used.insert(tile);
        } else if count == 2 {
            score += 7.0;
        }
        if is_honor(tile) {
            score -= if count == 1 { 4.6 } else { 2.0 };
        } else {
            let rank = tile_rank(tile);
            let neigh = neighbor_count(hand, tile) as f64;
            if tile_is_terminal(tile) {
                score -= 0.6;
            }
            score += neigh * 1.2;
            if (2..=8).contains(&rank) {
                score += 0.4;
            }
            if count == 1 && neigh == 0.0 {
                score -= 3.8;
            } else if count == 1 && neigh == 1.0 {
                score -= 1.2;
            }
        }
    }

    let mut working = hand.to_vec();
    sort_tiles(&mut working);
    let mut i = 0usize;
    while i + 2 < working.len() {
        let a = working[i];
        let b = working[i + 1];
        let c = working[i + 2];
        if is_suited(a)
            && tile_suit(a) == tile_suit(b)
            && tile_suit(a) == tile_suit(c)
            && a + 1 == b
            && b + 1 == c
        {
            score += 10.0;
            i += 3;
        } else {
            i += 1;
        }
    }

    score -= used.len() as f64 * 0.2;
    score
}

fn is_honor(tile: i32) -> bool {
    tile >= 31
}

fn is_suited(tile: i32) -> bool {
    matches!(tile, 1..=9 | 11..=19 | 21..=29)
}

fn neighbor_count(hand: &[i32], tile: i32) -> i32 {
    if !is_suited(tile) {
        return 0;
    }
    let suit = tile_suit(tile);
    let rank = tile_rank(tile);
    let mut count = 0;
    for delta in [-2, -1, 1, 2] {
        let candidate = suit * 10 + rank + delta;
        if candidate > 0 && candidate < 40 && tile_suit(candidate) == suit {
            count += hand.iter().filter(|&&item| item == candidate).count() as i32;
        }
    }
    count
}

fn next_position_after(current: usize, table: &AiPublicTable) -> usize {
    let mut positions: Vec<usize> = table.seats.keys().copied().collect();
    positions.sort_unstable();
    if positions.is_empty() {
        return current;
    }
    let idx = positions
        .iter()
        .position(|pos| *pos == current)
        .unwrap_or(0);
    positions[(idx + 1) % positions.len()]
}

fn tile_is_terminal(tile: i32) -> bool {
    matches!(tile_rank(tile), 1 | 9)
}

fn tile_rank(tile: i32) -> i32 {
    tile % 10
}

fn tile_suit(tile: i32) -> i32 {
    tile / 10
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::ai::observation::{AiClaimView, AiSeatView};
    use crate::rules::WIN_RULE_RELAXED;

    #[test]
    fn claim_gang_beats_peng_when_not_winning() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 35,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 35, 35, 35];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
            Some(AiClaimChoice::Gang)
        );
    }

    #[test]
    fn claim_hu_accepts_open_meld_remainder() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 35,
            from_position: 1,
            eligible_positions: vec![0],
        });
        table.seats.get_mut(&0).unwrap().melds = vec![
            share_type_public::games::shenyang_mahjong::WsShenyangMahjongMeld {
                kind: share_type_public::games::shenyang_mahjong::ShenyangMahjongMeldKind::PENG,
                tiles: vec![1, 1, 1],
                from_position: Some(2),
            },
        ];
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
            Some(AiClaimChoice::Hu)
        );
    }

    #[test]
    fn claim_hu_accepts_seven_pairs() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 35,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
            Some(AiClaimChoice::Hu)
        );
    }

    #[test]
    fn claim_hu_beats_other_claims() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 35,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
            Some(AiClaimChoice::Hu)
        );
    }

    #[test]
    fn discard_prefers_isolated_honor() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(31)
        );
    }

    #[test]
    fn discard_returns_none_for_seven_pairs_win() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            None
        );
    }

    #[test]
    fn discard_uses_public_discard_safety() {
        let table = table_with_discards(1, vec![31]);
        let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 32];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(31)
        );
    }

    fn table_with_discards(position: usize, discards: Vec<i32>) -> AiPublicTable {
        let mut seats = HashMap::new();
        seats.insert(
            0,
            AiSeatView {
                position: 0,
                is_ai: true,
                is_away: false,
                hand_count: 14,
                discards: Vec::new(),
                melds: Vec::new(),
            },
        );
        seats.insert(
            position,
            AiSeatView {
                position,
                is_ai: false,
                is_away: false,
                hand_count: 10,
                discards,
                melds: Vec::new(),
            },
        );
        AiPublicTable {
            current_position: 0,
            dealer_position: 1,
            wall_count: 60,
            claim_window: None,
            seats,
        }
    }
}
