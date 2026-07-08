use super::*;

pub(super) fn closed_opponent_threat_discard_bias(
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    own_tile_count: usize,
) -> f64 {
    if table.wall_count > 42 || public_discard_count(table, tile) > 0 {
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
            let pair_penalty = if own_tile_count >= 2 {
                if is_honor(tile) || tile_is_terminal(tile) {
                    4.0
                } else {
                    3.0
                }
            } else {
                0.0
            };
            (base - pair_penalty)
                * pressure_scale
                * exposure_scale
                * closed_hand_count_pressure_scale(seat)
                * closed_suit_shedding_scale(seat, tile)
        })
        .sum()
}

pub(super) fn is_closed_opponent_threat_candidate(seat: &AiSeatView) -> bool {
    !has_open_meld(&seat.melds)
        && (seat.hand_count >= 10 || (seat.hand_count > 0 && has_concealed_gang_meld(&seat.melds)))
}

pub(super) fn closed_hand_count_pressure_scale(seat: &AiSeatView) -> f64 {
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

pub(super) fn closed_suit_shedding_scale(seat: &AiSeatView, tile: i32) -> f64 {
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
                .filter(|discard| !is_suited(**discard) || tile_suit(**discard) != tile_suit(tile))
                .count();
            if off_suit_discards >= 4 { 1.25 } else { 1.0 }
        }
        1 => 0.78,
        2 => 0.55,
        3 => 0.25,
        _ => 0.15,
    }
}

pub(super) fn closed_threat_exposure_scale(table: &AiPublicTable, tile: i32) -> f64 {
    let exposed_meld_count = exposed_meld_tile_count(table, tile);
    match exposed_meld_count {
        0 => 1.0,
        1 => 0.7,
        2 => 0.45,
        3 => 0.15,
        _ => 0.0,
    }
}

pub(super) fn late_defense_discard_bias(table: &AiPublicTable, position: usize, tile: i32) -> f64 {
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

pub(super) fn late_defense_tile_safety_score(
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    own_tile_count: usize,
) -> f64 {
    late_defense_discard_bias(table, position, tile)
        + late_defense_exposed_meld_bias(table, tile)
        + late_defense_own_tile_shape_bias(table, tile, own_tile_count)
        + opponent_threat_discard_bias(table, position, tile, own_tile_count)
        + pure_one_suit_threat_discard_bias(table, position, tile, own_tile_count)
        + opponent_missing_suit_safety_bias(table, position, tile)
        + closed_opponent_threat_discard_bias(table, position, tile, own_tile_count)
        + estimate_pressure_for_tile(table, position, tile)
}

pub(super) fn late_defense_exposed_meld_bias(table: &AiPublicTable, tile: i32) -> f64 {
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

pub(super) fn late_defense_own_tile_shape_bias(
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

pub(super) fn public_defense_tile_safety_score(
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

pub(super) fn basic_heng_recovery_public_defense_bias(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    tile: i32,
    win_rule: i32,
) -> f64 {
    if loses_basic_heng_recovery_after_discard(hand, melds, table, tile, win_rule) {
        -22.0
    } else {
        0.0
    }
}

pub(super) fn public_defense_own_tile_shape_bias(tile: i32, own_tile_count: usize) -> f64 {
    match own_tile_count {
        0 | 1 => 0.0,
        2 if is_dragon(tile) => -18.0,
        2 if is_wind(tile) || tile_is_terminal(tile) => -12.0,
        2 => -8.0,
        _ if is_dragon(tile) => -28.0,
        _ if is_wind(tile) || tile_is_terminal(tile) => -20.0,
        _ => -14.0,
    }
}

pub(super) fn mid_round_public_discard_bias(
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
        16.0
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

pub(super) fn mid_round_open_meld_safety_bias(table: &AiPublicTable, tile: i32) -> f64 {
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

pub(super) fn mid_round_live_honor_risk_bias(
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
    if public_discard_count(table, tile) > 0 {
        return 0.0;
    }
    if !is_dragon(tile) {
        return -5.0;
    }
    -18.0 - open_opponent_live_dragon_risk(table, position, tile)
}

pub(super) fn open_opponent_live_dragon_risk(
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

pub(super) fn mid_round_live_suited_risk_bias(
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
        || public_discard_count(table, tile) > 0
        || should_keep_pairs_for_seven_pairs_discard(hand, melds, table, position, win_rule)
    {
        return 0.0;
    }
    let base = if tile_is_terminal(tile) { 7.0 } else { 10.0 };
    -(base
        + open_opponent_live_suited_risk(table, position, tile)
        + own_open_live_suited_pressure(melds, table, position, tile))
}

pub(super) fn open_opponent_live_suited_risk(
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

pub(super) fn live_risk_exposure_scale(table: &AiPublicTable, tile: i32) -> f64 {
    match exposed_meld_tile_count(table, tile) {
        0 => 1.0,
        1 => 0.8,
        2 => 0.55,
        3 => 0.25,
        _ => 0.0,
    }
}

pub(super) fn own_open_live_suited_pressure(
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
) -> f64 {
    if table.wall_count > 42 || !is_suited(tile) || public_discard_count(table, tile) > 0 {
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

pub(super) fn own_open_public_safety_bias(
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
) -> f64 {
    if table.wall_count > 42 || is_late_defense_round(table) {
        return 0.0;
    }
    let own_open_melds = melds.iter().filter(|meld| is_open_meld(meld)).count();
    if own_open_melds < 2 || !open_opponent_exists_for_tile(table, position, tile) {
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

pub(super) fn opponent_missing_suit_safety_bias(
    table: &AiPublicTable,
    position: usize,
    tile: i32,
) -> f64 {
    if !is_late_defense_round(table) || !is_suited(tile) {
        return 0.0;
    }
    opponent_missing_suit_safety_read(table, position, tile)
}

pub(super) fn mid_broken_opponent_missing_suit_safety_bias(
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

pub(super) fn opponent_missing_suit_safety_read(
    table: &AiPublicTable,
    position: usize,
    tile: i32,
) -> f64 {
    let suit = tile_suit(tile);
    if table.seats.iter().any(|(seat_position, seat)| {
        *seat_position != position && piao_threat_needs_suit(seat, suit)
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

pub(super) fn closed_opponent_may_need_suit(
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

pub(super) fn piao_threat_needs_suit(seat: &AiSeatView, suit: i32) -> bool {
    piao_threat_level(&seat.melds) >= 3
        && !piao_threat_cannot_satisfy_three_suits(&seat.melds, seat.hand_count)
        && piao_missing_suits_from_melds(&seat.melds).contains(&suit)
}

pub(super) fn opponent_threat_discard_bias(
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    own_tile_count: usize,
) -> f64 {
    let mut bias = 0.0;
    for (seat_position, seat) in &table.seats {
        if *seat_position == position {
            continue;
        }
        let threat_level = piao_threat_level(&seat.melds);
        if threat_level < 3 {
            continue;
        }
        if piao_threat_cannot_satisfy_three_suits(&seat.melds, seat.hand_count) {
            continue;
        }
        if seat.discards.contains(&tile) {
            bias += 4.5;
            continue;
        }
        let exposure_scale = piao_threat_exposure_scale(table, tile);
        if threat_level >= 4 && seat.hand_count <= 2 {
            let public_discount = (public_discard_count(table, tile) as f64 * 10.0
                + exposed_meld_tile_count(table, tile) as f64 * 8.0)
                .min(48.0);
            let final_pair_wait_matches =
                piao_final_pair_wait_satisfies_exposed_requirements(&seat.melds, tile);
            let terminal_or_honor_need_penalty = if final_pair_wait_matches {
                piao_terminal_or_honor_need_penalty(&seat.melds, tile)
            } else {
                0.0
            };
            let missing_suit_wait_penalty = if final_pair_wait_matches
                && piao_final_pair_missing_suit_wait_matches(&seat.melds, tile)
            {
                if own_tile_count >= 2 { 14.0 } else { 11.0 }
            } else {
                0.0
            };
            let single_wait_penalty = if !final_pair_wait_matches {
                0.0
            } else if is_dragon(tile) {
                86.0
            } else if is_honor(tile) || tile_is_terminal(tile) {
                80.0
            } else {
                72.0
            };
            let pair_penalty = piao_threat_pair_penalty(tile, own_tile_count);
            let late_multiplier = if is_late_round(table) { 1.25 } else { 1.0 };
            bias -= ((single_wait_penalty
                + pair_penalty
                + terminal_or_honor_need_penalty
                + missing_suit_wait_penalty)
                - public_discount)
                .max(10.0)
                * late_multiplier;
            continue;
        }
        let terminal_or_honor_need_penalty = piao_terminal_or_honor_need_penalty(&seat.melds, tile);
        let piao_wait_suit_penalty = if is_suited(tile)
            && piao_missing_suits_from_melds(&seat.melds).contains(&tile_suit(tile))
        {
            if own_tile_count >= 2 { 7.0 } else { 5.0 }
        } else {
            0.0
        };
        let live_tile_penalty = if is_dragon(tile) {
            7.0
        } else if is_wind(tile) {
            5.0
        } else if tile_is_terminal(tile) {
            4.0
        } else {
            5.5
        };
        let pair_penalty = piao_threat_pair_penalty(tile, own_tile_count);
        let late_multiplier = if is_late_round(table) { 1.35 } else { 1.0 };
        bias -= (live_tile_penalty
            + pair_penalty
            + piao_wait_suit_penalty
            + terminal_or_honor_need_penalty)
            * late_multiplier
            * exposure_scale;
    }
    bias
}

pub(super) fn piao_final_pair_wait_satisfies_exposed_requirements(
    melds: &[WsShenyangMahjongMeld],
    tile: i32,
) -> bool {
    let missing_suits = piao_missing_suits_from_melds(melds);
    if !missing_suits.is_empty() && (!is_suited(tile) || !missing_suits.contains(&tile_suit(tile)))
    {
        return false;
    }
    if piao_needs_terminal_or_honor_from_melds(melds) {
        return is_honor(tile) || tile_is_terminal(tile);
    }
    true
}

pub(super) fn piao_final_pair_missing_suit_wait_matches(
    melds: &[WsShenyangMahjongMeld],
    tile: i32,
) -> bool {
    if !is_suited(tile) || !piao_missing_suits_from_melds(melds).contains(&tile_suit(tile)) {
        return false;
    }
    if piao_needs_terminal_or_honor_from_melds(melds) {
        return tile_is_terminal(tile);
    }
    true
}

pub(super) fn piao_terminal_or_honor_need_penalty(
    melds: &[WsShenyangMahjongMeld],
    tile: i32,
) -> f64 {
    if !(is_honor(tile) || tile_is_terminal(tile))
        || !piao_needs_terminal_or_honor_from_melds(melds)
    {
        return 0.0;
    }
    if is_dragon(tile) {
        8.0
    } else if is_wind(tile) {
        7.0
    } else {
        6.0
    }
}

pub(super) fn piao_needs_terminal_or_honor_from_melds(melds: &[WsShenyangMahjongMeld]) -> bool {
    piao_threat_level(melds) >= 3
        && !melds
            .iter()
            .filter(|meld| is_triplet_like_meld(meld))
            .flat_map(|meld| meld.tiles.iter().copied())
            .any(|tile| is_honor(tile) || tile_is_terminal(tile))
}

pub(super) fn piao_threat_cannot_satisfy_three_suits(
    melds: &[WsShenyangMahjongMeld],
    hand_count: usize,
) -> bool {
    piao_threat_level(melds) >= 4
        && hand_count <= 2
        && piao_missing_suits_from_melds(melds).len() >= 2
}

pub(super) fn piao_threat_exposure_scale(table: &AiPublicTable, tile: i32) -> f64 {
    match exposed_meld_tile_count(table, tile) {
        0 => 1.0,
        1 => 0.8,
        2 => 0.55,
        _ => 0.25,
    }
}

pub(super) fn piao_threat_pair_penalty(tile: i32, own_tile_count: usize) -> f64 {
    if own_tile_count < 2 {
        return 0.0;
    }
    if is_honor(tile) || tile_is_terminal(tile) {
        6.0
    } else {
        4.0
    }
}

pub(super) fn pure_one_suit_threat_discard_bias(
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    own_tile_count: usize,
) -> f64 {
    if table.wall_count > 52 || !is_suited(tile) || public_discard_count(table, tile) > 0 {
        return 0.0;
    }
    let suit = tile_suit(tile);
    table
        .seats
        .iter()
        .filter(|(seat_position, _)| **seat_position != position)
        .filter_map(|(_, seat)| {
            let (threat_suit, open_melds) = pure_one_suit_threat_suit(seat)?;
            (threat_suit == suit && !seat_has_open_meld_tile(seat, tile)).then_some((
                seat,
                open_melds,
                threat_suit,
            ))
        })
        .map(|(seat, open_melds, threat_suit)| {
            let base = if tile_is_terminal(tile) { 7.0 } else { 10.0 };
            let pair_penalty = if own_tile_count >= 2 {
                if tile_is_terminal(tile) { 5.0 } else { 7.0 }
            } else {
                0.0
            };
            let meld_pressure = pure_one_suit_threat_meld_pressure(open_melds);
            let late_pressure = if table.wall_count <= 20 {
                1.35
            } else if table.wall_count <= 42 {
                1.15
            } else {
                1.0
            };
            let hand_pressure = if seat.hand_count <= 4 {
                1.3
            } else if seat.hand_count <= 7 {
                1.15
            } else {
                1.0
            };
            let exposed_discount = (exposed_meld_tile_count(table, tile) as f64 * 4.0).min(8.0);
            let discard_scale = pure_one_suit_threat_discard_scale(seat, threat_suit);
            -((base + pair_penalty) * meld_pressure * late_pressure * hand_pressure * discard_scale
                - exposed_discount)
                .max(2.0)
        })
        .sum()
}

pub(super) fn pure_one_suit_threat_discard_scale(seat: &AiSeatView, threat_suit: i32) -> f64 {
    let same_suit_discards = seat
        .discards
        .iter()
        .filter(|discard| is_suited(**discard) && tile_suit(**discard) == threat_suit)
        .count();
    match same_suit_discards {
        0 => {
            let off_suit_discards = seat
                .discards
                .iter()
                .filter(|discard| !is_suited(**discard) || tile_suit(**discard) != threat_suit)
                .count();
            if off_suit_discards >= 4 {
                1.25
            } else if off_suit_discards >= 2 {
                1.1
            } else {
                1.0
            }
        }
        1 => 0.7,
        2 => 0.45,
        _ => 0.25,
    }
}

pub(super) fn pure_one_suit_threat_meld_pressure(open_melds: usize) -> f64 {
    if open_melds <= 1 {
        0.55
    } else {
        (open_melds as f64 - 1.0).min(2.0)
    }
}

pub(super) fn pure_one_suit_threat_suit(seat: &AiSeatView) -> Option<(i32, usize)> {
    let mut open_meld_count = 0usize;
    let mut threat_suit = None;
    for meld in seat.melds.iter().filter(|meld| is_open_meld(meld)) {
        open_meld_count += 1;
        for tile in meld.tiles.iter().copied() {
            if !is_suited(tile) {
                return None;
            }
            let suit = tile_suit(tile);
            match threat_suit {
                Some(current) if current != suit => return None,
                Some(_) => {}
                None => threat_suit = Some(suit),
            }
        }
    }
    if open_meld_count == 0 {
        return pure_one_suit_closed_discard_threat_suit(seat).map(|suit| (suit, 0));
    }
    threat_suit.and_then(|suit| {
        (open_meld_count >= 2 || pure_one_suit_single_meld_discard_evidence(seat, suit))
            .then_some((suit, open_meld_count))
    })
}

pub(super) fn pure_one_suit_closed_discard_threat_suit(seat: &AiSeatView) -> Option<i32> {
    if has_open_meld(&seat.melds) || seat.discards.len() < 5 {
        return None;
    }

    let mut suit_discards = [0usize; 3];
    for discard in seat
        .discards
        .iter()
        .copied()
        .filter(|tile| is_suited(*tile))
    {
        suit_discards[tile_suit(discard) as usize] += 1;
    }
    let untouched_suits = suit_discards
        .iter()
        .enumerate()
        .filter_map(|(suit, count)| (*count == 0).then_some(suit as i32))
        .collect::<Vec<_>>();
    if untouched_suits.len() != 1 {
        return None;
    }
    let threat_suit = untouched_suits[0];
    let off_suit_discards = seat
        .discards
        .iter()
        .filter(|discard| !is_suited(**discard) || tile_suit(**discard) != threat_suit)
        .count();
    (off_suit_discards >= 5).then_some(threat_suit)
}

pub(super) fn pure_one_suit_single_meld_discard_evidence(
    seat: &AiSeatView,
    threat_suit: i32,
) -> bool {
    let same_suit_discards = seat
        .discards
        .iter()
        .filter(|discard| is_suited(**discard) && tile_suit(**discard) == threat_suit)
        .count();
    let off_suit_discards = seat
        .discards
        .iter()
        .filter(|discard| !is_suited(**discard) || tile_suit(**discard) != threat_suit)
        .count();
    same_suit_discards == 0 && off_suit_discards >= 4
}

pub(super) fn own_previous_discard_safety_bias(
    table: &AiPublicTable,
    position: usize,
    tile: i32,
) -> f64 {
    if !is_late_defense_round(table) {
        return 0.0;
    }
    own_previous_discard_count(table, position, tile) as f64 * 4.0
}

pub(super) fn wait_setting_discard_safety_adjustment(
    table: &AiPublicTable,
    position: usize,
    discard_tile: i32,
    own_tile_count: usize,
) -> f64 {
    let piao_threat = opponent_threat_discard_bias(table, position, discard_tile, own_tile_count);
    let pure_one_suit_threat =
        pure_one_suit_threat_discard_bias(table, position, discard_tile, own_tile_count);
    let safety = late_defense_tile_safety_score(table, position, discard_tile, own_tile_count)
        + mid_round_public_discard_bias(table, position, discard_tile)
        + mid_round_open_meld_safety_bias(table, discard_tile)
        + mid_broken_opponent_missing_suit_safety_bias(table, position, discard_tile);
    safety.clamp(-36.0, 36.0) * 0.6
        + piao_threat.min(0.0) * 1.5
        + pure_one_suit_threat.min(0.0) * 1.0
}

pub(super) fn should_open_broken_closed_hand_for_defense(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> bool {
    if has_open_meld(melds) || !is_mid_broken_hand_defense_round(table) {
        return false;
    }
    if should_preserve_seven_pairs_plan_for_context(hand, melds, table, position, win_rule)
        || pure_one_suit_plan_score_for_context(hand, melds, table, position) > 0.0
        || piao_plan_score_for_context(hand, melds, table, position) >= 22.0
    {
        return false;
    }
    if ready_tile_score(hand, melds, table, position, win_rule) > 0.0
        || one_step_wait_potential(hand, melds, table, position, win_rule) > 0.0
    {
        return false;
    }

    let (missing_rule_requirements, unrecoverable_rule_requirements) =
        if win_rule == WIN_RULE_SHENYANG_BASIC {
            let missing_rule_requirements = [
                !missing_suits(hand, melds).is_empty(),
                !has_terminal_or_honor_with_extra(hand, melds, None),
                !has_triplet_or_dragon_pair(hand, melds),
            ]
            .into_iter()
            .filter(|missing| *missing)
            .count();
            let unrecoverable_rule_requirements =
                unrecoverable_basic_rule_requirement_count(hand, melds, table);
            (missing_rule_requirements, unrecoverable_rule_requirements)
        } else {
            (0, 0)
        };
    let power = hand_power(hand);
    if !is_late_round(table) {
        return unrecoverable_rule_requirements >= 1
            || missing_rule_requirements >= 2
            || power < 14.0;
    }
    unrecoverable_rule_requirements >= 1 || missing_rule_requirements >= 1 || power < 18.0
}

pub(super) fn should_pass_late_unready_claim_for_defense(
    table: &AiPublicTable,
    current_ready_score: f64,
) -> bool {
    is_late_defense_round(table) && current_ready_score <= 0.0
}

pub(super) fn should_use_broken_hand_public_defense_discard(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> bool {
    if is_late_defense_round(table)
        || !is_mid_broken_hand_defense_round(table)
        || !unique_tiles(hand).into_iter().any(|tile| {
            public_discard_count(table, tile) > 0
                || mid_round_open_meld_safety_bias(table, tile) > 0.0
                || mid_broken_opponent_missing_suit_safety_bias(table, position, tile) > 0.0
        })
    {
        return false;
    }
    if should_lock_seven_pairs_plan(hand, melds, table, position, win_rule)
        || pure_one_suit_plan_score_for_context(hand, melds, table, position) > 0.0
        || piao_plan_score_for_context(hand, melds, table, position) >= 22.0
    {
        return false;
    }
    if best_ready_score_after_discard(hand, melds, table, position, win_rule) > 0.0
        || best_one_step_wait_potential_after_discard(hand, melds, table, position, win_rule) > 0.0
    {
        return false;
    }

    let missing_rule_requirements = if win_rule == WIN_RULE_SHENYANG_BASIC {
        [
            !missing_suits(hand, melds).is_empty(),
            !has_terminal_or_honor_with_extra(hand, melds, None),
            !has_triplet_or_dragon_pair(hand, melds),
        ]
        .into_iter()
        .filter(|missing| *missing)
        .count()
    } else {
        0
    };
    let unrecoverable_rule_requirements = if win_rule == WIN_RULE_SHENYANG_BASIC {
        unrecoverable_basic_rule_requirement_count(hand, melds, table)
    } else {
        0
    };
    if table.dealer_position == position && unrecoverable_rule_requirements == 0 {
        return false;
    }
    let power_threshold = if is_late_round(table) { 18.0 } else { 16.0 };
    unrecoverable_rule_requirements >= 1
        || missing_rule_requirements >= 2
        || hand_power(hand) < power_threshold
}
