use super::*;

pub(in crate::ai::decision) fn seven_pairs_pair_liveness_discard_bias(
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
        0 => 5.0,
        1 => 0.0,
        _ => -2.0,
    }
}

pub(in crate::ai::decision) fn seven_pairs_plan_discard_bias(
    hand: &[i32],
    tile: i32,
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> f64 {
    if !should_keep_pairs_for_seven_pairs_discard(hand, melds, table, position, win_rule) {
        return 0.0;
    }
    let count = hand.iter().filter(|item| **item == tile).count();
    if count >= 2 {
        let base = if is_honor(tile) {
            -18.0
        } else if tile_is_terminal(tile) {
            -15.0
        } else {
            -12.0
        };
        return base + seven_pairs_pair_liveness_discard_bias(hand, table, position, tile, count);
    }
    if is_dragon(tile) {
        18.0
    } else if is_honor(tile) {
        5.0
    } else if tile_is_terminal(tile) {
        0.0
    } else {
        3.0
    }
}

pub(in crate::ai::decision) fn should_keep_pairs_for_seven_pairs_discard(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> bool {
    if valid_meld_count(melds) > 0 || hand.len() != 14 {
        return false;
    }
    should_lock_seven_pairs_plan(hand, melds, table, position, win_rule)
}
