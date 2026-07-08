use super::*;

pub(super) fn early_piao_candidate_discard_bias(
    hand: &[i32],
    tile: i32,
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
) -> f64 {
    if piao_plan_is_capped(table) || table.dealer_position == position {
        return 0.0;
    }
    if melds.iter().any(is_sequence_meld)
        || pair_count(hand) < 3
        || !missing_suits(hand, melds).is_empty()
        || !has_terminal_or_honor_with_extra(hand, melds, None)
    {
        return 0.0;
    }
    let count = hand.iter().filter(|item| **item == tile).count();
    let only_terminal_or_honor = (is_honor(tile) || tile_is_terminal(tile))
        && count == 1
        && terminal_or_honor_count(hand, melds) == 1;
    let only_suit_tile =
        is_suited(tile) && suited_tile_count_for_suit(hand, melds, tile_suit(tile)) == 1;
    if count >= 3 {
        -10.0
    } else if count == 2 {
        -6.5
    } else if only_terminal_or_honor || only_suit_tile {
        -40.0
    } else {
        0.0
    }
}

pub(super) fn has_early_piao_singleton_discard(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
) -> bool {
    is_closed_early_piao_candidate(hand, melds, table, position)
        && unique_tiles(hand).into_iter().any(|tile| {
            hand.iter().filter(|item| **item == tile).count() == 1 && {
                let next = remove_n_tiles(hand, tile, 1);
                next.len() + 1 == hand.len() && has_piao_route_basics(&next, melds)
            }
        })
}

pub(super) fn piao_discard_bias(
    hand: &[i32],
    tile: i32,
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> f64 {
    if piao_plan_score_for_context(hand, melds, table, position) < 20.0 {
        return 0.0;
    }
    let count = hand.iter().filter(|item| **item == tile).count();
    let pure_one_suit_score = pure_one_suit_plan_score_for_context(hand, melds, table, position);
    let committed_groups = piao_committed_group_count(hand, melds);
    let only_terminal_or_honor = (is_honor(tile) || tile_is_terminal(tile))
        && count == 1
        && terminal_or_honor_count(hand, melds) == 1
        && pure_one_suit_score <= 0.0;
    let only_suit_tile = is_suited(tile)
        && suited_tile_count_for_suit(hand, melds, tile_suit(tile)) == 1
        && pure_one_suit_score <= 0.0;
    if count >= 3 {
        if committed_groups >= 2 { -28.0 } else { -20.0 }
    } else if count == 2 {
        let base = if committed_groups >= 2 { -24.0 } else { -16.0 };
        base + piao_dragon_pair_discard_bias(hand, table, position, tile, count)
            + piao_pair_liveness_discard_bias(hand, table, position, tile, count)
    } else if only_terminal_or_honor || only_suit_tile {
        -40.0
    } else if win_rule == WIN_RULE_SHENYANG_BASIC && is_dragon(tile) && pair_count(hand) >= 4 {
        16.0
    } else if is_honor(tile) || tile_is_terminal(tile) {
        1.0
    } else if neighbor_count(hand, tile) >= 2 {
        3.0
    } else {
        0.0
    }
}

pub(super) fn piao_pair_liveness_discard_bias(
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
        0 => 3.0,
        _ => 0.0,
    }
}

pub(super) fn piao_dragon_pair_discard_bias(
    hand: &[i32],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    count: usize,
) -> f64 {
    if count != 2 || !is_dragon(tile) {
        return 0.0;
    }
    match remaining_tile_count(hand, table, position, tile) {
        0 => -1.5,
        1 => -3.0,
        _ => -5.0,
    }
}

pub(super) fn piao_committed_group_count(hand: &[i32], melds: &[WsShenyangMahjongMeld]) -> usize {
    let open_triplets = melds
        .iter()
        .filter(|meld| is_triplet_like_meld(meld))
        .count();
    let mut counts: HashMap<i32, usize> = HashMap::new();
    for &tile in hand {
        *counts.entry(tile).or_default() += 1;
    }
    open_triplets + counts.values().filter(|count| **count >= 3).count()
}

pub(super) fn piao_plan_score(hand: &[i32], melds: &[WsShenyangMahjongMeld]) -> f64 {
    if melds.iter().any(is_sequence_meld) {
        return 0.0;
    }
    let open_triplets = melds
        .iter()
        .filter(|meld| is_triplet_like_meld(meld))
        .count();
    let mut counts: HashMap<i32, usize> = HashMap::new();
    for &tile in hand {
        *counts.entry(tile).or_default() += 1;
    }
    let triplets = counts.values().filter(|count| **count >= 3).count();
    let pairs = counts.values().filter(|count| **count >= 2).count();
    let score = open_triplets as f64 * 18.0 + triplets as f64 * 14.0 + pairs as f64 * 5.0;
    if open_triplets + triplets >= 2 || pairs >= 3 || (open_triplets >= 1 && pairs >= 2) {
        score
    } else {
        0.0
    }
}

pub(super) fn piao_plan_score_for_context(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
) -> f64 {
    let score = piao_plan_score(hand, melds);
    if score <= 0.0 || piao_plan_is_capped(table) || !has_piao_route_basics(hand, melds) {
        return 0.0;
    }
    if table.dealer_position == position && score < 40.0 {
        score * 0.35
    } else {
        score
    }
}

pub(super) fn piao_plan_is_capped(table: &AiPublicTable) -> bool {
    table.max_fan.is_some_and(|max_fan| max_fan <= 1)
}

pub(super) fn has_piao_route_basics(hand: &[i32], melds: &[WsShenyangMahjongMeld]) -> bool {
    missing_suits(hand, melds).is_empty() && has_terminal_or_honor_with_extra(hand, melds, None)
}

pub(super) fn piao_threat_level(melds: &[WsShenyangMahjongMeld]) -> usize {
    if melds.iter().any(is_sequence_meld) {
        return 0;
    }
    melds
        .iter()
        .filter(|meld| is_triplet_like_meld(meld))
        .count()
}

pub(super) fn choose_piao_single_wait_discard(
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

pub(super) fn piao_single_wait_tile_score(
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

pub(super) fn piao_missing_suits_from_melds(melds: &[WsShenyangMahjongMeld]) -> Vec<i32> {
    if piao_threat_level(melds) < 3 {
        return Vec::new();
    }
    let mut suits = [false; 3];
    for meld in melds.iter().filter(|meld| is_triplet_like_meld(meld)) {
        if let Some(tile) = meld_primary_tile(meld)
            && is_suited(tile)
        {
            suits[tile_suit(tile) as usize] = true;
        }
    }
    suits
        .into_iter()
        .enumerate()
        .filter_map(|(suit, present)| (!present).then_some(suit as i32))
        .collect()
}

pub(super) fn is_closed_early_piao_candidate(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
) -> bool {
    melds.is_empty()
        && pair_count(hand) >= 3
        && table.dealer_position != position
        && !piao_plan_is_capped(table)
        && has_piao_route_basics(hand, melds)
}
