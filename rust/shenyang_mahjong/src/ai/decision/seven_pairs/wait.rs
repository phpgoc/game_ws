use super::*;

pub(in crate::ai::decision) fn choose_seven_pairs_wait_discard(
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
                seven_pairs_wait_tile_score_after_discard(
                    wait_tile, &next, table, position, win_rule, tile,
                ) + wait_setting_discard_safety_adjustment(table, position, tile, own_tile_count),
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

pub(in crate::ai::decision) fn seven_pairs_regular_wait_reaches_cap(table: &AiPublicTable) -> bool {
    const SEVEN_PAIRS_VISIBLE_FAN: i32 = 4;
    const REGULAR_SINGLE_WAIT_FAN: i32 = 1;
    table.max_fan.is_some_and(|max_fan| {
        max_fan > 0 && SEVEN_PAIRS_VISIBLE_FAN + REGULAR_SINGLE_WAIT_FAN >= max_fan
    })
}

pub(in crate::ai::decision) fn seven_pairs_wait_discard_bias(
    hand: &[i32],
    tile: i32,
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> f64 {
    if valid_meld_count(melds) > 0 || hand.len() != 14 || pair_count(hand) != 6 {
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
    18.0 + seven_pairs_wait_tile_score(wait_tile, &next, table, position, win_rule)
        + wait_setting_discard_safety_adjustment(table, position, tile, own_tile_count)
}

pub(in crate::ai::decision) fn seven_pairs_wait_shape_tiebreaker(wait_tile: i32) -> f64 {
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

pub(in crate::ai::decision) fn seven_pairs_wait_tile_score(
    wait_tile: i32,
    hand_after_discard: &[i32],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> f64 {
    seven_pairs_wait_tile_score_with_simulated_discards(
        wait_tile,
        hand_after_discard,
        table,
        position,
        win_rule,
        &[],
    )
}

pub(in crate::ai::decision) fn seven_pairs_wait_tile_score_after_discard(
    wait_tile: i32,
    hand_after_discard: &[i32],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    discarded_tile: i32,
) -> f64 {
    seven_pairs_wait_tile_score_with_simulated_discards(
        wait_tile,
        hand_after_discard,
        table,
        position,
        win_rule,
        &[discarded_tile],
    )
}

fn seven_pairs_wait_tile_score_with_simulated_discards(
    wait_tile: i32,
    hand_after_discard: &[i32],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
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
    let speed_first = table.dealer_position == position
        || dealer_opponent_has_major_threat(table, position, win_rule)
        || is_late_defense_round(table);
    if speed_first || seven_pairs_regular_wait_reaches_cap(table) {
        let remaining_weight = if speed_first { 14.0 } else { 6.0 };
        return remaining * remaining_weight + seven_pairs_wait_shape_tiebreaker(wait_tile)
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
