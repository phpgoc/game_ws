use super::*;

const EDGE_WAIT_BONUS: f64 = 10.0;

pub(in crate::ai::decision) fn capped_basic_route_foundation_visible_fan_exceeds_half_cap(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    win_rule: i32,
) -> bool {
    let Some(max_fan) = table.max_fan.filter(|max_fan| *max_fan > 0) else {
        return false;
    };
    let visible_fan = 1 + estimated_visible_bonus_fan(hand, melds);
    has_basic_normal_route_foundation(hand, melds, win_rule) && visible_fan * 2 > max_fan
}

pub(in crate::ai::decision) fn capped_basic_route_foundation_visible_fan_reaches_cap(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    win_rule: i32,
) -> bool {
    let Some(max_fan) = table.max_fan.filter(|max_fan| *max_fan > 0) else {
        return false;
    };
    has_basic_normal_route_foundation(hand, melds, win_rule)
        && 1 + estimated_visible_bonus_fan(hand, melds) >= max_fan
}

pub(in crate::ai::decision) fn capped_normal_route_visible_fan_exceeds_half_cap(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    win_rule: i32,
) -> bool {
    let Some(max_fan) = table.max_fan.filter(|max_fan| *max_fan > 0) else {
        return false;
    };
    let visible_fan = 1 + estimated_visible_bonus_fan(hand, melds);
    has_normal_route_foundation(hand, melds, win_rule) && visible_fan * 2 > max_fan
}

pub(in crate::ai::decision) fn capped_normal_route_visible_fan_reaches_cap(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    win_rule: i32,
) -> bool {
    let Some(max_fan) = table.max_fan.filter(|max_fan| *max_fan > 0) else {
        return false;
    };
    has_normal_route_foundation(hand, melds, win_rule)
        && 1 + estimated_visible_bonus_fan(hand, melds) >= max_fan
}

pub(in crate::ai::decision) fn capped_open_basic_route_visible_fan_reaches_cap(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
) -> bool {
    let Some(max_fan) = table.max_fan.filter(|max_fan| *max_fan > 0) else {
        return false;
    };
    has_door_opening_meld(melds, table)
        && missing_suits(hand, melds).is_empty()
        && has_terminal_or_honor_with_extra(hand, melds, None)
        && has_triplet_or_dragon_pair(hand, melds)
        && 1 + estimated_visible_bonus_fan(hand, melds) >= max_fan
}

pub(in crate::ai::decision) fn capped_piao_route_visible_fan_projects_cap(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    next_hand: &[i32],
    next_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
) -> bool {
    capped_pattern_route_visible_fan_projects_cap(
        ShenyangMahjongWinPattern::PiaoHu,
        hand,
        melds,
        next_hand,
        next_melds,
        table,
    )
}

fn capped_pattern_route_visible_fan_projects_cap(
    pattern: ShenyangMahjongWinPattern,
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    next_hand: &[i32],
    next_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
) -> bool {
    let Some(max_fan) = table.max_fan.filter(|max_fan| *max_fan > 0) else {
        return false;
    };
    let base_fan = shenyang_win_pattern_base_fan(pattern);
    let current_fan = base_fan + estimated_visible_bonus_fan(hand, melds);
    let projected_fan = base_fan + estimated_visible_bonus_fan(next_hand, next_melds);
    current_fan * 2 > max_fan && current_fan < max_fan && projected_fan >= max_fan
}

pub(in crate::ai::decision) fn capped_pure_one_suit_route_visible_fan_projects_cap(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    next_hand: &[i32],
    next_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
) -> bool {
    capped_pattern_route_visible_fan_projects_cap(
        ShenyangMahjongWinPattern::PureOneSuit,
        hand,
        melds,
        next_hand,
        next_melds,
        table,
    )
}

pub(in crate::ai::decision) fn estimated_concealed_dragon_triplet_fan(hand: &[i32]) -> i32 {
    shenyang_score_concealed_dragon_triplet_fan(hand)
}

#[cfg(test)]
pub(in crate::ai::decision) fn estimated_fan_with_known_unavailable_wait(
    win_hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_tile: i32,
    _win_rule: i32,
    known_unavailable_tiles: &[i32],
) -> i32 {
    estimated_fan_with_known_unavailable_wait_for_rules(
        win_hand,
        melds,
        win_tile,
        ShenyangMahjongWinRules::new(),
        known_unavailable_tiles,
    )
}

fn estimated_fan_with_known_unavailable_wait_for_rules(
    win_hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_tile: i32,
    rules: ShenyangMahjongWinRules,
    known_unavailable_tiles: &[i32],
) -> i32 {
    shenyang_score_visible_win_fan(
        win_hand,
        melds,
        Some(win_tile),
        rules,
        known_unavailable_tiles,
    )
}

pub(in crate::ai::decision) fn estimated_fan_with_known_unavailable_wait_for_table(
    win_hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_tile: i32,
    table: &AiPublicTable,
    known_unavailable_tiles: &[i32],
) -> i32 {
    estimated_fan_with_known_unavailable_wait_for_rules(
        win_hand,
        melds,
        win_tile,
        win_rules_for_table(table),
        known_unavailable_tiles,
    )
}

#[cfg(test)]
pub(in crate::ai::decision) fn estimated_fan_with_wait(
    win_hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_tile: i32,
    win_rule: i32,
) -> i32 {
    estimated_fan_with_known_unavailable_wait(win_hand, melds, win_tile, win_rule, &[])
}

pub(in crate::ai::decision) fn estimated_four_gui_yi_fan(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
) -> i32 {
    shenyang_score_four_gui_yi_fan(hand, melds)
}

pub(in crate::ai::decision) fn estimated_meld_fan(melds: &[WsShenyangMahjongMeld]) -> i32 {
    shenyang_score_meld_fan(melds)
}

pub(in crate::ai::decision) fn estimated_visible_bonus_fan(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
) -> i32 {
    estimated_meld_fan(melds)
        + estimated_concealed_dragon_triplet_fan(hand)
        + estimated_four_gui_yi_fan(hand, melds)
}

#[cfg(test)]
pub(in crate::ai::decision) fn estimated_visible_fan_without_wait(
    win_hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    _win_rule: i32,
) -> i32 {
    estimated_visible_fan_without_wait_for_rules(win_hand, melds, ShenyangMahjongWinRules::new())
}

fn estimated_visible_fan_without_wait_for_rules(
    win_hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    rules: ShenyangMahjongWinRules,
) -> i32 {
    shenyang_score_visible_win_fan(win_hand, melds, None, rules, &[])
}

pub(in crate::ai::decision) fn estimated_visible_fan_without_wait_for_table(
    win_hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
) -> i32 {
    estimated_visible_fan_without_wait_for_rules(win_hand, melds, win_rules_for_table(table))
}

pub(in crate::ai::decision) fn fan_wait_bias(
    win_hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    win_tile: i32,
    remaining: i32,
    known_unavailable_tiles: &[i32],
) -> f64 {
    if table.dealer_position == position
        || dealer_opponent_has_major_threat(table, position, win_rule)
        || is_late_defense_round(table)
        || !is_single_wait_shape_for_table(
            win_hand,
            melds,
            win_tile,
            table,
            known_unavailable_tiles,
        )
    {
        return 0.0;
    }
    if remaining <= 1 {
        return 0.0;
    }
    if let Some(max_fan) = table.max_fan {
        let visible_fan = estimated_visible_fan_without_wait_for_table(win_hand, melds, table);
        if visible_fan * 2 > max_fan {
            return 0.0;
        }
        if visible_fan >= max_fan {
            return 0.0;
        }
        let total_fan = estimated_fan_with_known_unavailable_wait_for_table(
            win_hand,
            melds,
            win_tile,
            table,
            known_unavailable_tiles,
        );
        if total_fan >= max_fan {
            let fan_gap = max_fan - visible_fan;
            let wait_fan_gain = total_fan - visible_fan;
            if fan_gap == 1 && remaining >= 3 {
                return 14.0;
            }
            if fan_gap == 2 && wait_fan_gain >= 2 && remaining >= 2 {
                return 10.0;
            }
            return 0.0;
        }
    }

    let terminal_or_honor_bonus = if tile_is_terminal(win_tile) || is_honor(win_tile) {
        14.0
    } else {
        0.0
    };
    let edge_wait_bonus = if has_edge_wait_decomposition(win_hand, win_tile) {
        EDGE_WAIT_BONUS
    } else {
        0.0
    };
    let live_wait_scale = if remaining == 2 { 0.45 } else { 1.0 };
    (62.0 + terminal_or_honor_bonus + edge_wait_bonus)
        * live_wait_scale
        * pressured_open_wait_scale(table, position, melds)
}

pub(in crate::ai::decision) fn four_gui_yi_discard_bias(
    hand: &[i32],
    tile: i32,
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> f64 {
    let current_four_gui_yi = estimated_four_gui_yi_fan(hand, melds);
    if current_four_gui_yi <= 0 {
        return 0.0;
    }
    if capped_normal_route_visible_fan_exceeds_half_cap(hand, melds, table, win_rule) {
        return 0.0;
    }
    let next = remove_n_tiles(hand, tile, 1);
    if next.len() + 1 != hand.len() {
        return 0.0;
    }
    let after_four_gui_yi = estimated_four_gui_yi_fan(&next, melds);
    if after_four_gui_yi >= current_four_gui_yi {
        return 0.0;
    }
    if let Some(max_fan) = table.max_fan.filter(|max_fan| *max_fan > 0)
        && ready_hand_visible_fan_reaches_cap(&next, melds, table, position, win_rule, max_fan)
    {
        return 0.0;
    }
    if table.dealer_position == position
        || dealer_opponent_has_major_threat(table, position, win_rule)
    {
        return 0.0;
    }

    let fan_loss = (current_four_gui_yi - after_four_gui_yi) as f64;
    if ready_tile_score_after_discard(&next, melds, table, position, win_rule, tile) > 0.0 {
        return -28.0 * fan_loss;
    }
    if best_ready_score_after_discard(hand, melds, table, position, win_rule) > 0.0 {
        return -18.0 * fan_loss;
    }
    -6.0 * fan_loss
}

pub(in crate::ai::decision) fn has_basic_normal_route_foundation(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    _win_rule: i32,
) -> bool {
    missing_suits(hand, melds).is_empty()
        && has_terminal_or_honor_with_extra(hand, melds, None)
        && has_triplet_or_dragon_pair(hand, melds)
}

pub(in crate::ai::decision) fn has_normal_route_foundation(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    _win_rule: i32,
) -> bool {
    has_basic_normal_route_foundation(hand, melds, _win_rule)
}

pub(in crate::ai::decision) fn pressured_open_wait_scale(
    table: &AiPublicTable,
    position: usize,
    melds: &[WsShenyangMahjongMeld],
) -> f64 {
    if table.wall_count > 42 || !has_open_meld(melds) {
        return 1.0;
    }
    let open_opponents = table
        .seats
        .iter()
        .filter(|(seat_position, seat)| **seat_position != position && has_open_meld(&seat.melds))
        .count();
    if open_opponents == 0 {
        return 1.0;
    }
    let own_open_melds = melds.iter().filter(|meld| is_open_meld(meld)).count();
    if own_open_melds >= 2 { 0.2 } else { 0.45 }
}
