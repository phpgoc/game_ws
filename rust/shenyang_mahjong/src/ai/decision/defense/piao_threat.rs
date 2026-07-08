use super::*;

pub(in crate::ai::decision) fn piao_threat_needs_suit(seat: &AiSeatView, suit: i32) -> bool {
    piao_threat_level(&seat.melds) >= 3
        && !piao_threat_cannot_satisfy_three_suits(&seat.melds, seat.hand_count)
        && piao_missing_suits_from_melds(&seat.melds).contains(&suit)
}

pub(in crate::ai::decision) fn piao_threat_blocks_missing_suit_safety(
    seat: &AiSeatView,
    tile: i32,
) -> bool {
    if !is_suited(tile) {
        return false;
    }
    let threat_level = piao_threat_level(&seat.melds);
    if threat_level < 3 || piao_threat_cannot_satisfy_three_suits(&seat.melds, seat.hand_count) {
        return false;
    }
    if threat_level >= 4 && seat.hand_count <= 2 {
        return piao_final_pair_wait_satisfies_exposed_requirements(&seat.melds, tile);
    }
    piao_threat_needs_suit(seat, tile_suit(tile))
}

pub(in crate::ai::decision) fn opponent_threat_discard_bias(
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

pub(in crate::ai::decision) fn piao_final_pair_wait_satisfies_exposed_requirements(
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

pub(in crate::ai::decision) fn piao_final_pair_missing_suit_wait_matches(
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

pub(in crate::ai::decision) fn piao_terminal_or_honor_need_penalty(
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

pub(in crate::ai::decision) fn piao_needs_terminal_or_honor_from_melds(
    melds: &[WsShenyangMahjongMeld],
) -> bool {
    piao_threat_level(melds) >= 3
        && !melds
            .iter()
            .filter(|meld| is_triplet_like_meld(meld))
            .flat_map(|meld| meld.tiles.iter().copied())
            .any(|tile| is_honor(tile) || tile_is_terminal(tile))
}

pub(in crate::ai::decision) fn piao_threat_cannot_satisfy_three_suits(
    melds: &[WsShenyangMahjongMeld],
    hand_count: usize,
) -> bool {
    piao_threat_level(melds) >= 4
        && hand_count <= 2
        && piao_missing_suits_from_melds(melds).len() >= 2
}

pub(in crate::ai::decision) fn piao_threat_exposure_scale(table: &AiPublicTable, tile: i32) -> f64 {
    match exposed_meld_tile_count(table, tile) {
        0 => 1.0,
        1 => 0.8,
        2 => 0.55,
        _ => 0.25,
    }
}

pub(in crate::ai::decision) fn piao_threat_pair_penalty(tile: i32, own_tile_count: usize) -> f64 {
    if own_tile_count < 2 {
        return 0.0;
    }
    if is_honor(tile) || tile_is_terminal(tile) {
        6.0
    } else {
        4.0
    }
}
