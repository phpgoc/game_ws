use crate::ai::observation::{AiPublicTable, AiSeatView};

use crate::ai::decision::meld::{has_open_meld, is_open_meld};
use crate::ai::decision::tile::is_valid_tile;

pub(in crate::ai::decision) fn open_opponent_exists_for_tile(
    table: &AiPublicTable,
    position: usize,
    tile: i32,
) -> bool {
    if !is_valid_tile(tile) {
        return false;
    }
    table.seats.iter().any(|(seat_position, seat)| {
        *seat_position != position
            && has_open_meld(&seat.melds)
            && !seat.discards.contains(&tile)
            && !seat_has_open_meld_tile(seat, tile)
    })
}

pub(in crate::ai::decision) fn seat_has_open_meld_tile(seat: &AiSeatView, tile: i32) -> bool {
    if !is_valid_tile(tile) {
        return false;
    }
    seat.melds
        .iter()
        .any(|meld| is_open_meld(meld) && meld.tiles.contains(&tile))
}
