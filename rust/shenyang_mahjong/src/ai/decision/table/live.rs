use share_type_public::games::shenyang_mahjong::SHENYANG_MAHJONG_TILE_KINDS;

use crate::ai::observation::AiPublicTable;

use super::visible_tile_count;
use crate::ai::decision::tile::{is_honor, tile_is_terminal};

pub(in crate::ai::decision) fn live_terminal_or_honor_count(
    hand: &[i32],
    table: &AiPublicTable,
) -> i32 {
    SHENYANG_MAHJONG_TILE_KINDS
        .into_iter()
        .filter(|tile| is_honor(*tile) || tile_is_terminal(*tile))
        .map(|tile| {
            let visible = visible_tile_count(table, tile);
            let own_hand = hand.iter().filter(|item| **item == tile).count() as i32;
            (4 - visible - own_hand).max(0)
        })
        .sum()
}

pub(in crate::ai::decision) fn live_terminal_or_honor_count_after_discard(
    hand_after_discard: &[i32],
    table: &AiPublicTable,
    discarded_tile: i32,
) -> i32 {
    SHENYANG_MAHJONG_TILE_KINDS
        .into_iter()
        .filter(|tile| is_honor(*tile) || tile_is_terminal(*tile))
        .map(|tile| {
            let visible = visible_tile_count(table, tile);
            let own_hand = hand_after_discard
                .iter()
                .filter(|item| **item == tile)
                .count() as i32;
            let own_discard = i32::from(discarded_tile == tile);
            (4 - visible - own_hand - own_discard).max(0)
        })
        .sum()
}

pub(in crate::ai::decision) fn live_tile_count_for_suit(
    hand: &[i32],
    table: &AiPublicTable,
    suit: i32,
) -> i32 {
    (1..=9)
        .map(|rank| {
            let tile = suit * 10 + rank;
            let visible = visible_tile_count(table, tile);
            let own_hand = hand.iter().filter(|item| **item == tile).count() as i32;
            (4 - visible - own_hand).max(0)
        })
        .sum()
}

pub(in crate::ai::decision) fn live_tile_count_for_suit_after_discard(
    hand_after_discard: &[i32],
    table: &AiPublicTable,
    suit: i32,
    discarded_tile: i32,
) -> i32 {
    (1..=9)
        .map(|rank| {
            let tile = suit * 10 + rank;
            let visible = visible_tile_count(table, tile);
            let own_hand = hand_after_discard
                .iter()
                .filter(|item| **item == tile)
                .count() as i32;
            let own_discard = i32::from(discarded_tile == tile);
            (4 - visible - own_hand - own_discard).max(0)
        })
        .sum()
}
