use super::*;

pub(in crate::ai::decision) fn estimated_four_gui_yi_fan(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
) -> i32 {
    let mut counts = HashMap::<i32, i32>::new();
    for tile in hand.iter().copied().filter(|tile| is_valid_tile(*tile)) {
        *counts.entry(tile).or_default() += 1;
    }
    for meld in melds.iter().filter(|meld| is_four_gui_yi_meld(meld)) {
        for tile in meld.tiles.iter().copied() {
            *counts.entry(tile).or_default() += 1;
        }
    }
    counts.into_values().filter(|count| *count == 4).count() as i32
}

pub(in crate::ai::decision) fn is_four_gui_yi_meld(meld: &WsShenyangMahjongMeld) -> bool {
    match meld.kind {
        ShenyangMahjongMeldKind::PENG => is_triplet_like_meld(meld),
        ShenyangMahjongMeldKind::CHI => is_sequence_meld(meld),
        ShenyangMahjongMeldKind::GANG => false,
    }
}

pub(in crate::ai::decision) fn estimated_concealed_dragon_triplet_fan(hand: &[i32]) -> i32 {
    [35, 36, 37]
        .into_iter()
        .filter(|dragon| hand.iter().filter(|tile| **tile == *dragon).count() >= 3)
        .count() as i32
}

pub(in crate::ai::decision) fn estimated_meld_fan(melds: &[WsShenyangMahjongMeld]) -> i32 {
    melds
        .iter()
        .map(|meld| match meld.kind {
            ShenyangMahjongMeldKind::PENG if meld_primary_tile(meld).is_some_and(is_dragon) => 1,
            ShenyangMahjongMeldKind::GANG => {
                let concealed = meld.from_position.is_none();
                match meld_primary_tile(meld) {
                    Some(tile) if is_dragon(tile) && concealed => 4,
                    Some(tile) if is_dragon(tile) => 2,
                    Some(_) if concealed => 2,
                    Some(_) => 1,
                    None => 0,
                }
            }
            _ => 0,
        })
        .sum()
}

pub(in crate::ai::decision) fn estimated_visible_bonus_fan(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
) -> i32 {
    estimated_meld_fan(melds)
        + estimated_concealed_dragon_triplet_fan(hand)
        + estimated_four_gui_yi_fan(hand, melds)
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
    let next = remove_n_tiles(hand, tile, 1);
    if next.len() + 1 != hand.len() {
        return 0.0;
    }
    let after_four_gui_yi = estimated_four_gui_yi_fan(&next, melds);
    if after_four_gui_yi >= current_four_gui_yi {
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

pub(in crate::ai::decision) fn estimated_visible_fan_without_wait(
    win_hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_rule: i32,
) -> i32 {
    if !is_complete_win_with_melds(win_hand, melds, win_rule) {
        return 0;
    }
    let is_piao = is_piao_hu_win(win_hand, melds);
    let base = if is_piao {
        3
    } else if is_seven_pairs_win(win_hand) || is_pure_one_suit_win(win_hand, melds) {
        4
    } else {
        1
    };
    base + estimated_visible_bonus_fan(win_hand, melds)
}

pub(in crate::ai::decision) fn estimated_fan_with_wait(
    win_hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_tile: i32,
    win_rule: i32,
) -> i32 {
    let is_single_wait = is_single_wait_shape_with_rule(win_hand, melds, win_tile, win_rule);
    let wait_fan = if is_single_wait {
        single_wait_fan(win_tile)
    } else {
        0
    };
    let shou_ba_yi_fan = if is_single_wait
        && is_piao_hu_win(win_hand, melds)
        && melds.len() == 4
        && win_hand.len() == 2
    {
        1
    } else {
        0
    };
    estimated_visible_fan_without_wait(win_hand, melds, win_rule) + wait_fan + shou_ba_yi_fan
}

pub(in crate::ai::decision) fn single_wait_fan(win_tile: i32) -> i32 {
    1 + if tile_is_terminal(win_tile) || is_honor(win_tile) {
        1
    } else {
        0
    }
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

pub(in crate::ai::decision) fn fan_wait_bias(
    win_hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    win_tile: i32,
    remaining: i32,
) -> f64 {
    if table.dealer_position == position
        || is_late_defense_round(table)
        || !is_single_wait_shape_with_rule(win_hand, melds, win_tile, win_rule)
    {
        return 0.0;
    }
    if remaining <= 1 {
        return 0.0;
    }
    if let Some(max_fan) = table.max_fan {
        let visible_fan = estimated_visible_fan_without_wait(win_hand, melds, win_rule);
        if visible_fan >= max_fan {
            return 0.0;
        }
        let total_fan = estimated_fan_with_wait(win_hand, melds, win_tile, win_rule);
        if total_fan >= max_fan {
            let fan_gap = max_fan - visible_fan;
            return if fan_gap == 1 && remaining >= 3 {
                14.0
            } else {
                0.0
            };
        }
    }

    let terminal_or_honor_bonus = if tile_is_terminal(win_tile) || is_honor(win_tile) {
        14.0
    } else {
        0.0
    };
    let live_wait_scale = if remaining == 2 { 0.45 } else { 1.0 };
    (62.0 + terminal_or_honor_bonus)
        * live_wait_scale
        * pressured_open_wait_scale(table, position, melds)
}
