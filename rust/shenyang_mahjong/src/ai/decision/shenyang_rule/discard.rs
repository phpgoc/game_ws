use super::super::*;
use super::heng::loses_basic_heng_recovery_after_discard;

pub(in crate::ai::decision) fn terminal_or_honor_discard_bias(
    hand_after_discard: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    win_rule: i32,
) -> f64 {
    if violates_basic_terminal_or_honor_discard(
        hand_after_discard,
        melds,
        table,
        position,
        tile,
        win_rule,
    ) {
        -500.0
    } else {
        0.0
    }
}

pub(in crate::ai::decision) fn three_suits_discard_bias(
    hand_after_discard: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    win_rule: i32,
) -> f64 {
    if !is_suited(tile) {
        return 0.0;
    }
    let suit = tile_suit(tile);
    let before = suit_presence_with_extra(hand_after_discard, melds, Some(tile));
    let after = suit_presence(hand_after_discard, melds);
    let was_missing_suit = before.iter().any(|present| !*present);
    if melds.is_empty() && pair_count(hand_after_discard) >= 4 && was_missing_suit {
        return 0.0;
    }
    let capped_three_suit_hand =
        table.max_fan.is_some_and(|max_fan| max_fan <= 1) && !was_missing_suit;
    if !capped_three_suit_hand
        && !capped_basic_foundation_before_discard_reaches_cap(
            hand_after_discard,
            melds,
            table,
            tile,
            win_rule,
        )
        && pure_one_suit_plan_score_for_context(hand_after_discard, melds, table, position) > 0.0
    {
        return 0.0;
    }
    let mut bias = 0.0;
    if before[suit as usize] && !after[suit as usize] {
        bias -= if win_rule == WIN_RULE_SHENYANG_BASIC {
            80.0
        } else {
            14.0
        };
    }
    if after.into_iter().filter(|present| *present).count() < 3 {
        bias -= 2.5;
    }
    bias
}

pub(in crate::ai::decision) fn violates_basic_terminal_or_honor_discard(
    hand_after_discard: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    win_rule: i32,
) -> bool {
    if win_rule != WIN_RULE_SHENYANG_BASIC || !(is_honor(tile) || tile_is_terminal(tile)) {
        return false;
    }
    let before = has_terminal_or_honor_with_extra(hand_after_discard, melds, Some(tile));
    let after = has_terminal_or_honor_with_extra(hand_after_discard, melds, None);
    if !before || after {
        return false;
    }
    if should_preserve_seven_pairs_plan_for_context(
        hand_after_discard,
        melds,
        table,
        position,
        win_rule,
    ) {
        return false;
    }
    if pure_one_suit_plan_score_for_context(hand_after_discard, melds, table, position) > 0.0 {
        return false;
    }
    if table.max_fan.is_some_and(|max_fan| max_fan <= 1) {
        return true;
    }
    true
}

pub(in crate::ai::decision) fn violates_basic_heng_discard(
    hand_after_discard: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    win_rule: i32,
) -> bool {
    if win_rule != WIN_RULE_SHENYANG_BASIC {
        return false;
    }
    let had_heng = has_triplet_or_dragon_pair_with_extra(hand_after_discard, melds, Some(tile));
    let has_heng_after = has_triplet_or_dragon_pair(hand_after_discard, melds);
    if has_heng_after {
        return false;
    }
    let mut hand_before_discard = hand_after_discard.to_vec();
    hand_before_discard.push(tile);
    sort_tiles(&mut hand_before_discard);
    let lost_recoverable_heng = !had_heng
        && loses_basic_heng_recovery_after_discard(
            &hand_before_discard,
            melds,
            table,
            position,
            tile,
            win_rule,
        );
    if !had_heng && !lost_recoverable_heng {
        return false;
    }
    if should_preserve_seven_pairs_plan_for_context(
        hand_after_discard,
        melds,
        table,
        position,
        win_rule,
    ) {
        return false;
    }
    pure_one_suit_plan_score_for_context(hand_after_discard, melds, table, position) <= 0.0
}

pub(in crate::ai::decision) fn violates_basic_three_suits_discard(
    hand_after_discard: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    win_rule: i32,
) -> bool {
    if win_rule != WIN_RULE_SHENYANG_BASIC || !is_suited(tile) {
        return false;
    }
    let before = suit_presence_with_extra(hand_after_discard, melds, Some(tile));
    let after = suit_presence(hand_after_discard, melds);
    if before.into_iter().filter(|present| *present).count() < 3
        || after.into_iter().filter(|present| *present).count() >= 3
    {
        return false;
    }
    if table.max_fan.is_some_and(|max_fan| max_fan <= 1)
        && !is_seven_pairs_wait_shape(hand_after_discard)
        && pair_count(hand_after_discard) < 6
    {
        return true;
    }
    if should_preserve_seven_pairs_plan_for_context(
        hand_after_discard,
        melds,
        table,
        position,
        win_rule,
    ) {
        return false;
    }
    if capped_basic_foundation_before_discard_reaches_cap(
        hand_after_discard,
        melds,
        table,
        tile,
        win_rule,
    ) {
        return true;
    }
    if table.max_fan.is_some_and(|max_fan| max_fan <= 1) {
        return true;
    }
    pure_one_suit_plan_score_for_context(hand_after_discard, melds, table, position) <= 0.0
}

fn capped_basic_foundation_before_discard_reaches_cap(
    hand_after_discard: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    discarded_tile: i32,
    win_rule: i32,
) -> bool {
    let mut hand_before_discard = hand_after_discard.to_vec();
    hand_before_discard.push(discarded_tile);
    sort_tiles(&mut hand_before_discard);
    capped_basic_route_foundation_visible_fan_reaches_cap(
        &hand_before_discard,
        melds,
        table,
        win_rule,
    )
}
