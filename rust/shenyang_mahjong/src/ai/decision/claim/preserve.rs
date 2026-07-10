use super::*;

pub(in crate::ai::decision) fn should_preserve_piao_plan_for_chi(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
) -> bool {
    if melds.iter().any(is_sequence_meld) {
        return false;
    }
    let score = piao_plan_score_for_context(hand, melds, table, position);
    let early_piao_candidate = is_closed_early_piao_candidate(hand, melds, table, position);
    if !early_piao_candidate && score <= 0.0 {
        return false;
    }
    has_piao_route_basics(hand, melds) && (score >= 20.0 || early_piao_candidate)
}

pub(in crate::ai::decision) fn should_preserve_pinghu_sequence_over_peng(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    tile: i32,
) -> bool {
    if !is_suited(tile)
        || is_dragon(tile)
        || table.dealer_position == position
        || piao_plan_score_for_context(hand, melds, table, position) >= 22.0
        || !has_triplet_or_dragon_pair(hand, melds)
        || !tile_is_middle_of_sequence(hand, tile)
    {
        return false;
    }
    if win_rule == WIN_RULE_SHENYANG_BASIC && !has_door_opening_meld(melds, table) {
        return false;
    }
    true
}

pub(in crate::ai::decision) fn should_claim_capped_dragon_peng_over_five_pairs(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    tile: i32,
    from_position: usize,
) -> bool {
    if win_rule != WIN_RULE_SHENYANG_BASIC
        || !is_dragon(tile)
        || pair_count(hand) != 5
        || !can_peng(hand, tile)
    {
        return false;
    }
    let preserves_pair_route =
        should_lock_seven_pairs_plan(hand, current_melds, table, position, win_rule)
            || capped_basic_route_foundation_visible_fan_exceeds_half_cap(
                hand,
                current_melds,
                table,
                win_rule,
            );
    if !preserves_pair_route {
        return false;
    }
    let next_hand = remove_n_tiles(hand, tile, 2);
    if next_hand.len() + 2 != hand.len() {
        return false;
    }
    let mut next_melds = current_melds.to_vec();
    next_melds.push(claim_peng_meld(tile, from_position));
    capped_open_basic_route_visible_fan_reaches_cap(&next_hand, &next_melds, table)
}
