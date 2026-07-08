use super::*;

pub(in crate::ai::decision) fn should_claim_peng_for_basic_heng_and_opening(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    tile: i32,
    from_position: usize,
) -> bool {
    if win_rule != WIN_RULE_SHENYANG_BASIC
        || has_open_meld(current_melds)
        || has_triplet_or_dragon_pair(hand, current_melds)
        || !can_peng(hand, tile)
        || !missing_suits(hand, current_melds).is_empty()
        || !has_terminal_or_honor_with_extra(hand, current_melds, Some(tile))
        || should_preserve_seven_pairs_plan_for_context(
            hand,
            current_melds,
            table,
            position,
            win_rule,
        )
        || pure_one_suit_plan_score_for_context(hand, current_melds, table, position) > 0.0
    {
        return false;
    }

    let mut next = remove_n_tiles(hand, tile, 2);
    if next.len() + 2 != hand.len() {
        return false;
    }
    sort_tiles(&mut next);
    let mut melds = current_melds.to_vec();
    melds.push(claim_peng_meld(tile, from_position));

    unique_tiles(&next).into_iter().any(|discard| {
        let after_discard = remove_n_tiles(&next, discard, 1);
        missing_suits(&after_discard, &melds).is_empty()
            && has_terminal_or_honor_with_extra(&after_discard, &melds, None)
            && has_triplet_or_dragon_pair(&after_discard, &melds)
    })
}

pub(in crate::ai::decision) fn should_claim_peng_to_open_mid_basic_hand(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    tile: i32,
    from_position: usize,
) -> bool {
    if win_rule != WIN_RULE_SHENYANG_BASIC
        || has_open_meld(current_melds)
        || !is_mid_opening_round(table)
        || !can_peng(hand, tile)
        || !missing_suits(hand, current_melds).is_empty()
        || !has_terminal_or_honor_with_extra(hand, current_melds, Some(tile))
        || !has_triplet_or_dragon_pair(hand, current_melds)
        || should_preserve_seven_pairs_plan_for_context(
            hand,
            current_melds,
            table,
            position,
            win_rule,
        )
        || pure_one_suit_plan_score_for_context(hand, current_melds, table, position) > 0.0
    {
        return false;
    }

    let mut next = remove_n_tiles(hand, tile, 2);
    if next.len() + 2 != hand.len() {
        return false;
    }
    sort_tiles(&mut next);
    let mut melds = current_melds.to_vec();
    melds.push(claim_peng_meld(tile, from_position));

    unique_tiles(&next).into_iter().any(|discard| {
        let after_discard = remove_n_tiles(&next, discard, 1);
        missing_suits(&after_discard, &melds).is_empty()
            && has_terminal_or_honor_with_extra(&after_discard, &melds, None)
            && has_triplet_or_dragon_pair(&after_discard, &melds)
    })
}

pub(in crate::ai::decision) fn should_pass_closed_basic_peng_to_preserve_sequence(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    tile: i32,
) -> bool {
    win_rule == WIN_RULE_SHENYANG_BASIC
        && !has_open_meld(current_melds)
        && table.dealer_position != position
        && !table.max_fan.is_some_and(|max_fan| max_fan <= 1)
        && !is_late_round(table)
        && can_peng(hand, tile)
        && is_suited(tile)
        && has_triplet_like_group(hand, current_melds)
        && tile_is_middle_of_sequence(hand, tile)
        && piao_plan_score_for_context(hand, current_melds, table, position) < 22.0
        && pure_one_suit_plan_score_for_context(hand, current_melds, table, position) <= 0.0
        && !should_open_broken_closed_hand_for_defense(
            hand,
            current_melds,
            table,
            position,
            win_rule,
        )
}

pub(in crate::ai::decision) fn should_claim_peng_for_closed_early_piao_candidate(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    from_position: usize,
) -> bool {
    let pairs = pair_count(hand);
    if !current_melds.is_empty()
        || table.dealer_position == position
        || piao_plan_is_capped(table)
        || !(3..=4).contains(&pairs)
        || !can_peng(hand, tile)
        || !missing_suits(hand, current_melds).is_empty()
        || !has_terminal_or_honor_with_extra(hand, current_melds, None)
    {
        return false;
    }

    let mut next = remove_n_tiles(hand, tile, 2);
    if next.len() + 2 != hand.len() {
        return false;
    }
    sort_tiles(&mut next);
    let mut melds = current_melds.to_vec();
    melds.push(claim_peng_meld(tile, from_position));

    unique_tiles(&next).into_iter().any(|discard| {
        let after_discard = remove_n_tiles(&next, discard, 1);
        has_piao_route_basics(&after_discard, &melds)
            && piao_plan_score(&after_discard, &melds) > 0.0
    })
}

pub(in crate::ai::decision) fn should_peng_to_preserve_four_gui_yi_from_discard(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    tile: i32,
    from_position: usize,
) -> bool {
    if !should_preserve_four_gui_yi(tile)
        || !can_gang(hand, tile)
        || pure_one_suit_plan_score_for_context(hand, current_melds, table, position) > 0.0
    {
        return false;
    }

    let mut gang_hand = remove_n_tiles(hand, tile, 3);
    if gang_hand.len() + 3 != hand.len() {
        return false;
    }
    sort_tiles(&mut gang_hand);
    let mut gang_melds = current_melds.to_vec();
    gang_melds.push(claim_gang_meld(tile, from_position));
    let gang_ready_score = ready_tile_score(&gang_hand, &gang_melds, table, position, win_rule);
    if gang_ready_score <= 0.0 {
        return false;
    }
    let gang_visible_fan = estimated_visible_bonus_fan(&gang_hand, &gang_melds);
    let gang_four_gui_yi = estimated_four_gui_yi_fan(&gang_hand, &gang_melds);

    let mut peng_hand = remove_n_tiles(hand, tile, 2);
    if peng_hand.len() + 2 != hand.len() {
        return false;
    }
    sort_tiles(&mut peng_hand);
    let mut peng_melds = current_melds.to_vec();
    peng_melds.push(claim_peng_meld(tile, from_position));

    unique_tiles(&peng_hand).into_iter().any(|discard| {
        if discard == tile {
            return false;
        }
        let after_discard = remove_n_tiles(&peng_hand, discard, 1);
        estimated_four_gui_yi_fan(&after_discard, &peng_melds) > gang_four_gui_yi
            && estimated_visible_bonus_fan(&after_discard, &peng_melds) >= gang_visible_fan
            && ready_tile_score(&after_discard, &peng_melds, table, position, win_rule)
                >= gang_ready_score
    })
}

pub(in crate::ai::decision) fn claim_gang_from_discard_reaches_ready(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    tile: i32,
    from_position: usize,
) -> bool {
    let mut next = remove_n_tiles(hand, tile, 3);
    if next.len() + 3 != hand.len() {
        return false;
    }
    sort_tiles(&mut next);
    let mut melds = current_melds.to_vec();
    melds.push(claim_gang_meld(tile, from_position));
    ready_tile_score(&next, &melds, table, position, win_rule) > 0.0
}

pub(in crate::ai::decision) fn should_claim_ready_pure_one_suit_gang_from_discard(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    tile: i32,
    from_position: usize,
) -> bool {
    if !can_gang(hand, tile)
        || !is_main_pure_suit_tile(hand, current_melds, tile)
        || ready_visible_fan_reaches_cap(hand, current_melds, table, position, win_rule)
    {
        return false;
    }

    let mut next = remove_n_tiles(hand, tile, 3);
    if next.len() + 3 != hand.len() {
        return false;
    }
    sort_tiles(&mut next);
    let mut melds = current_melds.to_vec();
    melds.push(claim_gang_meld(tile, from_position));
    ready_tile_score(&next, &melds, table, position, win_rule) > 0.0
}

pub(in crate::ai::decision) fn should_claim_ready_open_pure_one_suit_peng_from_discard(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    tile: i32,
    from_position: usize,
    current_ready_score: f64,
) -> bool {
    if current_ready_score > 0.0
        || !has_open_meld(current_melds)
        || !can_peng(hand, tile)
        || !is_main_pure_suit_tile(hand, current_melds, tile)
        || should_preserve_seven_pairs_plan_for_context(
            hand,
            current_melds,
            table,
            position,
            win_rule,
        )
    {
        return false;
    }

    let mut next = remove_n_tiles(hand, tile, 2);
    if next.len() + 2 != hand.len() {
        return false;
    }
    sort_tiles(&mut next);
    let mut melds = current_melds.to_vec();
    melds.push(claim_peng_meld(tile, from_position));

    unique_tiles(&next).into_iter().any(|discard| {
        let mut after_discard = remove_n_tiles(&next, discard, 1);
        sort_tiles(&mut after_discard);
        ready_has_pure_one_suit_win(&after_discard, &melds, table, position, win_rule)
    })
}

pub(in crate::ai::decision) fn should_claim_ready_dragon_peng_from_discard(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    tile: i32,
    from_position: usize,
    current_ready_score: f64,
) -> bool {
    if !is_dragon(tile)
        || !can_peng(hand, tile)
        || should_preserve_seven_pairs_plan_for_context(
            hand,
            current_melds,
            table,
            position,
            win_rule,
        )
        || pure_one_suit_plan_score_for_context(hand, current_melds, table, position) > 0.0
        || ready_visible_fan_reaches_cap(hand, current_melds, table, position, win_rule)
    {
        return false;
    }

    let mut next = remove_n_tiles(hand, tile, 2);
    if next.len() + 2 != hand.len() {
        return false;
    }
    sort_tiles(&mut next);
    let mut melds = current_melds.to_vec();
    melds.push(claim_peng_meld(tile, from_position));

    let after_ready_score =
        best_ready_score_after_discard(&next, &melds, table, position, win_rule);
    if after_ready_score <= 0.0 {
        return false;
    }
    let keep_ratio = if table.dealer_position == position || is_late_round(table) {
        0.75
    } else {
        0.45
    };
    after_ready_score >= current_ready_score * keep_ratio
}

pub(in crate::ai::decision) fn should_claim_ready_piao_peng_for_shou_ba_yi(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    tile: i32,
    from_position: usize,
    current_ready_score: f64,
) -> bool {
    if current_ready_score <= 0.0
        || table.dealer_position == position
        || is_late_defense_round(table)
        || piao_plan_is_capped(table)
        || !can_peng(hand, tile)
        || piao_threat_level(current_melds) != 3
        || !has_piao_route_basics(hand, current_melds)
        || should_preserve_seven_pairs_plan_for_context(
            hand,
            current_melds,
            table,
            position,
            win_rule,
        )
        || pure_one_suit_plan_score_for_context(hand, current_melds, table, position) > 0.0
    {
        return false;
    }

    let mut next = remove_n_tiles(hand, tile, 2);
    if next.len() + 2 != hand.len() {
        return false;
    }
    sort_tiles(&mut next);
    let mut melds = current_melds.to_vec();
    melds.push(claim_peng_meld(tile, from_position));
    if piao_threat_level(&melds) != 4 {
        return false;
    }

    unique_tiles(&next).into_iter().any(|discard| {
        let after_discard = remove_n_tiles(&next, discard, 1);
        after_discard.len() == 1
            && ready_tile_score(&after_discard, &melds, table, position, win_rule) > 0.0
    })
}

pub(in crate::ai::decision) fn should_pass_peng_for_relaxed_pure_defense(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    tile: i32,
) -> bool {
    if win_rule == WIN_RULE_SHENYANG_BASIC
        || is_dragon(tile)
        || has_open_meld(melds)
        || table.dealer_position == position
        || !is_late_round(table)
        || should_preserve_seven_pairs_plan_for_context(hand, melds, table, position, win_rule)
        || pure_one_suit_plan_score_for_context(hand, melds, table, position) > 0.0
        || piao_plan_score_for_context(hand, melds, table, position) >= 22.0
        || should_open_broken_closed_hand_for_defense(hand, melds, table, position, win_rule)
    {
        return false;
    }
    ready_tile_score(hand, melds, table, position, win_rule) <= 0.0
        && one_step_wait_potential(hand, melds, table, position, win_rule) <= 0.0
        && hand_power(hand) < 18.0
}
