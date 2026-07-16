use super::*;

pub(in crate::ai::decision) fn defense_tile_safety_priority(
    table: &AiPublicTable,
    tile: i32,
    own_tile_count: usize,
) -> u8 {
    if late_defense_tile_fully_accounted(table, tile, own_tile_count) {
        return 5;
    }
    let public_discards = public_discard_count(table, tile);
    if public_discards == 0 {
        return 0;
    }
    if is_honor(tile) {
        4
    } else if public_discards >= 2 {
        3
    } else if tile_is_terminal(tile) {
        1
    } else {
        2
    }
}

pub(in crate::ai::decision) fn late_defense_discard_bias(
    table: &AiPublicTable,
    position: usize,
    tile: i32,
) -> f64 {
    if !is_late_defense_round(table) {
        return 0.0;
    }
    let public_discards = public_discard_count(table, tile);
    if public_discards > 0 {
        let honor_bonus = if is_honor(tile) { 26.0 } else { 0.0 };
        let suited_shape_bonus = if is_suited(tile) {
            if tile_is_terminal(tile) { -1.0 } else { 2.0 }
        } else {
            0.0
        };
        let multi_seat_bonus = public_discard_seat_count(table, tile) as f64 * 3.0;
        return 28.0
            + public_discards as f64 * 6.0
            + honor_bonus
            + suited_shape_bonus
            + multi_seat_bonus
            + own_previous_discard_safety_bias(table, position, tile);
    }
    if is_wind(tile) {
        -4.0
    } else if is_dragon(tile) {
        -8.0
    } else if tile_is_terminal(tile) {
        -14.0
    } else {
        -22.0
    }
}

pub(in crate::ai::decision) fn late_defense_exposed_meld_bias(
    table: &AiPublicTable,
    tile: i32,
) -> f64 {
    if !is_late_defense_round(table) || public_discard_count(table, tile) > 0 {
        return 0.0;
    }
    match exposed_meld_tile_count(table, tile) {
        0 => 0.0,
        1 => 5.0,
        2 => 20.0,
        _ => 28.0,
    }
}

pub(in crate::ai::decision) fn late_defense_fully_accounted_bias(
    table: &AiPublicTable,
    tile: i32,
    own_tile_count: usize,
) -> f64 {
    if !is_late_defense_round(table)
        || public_discard_count(table, tile) > 0
        || !late_defense_tile_fully_accounted(table, tile, own_tile_count)
    {
        return 0.0;
    }
    32.0
}

pub(in crate::ai::decision) fn late_defense_own_tile_shape_bias(
    table: &AiPublicTable,
    tile: i32,
    own_tile_count: usize,
) -> f64 {
    if !is_late_defense_round(table) || public_discard_count(table, tile) > 0 {
        return 0.0;
    }
    if own_tile_count <= 1 {
        return 0.0;
    }
    if is_dragon(tile) {
        -8.0
    } else if is_wind(tile) || tile_is_terminal(tile) {
        -5.0
    } else {
        -2.0
    }
}

pub(in crate::ai::decision) fn late_defense_tile_fully_accounted(
    table: &AiPublicTable,
    tile: i32,
    own_tile_count: usize,
) -> bool {
    exposed_meld_tile_count(table, tile) + own_tile_count >= 4
}

pub(in crate::ai::decision) fn late_defense_tile_safety_score(
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    own_tile_count: usize,
) -> f64 {
    late_defense_discard_bias(table, position, tile)
        + late_defense_exposed_meld_bias(table, tile)
        + late_defense_fully_accounted_bias(table, tile, own_tile_count)
        + late_defense_own_tile_shape_bias(table, tile, own_tile_count)
        + opponent_threat_discard_bias(table, position, tile, own_tile_count)
        + pure_one_suit_threat_discard_bias(table, position, tile, own_tile_count)
        + opponent_missing_suit_safety_bias(table, position, tile)
        + closed_opponent_threat_discard_bias(table, position, tile, own_tile_count)
        + estimate_pressure_for_tile(table, position, tile)
}
