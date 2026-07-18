use super::*;

pub(in crate::ai::decision) fn best_one_step_wait_potential_after_discard(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
) -> f64 {
    if hand.is_empty() {
        return one_step_wait_potential(hand, melds, table, position);
    }
    unique_tiles(hand)
        .into_iter()
        .map(|tile| {
            let mut next = hand.to_vec();
            if let Some(index) = next.iter().position(|item| *item == tile) {
                next.remove(index);
            }
            one_step_wait_potential_after_discard(&next, melds, table, position, tile)
        })
        .fold(0.0, f64::max)
}

pub(in crate::ai::decision) fn best_score_after_forced_discard(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
) -> f64 {
    if hand.is_empty() {
        return hand_progress_score(hand, melds, table, position);
    }
    let mut best = f64::NEG_INFINITY;
    for tile in unique_tiles(hand) {
        let mut next = hand.to_vec();
        if let Some(index) = next.iter().position(|item| *item == tile) {
            next.remove(index);
        }
        best = best.max(hand_progress_score_after_discard(
            &next, melds, table, position, tile,
        ));
    }
    best
}

pub(in crate::ai::decision) fn hand_progress_score(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
) -> f64 {
    hand_power(hand)
        + valid_meld_count(melds) as f64 * 10.0
        + ready_tile_score(hand, melds, table, position)
        + one_step_wait_potential(hand, melds, table, position)
        + seven_pairs_plan_score(hand, melds, table, position)
        + piao_plan_score_for_context(hand, melds, table, position)
        + shenyang_rule_progress_score(hand, melds, table, position)
}

pub(in crate::ai::decision) fn hand_progress_score_after_discard(
    hand_after_discard: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    discarded_tile: i32,
) -> f64 {
    hand_power(hand_after_discard)
        + valid_meld_count(melds) as f64 * 10.0
        + ready_tile_score_after_discard(hand_after_discard, melds, table, position, discarded_tile)
        + one_step_wait_potential_after_discard(
            hand_after_discard,
            melds,
            table,
            position,
            discarded_tile,
        )
        + seven_pairs_plan_score(hand_after_discard, melds, table, position)
        + piao_plan_score_for_context(hand_after_discard, melds, table, position)
        + shenyang_rule_progress_score(hand_after_discard, melds, table, position)
}

pub(in crate::ai::decision) fn one_step_wait_potential(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
) -> f64 {
    one_step_wait_potential_with_simulated_discards(hand, melds, table, position, &[])
}

pub(in crate::ai::decision) fn one_step_wait_potential_after_discard(
    hand_after_discard: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    discarded_tile: i32,
) -> f64 {
    one_step_wait_potential_with_simulated_discards(
        hand_after_discard,
        melds,
        table,
        position,
        &[discarded_tile],
    )
}

fn one_step_wait_potential_with_simulated_discards(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    simulated_discards: &[i32],
) -> f64 {
    if hand.len() % 3 != 1
        || ready_tile_score_with_simulated_discards(
            hand,
            melds,
            table,
            position,
            simulated_discards,
        ) > 0.0
    {
        return 0.0;
    }
    let open_normal_route_foundation = has_door_opening_meld(melds, table)
        && missing_suits(hand, melds).is_empty()
        && has_terminal_or_honor_with_extra(hand, melds, None)
        && has_triplet_or_dragon_pair(hand, melds);
    let closed_sequence_route_foundation = !table.allow_first_chi
        && melds.iter().all(is_xi_gang_meld)
        && missing_suits(hand, melds).is_empty()
        && has_terminal_or_honor_with_extra(hand, melds, None)
        && has_dragon_pair(hand);
    if hand_power(hand) < 50.0
        && pair_count(hand) < 4
        && !open_normal_route_foundation
        && !closed_sequence_route_foundation
    {
        return 0.0;
    }

    let mut score = 0.0;
    for draw_tile in SHENYANG_MAHJONG_TILE_KINDS {
        let remaining = remaining_tile_count_with_melds_after_discards(
            hand,
            melds,
            table,
            position,
            draw_tile,
            simulated_discards,
        );
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
            let mut projected_discards = simulated_discards.to_vec();
            projected_discards.push(discard_tile);
            let ready = ready_tile_score_with_simulated_discards(
                &next,
                melds,
                table,
                position,
                &projected_discards,
            );
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
