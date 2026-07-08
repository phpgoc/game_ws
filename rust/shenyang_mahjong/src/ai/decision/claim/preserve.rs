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
    let early_piao_candidate = melds.is_empty()
        && pair_count(hand) >= 3
        && table.dealer_position != position
        && !piao_plan_is_capped(table);
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
    if win_rule == WIN_RULE_SHENYANG_BASIC && !has_open_meld(melds) {
        return false;
    }
    true
}
