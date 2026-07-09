use share_type_public::games::shenyang_mahjong::WsShenyangMahjongMeld;

use crate::ai::observation::AiPublicTable;

use crate::ai::decision::meld::{is_open_meld, is_valid_meld};
use crate::ai::decision::tile::is_valid_tile;

pub(in crate::ai::decision) fn exposed_meld_tile_count(table: &AiPublicTable, tile: i32) -> usize {
    if !is_valid_tile(tile) {
        return 0;
    }
    table
        .seats
        .values()
        .flat_map(|seat| seat.melds.iter())
        .filter(|meld| is_valid_meld(meld))
        .flat_map(|meld| meld.tiles.iter().copied())
        .filter(|meld_tile| *meld_tile == tile)
        .count()
}

pub(in crate::ai::decision) fn known_unavailable_tiles_with_simulated_discards(
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
        for discard in seat
            .discards
            .iter()
            .copied()
            .filter(|tile| is_valid_tile(*tile))
        {
            if claimed_tile_in_projected_meld == Some(discard) && !skipped_claim_discard {
                skipped_claim_discard = true;
                continue;
            }
            tiles.push(discard);
        }
    }
    tiles.extend(
        simulated_discards
            .iter()
            .copied()
            .filter(|tile| is_valid_tile(*tile)),
    );
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

pub(in crate::ai::decision) fn open_meld_tile_count(table: &AiPublicTable, tile: i32) -> usize {
    if !is_valid_tile(tile) {
        return 0;
    }
    table
        .seats
        .values()
        .flat_map(|seat| seat.melds.iter())
        .filter(|meld| is_open_meld(meld))
        .flat_map(|meld| meld.tiles.iter().copied())
        .filter(|meld_tile| *meld_tile == tile)
        .count()
}

pub(in crate::ai::decision) fn own_previous_discard_count(
    table: &AiPublicTable,
    position: usize,
    tile: i32,
) -> usize {
    if !is_valid_tile(tile) {
        return 0;
    }
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

pub(in crate::ai::decision) fn public_discard_count(table: &AiPublicTable, tile: i32) -> usize {
    if !is_valid_tile(tile) {
        return 0;
    }
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

pub(in crate::ai::decision) fn public_discard_seat_count(
    table: &AiPublicTable,
    tile: i32,
) -> usize {
    if !is_valid_tile(tile) {
        return 0;
    }
    table
        .seats
        .values()
        .filter(|seat| seat.discards.iter().any(|discard| *discard == tile))
        .count()
}

pub(in crate::ai::decision) fn visible_tile_count(table: &AiPublicTable, tile: i32) -> i32 {
    if !is_valid_tile(tile) {
        return 0;
    }
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

pub(in crate::ai::decision::table) fn valid_meld_tile_count(
    melds: &[WsShenyangMahjongMeld],
    tile: i32,
) -> i32 {
    if !is_valid_tile(tile) {
        return 0;
    }
    melds
        .iter()
        .filter(|meld| is_valid_meld(meld))
        .flat_map(|meld| meld.tiles.iter().copied())
        .filter(|meld_tile| *meld_tile == tile)
        .count() as i32
}

pub(in crate::ai::decision::table) fn claim_tile_already_visible(
    table: &AiPublicTable,
    tile: i32,
) -> bool {
    if !is_valid_tile(tile) {
        return false;
    }
    let Some(claim_window) = &table.claim_window else {
        return false;
    };
    claim_window.tile == tile
        && table
            .seats
            .get(&claim_window.from_position)
            .is_some_and(|seat| seat.discards.contains(&tile))
}
