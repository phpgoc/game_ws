mod bias;
mod defense;
mod sequences;

use super::*;

pub(super) use bias::*;
pub(super) use defense::*;
pub(super) use sequences::*;

pub fn choose_discard_from_view(
    hand: &[i32],
    table: &AiPublicTable,
    position: usize,
) -> Option<i32> {
    choose_discard_from_view_inner(hand, table, position, false)
}

fn choose_discard_from_view_inner(
    hand: &[i32],
    table: &AiPublicTable,
    position: usize,
    must_discard: bool,
) -> Option<i32> {
    let melds = table
        .seats
        .get(&position)
        .map(|seat| seat.melds.as_slice())
        .unwrap_or(&[]);
    if !has_virtual_tile_count(hand, melds, 14)
        || !position_known_tile_counts_are_possible(hand, melds, table)
    {
        return None;
    }
    if !must_discard && is_complete_win_for_table(hand, melds, table) {
        return None;
    }
    if is_late_defense_round(table) && has_late_defense_known_safe_candidate(hand, table) {
        if let Some(tile) = choose_late_ready_discard(hand, melds, table, position) {
            return Some(tile);
        }
        if should_keep_pairs_for_seven_pairs_discard(hand, melds, table, position) {
            return choose_late_defense_discard_preserving_pairs(hand, table, position);
        }
        return choose_late_defense_discard(hand, table, position);
    }
    if let Some(tile) = choose_seven_pairs_wait_discard(hand, melds, table, position) {
        return Some(tile);
    }
    if let Some(tile) = choose_piao_single_wait_discard(hand, melds, table, position) {
        return Some(tile);
    }
    if is_late_defense_round(table) {
        if let Some(tile) = choose_late_ready_discard(hand, melds, table, position) {
            return Some(tile);
        }
        if should_keep_pairs_for_seven_pairs_discard(hand, melds, table, position) {
            return choose_late_defense_discard_preserving_pairs(hand, table, position);
        }
        return choose_late_defense_discard(hand, table, position);
    }
    if should_use_broken_hand_public_defense_discard(hand, melds, table, position)
        && let Some(tile) = choose_broken_hand_public_defense_discard(hand, melds, table, position)
    {
        return Some(tile);
    }

    let preserve_early_piao_pairs = has_early_piao_singleton_discard(hand, melds, table, position);
    let speed_first_wait = table.dealer_position == position
        || ready_visible_fan_reaches_cap(hand, melds, table, position)
        || ready_visible_fan_exceeds_half_cap(hand, melds, table, position);
    let mut best_allowed: Option<(i32, f64, i32)> = None;
    let mut best_any: Option<(i32, f64, i32)> = None;
    for tile in unique_tiles(hand) {
        let count = hand.iter().filter(|&&item| item == tile).count();
        if preserve_early_piao_pairs && count >= 2 {
            continue;
        }
        let mut next = hand.to_vec();
        if let Some(index) = next.iter().position(|item| *item == tile) {
            next.remove(index);
        }
        let violates_basic_hard_requirement =
            violates_basic_three_suits_discard(&next, melds, table, position, tile)
                || violates_basic_terminal_or_honor_discard(&next, melds, table, position, tile)
                || violates_basic_heng_discard(&next, melds, table, position, tile);
        let score = hand_progress_score_after_discard(&next, melds, table, position, tile);
        let pressure = estimate_pressure_for_tile(table, position, tile);
        let neigh = neighbor_count(hand, tile);
        let discard_bias = match (count, is_honor(tile), tile_is_terminal(tile), neigh) {
            (1, true, _, _) => honor_discard_bias(hand, tile),
            (1, _, true, 0) => 4.8,
            (1, _, _, 0) => isolated_suited_singleton_discard_bias(tile),
            (2, _, _, _) => pair_discard_bias(hand),
            (c, _, _, neigh) if c >= 3 => -4.5 - neigh as f64,
            _ => 0.0,
        } + three_suits_discard_bias(&next, melds, table, position, tile)
            + terminal_or_honor_discard_bias(&next, melds, table, position, tile)
            + piao_discard_bias(hand, tile, melds, table, position)
            + early_piao_candidate_discard_bias(hand, tile, melds, table, position)
            + basic_heng_seed_discard_bias(hand, tile, melds)
            + capped_spare_dragon_discard_bias(hand, tile, melds, table)
            + seven_pairs_plan_discard_bias(hand, tile, melds, table, position)
            + seven_pairs_wait_discard_bias(hand, tile, melds, table, position)
            + four_gui_yi_discard_bias(hand, tile, melds, table, position)
            + pure_one_suit_discard_bias(hand, tile, melds, table, position)
            + complete_sequence_discard_bias(hand, tile, melds, table, position)
            + incomplete_sequence_discard_bias(hand, tile, melds, table, position)
            + pinghu_sequence_route_discard_bias(hand, tile, melds, table, position)
            + mid_round_public_discard_bias(table, position, tile)
            + mid_round_open_meld_safety_bias(table, tile)
            + mid_round_live_honor_risk_bias(table, position, tile, count)
            + mid_round_live_suited_risk_bias(hand, melds, table, position, tile, count)
            + own_open_public_safety_bias(melds, table, position, tile)
            + opponent_threat_discard_bias(table, position, tile, count)
            + pure_one_suit_threat_discard_bias(table, position, tile, count)
            + closed_opponent_threat_discard_bias(table, position, tile, count)
            + late_defense_discard_bias(table, position, tile);
        let combined = score + discard_bias + pressure;
        let ready_live_tiles = if speed_first_wait {
            ready_live_tile_count_after_discard(&next, melds, table, position, tile)
        } else {
            0
        };
        let candidate = (ready_live_tiles, combined, tile);
        match best_any {
            None => best_any = Some(candidate),
            Some(best) => {
                if discard_candidate_is_better(candidate, best) {
                    best_any = Some(candidate);
                }
            }
        }
        if violates_basic_hard_requirement {
            continue;
        }
        match best_allowed {
            None => best_allowed = Some(candidate),
            Some(best) => {
                if discard_candidate_is_better(candidate, best) {
                    best_allowed = Some(candidate);
                }
            }
        }
    }
    best_allowed.or(best_any).map(|(_, _, tile)| tile)
}

pub fn choose_forced_discard_from_view(
    hand: &[i32],
    table: &AiPublicTable,
    position: usize,
) -> Option<i32> {
    choose_discard_from_view_inner(hand, table, position, true)
}

fn discard_candidate_is_better(candidate: (i32, f64, i32), current: (i32, f64, i32)) -> bool {
    candidate.0 > current.0
        || (candidate.0 == current.0
            && (candidate.1.partial_cmp(&current.1) == Some(Ordering::Greater)
                || (candidate.1 == current.1 && candidate.2 < current.2)))
}
