use super::*;

fn claim_gang_projects_capped_visible_fan(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    tile: i32,
    from_position: usize,
    committed_piao_plan: bool,
) -> bool {
    let next_hand = remove_n_tiles(hand, tile, 3);
    if next_hand.len() + 3 != hand.len() {
        return false;
    }
    let mut next_melds = current_melds.to_vec();
    next_melds.push(claim_gang_meld(tile, from_position));
    let normal_route_projects_cap =
        capped_normal_route_visible_fan_exceeds_half_cap(hand, current_melds, table)
            && !capped_normal_route_visible_fan_reaches_cap(hand, current_melds, table)
            && capped_normal_route_visible_fan_reaches_cap(&next_hand, &next_melds, table);
    let piao_route_projects_cap = committed_piao_plan
        && has_piao_route_basics(&next_hand, &next_melds)
        && capped_piao_route_visible_fan_projects_cap(
            hand,
            current_melds,
            &next_hand,
            &next_melds,
            table,
        );
    let pure_one_suit_route_projects_cap = has_established_pure_one_suit_route(hand, current_melds)
        && has_established_pure_one_suit_route(&next_hand, &next_melds)
        && capped_pure_one_suit_route_visible_fan_projects_cap(
            hand,
            current_melds,
            &next_hand,
            &next_melds,
            table,
        );
    normal_route_projects_cap || piao_route_projects_cap || pure_one_suit_route_projects_cap
}

pub(in crate::ai::decision) fn claim_gang_projects_capped_pure_one_suit_fan(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    tile: i32,
    from_position: usize,
) -> bool {
    let next_hand = remove_n_tiles(hand, tile, 3);
    if next_hand.len() + 3 != hand.len() {
        return false;
    }
    let mut next_melds = current_melds.to_vec();
    next_melds.push(claim_gang_meld(tile, from_position));
    has_established_pure_one_suit_route(hand, current_melds)
        && has_established_pure_one_suit_route(&next_hand, &next_melds)
        && capped_pure_one_suit_route_visible_fan_projects_cap(
            hand,
            current_melds,
            &next_hand,
            &next_melds,
            table,
        )
}

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
    let piao_score = piao_plan_score_for_context(hand, current_melds, table, position);
    let committed_piao_plan = piao_score >= 22.0
        && piao_threat_level(current_melds) > 0
        && piao_committed_group_count(hand, current_melds) >= 3;
    let projected_capped_visible_fan = claim_gang_projects_capped_visible_fan(
        hand,
        current_melds,
        table,
        tile,
        from_position,
        committed_piao_plan,
    );
    let speed_first_unready = current_ready_score <= 0.0
        && (table.dealer_position == position
            || table.max_fan.is_some_and(|max_fan| max_fan <= 1)
            || dealer_opponent_has_major_threat(table, position)
            || projected_capped_visible_fan);
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
    if should_claim_opening_gang_for_basic_hand(hand, current_melds, table, position, tile) {
        return true;
    }
    if should_open_broken_closed_hand_for_defense(hand, current_melds, table, position, win_rule) {
        return true;
    }

    if piao_score >= 22.0 {
        return reaches_ready;
    }
    reaches_ready
}

pub(in crate::ai::decision) fn should_claim_opening_gang_for_basic_hand(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
) -> bool {
    !has_door_opening_meld(current_melds, table)
        && can_gang(hand, tile)
        && table.max_fan.is_none_or(|max_fan| max_fan > 1)
        && !is_closed_early_piao_candidate(hand, current_melds, table, position)
        && !should_preserve_seven_pairs_plan_for_context(hand, current_melds, table, position)
        && pure_one_suit_plan_score_for_context(hand, current_melds, table, position) <= 0.0
        && piao_plan_score_for_context(hand, current_melds, table, position) < 22.0
}
