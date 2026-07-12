use super::*;

pub(in crate::ai::decision) fn closed_hand_count_pressure_scale(seat: &AiSeatView) -> f64 {
    let concealed_gangs = seat
        .melds
        .iter()
        .filter(|meld| meld.kind == ShenyangMahjongMeldKind::GANG && meld.from_position.is_none())
        .count();
    if concealed_gangs == 0 {
        return 1.0;
    }

    let gang_scale = match concealed_gangs {
        1 => 1.18,
        2 => 1.35,
        _ => 1.55,
    };
    let hand_count_scale = match seat.hand_count {
        0 => 0.0,
        1..=5 => 1.55,
        6..=9 => 1.35,
        10 => 1.18,
        11..=12 => 1.08,
        _ => 1.0,
    };
    f64::max(gang_scale, hand_count_scale)
}

pub(in crate::ai::decision) fn closed_route_commitment_scale(seat: &AiSeatView) -> f64 {
    let valid_discards = seat
        .discards
        .iter()
        .filter(|tile| is_valid_tile(**tile))
        .count();
    match valid_discards {
        0..=2 => 0.85,
        3..=5 => 1.0,
        6..=8 => 1.12,
        _ => 1.25,
    }
}

pub(in crate::ai::decision) fn closed_opponent_threat_discard_bias(
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    own_tile_count: usize,
) -> f64 {
    if table.wall_count > 42 || public_discard_count(table, tile) > 0 {
        return 0.0;
    }
    if closed_threat_tile_fully_accounted(table, tile, own_tile_count) {
        return 0.0;
    }
    let exposure_scale = closed_threat_exposure_scale(table, tile);
    if exposure_scale == 0.0 {
        return 0.0;
    }
    let pressure_scale = if is_late_defense_round(table) {
        1.0
    } else {
        0.45
    };

    table
        .seats
        .iter()
        .filter(|(seat_position, seat)| {
            **seat_position != position && is_closed_opponent_threat_candidate(seat)
        })
        .map(|(_, seat)| {
            let base = if is_dragon(tile) {
                -13.0
            } else if is_wind(tile) {
                -12.0
            } else if tile_is_terminal(tile) {
                -9.0
            } else {
                -5.0
            };
            let pair_penalty = closed_threat_pair_penalty(tile, own_tile_count);
            (base - pair_penalty)
                * pressure_scale
                * exposure_scale
                * closed_hand_count_pressure_scale(seat)
                * closed_route_commitment_scale(seat)
                * closed_suit_shedding_scale(seat, tile)
        })
        .sum()
}

pub(in crate::ai::decision) fn closed_suit_shedding_scale(seat: &AiSeatView, tile: i32) -> f64 {
    if !is_suited(tile) {
        return 1.0;
    }
    let discarded_in_suit = seat
        .discards
        .iter()
        .filter(|discard| is_suited(**discard) && tile_suit(**discard) == tile_suit(tile))
        .count();
    match discarded_in_suit {
        0 => {
            let off_suit_discards = seat
                .discards
                .iter()
                .filter(|discard| {
                    is_valid_tile(**discard)
                        && (!is_suited(**discard) || tile_suit(**discard) != tile_suit(tile))
                })
                .count();
            if off_suit_discards >= 4 { 1.25 } else { 1.0 }
        }
        1 => 0.78,
        2 => 0.55,
        3 => 0.25,
        _ => 0.15,
    }
}

pub(in crate::ai::decision) fn closed_threat_exposure_scale(
    table: &AiPublicTable,
    tile: i32,
) -> f64 {
    let exposed_meld_count = exposed_meld_tile_count(table, tile);
    match exposed_meld_count {
        0 => 1.0,
        1 => 0.7,
        2 => 0.45,
        3 => 0.15,
        _ => 0.0,
    }
}

pub(in crate::ai::decision) fn closed_threat_pair_penalty(tile: i32, own_tile_count: usize) -> f64 {
    match own_tile_count {
        0 | 1 => 0.0,
        2 if is_honor(tile) || tile_is_terminal(tile) => 4.0,
        2 => 3.0,
        _ if is_honor(tile) || tile_is_terminal(tile) => 1.5,
        _ => 1.0,
    }
}

pub(in crate::ai::decision) fn closed_threat_tile_fully_accounted(
    table: &AiPublicTable,
    tile: i32,
    own_tile_count: usize,
) -> bool {
    exposed_meld_tile_count(table, tile) + public_discard_count(table, tile) + own_tile_count >= 4
}

pub(in crate::ai::decision) fn is_closed_opponent_threat_candidate(seat: &AiSeatView) -> bool {
    !has_open_meld(&seat.melds)
        && (seat.hand_count >= 10 || (seat.hand_count > 0 && has_concealed_gang_meld(&seat.melds)))
}
