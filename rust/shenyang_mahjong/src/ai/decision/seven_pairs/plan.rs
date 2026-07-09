use super::*;

pub(in crate::ai::decision) fn should_chase_basic_missing_suit_four_pairs(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_rule: i32,
) -> bool {
    win_rule == WIN_RULE_SHENYANG_BASIC
        && pair_count(hand) == 4
        && melds.is_empty()
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
        && melds.is_empty()
        && !missing_suits(hand, melds).is_empty()
}

pub(in crate::ai::decision) fn has_basic_normal_route_foundation(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_rule: i32,
) -> bool {
    win_rule == WIN_RULE_SHENYANG_BASIC
        && missing_suits(hand, melds).is_empty()
        && has_terminal_or_honor_with_extra(hand, melds, None)
        && has_triplet_or_dragon_pair(hand, melds)
}

pub(in crate::ai::decision) fn should_lock_seven_pairs_plan(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> bool {
    if !melds.is_empty() || !(hand.len() == 13 || hand.len() == 14) {
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
    if capped_basic_route_foundation_reaches_cap(hand, melds, table, win_rule) {
        return false;
    }
    if table.dealer_position == position && has_basic_normal_route_foundation(hand, melds, win_rule)
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
    if !melds.is_empty() || !(hand.len() == 13 || hand.len() == 14) {
        return 0.0;
    }
    let pairs = pair_count(hand);
    if table.dealer_position == position
        && pairs < 6
        && !should_chase_basic_missing_suit_four_pairs(hand, melds, win_rule)
    {
        return 0.0;
    }
    if pairs < 6 && capped_basic_route_foundation_reaches_cap(hand, melds, table, win_rule) {
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

fn capped_basic_route_foundation_reaches_cap(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    win_rule: i32,
) -> bool {
    let Some(max_fan) = table.max_fan.filter(|max_fan| *max_fan > 0) else {
        return false;
    };
    has_basic_normal_route_foundation(hand, melds, win_rule)
        && 1 + estimated_visible_bonus_fan(hand, melds) >= max_fan
}
