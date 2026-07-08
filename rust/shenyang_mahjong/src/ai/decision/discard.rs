use super::*;

pub fn choose_discard_from_view(
    hand: &[i32],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> Option<i32> {
    if hand.len() % 3 != 2 {
        return None;
    }
    let melds = table
        .seats
        .get(&position)
        .map(|seat| seat.melds.as_slice())
        .unwrap_or(&[]);
    if is_complete_win_with_melds(hand, melds, win_rule) {
        return None;
    }
    if let Some(tile) = choose_seven_pairs_wait_discard(hand, melds, table, position, win_rule) {
        return Some(tile);
    }
    if let Some(tile) = choose_piao_single_wait_discard(hand, melds, table, position, win_rule) {
        return Some(tile);
    }
    if is_late_defense_round(table)
        && best_ready_score_after_discard(hand, melds, table, position, win_rule) <= 0.0
    {
        if should_keep_pairs_for_seven_pairs_discard(hand, melds, table, position, win_rule) {
            return choose_late_defense_discard_preserving_pairs(hand, table, position);
        }
        return choose_late_defense_discard(hand, table, position);
    }
    if should_use_broken_hand_public_defense_discard(hand, melds, table, position, win_rule) {
        if let Some(tile) =
            choose_broken_hand_public_defense_discard(hand, melds, table, position, win_rule)
        {
            return Some(tile);
        }
    }

    let preserve_early_piao_pairs = has_early_piao_singleton_discard(hand, melds, table, position);
    let mut best_allowed: Option<(f64, i32)> = None;
    let mut best_any: Option<(f64, i32)> = None;
    for tile in hand.iter().copied() {
        let count = hand.iter().filter(|&&item| item == tile).count();
        if preserve_early_piao_pairs && count >= 2 {
            continue;
        }
        let mut next = hand.to_vec();
        if let Some(index) = next.iter().position(|item| *item == tile) {
            next.remove(index);
        }
        let violates_basic_hard_requirement =
            violates_basic_three_suits_discard(&next, melds, table, position, tile, win_rule)
                || violates_basic_terminal_or_honor_discard(
                    &next, melds, table, position, tile, win_rule,
                )
                || violates_basic_heng_discard(&next, melds, table, position, tile, win_rule);
        let score = hand_progress_score(&next, melds, table, position, win_rule);
        let pressure = estimate_pressure_for_tile(table, position, tile);
        let neigh = neighbor_count(hand, tile);
        let discard_bias = match (count, is_honor(tile), tile_is_terminal(tile), neigh) {
            (c, true, _, _) if c == 1 => honor_discard_bias(hand, tile),
            (1, _, true, 0) => 4.8,
            (1, _, _, 0) => isolated_suited_singleton_discard_bias(tile),
            (2, _, _, _) => pair_discard_bias(hand),
            (c, _, _, neigh) if c >= 3 => -4.5 - neigh as f64,
            _ => 0.0,
        } + three_suits_discard_bias(
            &next, melds, table, position, tile, win_rule,
        ) + terminal_or_honor_discard_bias(
            &next, melds, table, position, tile, win_rule,
        ) + piao_discard_bias(hand, tile, melds, table, position, win_rule)
            + early_piao_candidate_discard_bias(hand, tile, melds, table, position)
            + basic_heng_seed_discard_bias(hand, tile, melds, win_rule)
            + seven_pairs_plan_discard_bias(hand, tile, melds, table, position, win_rule)
            + seven_pairs_wait_discard_bias(hand, tile, melds, table, position)
            + four_gui_yi_discard_bias(hand, tile, melds, table, position, win_rule)
            + pure_one_suit_discard_bias(hand, tile, melds, table, position)
            + complete_sequence_discard_bias(hand, tile, melds, table, position)
            + incomplete_sequence_discard_bias(hand, tile, melds, table, position, win_rule)
            + mid_round_public_discard_bias(table, position, tile)
            + mid_round_open_meld_safety_bias(table, tile)
            + mid_round_live_honor_risk_bias(table, position, tile, count)
            + mid_round_live_suited_risk_bias(hand, melds, table, position, tile, count, win_rule)
            + own_open_public_safety_bias(melds, table, position, tile)
            + opponent_threat_discard_bias(table, position, tile, count)
            + pure_one_suit_threat_discard_bias(table, position, tile, count)
            + closed_opponent_threat_discard_bias(table, position, tile, count)
            + late_defense_discard_bias(table, position, tile);
        let combined = score + discard_bias + pressure;
        match best_any {
            None => best_any = Some((combined, tile)),
            Some((best_score, best_tile)) => {
                let better = combined.partial_cmp(&best_score) == Some(Ordering::Greater);
                if better || (combined == best_score && tile < best_tile) {
                    best_any = Some((combined, tile));
                }
            }
        }
        if violates_basic_hard_requirement {
            continue;
        }
        match best_allowed {
            None => best_allowed = Some((combined, tile)),
            Some((best_score, best_tile)) => {
                let better = combined.partial_cmp(&best_score) == Some(Ordering::Greater);
                if better || (combined == best_score && tile < best_tile) {
                    best_allowed = Some((combined, tile));
                }
            }
        }
    }
    best_allowed.or(best_any).map(|(_, tile)| tile)
}

pub(super) fn choose_late_defense_discard(
    hand: &[i32],
    table: &AiPublicTable,
    position: usize,
) -> Option<i32> {
    choose_late_defense_discard_from_candidates(hand, table, position, unique_tiles(hand))
}

pub(super) fn choose_late_defense_discard_preserving_pairs(
    hand: &[i32],
    table: &AiPublicTable,
    position: usize,
) -> Option<i32> {
    let public_candidates = unique_tiles(hand)
        .into_iter()
        .filter(|tile| public_discard_count(table, *tile) > 0)
        .collect::<Vec<_>>();
    if !public_candidates.is_empty() {
        return choose_late_defense_discard_from_candidates(
            hand,
            table,
            position,
            public_candidates,
        );
    }

    let singletons = unique_tiles(hand)
        .into_iter()
        .filter(|tile| hand.iter().filter(|item| **item == *tile).count() == 1)
        .collect::<Vec<_>>();
    if singletons.is_empty() {
        choose_late_defense_discard(hand, table, position)
    } else {
        choose_late_defense_discard_from_candidates(hand, table, position, singletons)
    }
}

pub(super) fn choose_late_defense_discard_from_candidates(
    hand: &[i32],
    table: &AiPublicTable,
    position: usize,
    candidates: Vec<i32>,
) -> Option<i32> {
    let mut best: Option<(f64, i32)> = None;
    let public_candidates = candidates
        .iter()
        .copied()
        .filter(|tile| public_discard_count(table, *tile) > 0)
        .collect::<Vec<_>>();
    let candidates = if public_candidates.is_empty() {
        candidates
    } else {
        public_candidates
    };

    for tile in candidates {
        let own_tile_count = hand.iter().filter(|item| **item == tile).count();
        let score = late_defense_tile_safety_score(table, position, tile, own_tile_count);
        match best {
            None => best = Some((score, tile)),
            Some((best_score, best_tile)) => {
                if score > best_score || (score == best_score && tile < best_tile) {
                    best = Some((score, tile));
                }
            }
        }
    }
    best.map(|(_, tile)| tile)
}

pub(super) fn choose_broken_hand_public_defense_discard(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> Option<i32> {
    let public_candidates = unique_tiles(hand)
        .into_iter()
        .filter(|tile| public_discard_count(table, *tile) > 0)
        .collect::<Vec<_>>();
    if !public_candidates.is_empty() {
        return choose_public_defense_discard_from_candidates(
            hand,
            melds,
            table,
            position,
            win_rule,
            public_candidates,
        );
    }

    let open_meld_candidates = unique_tiles(hand)
        .into_iter()
        .filter(|tile| mid_round_open_meld_safety_bias(table, *tile) > 0.0)
        .collect::<Vec<_>>();
    if !open_meld_candidates.is_empty() {
        return choose_public_defense_discard_from_candidates(
            hand,
            melds,
            table,
            position,
            win_rule,
            open_meld_candidates,
        );
    }

    let missing_suit_candidates = unique_tiles(hand)
        .into_iter()
        .filter(|tile| mid_broken_opponent_missing_suit_safety_bias(table, position, *tile) > 0.0)
        .collect::<Vec<_>>();
    choose_public_defense_discard_from_candidates(
        hand,
        melds,
        table,
        position,
        win_rule,
        missing_suit_candidates,
    )
}

pub(super) fn choose_public_defense_discard_from_candidates(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    candidates: Vec<i32>,
) -> Option<i32> {
    let mut best: Option<(f64, i32)> = None;
    for tile in candidates {
        let own_tile_count = hand.iter().filter(|item| **item == tile).count();
        let score = public_defense_tile_safety_score(table, position, tile, own_tile_count)
            + basic_heng_recovery_public_defense_bias(hand, melds, table, tile, win_rule);
        match best {
            None => best = Some((score, tile)),
            Some((best_score, best_tile)) => {
                if score > best_score || (score == best_score && tile < best_tile) {
                    best = Some((score, tile));
                }
            }
        }
    }
    best.map(|(_, tile)| tile)
}

pub(super) fn dragon_value_bias(hand: &[i32], tile: i32) -> f64 {
    if !is_dragon(tile) {
        return 0.0;
    }
    let pairs = pair_count(hand);
    if pairs >= 4 { 10.4 } else { -3.0 }
}

pub(super) fn honor_discard_bias(hand: &[i32], tile: i32) -> f64 {
    if is_wind(tile) {
        8.0
    } else if is_dragon(tile) {
        4.8 + dragon_value_bias(hand, tile)
    } else {
        6.0
    }
}

pub(super) fn isolated_suited_singleton_discard_bias(tile: i32) -> f64 {
    if !is_suited(tile) {
        return 4.0;
    }
    match tile_rank(tile) {
        2 | 8 => 4.6,
        3 | 7 => 4.25,
        _ => 4.0,
    }
}

pub(super) fn pair_discard_bias(hand: &[i32]) -> f64 {
    if pair_count(hand) >= 4 { -4.4 } else { -1.8 }
}

pub(super) fn complete_sequence_discard_bias(
    hand: &[i32],
    tile: i32,
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
) -> f64 {
    if hand.iter().filter(|item| **item == tile).count() != 1 {
        return 0.0;
    }
    if tile_is_middle_of_sequence(hand, tile) {
        -6.0
    } else if is_closed_early_piao_candidate(hand, melds, table, position) {
        0.0
    } else if tile_is_part_of_complete_sequence(hand, tile) {
        -4.0
    } else {
        0.0
    }
}

pub(super) fn incomplete_sequence_discard_bias(
    hand: &[i32],
    tile: i32,
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> f64 {
    if hand.iter().filter(|item| **item == tile).count() != 1
        || !is_suited(tile)
        || tile_is_part_of_complete_sequence(hand, tile)
        || should_lock_seven_pairs_plan(hand, melds, table, position, win_rule)
        || is_closed_early_piao_candidate(hand, melds, table, position)
        || piao_plan_score_for_context(hand, melds, table, position) >= 20.0
        || pure_one_suit_plan_score_for_context(hand, melds, table, position) > 0.0
    {
        return 0.0;
    }
    if tile_is_weak_edge_wait_terminal(hand, tile) {
        3.2
    } else if tile_is_core_two_sided_wait_member(hand, tile)
        || tile_is_core_closed_middle_wait_member(hand, tile)
    {
        -3.0
    } else {
        0.0
    }
}
