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
            ready_tile_score(&next, melds, table, position, win_rule)
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
    if hand.len() % 3 != 1 {
        return false;
    }
    SHENYANG_MAHJONG_TILE_KINDS.into_iter().any(|tile| {
        remaining_tile_count(hand, table, position, tile) > 0 && {
            let mut next = hand.to_vec();
            next.push(tile);
            next.sort_unstable();
            is_complete_win_with_melds(&next, melds, win_rule)
                && estimated_fan_with_wait(&next, melds, tile, win_rule) >= max_fan
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
    if hand.len() % 3 != 1 {
        return 0.0;
    }

    let mut score = 0.0;
    let mut wait_kinds = 0;
    for tile in SHENYANG_MAHJONG_TILE_KINDS {
        let remaining = remaining_tile_count(hand, table, position, tile);
        if remaining <= 0 {
            continue;
        }
        let mut next = hand.to_vec();
        next.push(tile);
        next.sort_unstable();
        if is_complete_win_with_melds(&next, melds, win_rule) {
            wait_kinds += 1;
            score += 28.0 + remaining as f64 * 5.0;
            score += fan_wait_bias(&next, melds, table, position, win_rule, tile, remaining);
            if melds.is_empty() && is_seven_pairs_wait_shape(hand) && is_seven_pairs_win(&next) {
                score += seven_pairs_wait_tile_score(tile, hand, table, position);
            }
        }
    }
    if wait_kinds >= 2 {
        score += wait_kinds as f64 * 3.0;
    }
    score
}

pub(in crate::ai::decision) fn ready_has_pure_one_suit_win(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> bool {
    if hand.len() % 3 != 1 {
        return false;
    }

    SHENYANG_MAHJONG_TILE_KINDS.into_iter().any(|tile| {
        remaining_tile_count(hand, table, position, tile) > 0 && {
            let mut next = hand.to_vec();
            next.push(tile);
            next.sort_unstable();
            is_complete_win_with_melds(&next, melds, win_rule) && is_pure_one_suit_win(&next, melds)
        }
    })
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
            ready_hand_visible_fan_reaches_cap(&next, melds, table, position, win_rule, max_fan)
        });
    }
    ready_hand_visible_fan_reaches_cap(hand, melds, table, position, win_rule, max_fan)
}
