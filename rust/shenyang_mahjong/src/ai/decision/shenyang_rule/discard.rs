use super::super::*;
use super::heng::loses_basic_heng_recovery_after_discard;

fn capped_normal_route_before_discard_reaches_cap(
    hand_after_discard: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    discarded_tile: i32,
) -> bool {
    let mut hand_before_discard = hand_after_discard.to_vec();
    hand_before_discard.push(discarded_tile);
    sort_tiles(&mut hand_before_discard);
    capped_normal_route_visible_fan_reaches_cap(&hand_before_discard, melds, table)
}

fn loses_first_chi_disabled_closed_dragon_pair_after_xi_gang(
    hand_after_discard: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    discarded_tile: i32,
) -> bool {
    if table.allow_first_chi
        || !is_dragon(discarded_tile)
        || melds.is_empty()
        || !melds.iter().all(is_xi_gang_meld)
    {
        return false;
    }

    let mut hand_before_discard = hand_after_discard.to_vec();
    hand_before_discard.push(discarded_tile);
    has_dragon_pair(&hand_before_discard) && !has_dragon_pair(hand_after_discard)
}

pub(in crate::ai::decision) fn terminal_or_honor_discard_bias(
    hand_after_discard: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
) -> f64 {
    if violates_basic_terminal_or_honor_discard(hand_after_discard, melds, table, position, tile) {
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
) -> f64 {
    if !is_suited(tile) {
        return 0.0;
    }
    let suit = tile_suit(tile);
    let before = suit_presence_with_extra(hand_after_discard, melds, Some(tile));
    let after = suit_presence(hand_after_discard, melds);
    let was_missing_suit = before.iter().any(|present| !*present);
    if valid_meld_count(melds) == 0 && pair_count(hand_after_discard) >= 4 && was_missing_suit {
        return 0.0;
    }
    let capped_three_suit_hand = one_fan_reaches_score_cap(table) && !was_missing_suit;
    if !capped_three_suit_hand
        && !capped_normal_route_before_discard_reaches_cap(hand_after_discard, melds, table, tile)
        && pure_one_suit_plan_score_for_context(hand_after_discard, melds, table, position) > 0.0
    {
        return 0.0;
    }
    let mut bias = 0.0;
    if before[suit as usize] && !after[suit as usize] {
        bias -= 80.0;
    }
    if after.into_iter().filter(|present| *present).count() < 3 {
        bias -= 2.5;
    }
    bias
}

pub(in crate::ai::decision) fn violates_basic_heng_discard(
    hand_after_discard: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
) -> bool {
    let had_heng = has_triplet_or_dragon_pair_with_extra(hand_after_discard, melds, Some(tile));
    let has_heng_after = has_triplet_or_dragon_pair(hand_after_discard, melds);
    // A Xi Gang can supply Heng after opening, but cannot replace the dragon pair required here.
    let lost_closed_dragon_pair = loses_first_chi_disabled_closed_dragon_pair_after_xi_gang(
        hand_after_discard,
        melds,
        table,
        tile,
    );
    if has_heng_after && !lost_closed_dragon_pair {
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
        );
    if !had_heng && !lost_recoverable_heng && !lost_closed_dragon_pair {
        return false;
    }
    if should_preserve_seven_pairs_plan_for_context(hand_after_discard, melds, table, position) {
        return false;
    }
    pure_one_suit_plan_score_for_context(hand_after_discard, melds, table, position) <= 0.0
}

pub(in crate::ai::decision) fn violates_basic_terminal_or_honor_discard(
    hand_after_discard: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
) -> bool {
    if !(is_honor(tile) || tile_is_terminal(tile)) {
        return false;
    }
    let before = has_terminal_or_honor_with_extra(hand_after_discard, melds, Some(tile));
    let after = has_terminal_or_honor_with_extra(hand_after_discard, melds, None);
    if !before || after {
        return false;
    }
    if should_preserve_seven_pairs_plan_for_context(hand_after_discard, melds, table, position) {
        return false;
    }
    if pure_one_suit_plan_score_for_context(hand_after_discard, melds, table, position) > 0.0 {
        return false;
    }
    if one_fan_reaches_score_cap(table) {
        return true;
    }
    true
}

pub(in crate::ai::decision) fn violates_basic_three_suits_discard(
    hand_after_discard: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
) -> bool {
    if !is_suited(tile) {
        return false;
    }
    let before = suit_presence_with_extra(hand_after_discard, melds, Some(tile));
    let after = suit_presence(hand_after_discard, melds);
    if before.into_iter().filter(|present| *present).count() < 3
        || after.into_iter().filter(|present| *present).count() >= 3
    {
        return false;
    }
    if one_fan_reaches_score_cap(table)
        && !is_seven_pairs_wait_shape(hand_after_discard)
        && pair_count(hand_after_discard) < 6
    {
        return true;
    }
    if should_preserve_seven_pairs_plan_for_context(hand_after_discard, melds, table, position) {
        return false;
    }
    if capped_normal_route_before_discard_reaches_cap(hand_after_discard, melds, table, tile) {
        return true;
    }
    if one_fan_reaches_score_cap(table) {
        return true;
    }
    pure_one_suit_plan_score_for_context(hand_after_discard, melds, table, position) <= 0.0
}
