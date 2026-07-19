use super::*;

pub(in crate::ai::decision) fn choose_seven_pairs_wait_discard(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
) -> Option<i32> {
    if !should_keep_pairs_for_seven_pairs_discard(hand, melds, table, position)
        || pair_count(hand) != 6
    {
        return None;
    }
    if one_fan_reaches_score_cap(table) && terminal_or_honor_count(hand, melds) == 1 {
        return None;
    }

    let candidates = unique_tiles(hand)
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
            let remaining = remaining_tile_count_with_melds_after_discards(
                &next,
                &[],
                table,
                position,
                wait_tile,
                &[tile],
            );
            if remaining <= 0 {
                return None;
            }
            let payment_fan =
                seven_pairs_wait_payment_fan(wait_tile, &next, table, position, &[tile])?;
            let own_tile_count = hand.iter().filter(|item| **item == tile).count();
            Some((
                remaining,
                payment_fan,
                seven_pairs_wait_tile_score_after_discard(wait_tile, &next, table, position, tile)
                    + wait_setting_discard_safety_adjustment(table, position, tile, own_tile_count),
                tile,
            ))
        })
        .collect::<Vec<_>>();
    let speed_first = table.dealer_position == position
        || dealer_opponent_has_major_threat(table, position)
        || is_late_defense_round(table)
        || table.score_cap.is_some_and(|score_cap| {
            candidates.iter().any(|(_, payment_fan, _, _)| {
                shenyang_fan_score_exceeds_half_cap(*payment_fan, score_cap)
            })
        });

    candidates
        .into_iter()
        .max_by(|left, right| {
            let live_order = if speed_first {
                left.0.cmp(&right.0)
            } else {
                Ordering::Equal
            };
            live_order
                .then_with(|| left.2.partial_cmp(&right.2).unwrap_or(Ordering::Equal))
                .then_with(|| right.3.cmp(&left.3))
        })
        .map(|(_, _, _, tile)| tile)
}

fn seven_pairs_wait_payment_fan(
    wait_tile: i32,
    hand_after_discard: &[i32],
    table: &AiPublicTable,
    position: usize,
    simulated_discards: &[i32],
) -> Option<i32> {
    let mut win_hand = hand_after_discard.to_vec();
    win_hand.push(wait_tile);
    sort_tiles(&mut win_hand);
    if !is_seven_pairs_win(&win_hand) || !is_complete_win_for_table(&win_hand, &[], table) {
        return None;
    }
    let known_unavailable_tiles =
        known_unavailable_tiles_with_simulated_discards(table, position, &[], simulated_discards);
    Some(minimum_potential_payment_fan(
        estimated_fan_with_known_unavailable_wait_for_table(
            &win_hand,
            &[],
            wait_tile,
            table,
            &known_unavailable_tiles,
        ),
        table,
        position,
    ))
}

pub(in crate::ai::decision) fn seven_pairs_wait_reaches_cap(
    wait_tile: i32,
    hand_after_discard: &[i32],
    table: &AiPublicTable,
    position: usize,
    simulated_discards: &[i32],
) -> bool {
    table.score_cap.is_some_and(|score_cap| {
        seven_pairs_wait_payment_fan(
            wait_tile,
            hand_after_discard,
            table,
            position,
            simulated_discards,
        )
        .is_some_and(|fan| shenyang_fan_reaches_score_cap(fan, score_cap))
    })
}

pub(in crate::ai::decision) fn seven_pairs_wait_discard_bias(
    hand: &[i32],
    tile: i32,
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
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
    18.0 + seven_pairs_wait_tile_score(wait_tile, &next, table, position)
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
) -> f64 {
    seven_pairs_wait_tile_score_with_simulated_discards(
        wait_tile,
        hand_after_discard,
        table,
        position,
        &[],
    )
}

pub(in crate::ai::decision) fn seven_pairs_wait_tile_score_after_discard(
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
    let speed_first = table.dealer_position == position
        || dealer_opponent_has_major_threat(table, position)
        || is_late_defense_round(table);
    if speed_first
        || seven_pairs_wait_reaches_cap(
            wait_tile,
            hand_after_discard,
            table,
            position,
            simulated_discards,
        )
    {
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
