use super::*;

pub(super) fn basic_heng_seed_discard_bias(
    hand: &[i32],
    tile: i32,
    melds: &[WsShenyangMahjongMeld],
    win_rule: i32,
) -> f64 {
    if win_rule != WIN_RULE_SHENYANG_BASIC || has_triplet_or_dragon_pair(hand, melds) {
        return 0.0;
    }
    let count = hand.iter().filter(|item| **item == tile).count();
    if is_dragon(tile) && count == 1 {
        -7.0
    } else if count == 2 {
        -4.0
    } else {
        0.0
    }
}

pub(super) fn shenyang_rule_progress_score(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> f64 {
    if win_rule != WIN_RULE_SHENYANG_BASIC
        || should_lock_seven_pairs_plan(hand, melds, table, position, win_rule)
    {
        return 0.0;
    }
    let pure_score = pure_one_suit_plan_score_for_context(hand, melds, table, position);
    if pure_score > 0.0 {
        return pure_score;
    }
    let mut score = 0.0;
    let suits = suit_presence(hand, melds);
    let suit_count = suits.into_iter().filter(|present| *present).count();
    score += match suit_count {
        3 => 10.0,
        2 => -6.0,
        1 => -14.0,
        _ => -20.0,
    };
    if has_open_meld(melds) {
        score += 9.0;
    } else {
        score -= 8.0;
    }
    if has_terminal_or_honor_with_extra(hand, melds, None) {
        score += 7.0;
    } else {
        score -= 10.0;
    }
    if has_triplet_or_dragon_pair(hand, melds) {
        score += 8.0;
    } else {
        score -= 5.0;
    }
    score
}

pub(super) fn unrecoverable_basic_rule_requirement_count(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
) -> usize {
    let missing_suits = missing_suits(hand, melds)
        .into_iter()
        .filter(|suit| live_tile_count_for_suit(hand, table, *suit) <= 0)
        .count();
    let missing_terminal_or_honor = !has_terminal_or_honor_with_extra(hand, melds, None)
        && live_terminal_or_honor_count(hand, table) <= 0;
    let missing_heng = !can_recover_basic_heng(hand, melds, table);
    missing_suits + usize::from(missing_terminal_or_honor) + usize::from(missing_heng)
}

pub(super) fn can_recover_basic_heng(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
) -> bool {
    if has_triplet_or_dragon_pair(hand, melds) {
        return true;
    }

    let mut counts = HashMap::<i32, usize>::new();
    for tile in hand.iter().copied() {
        *counts.entry(tile).or_default() += 1;
    }

    SHENYANG_MAHJONG_TILE_KINDS.into_iter().any(|tile| {
        let count = counts.get(&tile).copied().unwrap_or(0);
        let remaining = remaining_tile_count(hand, table, 0, tile) as usize;
        let can_draw_triplet = count < 3 && remaining >= 3 - count;
        let can_draw_dragon_pair = is_dragon(tile) && count < 2 && remaining >= 2 - count;
        can_draw_triplet || can_draw_dragon_pair
    })
}

pub(super) fn can_recover_basic_heng_after_discard(
    hand_after_discard: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    discarded_tile: i32,
) -> bool {
    if has_triplet_or_dragon_pair(hand_after_discard, melds) {
        return true;
    }

    let mut counts = HashMap::<i32, usize>::new();
    for tile in hand_after_discard.iter().copied() {
        *counts.entry(tile).or_default() += 1;
    }

    SHENYANG_MAHJONG_TILE_KINDS.into_iter().any(|tile| {
        let count = counts.get(&tile).copied().unwrap_or(0);
        let remaining =
            remaining_tile_count_after_discard(hand_after_discard, table, discarded_tile, tile)
                as usize;
        let can_draw_triplet = count < 3 && remaining >= 3 - count;
        let can_draw_dragon_pair = is_dragon(tile) && count < 2 && remaining >= 2 - count;
        can_draw_triplet || can_draw_dragon_pair
    })
}

pub(super) fn loses_basic_heng_recovery_after_discard(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    tile: i32,
    win_rule: i32,
) -> bool {
    if win_rule != WIN_RULE_SHENYANG_BASIC
        || has_triplet_or_dragon_pair(hand, melds)
        || !can_recover_basic_heng(hand, melds, table)
    {
        return false;
    }

    let hand_after_discard = remove_n_tiles(hand, tile, 1);
    hand_after_discard.len() + 1 == hand.len()
        && !can_recover_basic_heng_after_discard(&hand_after_discard, melds, table, tile)
}

pub(super) fn terminal_or_honor_discard_bias(
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

pub(super) fn three_suits_discard_bias(
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

pub(super) fn violates_basic_terminal_or_honor_discard(
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
    if pure_one_suit_plan_score_for_context(hand_after_discard, melds, table, position) > 0.0
        && (is_honor(tile) || !is_main_pure_suit_tile(hand_after_discard, melds, tile))
    {
        return false;
    }
    if table.max_fan.is_some_and(|max_fan| max_fan <= 1) {
        return true;
    }
    true
}

pub(super) fn violates_basic_heng_discard(
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

pub(super) fn violates_basic_three_suits_discard(
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
    if table.max_fan.is_some_and(|max_fan| max_fan <= 1) {
        return true;
    }
    pure_one_suit_plan_score_for_context(hand_after_discard, melds, table, position) <= 0.0
}
