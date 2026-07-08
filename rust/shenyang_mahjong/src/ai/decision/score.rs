use super::*;

pub(super) fn best_ready_score_after_discard(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> f64 {
    if hand.len() % 3 != 2 {
        return ready_tile_score(hand, melds, table, position, win_rule);
    }
    unique_tiles(hand)
        .into_iter()
        .map(|tile| {
            let mut next = hand.to_vec();
            if let Some(index) = next.iter().position(|item| *item == tile) {
                next.remove(index);
            }
            ready_tile_score(&next, melds, table, position, win_rule)
        })
        .fold(0.0, f64::max)
}

pub(super) fn best_score_after_forced_discard(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> f64 {
    if hand.is_empty() {
        return hand_progress_score(hand, melds, table, position, win_rule);
    }
    let mut best = f64::NEG_INFINITY;
    for tile in unique_tiles(hand) {
        let mut next = hand.to_vec();
        if let Some(index) = next.iter().position(|item| *item == tile) {
            next.remove(index);
        }
        best = best.max(hand_progress_score(&next, melds, table, position, win_rule));
    }
    best
}

pub(super) fn best_one_step_wait_potential_after_discard(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> f64 {
    if hand.is_empty() {
        return one_step_wait_potential(hand, melds, table, position, win_rule);
    }
    unique_tiles(hand)
        .into_iter()
        .map(|tile| {
            let mut next = hand.to_vec();
            if let Some(index) = next.iter().position(|item| *item == tile) {
                next.remove(index);
            }
            one_step_wait_potential(&next, melds, table, position, win_rule)
        })
        .fold(0.0, f64::max)
}

pub(super) fn estimate_pressure_for_tile(table: &AiPublicTable, position: usize, tile: i32) -> f64 {
    let mut pressure = 0.0;
    for (seat_position, seat) in &table.seats {
        if *seat_position == position {
            continue;
        }
        let dist = seat.position.abs_diff(position);
        if seat.discards.contains(&tile) {
            pressure += 2.0;
        }
        if seat.melds.len() >= 2 {
            pressure -= 0.7;
        }
        if tile >= 31 && seat.hand_count >= 10 {
            pressure += 0.5 / (dist as f64 + 1.0);
        }
        if tile_is_terminal(tile) && seat.hand_count >= 8 {
            pressure += 0.8 / (dist as f64 + 1.0);
        }
    }
    if table.wall_count < 30 {
        pressure -= 0.3;
    }
    if table.current_position == position && table.dealer_position != position {
        pressure += 0.1;
    }
    pressure
}

pub(super) fn estimated_four_gui_yi_fan(hand: &[i32], melds: &[WsShenyangMahjongMeld]) -> i32 {
    let mut counts = HashMap::<i32, i32>::new();
    for tile in hand.iter().copied() {
        *counts.entry(tile).or_default() += 1;
    }
    for meld in melds.iter().filter(|meld| is_four_gui_yi_meld(meld)) {
        for tile in meld.tiles.iter().copied() {
            *counts.entry(tile).or_default() += 1;
        }
    }
    counts.into_values().filter(|count| *count == 4).count() as i32
}

pub(super) fn is_four_gui_yi_meld(meld: &WsShenyangMahjongMeld) -> bool {
    match meld.kind {
        ShenyangMahjongMeldKind::PENG => is_triplet_like_meld(meld),
        ShenyangMahjongMeldKind::CHI => is_sequence_meld(meld),
        ShenyangMahjongMeldKind::GANG => false,
    }
}

pub(super) fn estimated_concealed_dragon_triplet_fan(hand: &[i32]) -> i32 {
    [35, 36, 37]
        .into_iter()
        .filter(|dragon| hand.iter().filter(|tile| **tile == *dragon).count() >= 3)
        .count() as i32
}

pub(super) fn estimated_meld_fan(melds: &[WsShenyangMahjongMeld]) -> i32 {
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

pub(super) fn estimated_visible_bonus_fan(hand: &[i32], melds: &[WsShenyangMahjongMeld]) -> i32 {
    estimated_meld_fan(melds)
        + estimated_concealed_dragon_triplet_fan(hand)
        + estimated_four_gui_yi_fan(hand, melds)
}

pub(super) fn four_gui_yi_discard_bias(
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
    if ready_tile_score(&next, melds, table, position, win_rule) > 0.0 {
        return -28.0 * fan_loss;
    }
    if best_ready_score_after_discard(hand, melds, table, position, win_rule) > 0.0 {
        return -18.0 * fan_loss;
    }
    -6.0 * fan_loss
}

pub(super) fn estimated_visible_fan_without_wait(
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
    let shou_ba_yi_fan = if is_piao && melds.len() == 4 && win_hand.len() == 2 {
        1
    } else {
        0
    };
    base + estimated_visible_bonus_fan(win_hand, melds) + shou_ba_yi_fan
}

pub(super) fn estimated_fan_with_wait(
    win_hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_tile: i32,
    win_rule: i32,
) -> i32 {
    let wait_fan = if is_single_wait_shape_with_rule(win_hand, melds, win_tile, win_rule) {
        single_wait_fan(win_tile)
    } else {
        0
    };
    estimated_visible_fan_without_wait(win_hand, melds, win_rule) + wait_fan
}

pub(super) fn single_wait_fan(win_tile: i32) -> i32 {
    1 + if tile_is_terminal(win_tile) || is_honor(win_tile) {
        1
    } else {
        0
    }
}

pub(super) fn pressured_open_wait_scale(
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

pub(super) fn fan_wait_bias(
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
    let wait_fan = single_wait_fan(win_tile);
    if let Some(max_fan) = table.max_fan {
        let visible_fan = estimated_visible_fan_without_wait(win_hand, melds, win_rule);
        if visible_fan >= max_fan {
            return 0.0;
        }
        if visible_fan + wait_fan >= max_fan {
            return if remaining >= 3 { 14.0 } else { 0.0 };
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

pub(super) fn hand_progress_score(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> f64 {
    hand_power(hand)
        + melds.len() as f64 * 10.0
        + ready_tile_score(hand, melds, table, position, win_rule)
        + one_step_wait_potential(hand, melds, table, position, win_rule)
        + seven_pairs_plan_score(hand, melds, table, position, win_rule)
        + piao_plan_score_for_context(hand, melds, table, position)
        + shenyang_rule_progress_score(hand, melds, table, position, win_rule)
}

pub(super) fn one_step_wait_potential(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> f64 {
    if hand.len() % 3 != 1 || ready_tile_score(hand, melds, table, position, win_rule) > 0.0 {
        return 0.0;
    }
    let open_basic_route_foundation = win_rule == WIN_RULE_SHENYANG_BASIC
        && has_open_meld(melds)
        && missing_suits(hand, melds).is_empty()
        && has_terminal_or_honor_with_extra(hand, melds, None)
        && has_triplet_or_dragon_pair(hand, melds);
    if hand_power(hand) < 50.0 && pair_count(hand) < 4 && !open_basic_route_foundation {
        return 0.0;
    }

    let mut score = 0.0;
    for draw_tile in SHENYANG_MAHJONG_TILE_KINDS {
        let remaining = remaining_tile_count(hand, table, position, draw_tile);
        if remaining <= 0 {
            continue;
        }
        let mut after_draw = hand.to_vec();
        after_draw.push(draw_tile);
        after_draw.sort_unstable();
        let mut best_ready = 0.0;
        for discard_tile in unique_tiles(&after_draw) {
            let mut next = after_draw.clone();
            if let Some(index) = next.iter().position(|item| *item == discard_tile) {
                next.remove(index);
            }
            let ready = ready_tile_score(&next, melds, table, position, win_rule);
            if ready > best_ready {
                best_ready = ready;
            }
        }
        if best_ready > 0.0 {
            score += remaining as f64 * (1.2 + best_ready * 0.025);
        }
    }
    score
}

pub(super) fn ready_hand_visible_fan_reaches_cap(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    max_fan: i32,
) -> bool {
    if hand.len() % 3 != 1 {
        return false;
    }
    SHENYANG_MAHJONG_TILE_KINDS.into_iter().any(|tile| {
        remaining_tile_count(hand, table, position, tile) > 0 && {
            let mut next = hand.to_vec();
            next.push(tile);
            next.sort_unstable();
            is_complete_win_with_melds(&next, melds, win_rule)
                && estimated_fan_with_wait(&next, melds, tile, win_rule) >= max_fan
        }
    })
}

pub(super) fn ready_tile_score(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> f64 {
    if hand.len() % 3 != 1 {
        return 0.0;
    }

    let mut score = 0.0;
    let mut wait_kinds = 0;
    for tile in SHENYANG_MAHJONG_TILE_KINDS {
        let remaining = remaining_tile_count(hand, table, position, tile);
        if remaining <= 0 {
            continue;
        }
        let mut next = hand.to_vec();
        next.push(tile);
        next.sort_unstable();
        if is_complete_win_with_melds(&next, melds, win_rule) {
            wait_kinds += 1;
            score += 28.0 + remaining as f64 * 5.0;
            score += fan_wait_bias(&next, melds, table, position, win_rule, tile, remaining);
            if melds.is_empty() && is_seven_pairs_wait_shape(hand) && is_seven_pairs_win(&next) {
                score += seven_pairs_wait_tile_score(tile, hand, table, position);
            }
        }
    }
    if wait_kinds >= 2 {
        score += wait_kinds as f64 * 3.0;
    }
    score
}

pub(super) fn ready_has_pure_one_suit_win(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> bool {
    if hand.len() % 3 != 1 {
        return false;
    }

    SHENYANG_MAHJONG_TILE_KINDS.into_iter().any(|tile| {
        remaining_tile_count(hand, table, position, tile) > 0 && {
            let mut next = hand.to_vec();
            next.push(tile);
            next.sort_unstable();
            is_complete_win_with_melds(&next, melds, win_rule) && is_pure_one_suit_win(&next, melds)
        }
    })
}

pub(super) fn ready_visible_fan_reaches_cap(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> bool {
    let Some(max_fan) = table.max_fan.filter(|max_fan| *max_fan > 0) else {
        return false;
    };
    if hand.len() % 3 == 2 {
        return unique_tiles(hand).into_iter().any(|discard| {
            let next = remove_n_tiles(hand, discard, 1);
            ready_hand_visible_fan_reaches_cap(&next, melds, table, position, win_rule, max_fan)
        });
    }
    ready_hand_visible_fan_reaches_cap(hand, melds, table, position, win_rule, max_fan)
}
