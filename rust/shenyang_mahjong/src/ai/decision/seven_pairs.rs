use super::*;

pub(super) fn seven_pairs_plan_discard_bias(
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

pub(super) fn seven_pairs_pair_liveness_discard_bias(
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
        0 => 4.0,
        1 => 0.0,
        _ => -2.0,
    }
}

pub(super) fn should_keep_pairs_for_seven_pairs_discard(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> bool {
    if !melds.is_empty() || hand.len() != 14 {
        return false;
    }
    should_lock_seven_pairs_plan(hand, melds, table, position, win_rule)
}

pub(super) fn should_chase_basic_missing_suit_four_pairs(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_rule: i32,
) -> bool {
    win_rule == WIN_RULE_SHENYANG_BASIC
        && pair_count(hand) == 4
        && melds.is_empty()
        && !missing_suits(hand, melds).is_empty()
}

pub(super) fn should_chase_basic_missing_suit_pairs(
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

pub(super) fn has_basic_normal_route_foundation(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_rule: i32,
) -> bool {
    win_rule == WIN_RULE_SHENYANG_BASIC
        && missing_suits(hand, melds).is_empty()
        && has_terminal_or_honor_with_extra(hand, melds, None)
        && has_triplet_or_dragon_pair(hand, melds)
}

pub(super) fn should_lock_seven_pairs_plan(
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
    if table.max_fan.is_some_and(|max_fan| max_fan <= 1)
        && has_basic_normal_route_foundation(hand, melds, win_rule)
    {
        return false;
    }
    if table.dealer_position == position && has_basic_normal_route_foundation(hand, melds, win_rule)
    {
        return false;
    }
    true
}

pub(super) fn seven_pairs_plan_score(
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

pub(super) fn seven_pairs_wait_discard_bias(
    hand: &[i32],
    tile: i32,
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
) -> f64 {
    if !melds.is_empty() || hand.len() != 14 || pair_count(hand) != 6 {
        return 0.0;
    }
    if hand.iter().filter(|item| **item == tile).count() % 2 != 1 {
        return 0.0;
    }
    let mut next = hand.to_vec();
    if let Some(index) = next.iter().position(|item| *item == tile) {
        next.remove(index);
    }
    if !is_seven_pairs_wait_shape(&next) {
        return 0.0;
    }
    let Some(wait_tile) = single_tile(&next) else {
        return 0.0;
    };
    let own_tile_count = hand.iter().filter(|item| **item == tile).count();
    18.0 + seven_pairs_wait_tile_score(wait_tile, &next, table, position)
        + wait_setting_discard_safety_adjustment(table, position, tile, own_tile_count)
}

pub(super) fn choose_seven_pairs_wait_discard(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> Option<i32> {
    if !should_keep_pairs_for_seven_pairs_discard(hand, melds, table, position, win_rule)
        || pair_count(hand) != 6
    {
        return None;
    }
    if table.max_fan.is_some_and(|max_fan| max_fan <= 1)
        && terminal_or_honor_count(hand, melds) == 1
    {
        return None;
    }

    unique_tiles(hand)
        .into_iter()
        .filter(|tile| hand.iter().filter(|item| **item == *tile).count() % 2 == 1)
        .filter_map(|tile| {
            let mut next = hand.to_vec();
            if let Some(index) = next.iter().position(|item| *item == tile) {
                next.remove(index);
            }
            if !is_seven_pairs_wait_shape(&next) {
                return None;
            }
            let wait_tile = single_tile(&next)?;
            let own_tile_count = hand.iter().filter(|item| **item == tile).count();
            Some((
                seven_pairs_wait_tile_score_after_discard(wait_tile, &next, table, position, tile)
                    + wait_setting_discard_safety_adjustment(table, position, tile, own_tile_count),
                tile,
            ))
        })
        .max_by(|(left_score, left_tile), (right_score, right_tile)| {
            left_score
                .partial_cmp(right_score)
                .unwrap_or(Ordering::Equal)
                .then_with(|| right_tile.cmp(left_tile))
        })
        .map(|(_, tile)| tile)
}

pub(super) fn seven_pairs_wait_tile_score(
    wait_tile: i32,
    hand_after_discard: &[i32],
    table: &AiPublicTable,
    position: usize,
) -> f64 {
    seven_pairs_wait_tile_score_with_simulated_discards(
        wait_tile,
        hand_after_discard,
        table,
        position,
        &[],
    )
}

pub(super) fn seven_pairs_wait_tile_score_after_discard(
    wait_tile: i32,
    hand_after_discard: &[i32],
    table: &AiPublicTable,
    position: usize,
    discarded_tile: i32,
) -> f64 {
    seven_pairs_wait_tile_score_with_simulated_discards(
        wait_tile,
        hand_after_discard,
        table,
        position,
        &[discarded_tile],
    )
}

fn seven_pairs_wait_tile_score_with_simulated_discards(
    wait_tile: i32,
    hand_after_discard: &[i32],
    table: &AiPublicTable,
    position: usize,
    simulated_discards: &[i32],
) -> f64 {
    let public_discards = public_discard_count(table, wait_tile) as f64;
    let remaining = remaining_tile_count_with_melds_after_discards(
        hand_after_discard,
        &[],
        table,
        position,
        wait_tile,
        simulated_discards,
    ) as f64;
    if remaining <= 0.0 {
        return -240.0 - public_discards * 12.0;
    }
    if seven_pairs_regular_wait_reaches_cap(table) {
        return remaining * 6.0 + seven_pairs_wait_shape_tiebreaker(wait_tile)
            - public_discards * 12.0;
    }
    let shape = if is_wind(wait_tile) {
        10.0
    } else if is_dragon(wait_tile) {
        7.0
    } else if tile_is_terminal(wait_tile) {
        8.0
    } else {
        -4.0
    };
    shape + remaining * 5.0 - public_discards * 12.0
}

pub(super) fn seven_pairs_wait_shape_tiebreaker(wait_tile: i32) -> f64 {
    if is_wind(wait_tile) {
        2.0
    } else if tile_is_terminal(wait_tile) {
        1.5
    } else if is_dragon(wait_tile) {
        1.0
    } else {
        0.0
    }
}

pub(super) fn seven_pairs_regular_wait_reaches_cap(table: &AiPublicTable) -> bool {
    const SEVEN_PAIRS_VISIBLE_FAN: i32 = 4;
    const REGULAR_SINGLE_WAIT_FAN: i32 = 1;
    table.max_fan.is_some_and(|max_fan| {
        max_fan > 0 && SEVEN_PAIRS_VISIBLE_FAN + REGULAR_SINGLE_WAIT_FAN >= max_fan
    })
}

pub(super) fn should_preserve_seven_pairs_plan_for_context(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> bool {
    hand.len() == 13 && should_lock_seven_pairs_plan(hand, melds, table, position, win_rule)
}
