use super::*;

pub(super) fn can_self_gang_candidate(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    tile: i32,
) -> bool {
    if !is_valid_tile(tile) {
        return false;
    }
    let hand_count = hand.iter().filter(|item| **item == tile).count();
    let peng_meld_count = melds
        .iter()
        .filter(|meld| is_open_peng_meld(meld, tile))
        .count();
    (hand_count == 4 && peng_meld_count == 0) || (hand_count == 1 && peng_meld_count == 1)
}

pub(in crate::ai::decision) fn self_gang_known_tile_count_is_possible(
    hand: &[i32],
    table: &AiPublicTable,
    tile: i32,
) -> bool {
    hand.iter().filter(|item| **item == tile).count() + visible_tile_count(table, tile) as usize
        <= 4
}

fn self_gang_position_known_tile_counts_are_possible(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
) -> bool {
    let mut owned_tiles = hand.to_vec();
    owned_tiles.extend(valid_meld_tiles(melds));
    unique_tiles(&owned_tiles)
        .into_iter()
        .all(|tile| self_gang_known_tile_count_is_possible(hand, table, tile))
}

pub fn choose_self_gang_from_view(
    hand: &[i32],
    candidate_tiles: &[i32],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> Option<i32> {
    let melds = table
        .seats
        .get(&position)
        .map(|seat| seat.melds.as_slice())
        .unwrap_or(&[]);
    if !has_virtual_tile_count(hand, melds, 14)
        || !self_gang_position_known_tile_counts_are_possible(hand, melds, table)
        || table.wall_count == 0
        || candidate_tiles.is_empty()
        || should_preserve_seven_pairs_for_self_gang(hand, melds, table, position, win_rule)
    {
        return None;
    }

    let current_ready_score =
        best_ready_score_after_discard(hand, melds, table, position, win_rule);
    if should_pass_late_unready_self_gang_for_defense(table, current_ready_score) {
        return None;
    }
    let current_score = best_score_after_forced_discard(hand, melds, table, position, win_rule);
    let mut best: Option<(f64, i32)> = None;
    for tile in candidate_tiles.iter().copied() {
        if !can_self_gang_candidate(hand, melds, tile)
            || !self_gang_known_tile_count_is_possible(hand, table, tile)
        {
            continue;
        }
        let score = self_gang_score(tile, hand, melds, table, position, win_rule, current_score);
        match best {
            None => best = Some((score, tile)),
            Some((best_score, best_tile)) => {
                if score > best_score || (score == best_score && tile < best_tile) {
                    best = Some((score, tile));
                }
            }
        }
    }
    best.and_then(|(score, tile)| (score >= 0.0).then_some(tile))
}

pub(super) fn self_gang_score(
    tile: i32,
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    current_score: f64,
) -> f64 {
    let is_added_gang = has_peng_meld(melds, tile);
    let is_ready = best_ready_score_after_discard(hand, melds, table, position, win_rule) > 0.0;
    let pure_one_suit_score = pure_one_suit_plan_score_for_context(hand, melds, table, position);
    let piao_score = piao_plan_score_for_context(hand, melds, table, position);
    if pure_one_suit_score > 0.0 {
        if is_honor(tile) || !is_main_pure_suit_tile(hand, melds, tile) || !is_ready {
            return f64::NEG_INFINITY;
        }
    }
    if is_ready && ready_visible_fan_reaches_cap(hand, melds, table, position, win_rule) {
        return f64::NEG_INFINITY;
    }
    if is_ready && ready_visible_fan_exceeds_half_cap(hand, melds, table, position, win_rule) {
        return f64::NEG_INFINITY;
    }
    if !is_ready && capped_open_basic_route_visible_fan_reaches_cap(hand, melds, table) {
        return f64::NEG_INFINITY;
    }
    if !is_ready && table.max_fan.is_some_and(|max_fan| max_fan <= 1) {
        return f64::NEG_INFINITY;
    }
    if !is_ready && !is_dragon(tile) {
        return f64::NEG_INFINITY;
    }
    if !is_added_gang && !is_ready && is_dragon(tile) && has_open_meld(melds) && piao_score >= 22.0
    {
        return f64::NEG_INFINITY;
    }
    if !is_added_gang
        && !is_ready
        && win_rule == WIN_RULE_SHENYANG_BASIC
        && (!is_dragon(tile) || !has_door_opening_meld(melds, table))
    {
        return f64::NEG_INFINITY;
    }

    let mut next = remove_n_tiles(hand, tile, if is_added_gang { 1 } else { 4 });
    sort_tiles(&mut next);
    let mut next_melds = if is_added_gang {
        promoted_added_gang_melds(melds, tile)
    } else {
        melds.to_vec()
    };
    if !is_added_gang {
        next_melds.push(WsShenyangMahjongMeld {
            kind: ShenyangMahjongMeldKind::GANG,
            tiles: vec![tile, tile, tile, tile],
            from_position: None,
        });
    }
    let after_ready_score = ready_tile_score(&next, &next_melds, table, position, win_rule);
    let keeps_pure_one_suit_ready = pure_one_suit_score > 0.0
        && ready_has_pure_one_suit_win(&next, &next_melds, table, position, win_rule);
    if pure_one_suit_score > 0.0 && !keeps_pure_one_suit_ready {
        return f64::NEG_INFINITY;
    }
    let committed_piao_plan = piao_score >= 22.0
        && piao_threat_level(melds) > 0
        && piao_committed_group_count(hand, melds) >= 3;
    let keeps_piao_ready =
        committed_piao_plan && ready_has_piao_win(&next, &next_melds, table, position, win_rule);
    if committed_piao_plan && !keeps_piao_ready {
        return f64::NEG_INFINITY;
    }
    if is_ready && after_ready_score <= 0.0 {
        return f64::NEG_INFINITY;
    }
    if is_added_gang && should_preserve_four_gui_yi(tile) {
        let loses_four_gui_yi =
            estimated_four_gui_yi_fan(hand, melds) > estimated_four_gui_yi_fan(&next, &next_melds);
        let visible_fan_gain = estimated_visible_bonus_fan(&next, &next_melds)
            - estimated_visible_bonus_fan(hand, melds);
        if loses_four_gui_yi && visible_fan_gain <= 0 && !keeps_pure_one_suit_ready {
            return f64::NEG_INFINITY;
        }
    }
    let after_score = hand_progress_score(&next, &next_melds, table, position, win_rule);
    let mut score = after_score - current_score + 34.0;

    if is_dragon(tile) {
        score += 36.0;
    } else if tile_is_terminal(tile) || is_honor(tile) {
        score += 5.0;
    }
    if is_ready {
        score += 24.0;
    }
    if is_added_gang {
        score += 8.0;
    } else if has_open_meld(melds) {
        score += 5.0;
    } else if !is_ready {
        score -= 14.0;
    } else if win_rule == WIN_RULE_SHENYANG_BASIC && table.dealer_position != position {
        score -= if is_late_defense_round(table) {
            4.0
        } else {
            12.0
        };
    }
    if piao_score >= 22.0 {
        score += 8.0;
    }
    if is_ready && has_open_meld(melds) {
        score = score.max(6.0);
    }
    if is_dragon(tile) {
        score = score.max(12.0);
    }
    score
}

pub(super) fn should_pass_late_unready_self_gang_for_defense(
    table: &AiPublicTable,
    current_ready_score: f64,
) -> bool {
    is_late_defense_round(table) && current_ready_score <= 0.0
}

pub(super) fn should_preserve_four_gui_yi(tile: i32) -> bool {
    !is_dragon(tile)
}

pub(super) fn should_preserve_seven_pairs_for_self_gang(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> bool {
    should_lock_seven_pairs_plan(hand, melds, table, position, win_rule)
}
