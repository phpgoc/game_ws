use std::collections::{HashMap, HashSet};

use share_type_public::games::shenyang_mahjong::WsShenyangMahjongMeld;

use crate::rules::{
    has_dragon_pair_as_standard_pair, has_triplet_in_standard_decomposition, is_complete_win,
    sort_tiles,
};

use super::meld::{is_triplet_like_meld, valid_meld_tiles};
use super::tile::{is_honor, is_suited, tile_is_terminal, tile_rank, tile_suit, unique_tiles};

pub(super) fn hand_power(hand: &[i32]) -> f64 {
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

pub(super) fn has_terminal_or_honor_with_extra(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    extra: Option<i32>,
) -> bool {
    hand.iter()
        .copied()
        .chain(extra)
        .chain(valid_meld_tiles(melds))
        .any(|tile| is_honor(tile) || tile_is_terminal(tile))
}

pub(super) fn has_triplet_like_group(hand: &[i32], melds: &[WsShenyangMahjongMeld]) -> bool {
    melds.iter().any(is_triplet_like_meld)
        || unique_tiles(hand)
            .into_iter()
            .any(|tile| hand.iter().filter(|item| **item == tile).count() >= 3)
}

pub(super) fn has_triplet_or_dragon_pair(hand: &[i32], melds: &[WsShenyangMahjongMeld]) -> bool {
    has_triplet_or_dragon_pair_with_extra(hand, melds, None)
}

pub(super) fn has_triplet_or_dragon_pair_with_extra(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    extra: Option<i32>,
) -> bool {
    let tiles = hand.iter().copied().chain(extra).collect::<Vec<_>>();
    if is_complete_win(&tiles, melds.len()) {
        return melds.iter().any(is_triplet_like_meld)
            || has_triplet_in_standard_decomposition(&tiles)
            || has_dragon_pair_as_standard_pair(&tiles);
    }

    let mut counts = HashMap::<i32, usize>::new();
    for tile in tiles {
        *counts.entry(tile).or_default() += 1;
    }
    melds.iter().any(is_triplet_like_meld)
        || counts.values().any(|count| *count >= 3)
        || [35, 36, 37]
            .into_iter()
            .any(|tile| counts.get(&tile).copied().unwrap_or(0) >= 2)
}

pub(super) fn is_seven_pairs_wait_shape(hand: &[i32]) -> bool {
    if hand.len() != 13 {
        return false;
    }
    let mut counts: HashMap<i32, usize> = HashMap::new();
    for &tile in hand {
        *counts.entry(tile).or_default() += 1;
    }
    let pairs = counts.values().map(|count| count / 2).sum::<usize>();
    let singles = counts.values().filter(|&&count| count % 2 == 1).count();
    pairs == 6 && singles == 1
}

pub(super) fn missing_suits(hand: &[i32], melds: &[WsShenyangMahjongMeld]) -> Vec<i32> {
    suit_presence(hand, melds)
        .into_iter()
        .enumerate()
        .filter_map(|(suit, present)| (!present).then_some(suit as i32))
        .collect()
}

pub(super) fn neighbor_count(hand: &[i32], tile: i32) -> i32 {
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

pub(super) fn pair_count(hand: &[i32]) -> usize {
    let mut counts: HashMap<i32, usize> = HashMap::new();
    for &tile in hand {
        *counts.entry(tile).or_default() += 1;
    }
    counts.values().map(|count| count / 2).sum()
}

pub(super) fn remove_n_tiles(hand: &[i32], tile: i32, count: usize) -> Vec<i32> {
    let mut removed = 0usize;
    let mut next = Vec::with_capacity(hand.len().saturating_sub(count));
    for &item in hand {
        if item == tile && removed < count {
            removed += 1;
        } else {
            next.push(item);
        }
    }
    next
}

pub(super) fn single_tile(hand: &[i32]) -> Option<i32> {
    let mut counts: HashMap<i32, usize> = HashMap::new();
    for &tile in hand {
        *counts.entry(tile).or_default() += 1;
    }
    counts
        .into_iter()
        .find_map(|(tile, count)| (count % 2 == 1).then_some(tile))
}

pub(super) fn suit_presence(hand: &[i32], melds: &[WsShenyangMahjongMeld]) -> [bool; 3] {
    suit_presence_with_extra(hand, melds, None)
}

pub(super) fn suit_presence_with_extra(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    extra: Option<i32>,
) -> [bool; 3] {
    let mut suits = [false; 3];
    for tile in hand.iter().copied().chain(extra) {
        if is_suited(tile) {
            suits[tile_suit(tile) as usize] = true;
        }
    }
    for tile in valid_meld_tiles(melds) {
        if is_suited(tile) {
            suits[tile_suit(tile) as usize] = true;
        }
    }
    suits
}

pub(super) fn suited_tile_count_for_suit(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    suit: i32,
) -> usize {
    hand.iter()
        .copied()
        .chain(valid_meld_tiles(melds))
        .filter(|tile| is_suited(*tile) && tile_suit(*tile) == suit)
        .count()
}

pub(super) fn terminal_or_honor_count(hand: &[i32], melds: &[WsShenyangMahjongMeld]) -> usize {
    hand.iter()
        .copied()
        .chain(valid_meld_tiles(melds))
        .filter(|tile| is_honor(*tile) || tile_is_terminal(*tile))
        .count()
}

pub(super) fn tile_is_core_closed_middle_wait_member(hand: &[i32], tile: i32) -> bool {
    if !is_suited(tile) {
        return false;
    }
    [-2, 2].into_iter().any(|offset| {
        let other = tile + offset;
        is_suited(other)
            && tile_suit(other) == tile_suit(tile)
            && hand.iter().any(|item| *item == other)
            && {
                let low_rank = tile_rank(tile).min(tile_rank(other));
                let high_rank = tile_rank(tile).max(tile_rank(other));
                matches!((low_rank, high_rank), (3, 5) | (4, 6) | (5, 7))
            }
    })
}

pub(super) fn tile_is_core_two_sided_wait_member(hand: &[i32], tile: i32) -> bool {
    if !is_suited(tile) {
        return false;
    }
    [-1, 1].into_iter().any(|offset| {
        let other = tile + offset;
        is_suited(other)
            && tile_suit(other) == tile_suit(tile)
            && hand.iter().any(|item| *item == other)
            && {
                let low_rank = tile_rank(tile).min(tile_rank(other));
                let high_rank = tile_rank(tile).max(tile_rank(other));
                matches!((low_rank, high_rank), (3, 4) | (4, 5) | (5, 6) | (6, 7))
            }
    })
}

pub(super) fn tile_is_middle_of_sequence(hand: &[i32], tile: i32) -> bool {
    if !is_suited(tile) || !(2..=8).contains(&tile_rank(tile)) {
        return false;
    }
    let left = tile - 1;
    let right = tile + 1;
    hand.iter().any(|item| *item == left) && hand.iter().any(|item| *item == right)
}

pub(super) fn tile_is_part_of_complete_sequence(hand: &[i32], tile: i32) -> bool {
    if !is_suited(tile) {
        return false;
    }
    let rank = tile_rank(tile);
    let suit = tile_suit(tile);
    let min_start = (rank - 2).max(1);
    let max_start = rank.min(7);
    (min_start..=max_start).any(|start| {
        (start..start + 3).all(|sequence_rank| {
            let sequence_tile = suit * 10 + sequence_rank;
            hand.iter().any(|item| *item == sequence_tile)
        })
    })
}

pub(super) fn tile_is_weak_edge_wait_terminal(hand: &[i32], tile: i32) -> bool {
    if !is_suited(tile) {
        return false;
    }
    match tile_rank(tile) {
        1 => hand
            .iter()
            .any(|item| *item == tile + 1 || *item == tile + 2),
        9 => hand
            .iter()
            .any(|item| *item == tile - 1 || *item == tile - 2),
        _ => false,
    }
}
