use super::*;

pub(in crate::ai::decision) fn choose_piao_single_wait_discard(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> Option<i32> {
    if hand.len() != 2 || melds.len() != 4 || piao_threat_level(melds) != 4 {
        return None;
    }

    unique_tiles(hand)
        .into_iter()
        .filter_map(|tile| {
            let next = remove_n_tiles(hand, tile, 1);
            if next.len() + 1 != hand.len() || next.len() != 1 {
                return None;
            }
            let wait_tile = next[0];
            let mut win_hand = next.clone();
            win_hand.push(wait_tile);
            win_hand.sort_unstable();
            if !is_piao_hu_win(&win_hand, melds)
                || !is_complete_win_with_melds(&win_hand, melds, win_rule)
            {
                return None;
            }
            let own_tile_count = hand.iter().filter(|item| **item == tile).count();
            Some((
                piao_single_wait_tile_score(wait_tile, &next, melds, table, position, win_rule)
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

pub(in crate::ai::decision) fn piao_single_wait_tile_score(
    wait_tile: i32,
    hand_after_discard: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> f64 {
    let remaining = remaining_tile_count(hand_after_discard, table, position, wait_tile);
    if remaining <= 0 {
        return -240.0;
    }

    let mut win_hand = hand_after_discard.to_vec();
    win_hand.push(wait_tile);
    win_hand.sort_unstable();
    let estimated_fan = estimated_fan_with_wait(&win_hand, melds, wait_tile, win_rule);
    let capped_fan = table
        .max_fan
        .filter(|max_fan| *max_fan > 0)
        .map(|max_fan| estimated_fan.min(max_fan))
        .unwrap_or(estimated_fan);
    let speed_first = table.dealer_position == position || is_late_defense_round(table);
    let remaining_weight = if speed_first { 14.0 } else { 7.0 };
    let fan_weight = if speed_first { 2.0 } else { 7.0 };
    let wait_shape_bias = if table
        .max_fan
        .is_some_and(|max_fan| max_fan > 0 && estimated_fan >= max_fan)
    {
        0.0
    } else {
        seven_pairs_wait_shape_tiebreaker(wait_tile)
    };

    remaining as f64 * remaining_weight + capped_fan as f64 * fan_weight + wait_shape_bias
}
