use super::*;

fn minimum_draws_to_complete_seven_pairs(
    hand: &[i32],
    table: &AiPublicTable,
    position: usize,
) -> Option<usize> {
    let missing_pairs = 7usize.saturating_sub(pair_count(hand));
    if missing_pairs == 0 {
        return Some(0);
    }

    let mut pair_draw_costs = Vec::new();
    for tile in SHENYANG_MAHJONG_TILE_KINDS {
        let own_count = hand.iter().filter(|item| **item == tile).count();
        let remaining = remaining_tile_count(hand, table, position, tile) as usize;
        let mut pairs = own_count / 2;
        let mut previous_pair_draw = 0;
        for draws in 1..=remaining {
            let next_pairs = (own_count + draws) / 2;
            if next_pairs > pairs {
                pair_draw_costs.push(draws - previous_pair_draw);
                previous_pair_draw = draws;
                pairs = next_pairs;
            }
        }
    }
    if pair_draw_costs.len() < missing_pairs {
        return None;
    }
    pair_draw_costs.sort_unstable();
    Some(pair_draw_costs.into_iter().take(missing_pairs).sum())
}

fn seven_pairs_plan_is_reachable(hand: &[i32], table: &AiPublicTable, position: usize) -> bool {
    minimum_draws_to_complete_seven_pairs(hand, table, position)
        .is_some_and(|draws| draws <= table.wall_count)
}

pub(in crate::ai::decision) fn should_chase_basic_missing_suit_four_pairs(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_rule: i32,
) -> bool {
    win_rule == WIN_RULE_SHENYANG_BASIC
        && pair_count(hand) == 4
        && valid_meld_count(melds) == 0
        && !missing_suits(hand, melds).is_empty()
}

pub(in crate::ai::decision) fn should_chase_basic_missing_suit_pairs(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_rule: i32,
    pairs: usize,
) -> bool {
    win_rule == WIN_RULE_SHENYANG_BASIC
        && pairs >= 4
        && valid_meld_count(melds) == 0
        && !missing_suits(hand, melds).is_empty()
}

pub(in crate::ai::decision) fn should_lock_seven_pairs_plan(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> bool {
    if valid_meld_count(melds) > 0 || !(hand.len() == 13 || hand.len() == 14) {
        return false;
    }
    if !seven_pairs_plan_is_reachable(hand, table, position) {
        return false;
    }
    if is_seven_pairs_wait_shape(hand) {
        return true;
    }
    let pairs = pair_count(hand);
    if pairs >= 6 {
        return true;
    }
    if should_chase_basic_missing_suit_pairs(hand, melds, win_rule, pairs) {
        return true;
    }
    if pairs < 5 {
        return false;
    }
    if capped_basic_route_foundation_visible_fan_exceeds_half_cap(hand, melds, table, win_rule) {
        return false;
    }
    if capped_basic_route_foundation_visible_fan_reaches_cap(hand, melds, table, win_rule) {
        return false;
    }
    if (table.dealer_position == position
        || dealer_opponent_has_major_threat(table, position, win_rule))
        && has_basic_normal_route_foundation(hand, melds, win_rule)
    {
        return false;
    }
    true
}

pub(in crate::ai::decision) fn seven_pairs_plan_score(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> f64 {
    if valid_meld_count(melds) > 0 || !(hand.len() == 13 || hand.len() == 14) {
        return 0.0;
    }
    if !seven_pairs_plan_is_reachable(hand, table, position) {
        return 0.0;
    }
    let pairs = pair_count(hand);
    if table.dealer_position == position
        && pairs < 6
        && !should_chase_basic_missing_suit_four_pairs(hand, melds, win_rule)
    {
        return 0.0;
    }
    if pairs < 6
        && capped_basic_route_foundation_visible_fan_exceeds_half_cap(hand, melds, table, win_rule)
    {
        return 0.0;
    }
    if pairs < 6
        && capped_basic_route_foundation_visible_fan_reaches_cap(hand, melds, table, win_rule)
    {
        return 0.0;
    }
    if win_rule == WIN_RULE_SHENYANG_BASIC && pairs < 6 && missing_suits(hand, melds).is_empty() {
        return 0.0;
    }
    match pairs {
        6.. => 42.0,
        5 => 24.0,
        4 => 10.0,
        _ => 0.0,
    }
}

pub(in crate::ai::decision) fn should_preserve_seven_pairs_plan_for_context(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> bool {
    hand.len() == 13 && should_lock_seven_pairs_plan(hand, melds, table, position, win_rule)
}
