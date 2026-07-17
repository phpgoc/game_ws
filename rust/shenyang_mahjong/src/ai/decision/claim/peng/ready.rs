use super::*;

pub(in crate::ai::decision) fn claim_gang_from_discard_reaches_piao_ready(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
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
    ready_has_piao_win(&next, &melds, table, position)
}

pub(in crate::ai::decision) fn claim_gang_from_discard_reaches_ready(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
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
    ready_tile_score(&next, &melds, table, position) > 0.0
}

pub(in crate::ai::decision) fn should_claim_ready_dragon_peng_from_discard(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    from_position: usize,
    current_ready_score: f64,
) -> bool {
    if !is_dragon(tile)
        || !can_peng(hand, tile)
        || should_preserve_seven_pairs_plan_for_context(hand, current_melds, table, position)
        || pure_one_suit_plan_score_for_context(hand, current_melds, table, position) > 0.0
        || ready_visible_fan_reaches_cap(hand, current_melds, table, position)
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

    let after_ready_score = best_ready_score_after_discard(&next, &melds, table, position);
    if after_ready_score <= 0.0 {
        return false;
    }
    let keep_ratio = if table.dealer_position == position
        || dealer_opponent_has_major_threat(table, position)
        || is_late_round(table)
    {
        0.75
    } else {
        0.45
    };
    after_ready_score >= current_ready_score * keep_ratio
}

pub(in crate::ai::decision) fn should_claim_ready_pure_one_suit_gang_from_discard(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    from_position: usize,
) -> bool {
    if !can_gang(hand, tile)
        || !is_main_pure_suit_tile(hand, current_melds, tile)
        || ready_visible_fan_reaches_cap(hand, current_melds, table, position)
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
    ready_has_pure_one_suit_win(&next, &melds, table, position)
}
