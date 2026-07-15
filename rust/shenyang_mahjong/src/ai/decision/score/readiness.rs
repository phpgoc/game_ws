use super::*;

pub(in crate::ai::decision) fn best_ready_score_after_discard(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> f64 {
    if hand.len() % 3 != 2 {
        return ready_tile_score(hand, melds, table, position, win_rule);
    }
    unique_tiles(hand)
        .into_iter()
        .map(|tile| {
            let mut next = hand.to_vec();
            if let Some(index) = next.iter().position(|item| *item == tile) {
                next.remove(index);
            }
            ready_tile_score_after_discard(&next, melds, table, position, win_rule, tile)
        })
        .fold(0.0, f64::max)
}

pub(in crate::ai::decision) fn ready_hand_visible_fan_reaches_cap(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    max_fan: i32,
) -> bool {
    ready_hand_visible_fan_reaches_cap_with_simulated_discards(
        hand,
        melds,
        table,
        position,
        win_rule,
        max_fan,
        &[],
    )
}

fn ready_hand_visible_fan_reaches_cap_with_simulated_discards(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    max_fan: i32,
    simulated_discards: &[i32],
) -> bool {
    if hand.len() % 3 != 1 {
        return false;
    }
    let known_unavailable_tiles =
        known_unavailable_tiles_with_simulated_discards(table, position, melds, simulated_discards);
    SHENYANG_MAHJONG_TILE_KINDS.into_iter().any(|tile| {
        remaining_tile_count_with_melds_after_discards(
            hand,
            melds,
            table,
            position,
            tile,
            simulated_discards,
        ) > 0
            && {
                let mut next = hand.to_vec();
                next.push(tile);
                next.sort_unstable();
                is_complete_win_for_table(&next, melds, table, win_rule)
                    && estimated_fan_with_known_unavailable_wait_for_table(
                        &next,
                        melds,
                        tile,
                        table,
                        win_rule,
                        &known_unavailable_tiles,
                    ) >= max_fan
            }
    })
}

pub(in crate::ai::decision) fn ready_has_pure_one_suit_win(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> bool {
    ready_has_pure_one_suit_win_with_simulated_discards(hand, melds, table, position, win_rule, &[])
}

pub(in crate::ai::decision) fn ready_has_piao_win(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> bool {
    ready_has_piao_win_with_simulated_discards(hand, melds, table, position, win_rule, &[])
}

#[cfg(test)]
pub(in crate::ai::decision) fn ready_has_pure_one_suit_win_after_discard(
    hand_after_discard: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    discarded_tile: i32,
) -> bool {
    ready_has_pure_one_suit_win_with_simulated_discards(
        hand_after_discard,
        melds,
        table,
        position,
        win_rule,
        &[discarded_tile],
    )
}

fn ready_has_pure_one_suit_win_with_simulated_discards(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    simulated_discards: &[i32],
) -> bool {
    if hand.len() % 3 != 1 {
        return false;
    }

    SHENYANG_MAHJONG_TILE_KINDS.into_iter().any(|tile| {
        remaining_tile_count_with_melds_after_discards(
            hand,
            melds,
            table,
            position,
            tile,
            simulated_discards,
        ) > 0
            && {
                let mut next = hand.to_vec();
                next.push(tile);
                next.sort_unstable();
                is_complete_win_for_table(&next, melds, table, win_rule)
                    && is_pure_one_suit_win(&next, melds)
            }
    })
}

fn ready_has_piao_win_with_simulated_discards(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    simulated_discards: &[i32],
) -> bool {
    if hand.len() % 3 != 1 {
        return false;
    }

    SHENYANG_MAHJONG_TILE_KINDS.into_iter().any(|tile| {
        remaining_tile_count_with_melds_after_discards(
            hand,
            melds,
            table,
            position,
            tile,
            simulated_discards,
        ) > 0
            && {
                let mut next = hand.to_vec();
                next.push(tile);
                next.sort_unstable();
                is_complete_win_for_table(&next, melds, table, win_rule)
                    && is_piao_hu_win(&next, melds)
            }
    })
}

pub(in crate::ai::decision) fn ready_tile_score(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> f64 {
    ready_tile_score_with_simulated_discards(hand, melds, table, position, win_rule, &[])
}

pub(in crate::ai::decision) fn ready_tile_score_after_discard(
    hand_after_discard: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    discarded_tile: i32,
) -> f64 {
    ready_tile_score_with_simulated_discards(
        hand_after_discard,
        melds,
        table,
        position,
        win_rule,
        &[discarded_tile],
    )
}

pub(in crate::ai::decision) fn ready_live_tile_count_after_discard(
    hand_after_discard: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    discarded_tile: i32,
) -> i32 {
    if hand_after_discard.len() % 3 != 1 {
        return 0;
    }
    SHENYANG_MAHJONG_TILE_KINDS
        .into_iter()
        .map(|tile| {
            let remaining = remaining_tile_count_with_melds_after_discards(
                hand_after_discard,
                melds,
                table,
                position,
                tile,
                &[discarded_tile],
            );
            if remaining <= 0 {
                return 0;
            }
            let mut next = hand_after_discard.to_vec();
            next.push(tile);
            next.sort_unstable();
            if is_complete_win_for_table(&next, melds, table, win_rule) {
                remaining
            } else {
                0
            }
        })
        .sum()
}

pub(in crate::ai::decision) fn ready_tile_score_with_simulated_discards(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    simulated_discards: &[i32],
) -> f64 {
    if hand.len() % 3 != 1 {
        return 0.0;
    }

    let known_unavailable_tiles =
        known_unavailable_tiles_with_simulated_discards(table, position, melds, simulated_discards);
    let mut score = 0.0;
    let mut wait_kinds = 0;
    for tile in SHENYANG_MAHJONG_TILE_KINDS {
        let remaining = remaining_tile_count_with_melds_after_discards(
            hand,
            melds,
            table,
            position,
            tile,
            simulated_discards,
        );
        if remaining <= 0 {
            continue;
        }
        let mut next = hand.to_vec();
        next.push(tile);
        next.sort_unstable();
        if is_complete_win_for_table(&next, melds, table, win_rule) {
            wait_kinds += 1;
            score += 28.0 + remaining as f64 * 5.0;
            score += fan_wait_bias(
                &next,
                melds,
                table,
                position,
                win_rule,
                tile,
                remaining,
                &known_unavailable_tiles,
            );
            if melds.is_empty() && is_seven_pairs_wait_shape(hand) && is_seven_pairs_win(&next) {
                score += seven_pairs_wait_tile_score(tile, hand, table, position, win_rule);
            }
        }
    }
    if wait_kinds >= 2 {
        score += wait_kinds as f64 * 3.0;
    }
    score
}

pub(in crate::ai::decision) fn ready_visible_fan_reaches_cap(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> bool {
    let Some(max_fan) = table.max_fan.filter(|max_fan| *max_fan > 0) else {
        return false;
    };
    if hand.len() % 3 == 2 {
        return unique_tiles(hand).into_iter().any(|discard| {
            let next = remove_n_tiles(hand, discard, 1);
            ready_hand_visible_fan_reaches_cap_with_simulated_discards(
                &next,
                melds,
                table,
                position,
                win_rule,
                max_fan,
                &[discard],
            )
        });
    }
    ready_hand_visible_fan_reaches_cap(hand, melds, table, position, win_rule, max_fan)
}

pub(in crate::ai::decision) fn ready_visible_fan_exceeds_half_cap(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> bool {
    let Some(max_fan) = table.max_fan.filter(|max_fan| *max_fan > 0) else {
        return false;
    };
    if hand.len() % 3 == 2 {
        return unique_tiles(hand).into_iter().any(|discard| {
            let next = remove_n_tiles(hand, discard, 1);
            ready_hand_visible_fan_exceeds_half_cap_with_simulated_discards(
                &next,
                melds,
                table,
                position,
                win_rule,
                max_fan,
                &[discard],
            )
        });
    }
    ready_hand_visible_fan_exceeds_half_cap_with_simulated_discards(
        hand,
        melds,
        table,
        position,
        win_rule,
        max_fan,
        &[],
    )
}

fn ready_hand_visible_fan_exceeds_half_cap_with_simulated_discards(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    max_fan: i32,
    simulated_discards: &[i32],
) -> bool {
    if hand.len() % 3 != 1 {
        return false;
    }
    let known_unavailable_tiles =
        known_unavailable_tiles_with_simulated_discards(table, position, melds, simulated_discards);
    SHENYANG_MAHJONG_TILE_KINDS.into_iter().any(|tile| {
        remaining_tile_count_with_melds_after_discards(
            hand,
            melds,
            table,
            position,
            tile,
            simulated_discards,
        ) > 0
            && {
                let mut next = hand.to_vec();
                next.push(tile);
                next.sort_unstable();
                is_complete_win_for_table(&next, melds, table, win_rule)
                    && estimated_fan_with_known_unavailable_wait_for_table(
                        &next,
                        melds,
                        tile,
                        table,
                        win_rule,
                        &known_unavailable_tiles,
                    ) * 2
                        > max_fan
            }
    })
}
