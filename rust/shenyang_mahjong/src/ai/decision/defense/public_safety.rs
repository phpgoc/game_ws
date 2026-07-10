use super::*;

pub(in crate::ai::decision) fn public_defense_tile_safety_score(
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    own_tile_count: usize,
) -> f64 {
    late_defense_tile_safety_score(table, position, tile, own_tile_count)
        + public_defense_own_tile_shape_bias(tile, own_tile_count)
        + mid_round_public_discard_bias(table, position, tile)
        + mid_round_open_meld_safety_bias(table, tile)
        + mid_broken_opponent_missing_suit_safety_bias(table, position, tile)
}

pub(in crate::ai::decision) fn basic_heng_recovery_public_defense_bias(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    win_rule: i32,
) -> f64 {
    if loses_basic_heng_recovery_after_discard(hand, melds, table, position, tile, win_rule) {
        -22.0
    } else {
        0.0
    }
}

pub(in crate::ai::decision) fn public_defense_own_tile_shape_bias(
    tile: i32,
    own_tile_count: usize,
) -> f64 {
    match own_tile_count {
        0 | 1 => 0.0,
        2 if is_dragon(tile) => -22.0,
        2 if is_wind(tile) || tile_is_terminal(tile) => -12.0,
        2 => -8.0,
        _ if is_dragon(tile) => -28.0,
        _ if is_wind(tile) || tile_is_terminal(tile) => -20.0,
        _ => -14.0,
    }
}

pub(in crate::ai::decision) fn mid_round_public_discard_bias(
    table: &AiPublicTable,
    position: usize,
    tile: i32,
) -> f64 {
    if !is_mid_round(table) || is_late_defense_round(table) {
        return 0.0;
    }
    let public_discards = public_discard_count(table, tile);
    if public_discards == 0 {
        return 0.0;
    }
    let shape_bonus = if is_honor(tile) {
        22.0
    } else if tile_is_terminal(tile) {
        1.5
    } else {
        2.0
    };
    let multi_seat_bonus = public_discard_seat_count(table, tile) as f64 * 2.0;
    9.0 + public_discards as f64 * 4.0
        + shape_bonus
        + multi_seat_bonus
        + own_previous_discard_count(table, position, tile) as f64 * 4.0
}

pub(in crate::ai::decision) fn mid_round_open_meld_safety_bias(
    table: &AiPublicTable,
    tile: i32,
) -> f64 {
    if !is_mid_round(table) || is_late_defense_round(table) || public_discard_count(table, tile) > 0
    {
        return 0.0;
    }
    match open_meld_tile_count(table, tile) {
        0 | 1 => 0.0,
        2 => 6.0,
        _ => 20.0,
    }
}

pub(in crate::ai::decision) fn mid_round_live_honor_risk_bias(
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    own_tile_count: usize,
) -> f64 {
    if table.wall_count > 60
        || is_late_defense_round(table)
        || own_tile_count != 1
        || !is_honor(tile)
    {
        return 0.0;
    }
    if tile_known_safe_for_live_risk(table, tile, own_tile_count) {
        return 0.0;
    }
    if !is_dragon(tile) {
        return -5.0;
    }
    -18.0 - open_opponent_live_dragon_risk(table, position, tile)
}

pub(in crate::ai::decision) fn open_opponent_live_dragon_risk(
    table: &AiPublicTable,
    position: usize,
    tile: i32,
) -> f64 {
    if !is_dragon(tile) {
        return 0.0;
    }
    let open_opponents = table
        .seats
        .iter()
        .filter(|(seat_position, seat)| {
            **seat_position != position
                && has_open_meld(&seat.melds)
                && !seat.discards.contains(&tile)
                && !seat_has_open_meld_tile(seat, tile)
        })
        .count();
    if open_opponents == 0 {
        return 0.0;
    }
    let open_risk = (open_opponents as f64 * 4.0).min(12.0);
    let late_round_risk = if is_late_round(table) { 4.0 } else { 0.0 };
    (open_risk + late_round_risk) * live_risk_exposure_scale(table, tile)
}

pub(in crate::ai::decision) fn mid_round_live_suited_risk_bias(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    own_tile_count: usize,
    win_rule: i32,
) -> f64 {
    if table.wall_count > 60
        || is_late_defense_round(table)
        || own_tile_count != 1
        || !is_suited(tile)
        || should_keep_pairs_for_seven_pairs_discard(hand, melds, table, position, win_rule)
    {
        return 0.0;
    }
    if tile_known_safe_for_live_risk(table, tile, own_tile_count) {
        return 0.0;
    }
    let base = if tile_is_terminal(tile) { 7.0 } else { 10.0 };
    -(base
        + open_opponent_live_suited_risk(table, position, tile)
        + own_open_live_suited_pressure(melds, table, position, tile, own_tile_count))
}

pub(in crate::ai::decision) fn open_opponent_live_suited_risk(
    table: &AiPublicTable,
    position: usize,
    tile: i32,
) -> f64 {
    if !is_suited(tile) {
        return 0.0;
    }
    let open_opponents = table
        .seats
        .iter()
        .filter(|(seat_position, seat)| {
            **seat_position != position
                && has_open_meld(&seat.melds)
                && !seat.discards.contains(&tile)
                && !seat_has_open_meld_tile(seat, tile)
        })
        .count();
    if open_opponents == 0 {
        return 0.0;
    }
    let per_open = if tile_is_terminal(tile) { 2.5 } else { 3.5 };
    let cap = if tile_is_terminal(tile) { 7.5 } else { 10.5 };
    let open_risk = (open_opponents as f64 * per_open).min(cap);
    let late_round_risk = if is_late_round(table) { 2.5 } else { 0.0 };
    (open_risk + late_round_risk) * live_risk_exposure_scale(table, tile)
}

pub(in crate::ai::decision) fn live_risk_exposure_scale(table: &AiPublicTable, tile: i32) -> f64 {
    match exposed_meld_tile_count(table, tile) {
        0 => 1.0,
        1 => 0.8,
        2 => 0.55,
        3 => 0.25,
        _ => 0.0,
    }
}

fn tile_known_safe_for_live_risk(table: &AiPublicTable, tile: i32, own_tile_count: usize) -> bool {
    public_discard_count(table, tile) > 0
        || exposed_meld_tile_count(table, tile) + own_tile_count >= 4
}

pub(in crate::ai::decision) fn own_open_live_suited_pressure(
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    own_tile_count: usize,
) -> f64 {
    if table.wall_count > 42
        || !is_suited(tile)
        || tile_known_safe_for_live_risk(table, tile, own_tile_count)
    {
        return 0.0;
    }
    let own_open_melds = melds.iter().filter(|meld| is_open_meld(meld)).count();
    if own_open_melds < 2 {
        return 0.0;
    }
    if !open_opponent_exists_for_tile(table, position, tile) {
        return 0.0;
    }
    if tile_is_terminal(tile) { 36.0 } else { 24.0 }
}

pub(in crate::ai::decision) fn own_open_public_safety_bias(
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
) -> f64 {
    if table.wall_count > 42 || is_late_defense_round(table) {
        return 0.0;
    }
    let own_open_melds = melds.iter().filter(|meld| is_open_meld(meld)).count();
    if own_open_melds == 0 || !open_opponent_exists_for_tile(table, position, tile) {
        return 0.0;
    }
    let public_discards = public_discard_count(table, tile);
    if public_discards == 0 {
        return 0.0;
    }
    let shape_bonus = if is_honor(tile) {
        8.0
    } else if tile_is_terminal(tile) {
        3.0
    } else {
        6.0
    };
    18.0 + public_discards as f64 * 6.0 + shape_bonus
}
