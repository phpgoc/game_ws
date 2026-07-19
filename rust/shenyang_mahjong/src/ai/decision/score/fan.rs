use super::*;

const EDGE_WAIT_BONUS: f64 = 10.0;

pub(in crate::ai::decision) fn payment_fans_for_table(
    winner_fan: i32,
    table: &AiPublicTable,
    winner_position: usize,
    from_position: Option<usize>,
) -> Vec<i32> {
    let payer_positions = match from_position {
        Some(position) => vec![position],
        None => table
            .seats
            .keys()
            .copied()
            .filter(|position| *position != winner_position)
            .collect::<Vec<_>>(),
    };
    if payer_positions.is_empty() {
        return Vec::new();
    }

    let potential_loser_positions = table
        .seats
        .keys()
        .copied()
        .filter(|position| *position != winner_position)
        .collect::<Vec<_>>();
    let all_losers_closed = !table.claim_has_hu_response
        && potential_loser_positions.len() == 3
        && potential_loser_positions.iter().all(|position| {
            table
                .seats
                .get(position)
                .is_some_and(|seat| !has_open_meld(&seat.melds))
        });

    payer_positions
        .into_iter()
        .map(|payer_position| {
            let payer_is_closed = table
                .seats
                .get(&payer_position)
                .is_some_and(|seat| !has_open_meld(&seat.melds));
            shenyang_payment_fan(
                winner_fan,
                winner_position == table.dealer_position,
                payer_position == table.dealer_position,
                payer_is_closed,
                all_losers_closed,
            )
        })
        .collect()
}

pub(in crate::ai::decision) fn minimum_potential_payment_fan(
    winner_fan: i32,
    table: &AiPublicTable,
    winner_position: usize,
) -> i32 {
    let payment_fans = payment_fans_for_table(winner_fan, table, winner_position, None);
    if payment_fans.len() != 3 {
        return winner_fan;
    }
    payment_fans.into_iter().min().unwrap_or(winner_fan)
}

pub(in crate::ai::decision) fn capped_normal_route_visible_fan_exceeds_half_cap(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
) -> bool {
    let Some(score_cap) = table.score_cap.filter(|score_cap| *score_cap > 0) else {
        return false;
    };
    let visible_fan = minimum_potential_payment_fan(
        1 + estimated_visible_bonus_fan(hand, melds),
        table,
        position,
    );
    has_normal_route_foundation(hand, melds)
        && shenyang_fan_score_exceeds_half_cap(visible_fan, score_cap)
}

pub(in crate::ai::decision) fn capped_normal_route_visible_fan_reaches_cap(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
) -> bool {
    let Some(score_cap) = table.score_cap.filter(|score_cap| *score_cap > 0) else {
        return false;
    };
    has_normal_route_foundation(hand, melds)
        && shenyang_fan_reaches_score_cap(
            minimum_potential_payment_fan(
                1 + estimated_visible_bonus_fan(hand, melds),
                table,
                position,
            ),
            score_cap,
        )
}

pub(in crate::ai::decision) fn capped_open_normal_route_visible_fan_reaches_cap(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
) -> bool {
    let Some(score_cap) = table.score_cap.filter(|score_cap| *score_cap > 0) else {
        return false;
    };
    has_door_opening_meld(melds, table)
        && missing_suits(hand, melds).is_empty()
        && has_terminal_or_honor_with_extra(hand, melds, None)
        && has_triplet_or_dragon_pair(hand, melds)
        && shenyang_fan_reaches_score_cap(
            minimum_potential_payment_fan(
                1 + estimated_visible_bonus_fan(hand, melds),
                table,
                position,
            ),
            score_cap,
        )
}

pub(in crate::ai::decision) fn capped_piao_route_visible_fan_projects_cap(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    next_hand: &[i32],
    next_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
) -> bool {
    capped_pattern_route_visible_fan_projects_cap(
        ShenyangMahjongWinPattern::PiaoHu,
        hand,
        melds,
        next_hand,
        next_melds,
        table,
        position,
    )
}

fn capped_pattern_route_visible_fan_projects_cap(
    pattern: ShenyangMahjongWinPattern,
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    next_hand: &[i32],
    next_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
) -> bool {
    let Some(score_cap) = table.score_cap.filter(|score_cap| *score_cap > 0) else {
        return false;
    };
    let base_fan = shenyang_win_pattern_base_fan(pattern);
    let current_fan = minimum_potential_payment_fan(
        base_fan + estimated_visible_bonus_fan(hand, melds),
        table,
        position,
    );
    let projected_fan = minimum_potential_payment_fan(
        base_fan + estimated_visible_bonus_fan(next_hand, next_melds),
        table,
        position,
    );
    shenyang_fan_score_exceeds_half_cap(current_fan, score_cap)
        && !shenyang_fan_reaches_score_cap(current_fan, score_cap)
        && shenyang_fan_reaches_score_cap(projected_fan, score_cap)
}

pub(in crate::ai::decision) fn capped_pure_one_suit_route_visible_fan_projects_cap(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    next_hand: &[i32],
    next_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
) -> bool {
    capped_pattern_route_visible_fan_projects_cap(
        ShenyangMahjongWinPattern::PureOneSuit,
        hand,
        melds,
        next_hand,
        next_melds,
        table,
        position,
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
    known_unavailable_tiles: &[i32],
) -> i32 {
    estimated_fan_with_known_unavailable_wait_with_context(
        win_hand,
        melds,
        win_tile,
        ShenyangMahjongWinContext::new(),
        known_unavailable_tiles,
    )
}

fn estimated_fan_with_known_unavailable_wait_with_context(
    win_hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_tile: i32,
    context: ShenyangMahjongWinContext,
    known_unavailable_tiles: &[i32],
) -> i32 {
    shenyang_score_visible_win_fan(
        win_hand,
        melds,
        Some(win_tile),
        context,
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
    estimated_fan_with_known_unavailable_wait_with_context(
        win_hand,
        melds,
        win_tile,
        win_context_for_table(table),
        known_unavailable_tiles,
    )
}

#[cfg(test)]
pub(in crate::ai::decision) fn estimated_fan_with_wait(
    win_hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_tile: i32,
) -> i32 {
    estimated_fan_with_known_unavailable_wait(win_hand, melds, win_tile, &[])
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
) -> i32 {
    estimated_visible_fan_without_wait_with_context(
        win_hand,
        melds,
        ShenyangMahjongWinContext::new(),
    )
}

fn estimated_visible_fan_without_wait_with_context(
    win_hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    context: ShenyangMahjongWinContext,
) -> i32 {
    shenyang_score_visible_win_fan(win_hand, melds, None, context, &[])
}

pub(in crate::ai::decision) fn estimated_visible_fan_without_wait_for_table(
    win_hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
) -> i32 {
    estimated_visible_fan_without_wait_with_context(win_hand, melds, win_context_for_table(table))
}

pub(in crate::ai::decision) fn fan_wait_bias(
    win_hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_tile: i32,
    remaining: i32,
    known_unavailable_tiles: &[i32],
) -> f64 {
    if table.dealer_position == position
        || dealer_opponent_has_major_threat(table, position)
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
    if let Some(score_cap) = table.score_cap {
        let visible_fan = minimum_potential_payment_fan(
            estimated_visible_fan_without_wait_for_table(win_hand, melds, table),
            table,
            position,
        );
        if shenyang_fan_score_exceeds_half_cap(visible_fan, score_cap) {
            return 0.0;
        }
        if shenyang_fan_reaches_score_cap(visible_fan, score_cap) {
            return 0.0;
        }
        let total_fan = minimum_potential_payment_fan(
            estimated_fan_with_known_unavailable_wait_for_table(
                win_hand,
                melds,
                win_tile,
                table,
                known_unavailable_tiles,
            ),
            table,
            position,
        );
        if shenyang_fan_reaches_score_cap(total_fan, score_cap) {
            let fan_gap = shenyang_fan_needed_for_score_cap(score_cap) - visible_fan;
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
) -> f64 {
    let current_four_gui_yi = estimated_four_gui_yi_fan(hand, melds);
    if current_four_gui_yi <= 0 {
        return 0.0;
    }
    if capped_normal_route_visible_fan_exceeds_half_cap(hand, melds, table, position) {
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
    if let Some(score_cap) = table.score_cap.filter(|score_cap| *score_cap > 0)
        && ready_hand_visible_fan_reaches_cap(&next, melds, table, position, score_cap)
    {
        return 0.0;
    }
    if table.dealer_position == position || dealer_opponent_has_major_threat(table, position) {
        return 0.0;
    }

    let fan_loss = (current_four_gui_yi - after_four_gui_yi) as f64;
    if ready_tile_score_after_discard(&next, melds, table, position, tile) > 0.0 {
        return -28.0 * fan_loss;
    }
    if best_ready_score_after_discard(hand, melds, table, position) > 0.0 {
        return -18.0 * fan_loss;
    }
    -6.0 * fan_loss
}

pub(in crate::ai::decision) fn has_normal_route_foundation(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
) -> bool {
    missing_suits(hand, melds).is_empty()
        && has_terminal_or_honor_with_extra(hand, melds, None)
        && has_triplet_or_dragon_pair(hand, melds)
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
