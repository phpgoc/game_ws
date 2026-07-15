use super::*;

pub(in crate::ai::decision) fn should_claim_gang_from_discard(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    tile: i32,
    from_position: usize,
) -> bool {
    let current_ready_score = ready_tile_score(hand, current_melds, table, position, win_rule);
    let speed_first_unready = current_ready_score <= 0.0
        && (table.dealer_position == position || table.max_fan.is_some_and(|max_fan| max_fan <= 1));
    if ready_visible_fan_reaches_cap(hand, current_melds, table, position, win_rule) {
        return false;
    }
    if !speed_first_unready
        && capped_open_basic_route_visible_fan_reaches_cap(hand, current_melds, table)
    {
        return false;
    }
    let reaches_ready = claim_gang_from_discard_reaches_ready(
        hand,
        current_melds,
        table,
        position,
        win_rule,
        tile,
        from_position,
    );
    let committed_piao_plan =
        piao_plan_score_for_context(hand, current_melds, table, position, win_rule) >= 22.0
            && piao_threat_level(current_melds) > 0
            && piao_committed_group_count(hand, current_melds) >= 3;
    if committed_piao_plan {
        return speed_first_unready
            || claim_gang_from_discard_reaches_piao_ready(
                hand,
                current_melds,
                table,
                position,
                win_rule,
                tile,
                from_position,
            );
    }
    if current_ready_score > 0.0 {
        return reaches_ready;
    }
    if speed_first_unready {
        return true;
    }
    if is_dragon(tile) {
        return true;
    }
    if should_claim_opening_gang_for_basic_hand(
        hand,
        current_melds,
        table,
        position,
        win_rule,
        tile,
    ) {
        return true;
    }
    if should_open_broken_closed_hand_for_defense(hand, current_melds, table, position, win_rule) {
        return true;
    }

    if piao_plan_score_for_context(hand, current_melds, table, position, win_rule) >= 22.0 {
        return reaches_ready;
    }
    reaches_ready
}

pub(in crate::ai::decision) fn should_claim_opening_gang_for_basic_hand(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    tile: i32,
) -> bool {
    win_rule == WIN_RULE_SHENYANG_BASIC
        && !has_door_opening_meld(current_melds, table)
        && can_gang(hand, tile)
        && !table.max_fan.is_some_and(|max_fan| max_fan <= 1)
        && !is_closed_early_piao_candidate(hand, current_melds, table, position, win_rule)
        && !should_preserve_seven_pairs_plan_for_context(
            hand,
            current_melds,
            table,
            position,
            win_rule,
        )
        && pure_one_suit_plan_score_for_context(hand, current_melds, table, position, win_rule)
            <= 0.0
        && piao_plan_score_for_context(hand, current_melds, table, position, win_rule) < 22.0
}
