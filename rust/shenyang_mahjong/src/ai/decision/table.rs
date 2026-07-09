use share_type_public::games::shenyang_mahjong::{
    SHENYANG_MAHJONG_TILE_KINDS, WsShenyangMahjongMeld,
};

use crate::ai::observation::{AiPublicTable, AiSeatView};

use super::meld::{has_open_meld, is_open_meld, is_valid_meld};
use super::tile::{is_honor, tile_is_terminal};

pub(super) fn exposed_meld_tile_count(table: &AiPublicTable, tile: i32) -> usize {
    table
        .seats
        .values()
        .flat_map(|seat| seat.melds.iter())
        .filter(|meld| is_valid_meld(meld))
        .flat_map(|meld| meld.tiles.iter().copied())
        .filter(|meld_tile| *meld_tile == tile)
        .count()
}

pub(super) fn live_terminal_or_honor_count(hand: &[i32], table: &AiPublicTable) -> i32 {
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

pub(super) fn live_terminal_or_honor_count_after_discard(
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

pub(super) fn live_tile_count_for_suit(hand: &[i32], table: &AiPublicTable, suit: i32) -> i32 {
    (1..=9)
        .map(|rank| {
            let tile = suit * 10 + rank;
            let visible = visible_tile_count(table, tile);
            let own_hand = hand.iter().filter(|item| **item == tile).count() as i32;
            (4 - visible - own_hand).max(0)
        })
        .sum()
}

pub(super) fn live_tile_count_for_suit_after_discard(
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

pub(super) fn known_unavailable_tiles_with_simulated_discards(
    table: &AiPublicTable,
    position: usize,
    projected_melds: &[WsShenyangMahjongMeld],
    simulated_discards: &[i32],
) -> Vec<i32> {
    let claimed_tile_in_projected_meld = table.claim_window.as_ref().and_then(|claim| {
        let current_meld_count = table
            .seats
            .get(&position)
            .map(|seat| valid_meld_tile_count(&seat.melds, claim.tile))
            .unwrap_or(0);
        let projected_meld_count = valid_meld_tile_count(projected_melds, claim.tile);
        (projected_meld_count > current_meld_count && claim_tile_already_visible(table, claim.tile))
            .then_some(claim.tile)
    });
    let mut skipped_claim_discard = false;
    let mut tiles = Vec::new();
    for seat in table.seats.values() {
        for discard in &seat.discards {
            if claimed_tile_in_projected_meld == Some(*discard) && !skipped_claim_discard {
                skipped_claim_discard = true;
                continue;
            }
            tiles.push(*discard);
        }
    }
    tiles.extend(simulated_discards.iter().copied());
    for (seat_position, seat) in &table.seats {
        if *seat_position == position {
            continue;
        }
        for meld in seat.melds.iter().filter(|meld| is_valid_meld(meld)) {
            tiles.extend(meld.tiles.iter().copied());
        }
    }
    tiles
}

pub(super) fn next_position_after(current: usize, table: &AiPublicTable) -> usize {
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

pub(super) fn open_meld_tile_count(table: &AiPublicTable, tile: i32) -> usize {
    table
        .seats
        .values()
        .flat_map(|seat| seat.melds.iter())
        .filter(|meld| is_open_meld(meld))
        .flat_map(|meld| meld.tiles.iter().copied())
        .filter(|meld_tile| *meld_tile == tile)
        .count()
}

pub(super) fn open_opponent_exists_for_tile(
    table: &AiPublicTable,
    position: usize,
    tile: i32,
) -> bool {
    table.seats.iter().any(|(seat_position, seat)| {
        *seat_position != position
            && has_open_meld(&seat.melds)
            && !seat.discards.contains(&tile)
            && !seat_has_open_meld_tile(seat, tile)
    })
}

pub(super) fn own_previous_discard_count(
    table: &AiPublicTable,
    position: usize,
    tile: i32,
) -> usize {
    table
        .seats
        .get(&position)
        .map(|seat| {
            seat.discards
                .iter()
                .filter(|discard| **discard == tile)
                .count()
        })
        .unwrap_or(0)
}

pub(super) fn public_discard_count(table: &AiPublicTable, tile: i32) -> usize {
    table
        .seats
        .values()
        .map(|seat| {
            seat.discards
                .iter()
                .filter(|discard| **discard == tile)
                .count()
        })
        .sum()
}

pub(super) fn public_discard_seat_count(table: &AiPublicTable, tile: i32) -> usize {
    table
        .seats
        .values()
        .filter(|seat| seat.discards.iter().any(|discard| *discard == tile))
        .count()
}

pub(super) fn remaining_tile_count(
    hand: &[i32],
    table: &AiPublicTable,
    _position: usize,
    tile: i32,
) -> i32 {
    let visible = visible_tile_count(table, tile);
    let own = hand.iter().filter(|&&item| item == tile).count() as i32;
    (4 - visible - own).max(0)
}

#[cfg(test)]
pub(super) fn remaining_tile_count_with_melds(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
) -> i32 {
    remaining_tile_count_with_melds_after_discards(hand, melds, table, position, tile, &[])
}

pub(super) fn remaining_tile_count_with_melds_after_discards(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    discarded_tiles: &[i32],
) -> i32 {
    let visible = visible_tile_count(table, tile);
    let own = hand.iter().filter(|&&item| item == tile).count() as i32;
    let simulated_melds = simulated_meld_tile_count(table, position, melds, tile);
    let simulated_discards = discarded_tiles
        .iter()
        .filter(|discarded_tile| **discarded_tile == tile)
        .count() as i32;
    (4 - visible - own - simulated_melds - simulated_discards).max(0)
}

pub(super) fn remaining_tile_count_after_discard(
    hand_after_discard: &[i32],
    table: &AiPublicTable,
    discarded_tile: i32,
    tile: i32,
) -> i32 {
    let visible = visible_tile_count(table, tile);
    let own = hand_after_discard
        .iter()
        .filter(|&&item| item == tile)
        .count() as i32;
    let own_discard = i32::from(discarded_tile == tile);
    (4 - visible - own - own_discard).max(0)
}

pub(super) fn seat_has_open_meld_tile(seat: &AiSeatView, tile: i32) -> bool {
    seat.melds
        .iter()
        .any(|meld| is_open_meld(meld) && meld.tiles.contains(&tile))
}

pub(super) fn visible_tile_count(table: &AiPublicTable, tile: i32) -> i32 {
    table
        .seats
        .values()
        .map(|seat| {
            let discard_count = seat.discards.iter().filter(|&&item| item == tile).count();
            let meld_count = seat
                .melds
                .iter()
                .filter(|meld| is_valid_meld(meld))
                .flat_map(|meld| meld.tiles.iter().copied())
                .filter(|item| *item == tile)
                .count();
            discard_count + meld_count
        })
        .sum::<usize>() as i32
}

fn simulated_meld_tile_count(
    table: &AiPublicTable,
    position: usize,
    melds: &[WsShenyangMahjongMeld],
    tile: i32,
) -> i32 {
    let current_table_melds = table
        .seats
        .get(&position)
        .map(|seat| valid_meld_tile_count(&seat.melds, tile))
        .unwrap_or(0);
    let projected_melds = valid_meld_tile_count(melds, tile);
    let added_meld_tiles = (projected_melds - current_table_melds).max(0);
    let already_visible_claim_tile =
        i32::from(added_meld_tiles > 0 && claim_tile_already_visible(table, tile));
    (added_meld_tiles - already_visible_claim_tile).max(0)
}

fn valid_meld_tile_count(melds: &[WsShenyangMahjongMeld], tile: i32) -> i32 {
    melds
        .iter()
        .filter(|meld| is_valid_meld(meld))
        .flat_map(|meld| meld.tiles.iter().copied())
        .filter(|meld_tile| *meld_tile == tile)
        .count() as i32
}

fn claim_tile_already_visible(table: &AiPublicTable, tile: i32) -> bool {
    let Some(claim_window) = &table.claim_window else {
        return false;
    };
    claim_window.tile == tile
        && table
            .seats
            .get(&claim_window.from_position)
            .is_some_and(|seat| seat.discards.contains(&tile))
}
