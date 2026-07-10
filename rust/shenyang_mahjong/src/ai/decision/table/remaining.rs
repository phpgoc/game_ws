use share_type_public::games::shenyang_mahjong::WsShenyangMahjongMeld;

use crate::ai::observation::AiPublicTable;

use super::{claim_tile_already_visible, valid_meld_tile_count, visible_tile_count};
use crate::ai::decision::tile::is_valid_tile;

pub(in crate::ai::decision) fn remaining_tile_count(
    hand: &[i32],
    table: &AiPublicTable,
    _position: usize,
    tile: i32,
) -> i32 {
    if !is_valid_tile(tile) {
        return 0;
    }
    let visible = visible_tile_count(table, tile);
    let own = hand.iter().filter(|&&item| item == tile).count() as i32;
    (4 - visible - own).max(0)
}

#[cfg(test)]
pub(in crate::ai::decision) fn remaining_tile_count_with_melds(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
) -> i32 {
    remaining_tile_count_with_melds_after_discards(hand, melds, table, position, tile, &[])
}

pub(in crate::ai::decision) fn remaining_tile_count_with_melds_after_discards(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    discarded_tiles: &[i32],
) -> i32 {
    if !is_valid_tile(tile) {
        return 0;
    }
    let visible = visible_tile_count(table, tile);
    let own = hand.iter().filter(|&&item| item == tile).count() as i32;
    let simulated_melds = simulated_meld_tile_count(table, position, melds, tile);
    let simulated_discards = discarded_tiles
        .iter()
        .filter(|discarded_tile| **discarded_tile == tile)
        .count() as i32;
    (4 - visible - own - simulated_melds - simulated_discards).max(0)
}

fn simulated_meld_tile_count(
    table: &AiPublicTable,
    position: usize,
    melds: &[WsShenyangMahjongMeld],
    tile: i32,
) -> i32 {
    if !is_valid_tile(tile) {
        return 0;
    }
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
