use super::*;

pub(in crate::ai::decision) fn neighbor_count(hand: &[i32], tile: i32) -> i32 {
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

pub(in crate::ai::decision) fn tile_is_core_closed_middle_wait_member(
    hand: &[i32],
    tile: i32,
) -> bool {
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

pub(in crate::ai::decision) fn tile_is_core_two_sided_wait_member(hand: &[i32], tile: i32) -> bool {
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

pub(in crate::ai::decision) fn tile_is_middle_of_sequence(hand: &[i32], tile: i32) -> bool {
    if !is_suited(tile) || !(2..=8).contains(&tile_rank(tile)) {
        return false;
    }
    let left = tile - 1;
    let right = tile + 1;
    hand.iter().any(|item| *item == left) && hand.iter().any(|item| *item == right)
}

pub(in crate::ai::decision) fn tile_is_part_of_complete_sequence(hand: &[i32], tile: i32) -> bool {
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

pub(in crate::ai::decision) fn tile_is_weak_edge_wait_terminal(hand: &[i32], tile: i32) -> bool {
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
