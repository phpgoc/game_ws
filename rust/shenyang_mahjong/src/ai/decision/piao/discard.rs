use super::*;

pub(in crate::ai::decision) fn early_piao_candidate_discard_bias(
    hand: &[i32],
    tile: i32,
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
) -> f64 {
    if !is_closed_early_piao_candidate(hand, melds, table, position) {
        return 0.0;
    }
    let count = hand.iter().filter(|item| **item == tile).count();
    let only_terminal_or_honor = (is_honor(tile) || tile_is_terminal(tile))
        && count == 1
        && terminal_or_honor_count(hand, melds) == 1;
    let only_suit_tile =
        is_suited(tile) && suited_tile_count_for_suit(hand, melds, tile_suit(tile)) == 1;
    if count >= 3 {
        -10.0
    } else if count == 2 {
        -6.5
    } else if only_terminal_or_honor || only_suit_tile {
        -40.0
    } else {
        0.0
    }
}

pub(in crate::ai::decision) fn has_early_piao_singleton_discard(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
) -> bool {
    is_closed_early_piao_candidate(hand, melds, table, position)
        && unique_tiles(hand).into_iter().any(|tile| {
            hand.iter().filter(|item| **item == tile).count() == 1 && {
                let next = remove_n_tiles(hand, tile, 1);
                next.len() + 1 == hand.len() && has_piao_route_basics(&next, melds)
            }
        })
}

pub(in crate::ai::decision) fn piao_discard_bias(
    hand: &[i32],
    tile: i32,
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> f64 {
    if piao_plan_score_for_context(hand, melds, table, position) < 20.0 {
        return 0.0;
    }
    let count = hand.iter().filter(|item| **item == tile).count();
    let pure_one_suit_score = pure_one_suit_plan_score_for_context(hand, melds, table, position);
    let committed_groups = piao_committed_group_count(hand, melds);
    let only_terminal_or_honor = (is_honor(tile) || tile_is_terminal(tile))
        && count == 1
        && terminal_or_honor_count(hand, melds) == 1
        && pure_one_suit_score <= 0.0;
    let only_suit_tile = is_suited(tile)
        && suited_tile_count_for_suit(hand, melds, tile_suit(tile)) == 1
        && pure_one_suit_score <= 0.0;
    if count >= 3 {
        if committed_groups >= 2 { -28.0 } else { -20.0 }
    } else if count == 2 {
        let base = if committed_groups >= 2 { -24.0 } else { -16.0 };
        base + piao_dragon_pair_discard_bias(hand, table, position, tile, count)
            + piao_pair_liveness_discard_bias(hand, table, position, tile, count)
    } else if only_terminal_or_honor || only_suit_tile {
        -40.0
    } else if win_rule == WIN_RULE_SHENYANG_BASIC && is_dragon(tile) && pair_count(hand) >= 4 {
        16.0
    } else if is_honor(tile) || tile_is_terminal(tile) {
        1.0
    } else if neighbor_count(hand, tile) >= 2 {
        3.0
    } else {
        0.0
    }
}

pub(in crate::ai::decision) fn piao_pair_liveness_discard_bias(
    hand: &[i32],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    count: usize,
) -> f64 {
    if count != 2 {
        return 0.0;
    }
    match remaining_tile_count(hand, table, position, tile) {
        0 => 3.0,
        _ => 0.0,
    }
}

pub(in crate::ai::decision) fn piao_dragon_pair_discard_bias(
    hand: &[i32],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    count: usize,
) -> f64 {
    if count != 2 || !is_dragon(tile) {
        return 0.0;
    }
    match remaining_tile_count(hand, table, position, tile) {
        0 => -1.5,
        1 => -3.0,
        _ => -5.0,
    }
}
