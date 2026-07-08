use super::*;

pub(in crate::ai::decision) fn opponent_missing_suit_safety_bias(
    table: &AiPublicTable,
    position: usize,
    tile: i32,
) -> f64 {
    if !is_late_defense_round(table) || !is_suited(tile) {
        return 0.0;
    }
    opponent_missing_suit_safety_read(table, position, tile)
}

pub(in crate::ai::decision) fn mid_broken_opponent_missing_suit_safety_bias(
    table: &AiPublicTable,
    position: usize,
    tile: i32,
) -> f64 {
    if !is_mid_broken_hand_defense_round(table) || is_late_defense_round(table) || !is_suited(tile)
    {
        return 0.0;
    }
    opponent_missing_suit_safety_read(table, position, tile) * 0.7
}

pub(in crate::ai::decision) fn opponent_missing_suit_safety_read(
    table: &AiPublicTable,
    position: usize,
    tile: i32,
) -> f64 {
    let suit = tile_suit(tile);
    if table.seats.iter().any(|(seat_position, seat)| {
        *seat_position != position && piao_threat_blocks_missing_suit_safety(seat, tile)
    }) {
        return 0.0;
    }
    if closed_opponent_may_need_suit(table, position, suit) {
        return 0.0;
    }
    table
        .seats
        .iter()
        .filter(|(seat_position, _)| **seat_position != position)
        .map(|(_, seat)| {
            let discarded_in_suit = seat
                .discards
                .iter()
                .filter(|discard| is_suited(**discard) && tile_suit(**discard) == suit)
                .count();
            let exposed_in_suit = seat.melds.iter().any(|meld| {
                if !is_valid_meld(meld) {
                    return false;
                }
                meld.tiles
                    .iter()
                    .any(|meld_tile| is_suited(*meld_tile) && tile_suit(*meld_tile) == suit)
            });
            if exposed_in_suit {
                0.0
            } else if discarded_in_suit >= 3 {
                12.0 + (discarded_in_suit - 3) as f64 * 2.0
            } else if discarded_in_suit >= 2 {
                5.0
            } else {
                0.0
            }
        })
        .sum()
}

pub(in crate::ai::decision) fn closed_opponent_may_need_suit(
    table: &AiPublicTable,
    position: usize,
    suit: i32,
) -> bool {
    table.seats.iter().any(|(seat_position, seat)| {
        *seat_position != position
            && !has_open_meld(&seat.melds)
            && (seat.hand_count >= 13
                || (seat.hand_count > 0 && has_concealed_gang_meld(&seat.melds)))
            && seat
                .discards
                .iter()
                .filter(|discard| is_suited(**discard) && tile_suit(**discard) == suit)
                .count()
                < 2
    })
}
