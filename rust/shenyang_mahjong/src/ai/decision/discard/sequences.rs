use super::*;

pub(in crate::ai::decision) fn complete_sequence_discard_bias(
    hand: &[i32],
    tile: i32,
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> f64 {
    if hand.iter().filter(|item| **item == tile).count() != 1 {
        return 0.0;
    }
    if tile_is_middle_of_sequence(hand, tile) {
        -6.0
    } else if is_closed_early_piao_candidate(hand, melds, table, position, win_rule) {
        0.0
    } else if tile_is_part_of_complete_sequence(hand, tile) {
        -4.0
    } else {
        0.0
    }
}

pub(in crate::ai::decision) fn incomplete_sequence_discard_bias(
    hand: &[i32],
    tile: i32,
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> f64 {
    if hand.iter().filter(|item| **item == tile).count() != 1
        || !is_suited(tile)
        || tile_is_part_of_complete_sequence(hand, tile)
        || should_lock_seven_pairs_plan(hand, melds, table, position, win_rule)
        || is_closed_early_piao_candidate(hand, melds, table, position, win_rule)
        || piao_plan_score_for_context(hand, melds, table, position, win_rule) >= 20.0
        || pure_one_suit_plan_score_for_context(hand, melds, table, position) > 0.0
    {
        return 0.0;
    }
    if tile_is_weak_edge_wait_terminal(hand, tile) {
        3.2
    } else if tile_is_core_two_sided_wait_member(hand, tile)
        || tile_is_core_closed_middle_wait_member(hand, tile)
    {
        -3.0
    } else {
        0.0
    }
}

pub(in crate::ai::decision) fn pinghu_sequence_route_discard_bias(
    hand: &[i32],
    tile: i32,
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> f64 {
    if table.wall_count <= 55
        || hand.iter().filter(|item| **item == tile).count() != 1
        || pair_count(hand) > 3
        || should_lock_seven_pairs_plan(hand, melds, table, position, win_rule)
        || is_closed_early_piao_candidate(hand, melds, table, position, win_rule)
        || piao_plan_score_for_context(hand, melds, table, position, win_rule) >= 20.0
        || pure_one_suit_plan_score_for_context(hand, melds, table, position) > 0.0
        || best_ready_score_after_discard(hand, melds, table, position, win_rule) > 0.0
        || pinghu_sequence_route_tile_count(hand) < 5
    {
        return 0.0;
    }

    if tile_is_middle_of_sequence(hand, tile) {
        -24.0
    } else if tile_is_part_of_complete_sequence(hand, tile) {
        -20.0
    } else if tile_is_core_two_sided_wait_member(hand, tile)
        || tile_is_core_closed_middle_wait_member(hand, tile)
    {
        -24.0
    } else {
        0.0
    }
}

pub(in crate::ai::decision) fn pinghu_sequence_route_tile_count(hand: &[i32]) -> usize {
    unique_tiles(hand)
        .into_iter()
        .filter(|tile| {
            tile_is_part_of_complete_sequence(hand, *tile)
                || tile_is_core_two_sided_wait_member(hand, *tile)
                || tile_is_core_closed_middle_wait_member(hand, *tile)
        })
        .count()
}
