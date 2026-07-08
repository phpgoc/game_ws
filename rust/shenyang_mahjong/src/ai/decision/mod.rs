use std::cmp::Ordering;
use std::collections::HashMap;

mod hand;
mod meld;
mod table;
mod tile;

use share_type_public::games::shenyang_mahjong::{
    SHENYANG_MAHJONG_TILE_KINDS, ShenyangMahjongMeldKind, WsShenyangMahjongMeld,
};

use crate::rules::{
    WIN_RULE_SHENYANG_BASIC, can_chi, can_gang, can_peng, is_complete_win_with_melds,
    is_piao_hu_win, is_pure_one_suit_win, is_seven_pairs_win, is_single_wait_shape_with_rule,
    sort_tiles,
};
#[cfg(test)]
use crate::rules::{has_triplet_in_standard_decomposition, is_complete_win};

use super::observation::{AiClaimView, AiPublicTable, AiSeatView};
use hand::{
    hand_power, has_terminal_or_honor_with_extra, has_triplet_like_group,
    has_triplet_or_dragon_pair, has_triplet_or_dragon_pair_with_extra, is_seven_pairs_wait_shape,
    missing_suits, neighbor_count, pair_count, remove_n_tiles, single_tile, suit_presence,
    suit_presence_with_extra, suited_tile_count_for_suit, terminal_or_honor_count,
    tile_is_core_closed_middle_wait_member, tile_is_core_two_sided_wait_member,
    tile_is_middle_of_sequence, tile_is_part_of_complete_sequence, tile_is_weak_edge_wait_terminal,
};
use meld::{
    claim_gang_meld, claim_peng_meld, has_concealed_gang_meld, has_open_meld, has_peng_meld,
    is_open_meld, is_sequence_meld, is_triplet_like_meld, is_valid_meld, meld_primary_tile,
    promoted_added_gang_melds, valid_meld_tiles,
};
#[cfg(test)]
use table::visible_tile_count;
use table::{
    exposed_meld_tile_count, live_terminal_or_honor_count,
    live_terminal_or_honor_count_after_discard, live_tile_count_for_suit,
    live_tile_count_for_suit_after_discard, next_position_after, open_meld_tile_count,
    open_opponent_exists_for_tile, own_previous_discard_count, public_discard_count,
    public_discard_seat_count, remaining_tile_count, remaining_tile_count_after_discard,
    seat_has_open_meld_tile,
};
use tile::{
    is_dragon, is_honor, is_suited, is_valid_tile, is_wind, tile_is_terminal, tile_rank, tile_suit,
    unique_tiles,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiClaimChoice {
    Pass,
    Peng,
    Gang,
    Chi { consume_tiles: Vec<i32> },
    Hu,
}

fn chi_options(hand: &[i32], tile: i32) -> Vec<Vec<i32>> {
    let mut options = Vec::new();
    for consume_tiles in [
        [tile - 2, tile - 1],
        [tile - 1, tile + 1],
        [tile + 1, tile + 2],
    ] {
        if !can_chi(hand, tile, &consume_tiles) {
            continue;
        }
        options.push(consume_tiles.to_vec());
    }
    options
}

fn best_ready_score_after_discard(
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

fn best_score_after_forced_discard(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> f64 {
    if hand.is_empty() {
        return hand_progress_score(hand, melds, table, position, win_rule);
    }
    let mut best = f64::NEG_INFINITY;
    for tile in unique_tiles(hand) {
        let mut next = hand.to_vec();
        if let Some(index) = next.iter().position(|item| *item == tile) {
            next.remove(index);
        }
        best = best.max(hand_progress_score(&next, melds, table, position, win_rule));
    }
    best
}

fn best_one_step_wait_potential_after_discard(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> f64 {
    if hand.is_empty() {
        return one_step_wait_potential(hand, melds, table, position, win_rule);
    }
    unique_tiles(hand)
        .into_iter()
        .map(|tile| {
            let mut next = hand.to_vec();
            if let Some(index) = next.iter().position(|item| *item == tile) {
                next.remove(index);
            }
            one_step_wait_potential(&next, melds, table, position, win_rule)
        })
        .fold(0.0, f64::max)
}

pub fn choose_claim_from_view(
    hand: &[i32],
    claim: &AiClaimView,
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> Option<AiClaimChoice> {
    if !claim.eligible_positions.contains(&position) {
        return None;
    }
    let tile = claim.tile;
    let mut win_hand = hand.to_vec();
    win_hand.push(tile);
    win_hand.sort_unstable();
    let melds = table
        .seats
        .get(&position)
        .map(|seat| seat.melds.as_slice())
        .unwrap_or(&[]);
    if is_complete_win_with_melds(&win_hand, melds, win_rule) {
        return Some(AiClaimChoice::Hu);
    }

    let current_melds = table
        .seats
        .get(&position)
        .map(|seat| seat.melds.clone())
        .unwrap_or_default();
    let current_ready_score = ready_tile_score(hand, &current_melds, table, position, win_rule);
    if should_pass_late_unready_claim_for_defense(table, current_ready_score) {
        return Some(AiClaimChoice::Pass);
    }
    if ready_visible_fan_reaches_cap(hand, &current_melds, table, position, win_rule) {
        return Some(AiClaimChoice::Pass);
    }

    if can_gang(hand, tile) {
        if pure_one_suit_plan_score_for_context(hand, &current_melds, table, position) > 0.0
            && !should_claim_ready_pure_one_suit_gang_from_discard(
                hand,
                &current_melds,
                table,
                position,
                win_rule,
                tile,
                claim.from_position,
            )
        {
            return Some(AiClaimChoice::Pass);
        }
        if should_preserve_seven_pairs_plan_for_context(
            hand,
            &current_melds,
            table,
            position,
            win_rule,
        ) {
            return Some(AiClaimChoice::Pass);
        }
        if claim_leaves_unrecoverable_basic_requirement(
            hand,
            &current_melds,
            table,
            position,
            win_rule,
            ShenyangMahjongMeldKind::GANG,
            tile,
            claim.from_position,
        ) && !should_open_broken_closed_hand_for_defense(
            hand,
            &current_melds,
            table,
            position,
            win_rule,
        ) {
            return Some(AiClaimChoice::Pass);
        }
        if should_peng_to_preserve_four_gui_yi_from_discard(
            hand,
            &current_melds,
            table,
            position,
            win_rule,
            tile,
            claim.from_position,
        ) {
            return Some(AiClaimChoice::Peng);
        }
        if should_claim_gang_from_discard(
            hand,
            &current_melds,
            table,
            position,
            win_rule,
            tile,
            claim.from_position,
        ) {
            return Some(AiClaimChoice::Gang);
        }
    }

    let current_score = hand_progress_score(hand, &current_melds, table, position, win_rule);
    let missing_suits = missing_suits(hand, &current_melds);

    if can_peng(hand, tile) {
        if pure_one_suit_plan_score_for_context(hand, &current_melds, table, position) > 0.0 {
            if should_claim_ready_open_pure_one_suit_peng_from_discard(
                hand,
                &current_melds,
                table,
                position,
                win_rule,
                tile,
                claim.from_position,
                current_ready_score,
            ) {
                return Some(AiClaimChoice::Peng);
            }
            return Some(AiClaimChoice::Pass);
        }
        if should_preserve_seven_pairs_plan_for_context(
            hand,
            &current_melds,
            table,
            position,
            win_rule,
        ) {
            return Some(AiClaimChoice::Pass);
        }
        if claim_leaves_unrecoverable_basic_requirement(
            hand,
            &current_melds,
            table,
            position,
            win_rule,
            ShenyangMahjongMeldKind::PENG,
            tile,
            claim.from_position,
        ) && !should_open_broken_closed_hand_for_defense(
            hand,
            &current_melds,
            table,
            position,
            win_rule,
        ) {
            return Some(AiClaimChoice::Pass);
        }
        if should_pass_peng_for_relaxed_pure_defense(
            hand,
            &current_melds,
            table,
            position,
            win_rule,
            tile,
        ) {
            return Some(AiClaimChoice::Pass);
        }
        if current_ready_score > 0.0 {
            if should_claim_ready_dragon_peng_from_discard(
                hand,
                &current_melds,
                table,
                position,
                win_rule,
                tile,
                claim.from_position,
                current_ready_score,
            ) {
                return Some(AiClaimChoice::Peng);
            }
            return Some(AiClaimChoice::Pass);
        }
        if should_claim_peng_for_closed_early_piao_candidate(
            hand,
            &current_melds,
            table,
            position,
            tile,
            claim.from_position,
        ) {
            return Some(AiClaimChoice::Peng);
        }
        if is_dragon(tile) {
            return Some(AiClaimChoice::Peng);
        }
        if should_claim_peng_for_basic_heng_and_opening(
            hand,
            &current_melds,
            table,
            position,
            win_rule,
            tile,
            claim.from_position,
        ) {
            return Some(AiClaimChoice::Peng);
        }
        if should_pass_closed_basic_peng_to_preserve_sequence(
            hand,
            &current_melds,
            table,
            position,
            win_rule,
            tile,
        ) {
            return Some(AiClaimChoice::Pass);
        }
        if should_claim_peng_to_open_mid_basic_hand(
            hand,
            &current_melds,
            table,
            position,
            win_rule,
            tile,
            claim.from_position,
        ) {
            return Some(AiClaimChoice::Peng);
        }
        if win_rule == WIN_RULE_SHENYANG_BASIC
            && !has_open_meld(&current_melds)
            && can_gang(hand, tile)
        {
            return Some(AiClaimChoice::Peng);
        }
        if win_rule == WIN_RULE_SHENYANG_BASIC && table.dealer_position == position {
            return Some(AiClaimChoice::Peng);
        }
        if piao_plan_score_for_context(hand, &current_melds, table, position) >= 32.0 {
            return Some(AiClaimChoice::Peng);
        }
        let mut next = remove_n_tiles(hand, tile, 2);
        let mut melds = current_melds.clone();
        melds.push(claim_peng_meld(tile, claim.from_position));
        sort_tiles(&mut next);
        let after = best_score_after_forced_discard(&next, &melds, table, position, win_rule);
        if should_open_broken_closed_hand_for_defense(
            hand,
            &current_melds,
            table,
            position,
            win_rule,
        ) {
            return Some(AiClaimChoice::Peng);
        }
        if should_preserve_pinghu_sequence_over_peng(
            hand,
            &current_melds,
            table,
            position,
            win_rule,
            tile,
        ) {
            return Some(AiClaimChoice::Pass);
        }
        let mut required_gain = if is_honor(tile) || tile_is_terminal(tile) {
            6.0
        } else {
            10.0
        };
        if is_suited(tile) && neighbor_count(hand, tile) >= 2 {
            required_gain += 8.0;
        }
        if is_suited(tile) && missing_suits.contains(&tile_suit(tile)) {
            required_gain -= 5.0;
        }
        if win_rule == WIN_RULE_SHENYANG_BASIC && !has_open_meld(&current_melds) {
            required_gain -= 4.0;
        }
        if win_rule == WIN_RULE_SHENYANG_BASIC && !has_triplet_or_dragon_pair(hand, &current_melds)
        {
            required_gain -= 3.0;
        }
        if piao_plan_score_for_context(hand, &current_melds, table, position) >= 22.0 {
            required_gain -= 7.0;
        }
        if win_rule == WIN_RULE_SHENYANG_BASIC {
            required_gain -= 4.0;
        }
        if win_rule == WIN_RULE_SHENYANG_BASIC && table.dealer_position == position {
            required_gain -= 8.0;
        }
        if current_melds.is_empty() && pair_count(hand) >= 4 {
            required_gain += 8.0;
        }
        if after >= current_score + required_gain {
            return Some(AiClaimChoice::Peng);
        }
    }

    if win_rule != WIN_RULE_SHENYANG_BASIC
        && position == next_position_after(claim.from_position, table)
    {
        if should_preserve_seven_pairs_plan_for_context(
            hand,
            &current_melds,
            table,
            position,
            win_rule,
        ) {
            return Some(AiClaimChoice::Pass);
        }
        if should_preserve_piao_plan_for_chi(hand, &current_melds, table, position) {
            return Some(AiClaimChoice::Pass);
        }
        if current_ready_score > 0.0 {
            return Some(AiClaimChoice::Pass);
        }
        if !is_mid_opening_round(table) {
            return Some(AiClaimChoice::Pass);
        }
        let defensive_open = should_claim_chi_to_open_broken_hand_for_defense(
            hand,
            &current_melds,
            table,
            position,
            win_rule,
        );
        let pure_chi_suit =
            (pure_one_suit_plan_score_for_context(hand, &current_melds, table, position) > 0.0)
                .then(|| dominant_pure_suit(hand, &current_melds))
                .flatten();
        let mut best_ready_chi: Option<(f64, f64, Vec<i32>)> = None;
        let mut best_progress_chi: Option<(f64, Vec<i32>)> = None;
        for consume_tiles in chi_options(hand, tile) {
            if let Some(main_suit) = pure_chi_suit {
                let preserves_pure_suit = std::iter::once(tile)
                    .chain(consume_tiles.iter().copied())
                    .all(|meld_tile| is_suited(meld_tile) && tile_suit(meld_tile) == main_suit);
                if !preserves_pure_suit {
                    continue;
                }
            }
            let mut next = hand.to_vec();
            for consume in &consume_tiles {
                if let Some(index) = next.iter().position(|item| item == consume) {
                    next.remove(index);
                }
            }
            next.sort_unstable();
            let mut melds = current_melds.clone();
            melds.push(WsShenyangMahjongMeld {
                kind: ShenyangMahjongMeldKind::CHI,
                tiles: {
                    let mut tiles = vec![tile, consume_tiles[0], consume_tiles[1]];
                    tiles.sort_unstable();
                    tiles
                },
                from_position: Some(claim.from_position as i32),
            });
            let after = best_score_after_forced_discard(&next, &melds, table, position, win_rule);
            let after_ready =
                best_ready_score_after_discard(&next, &melds, table, position, win_rule);
            if after_ready > 0.0 {
                match &best_ready_chi {
                    None => best_ready_chi = Some((after_ready, after, consume_tiles)),
                    Some((best_ready, best_after, best_tiles)) => {
                        if after_ready > *best_ready
                            || (after_ready == *best_ready
                                && (after > *best_after
                                    || (after == *best_after && consume_tiles < *best_tiles)))
                        {
                            best_ready_chi = Some((after_ready, after, consume_tiles));
                        }
                    }
                }
                continue;
            }
            match &best_progress_chi {
                None => best_progress_chi = Some((after, consume_tiles)),
                Some((best_after, best_tiles)) => {
                    if after > *best_after || (after == *best_after && consume_tiles < *best_tiles)
                    {
                        best_progress_chi = Some((after, consume_tiles));
                    }
                }
            }
        }
        if let Some((_, _, consume_tiles)) = best_ready_chi {
            return Some(AiClaimChoice::Chi { consume_tiles });
        }
        if !defensive_open {
            return Some(AiClaimChoice::Pass);
        }
        if let Some((_, consume_tiles)) = best_progress_chi {
            return Some(AiClaimChoice::Chi { consume_tiles });
        }
    }

    Some(AiClaimChoice::Pass)
}

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

fn choose_late_defense_discard(
    hand: &[i32],
    table: &AiPublicTable,
    position: usize,
) -> Option<i32> {
    choose_late_defense_discard_from_candidates(hand, table, position, unique_tiles(hand))
}

fn choose_late_defense_discard_preserving_pairs(
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

fn choose_late_defense_discard_from_candidates(
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

fn choose_broken_hand_public_defense_discard(
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

fn choose_public_defense_discard_from_candidates(
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
    if candidate_tiles.is_empty()
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
        if !can_self_gang_candidate(hand, melds, tile) {
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

fn can_self_gang_candidate(hand: &[i32], melds: &[WsShenyangMahjongMeld], tile: i32) -> bool {
    if !is_valid_tile(tile) {
        return false;
    }
    let hand_count = hand.iter().filter(|item| **item == tile).count();
    hand_count >= 4 || (hand_count >= 1 && has_peng_meld(melds, tile))
}

fn claim_leaves_unrecoverable_basic_requirement(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    kind: ShenyangMahjongMeldKind,
    tile: i32,
    from_position: usize,
) -> bool {
    claim_leaves_unrecoverable_missing_suit(
        hand,
        current_melds,
        table,
        win_rule,
        kind,
        tile,
        from_position,
    ) || claim_leaves_unrecoverable_terminal_or_honor(
        hand,
        current_melds,
        table,
        position,
        win_rule,
        kind,
        tile,
        from_position,
    )
}

fn claim_leaves_unrecoverable_missing_suit(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    win_rule: i32,
    kind: ShenyangMahjongMeldKind,
    tile: i32,
    from_position: usize,
) -> bool {
    if win_rule != WIN_RULE_SHENYANG_BASIC {
        return false;
    }

    let (remove_count, claimed_meld) = match kind {
        ShenyangMahjongMeldKind::PENG => (2, claim_peng_meld(tile, from_position)),
        ShenyangMahjongMeldKind::GANG => (3, claim_gang_meld(tile, from_position)),
        ShenyangMahjongMeldKind::CHI => return false,
    };
    let mut next = remove_n_tiles(hand, tile, remove_count);
    if next.len() + remove_count != hand.len() || next.is_empty() {
        return false;
    }
    sort_tiles(&mut next);
    let mut melds = current_melds.to_vec();
    melds.push(claimed_meld);

    !unique_tiles(&next).into_iter().any(|discard| {
        let after_discard = remove_n_tiles(&next, discard, 1);
        let missing = missing_suits(&after_discard, &melds);
        missing.is_empty()
            || missing.iter().all(|suit| {
                live_tile_count_for_suit_after_discard(&after_discard, table, *suit, discard) > 0
            })
    })
}

fn claim_leaves_unrecoverable_terminal_or_honor(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    kind: ShenyangMahjongMeldKind,
    tile: i32,
    from_position: usize,
) -> bool {
    if win_rule != WIN_RULE_SHENYANG_BASIC {
        return false;
    }

    let (remove_count, claimed_meld) = match kind {
        ShenyangMahjongMeldKind::PENG => (2, claim_peng_meld(tile, from_position)),
        ShenyangMahjongMeldKind::GANG => (3, claim_gang_meld(tile, from_position)),
        ShenyangMahjongMeldKind::CHI => return false,
    };
    let mut next = remove_n_tiles(hand, tile, remove_count);
    if next.len() + remove_count != hand.len() || next.is_empty() {
        return false;
    }
    sort_tiles(&mut next);
    let mut melds = current_melds.to_vec();
    melds.push(claimed_meld);

    !unique_tiles(&next).into_iter().any(|discard| {
        let after_discard = remove_n_tiles(&next, discard, 1);
        has_terminal_or_honor_with_extra(&after_discard, &melds, None)
            || live_terminal_or_honor_count_after_discard(&after_discard, table, discard) > 0
            || pure_one_suit_plan_score_for_context(&after_discard, &melds, table, position) > 0.0
    })
}

fn closed_opponent_threat_discard_bias(
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    own_tile_count: usize,
) -> f64 {
    if table.wall_count > 42 || public_discard_count(table, tile) > 0 {
        return 0.0;
    }
    let exposure_scale = closed_threat_exposure_scale(table, tile);
    if exposure_scale == 0.0 {
        return 0.0;
    }
    let pressure_scale = if is_late_defense_round(table) {
        1.0
    } else {
        0.45
    };

    table
        .seats
        .iter()
        .filter(|(seat_position, seat)| {
            **seat_position != position && is_closed_opponent_threat_candidate(seat)
        })
        .map(|(_, seat)| {
            let base = if is_dragon(tile) {
                -13.0
            } else if is_wind(tile) {
                -12.0
            } else if tile_is_terminal(tile) {
                -9.0
            } else {
                -5.0
            };
            let pair_penalty = if own_tile_count >= 2 {
                if is_honor(tile) || tile_is_terminal(tile) {
                    4.0
                } else {
                    3.0
                }
            } else {
                0.0
            };
            (base - pair_penalty)
                * pressure_scale
                * exposure_scale
                * closed_hand_count_pressure_scale(seat)
                * closed_suit_shedding_scale(seat, tile)
        })
        .sum()
}

fn is_closed_opponent_threat_candidate(seat: &AiSeatView) -> bool {
    !has_open_meld(&seat.melds)
        && (seat.hand_count >= 10 || (seat.hand_count > 0 && has_concealed_gang_meld(&seat.melds)))
}

fn closed_hand_count_pressure_scale(seat: &AiSeatView) -> f64 {
    let concealed_gangs = seat
        .melds
        .iter()
        .filter(|meld| meld.kind == ShenyangMahjongMeldKind::GANG && meld.from_position.is_none())
        .count();
    if concealed_gangs == 0 {
        return 1.0;
    }

    let gang_scale = match concealed_gangs {
        1 => 1.18,
        2 => 1.35,
        _ => 1.55,
    };
    let hand_count_scale = match seat.hand_count {
        0 => 0.0,
        1..=5 => 1.55,
        6..=9 => 1.35,
        10 => 1.18,
        11..=12 => 1.08,
        _ => 1.0,
    };
    f64::max(gang_scale, hand_count_scale)
}

fn closed_suit_shedding_scale(seat: &AiSeatView, tile: i32) -> f64 {
    if !is_suited(tile) {
        return 1.0;
    }
    let discarded_in_suit = seat
        .discards
        .iter()
        .filter(|discard| is_suited(**discard) && tile_suit(**discard) == tile_suit(tile))
        .count();
    match discarded_in_suit {
        0 => {
            let off_suit_discards = seat
                .discards
                .iter()
                .filter(|discard| !is_suited(**discard) || tile_suit(**discard) != tile_suit(tile))
                .count();
            if off_suit_discards >= 4 { 1.25 } else { 1.0 }
        }
        1 => 0.78,
        2 => 0.55,
        3 => 0.25,
        _ => 0.15,
    }
}

fn closed_threat_exposure_scale(table: &AiPublicTable, tile: i32) -> f64 {
    let exposed_meld_count = exposed_meld_tile_count(table, tile);
    match exposed_meld_count {
        0 => 1.0,
        1 => 0.7,
        2 => 0.45,
        3 => 0.15,
        _ => 0.0,
    }
}

fn dominant_pure_suit(hand: &[i32], melds: &[WsShenyangMahjongMeld]) -> Option<i32> {
    let mut suit_counts = [0usize; 3];
    for tile in hand.iter().copied().chain(valid_meld_tiles(melds)) {
        if is_suited(tile) {
            suit_counts[tile_suit(tile) as usize] += 1;
        }
    }
    suit_counts
        .into_iter()
        .enumerate()
        .max_by_key(|(_, count)| *count)
        .and_then(|(suit, count)| (count > 0).then_some(suit as i32))
}

fn dragon_value_bias(hand: &[i32], tile: i32) -> f64 {
    if !is_dragon(tile) {
        return 0.0;
    }
    let pairs = pair_count(hand);
    if pairs >= 4 { 10.4 } else { -3.0 }
}

fn early_piao_candidate_discard_bias(
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

fn has_early_piao_singleton_discard(
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

fn estimate_pressure_for_tile(table: &AiPublicTable, position: usize, tile: i32) -> f64 {
    let mut pressure = 0.0;
    for (seat_position, seat) in &table.seats {
        if *seat_position == position {
            continue;
        }
        let dist = seat.position.abs_diff(position);
        if seat.discards.contains(&tile) {
            pressure += 2.0;
        }
        if seat.melds.len() >= 2 {
            pressure -= 0.7;
        }
        if tile >= 31 && seat.hand_count >= 10 {
            pressure += 0.5 / (dist as f64 + 1.0);
        }
        if tile_is_terminal(tile) && seat.hand_count >= 8 {
            pressure += 0.8 / (dist as f64 + 1.0);
        }
    }
    if table.wall_count < 30 {
        pressure -= 0.3;
    }
    if table.current_position == position && table.dealer_position != position {
        pressure += 0.1;
    }
    pressure
}

fn estimated_four_gui_yi_fan(hand: &[i32], melds: &[WsShenyangMahjongMeld]) -> i32 {
    let mut counts = HashMap::<i32, i32>::new();
    for tile in hand.iter().copied() {
        *counts.entry(tile).or_default() += 1;
    }
    for meld in melds.iter().filter(|meld| is_four_gui_yi_meld(meld)) {
        for tile in meld.tiles.iter().copied() {
            *counts.entry(tile).or_default() += 1;
        }
    }
    counts.into_values().filter(|count| *count == 4).count() as i32
}

fn is_four_gui_yi_meld(meld: &WsShenyangMahjongMeld) -> bool {
    match meld.kind {
        ShenyangMahjongMeldKind::PENG => is_triplet_like_meld(meld),
        ShenyangMahjongMeldKind::CHI => is_sequence_meld(meld),
        ShenyangMahjongMeldKind::GANG => false,
    }
}

fn estimated_concealed_dragon_triplet_fan(hand: &[i32]) -> i32 {
    [35, 36, 37]
        .into_iter()
        .filter(|dragon| hand.iter().filter(|tile| **tile == *dragon).count() >= 3)
        .count() as i32
}

fn estimated_meld_fan(melds: &[WsShenyangMahjongMeld]) -> i32 {
    melds
        .iter()
        .map(|meld| match meld.kind {
            ShenyangMahjongMeldKind::PENG if meld_primary_tile(meld).is_some_and(is_dragon) => 1,
            ShenyangMahjongMeldKind::GANG => {
                let concealed = meld.from_position.is_none();
                match meld_primary_tile(meld) {
                    Some(tile) if is_dragon(tile) && concealed => 4,
                    Some(tile) if is_dragon(tile) => 2,
                    Some(_) if concealed => 2,
                    Some(_) => 1,
                    None => 0,
                }
            }
            _ => 0,
        })
        .sum()
}

fn estimated_visible_bonus_fan(hand: &[i32], melds: &[WsShenyangMahjongMeld]) -> i32 {
    estimated_meld_fan(melds)
        + estimated_concealed_dragon_triplet_fan(hand)
        + estimated_four_gui_yi_fan(hand, melds)
}

fn four_gui_yi_discard_bias(
    hand: &[i32],
    tile: i32,
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> f64 {
    let current_four_gui_yi = estimated_four_gui_yi_fan(hand, melds);
    if current_four_gui_yi <= 0 {
        return 0.0;
    }
    let next = remove_n_tiles(hand, tile, 1);
    if next.len() + 1 != hand.len() {
        return 0.0;
    }
    let after_four_gui_yi = estimated_four_gui_yi_fan(&next, melds);
    if after_four_gui_yi >= current_four_gui_yi {
        return 0.0;
    }

    let fan_loss = (current_four_gui_yi - after_four_gui_yi) as f64;
    if ready_tile_score(&next, melds, table, position, win_rule) > 0.0 {
        return -28.0 * fan_loss;
    }
    if best_ready_score_after_discard(hand, melds, table, position, win_rule) > 0.0 {
        return -18.0 * fan_loss;
    }
    -6.0 * fan_loss
}

fn estimated_visible_fan_without_wait(
    win_hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_rule: i32,
) -> i32 {
    if !is_complete_win_with_melds(win_hand, melds, win_rule) {
        return 0;
    }
    let is_piao = is_piao_hu_win(win_hand, melds);
    let base = if is_piao {
        3
    } else if is_seven_pairs_win(win_hand) || is_pure_one_suit_win(win_hand, melds) {
        4
    } else {
        1
    };
    let shou_ba_yi_fan = if is_piao && melds.len() == 4 && win_hand.len() == 2 {
        1
    } else {
        0
    };
    base + estimated_visible_bonus_fan(win_hand, melds) + shou_ba_yi_fan
}

fn estimated_fan_with_wait(
    win_hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_tile: i32,
    win_rule: i32,
) -> i32 {
    let wait_fan = if is_single_wait_shape_with_rule(win_hand, melds, win_tile, win_rule) {
        single_wait_fan(win_tile)
    } else {
        0
    };
    estimated_visible_fan_without_wait(win_hand, melds, win_rule) + wait_fan
}

fn single_wait_fan(win_tile: i32) -> i32 {
    1 + if tile_is_terminal(win_tile) || is_honor(win_tile) {
        1
    } else {
        0
    }
}

fn pressured_open_wait_scale(
    table: &AiPublicTable,
    position: usize,
    melds: &[WsShenyangMahjongMeld],
) -> f64 {
    if table.wall_count > 42 || !has_open_meld(melds) {
        return 1.0;
    }
    let open_opponents = table
        .seats
        .iter()
        .filter(|(seat_position, seat)| **seat_position != position && has_open_meld(&seat.melds))
        .count();
    if open_opponents == 0 {
        return 1.0;
    }
    let own_open_melds = melds.iter().filter(|meld| is_open_meld(meld)).count();
    if own_open_melds >= 2 { 0.2 } else { 0.45 }
}

fn fan_wait_bias(
    win_hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    win_tile: i32,
    remaining: i32,
) -> f64 {
    if table.dealer_position == position
        || is_late_defense_round(table)
        || !is_single_wait_shape_with_rule(win_hand, melds, win_tile, win_rule)
    {
        return 0.0;
    }
    if remaining <= 1 {
        return 0.0;
    }
    let wait_fan = single_wait_fan(win_tile);
    if let Some(max_fan) = table.max_fan {
        let visible_fan = estimated_visible_fan_without_wait(win_hand, melds, win_rule);
        if visible_fan >= max_fan {
            return 0.0;
        }
        if visible_fan + wait_fan >= max_fan {
            return if remaining >= 3 { 14.0 } else { 0.0 };
        }
    }

    let terminal_or_honor_bonus = if tile_is_terminal(win_tile) || is_honor(win_tile) {
        14.0
    } else {
        0.0
    };
    let live_wait_scale = if remaining == 2 { 0.45 } else { 1.0 };
    (62.0 + terminal_or_honor_bonus)
        * live_wait_scale
        * pressured_open_wait_scale(table, position, melds)
}

fn hand_progress_score(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> f64 {
    hand_power(hand)
        + melds.len() as f64 * 10.0
        + ready_tile_score(hand, melds, table, position, win_rule)
        + one_step_wait_potential(hand, melds, table, position, win_rule)
        + seven_pairs_plan_score(hand, melds, table, position, win_rule)
        + piao_plan_score_for_context(hand, melds, table, position)
        + shenyang_rule_progress_score(hand, melds, table, position, win_rule)
}

fn honor_discard_bias(hand: &[i32], tile: i32) -> f64 {
    if is_wind(tile) {
        8.0
    } else if is_dragon(tile) {
        4.8 + dragon_value_bias(hand, tile)
    } else {
        6.0
    }
}

fn is_late_defense_round(table: &AiPublicTable) -> bool {
    table.wall_count <= 20
}

fn is_late_round(table: &AiPublicTable) -> bool {
    table.wall_count <= 42
}

fn is_mid_round(table: &AiPublicTable) -> bool {
    table.wall_count <= 60
}

fn is_mid_broken_hand_defense_round(table: &AiPublicTable) -> bool {
    table.wall_count <= 52
}

fn is_mid_opening_round(table: &AiPublicTable) -> bool {
    table.wall_count <= 52
}

fn is_main_pure_suit_tile(hand: &[i32], melds: &[WsShenyangMahjongMeld], tile: i32) -> bool {
    dominant_pure_suit(hand, melds).is_some_and(|suit| is_suited(tile) && tile_suit(tile) == suit)
}

fn isolated_suited_singleton_discard_bias(tile: i32) -> f64 {
    if !is_suited(tile) {
        return 4.0;
    }
    match tile_rank(tile) {
        2 | 8 => 4.6,
        3 | 7 => 4.25,
        _ => 4.0,
    }
}

fn late_defense_discard_bias(table: &AiPublicTable, position: usize, tile: i32) -> f64 {
    if !is_late_defense_round(table) {
        return 0.0;
    }
    let public_discards = public_discard_count(table, tile);
    if public_discards > 0 {
        let honor_bonus = if is_honor(tile) { 26.0 } else { 0.0 };
        let suited_shape_bonus = if is_suited(tile) {
            if tile_is_terminal(tile) { -1.0 } else { 2.0 }
        } else {
            0.0
        };
        let multi_seat_bonus = public_discard_seat_count(table, tile) as f64 * 3.0;
        return 28.0
            + public_discards as f64 * 6.0
            + honor_bonus
            + suited_shape_bonus
            + multi_seat_bonus
            + own_previous_discard_safety_bias(table, position, tile);
    }
    if is_wind(tile) {
        -4.0
    } else if is_dragon(tile) {
        -8.0
    } else if tile_is_terminal(tile) {
        -14.0
    } else {
        -22.0
    }
}

fn late_defense_tile_safety_score(
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    own_tile_count: usize,
) -> f64 {
    late_defense_discard_bias(table, position, tile)
        + late_defense_exposed_meld_bias(table, tile)
        + late_defense_own_tile_shape_bias(table, tile, own_tile_count)
        + opponent_threat_discard_bias(table, position, tile, own_tile_count)
        + pure_one_suit_threat_discard_bias(table, position, tile, own_tile_count)
        + opponent_missing_suit_safety_bias(table, position, tile)
        + closed_opponent_threat_discard_bias(table, position, tile, own_tile_count)
        + estimate_pressure_for_tile(table, position, tile)
}

fn late_defense_exposed_meld_bias(table: &AiPublicTable, tile: i32) -> f64 {
    if !is_late_defense_round(table) || public_discard_count(table, tile) > 0 {
        return 0.0;
    }
    match exposed_meld_tile_count(table, tile) {
        0 => 0.0,
        1 => 5.0,
        2 => 20.0,
        _ => 28.0,
    }
}

fn late_defense_own_tile_shape_bias(
    table: &AiPublicTable,
    tile: i32,
    own_tile_count: usize,
) -> f64 {
    if !is_late_defense_round(table) || public_discard_count(table, tile) > 0 {
        return 0.0;
    }
    if own_tile_count <= 1 {
        return 0.0;
    }
    if is_dragon(tile) {
        -8.0
    } else if is_wind(tile) || tile_is_terminal(tile) {
        -5.0
    } else {
        -2.0
    }
}

fn public_defense_tile_safety_score(
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    own_tile_count: usize,
) -> f64 {
    late_defense_tile_safety_score(table, position, tile, own_tile_count)
        + public_defense_own_tile_shape_bias(tile, own_tile_count)
        + mid_round_public_discard_bias(table, position, tile)
        + mid_round_open_meld_safety_bias(table, tile)
        + mid_broken_opponent_missing_suit_safety_bias(table, position, tile)
}

fn basic_heng_recovery_public_defense_bias(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    tile: i32,
    win_rule: i32,
) -> f64 {
    if loses_basic_heng_recovery_after_discard(hand, melds, table, tile, win_rule) {
        -22.0
    } else {
        0.0
    }
}

fn public_defense_own_tile_shape_bias(tile: i32, own_tile_count: usize) -> f64 {
    match own_tile_count {
        0 | 1 => 0.0,
        2 if is_dragon(tile) => -18.0,
        2 if is_wind(tile) || tile_is_terminal(tile) => -12.0,
        2 => -8.0,
        _ if is_dragon(tile) => -28.0,
        _ if is_wind(tile) || tile_is_terminal(tile) => -20.0,
        _ => -14.0,
    }
}

fn mid_round_public_discard_bias(table: &AiPublicTable, position: usize, tile: i32) -> f64 {
    if !is_mid_round(table) || is_late_defense_round(table) {
        return 0.0;
    }
    let public_discards = public_discard_count(table, tile);
    if public_discards == 0 {
        return 0.0;
    }
    let shape_bonus = if is_honor(tile) {
        16.0
    } else if tile_is_terminal(tile) {
        1.5
    } else {
        2.0
    };
    let multi_seat_bonus = public_discard_seat_count(table, tile) as f64 * 2.0;
    9.0 + public_discards as f64 * 4.0
        + shape_bonus
        + multi_seat_bonus
        + own_previous_discard_count(table, position, tile) as f64 * 4.0
}

fn mid_round_open_meld_safety_bias(table: &AiPublicTable, tile: i32) -> f64 {
    if !is_mid_round(table) || is_late_defense_round(table) || public_discard_count(table, tile) > 0
    {
        return 0.0;
    }
    match open_meld_tile_count(table, tile) {
        0 | 1 => 0.0,
        2 => 6.0,
        _ => 20.0,
    }
}

fn mid_round_live_honor_risk_bias(
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    own_tile_count: usize,
) -> f64 {
    if table.wall_count > 60
        || is_late_defense_round(table)
        || own_tile_count != 1
        || !is_honor(tile)
    {
        return 0.0;
    }
    if public_discard_count(table, tile) > 0 {
        return 0.0;
    }
    if !is_dragon(tile) {
        return -5.0;
    }
    -18.0 - open_opponent_live_dragon_risk(table, position, tile)
}

fn open_opponent_live_dragon_risk(table: &AiPublicTable, position: usize, tile: i32) -> f64 {
    if !is_dragon(tile) {
        return 0.0;
    }
    let open_opponents = table
        .seats
        .iter()
        .filter(|(seat_position, seat)| {
            **seat_position != position
                && has_open_meld(&seat.melds)
                && !seat.discards.contains(&tile)
                && !seat_has_open_meld_tile(seat, tile)
        })
        .count();
    if open_opponents == 0 {
        return 0.0;
    }
    let open_risk = (open_opponents as f64 * 4.0).min(12.0);
    let late_round_risk = if is_late_round(table) { 4.0 } else { 0.0 };
    (open_risk + late_round_risk) * live_risk_exposure_scale(table, tile)
}

fn mid_round_live_suited_risk_bias(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    own_tile_count: usize,
    win_rule: i32,
) -> f64 {
    if table.wall_count > 60
        || is_late_defense_round(table)
        || own_tile_count != 1
        || !is_suited(tile)
        || public_discard_count(table, tile) > 0
        || should_keep_pairs_for_seven_pairs_discard(hand, melds, table, position, win_rule)
    {
        return 0.0;
    }
    let base = if tile_is_terminal(tile) { 7.0 } else { 10.0 };
    -(base
        + open_opponent_live_suited_risk(table, position, tile)
        + own_open_live_suited_pressure(melds, table, position, tile))
}

fn open_opponent_live_suited_risk(table: &AiPublicTable, position: usize, tile: i32) -> f64 {
    if !is_suited(tile) {
        return 0.0;
    }
    let open_opponents = table
        .seats
        .iter()
        .filter(|(seat_position, seat)| {
            **seat_position != position
                && has_open_meld(&seat.melds)
                && !seat.discards.contains(&tile)
                && !seat_has_open_meld_tile(seat, tile)
        })
        .count();
    if open_opponents == 0 {
        return 0.0;
    }
    let per_open = if tile_is_terminal(tile) { 2.5 } else { 3.5 };
    let cap = if tile_is_terminal(tile) { 7.5 } else { 10.5 };
    let open_risk = (open_opponents as f64 * per_open).min(cap);
    let late_round_risk = if is_late_round(table) { 2.5 } else { 0.0 };
    (open_risk + late_round_risk) * live_risk_exposure_scale(table, tile)
}

fn live_risk_exposure_scale(table: &AiPublicTable, tile: i32) -> f64 {
    match exposed_meld_tile_count(table, tile) {
        0 => 1.0,
        1 => 0.8,
        2 => 0.55,
        3 => 0.25,
        _ => 0.0,
    }
}

fn own_open_live_suited_pressure(
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
) -> f64 {
    if table.wall_count > 42 || !is_suited(tile) || public_discard_count(table, tile) > 0 {
        return 0.0;
    }
    let own_open_melds = melds.iter().filter(|meld| is_open_meld(meld)).count();
    if own_open_melds < 2 {
        return 0.0;
    }
    if !open_opponent_exists_for_tile(table, position, tile) {
        return 0.0;
    }
    if tile_is_terminal(tile) { 36.0 } else { 24.0 }
}

fn own_open_public_safety_bias(
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
) -> f64 {
    if table.wall_count > 42 || is_late_defense_round(table) {
        return 0.0;
    }
    let own_open_melds = melds.iter().filter(|meld| is_open_meld(meld)).count();
    if own_open_melds < 2 || !open_opponent_exists_for_tile(table, position, tile) {
        return 0.0;
    }
    let public_discards = public_discard_count(table, tile);
    if public_discards == 0 {
        return 0.0;
    }
    let shape_bonus = if is_honor(tile) {
        8.0
    } else if tile_is_terminal(tile) {
        3.0
    } else {
        6.0
    };
    18.0 + public_discards as f64 * 6.0 + shape_bonus
}

fn one_step_wait_potential(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> f64 {
    if hand.len() % 3 != 1 || ready_tile_score(hand, melds, table, position, win_rule) > 0.0 {
        return 0.0;
    }
    let open_basic_route_foundation = win_rule == WIN_RULE_SHENYANG_BASIC
        && has_open_meld(melds)
        && missing_suits(hand, melds).is_empty()
        && has_terminal_or_honor_with_extra(hand, melds, None)
        && has_triplet_or_dragon_pair(hand, melds);
    if hand_power(hand) < 50.0 && pair_count(hand) < 4 && !open_basic_route_foundation {
        return 0.0;
    }

    let mut score = 0.0;
    for draw_tile in SHENYANG_MAHJONG_TILE_KINDS {
        let remaining = remaining_tile_count(hand, table, position, draw_tile);
        if remaining <= 0 {
            continue;
        }
        let mut after_draw = hand.to_vec();
        after_draw.push(draw_tile);
        after_draw.sort_unstable();
        let mut best_ready = 0.0;
        for discard_tile in unique_tiles(&after_draw) {
            let mut next = after_draw.clone();
            if let Some(index) = next.iter().position(|item| *item == discard_tile) {
                next.remove(index);
            }
            let ready = ready_tile_score(&next, melds, table, position, win_rule);
            if ready > best_ready {
                best_ready = ready;
            }
        }
        if best_ready > 0.0 {
            score += remaining as f64 * (1.2 + best_ready * 0.025);
        }
    }
    score
}

fn opponent_missing_suit_safety_bias(table: &AiPublicTable, position: usize, tile: i32) -> f64 {
    if !is_late_defense_round(table) || !is_suited(tile) {
        return 0.0;
    }
    opponent_missing_suit_safety_read(table, position, tile)
}

fn mid_broken_opponent_missing_suit_safety_bias(
    table: &AiPublicTable,
    position: usize,
    tile: i32,
) -> f64 {
    if !is_mid_broken_hand_defense_round(table) || is_late_defense_round(table) || !is_suited(tile)
    {
        return 0.0;
    }
    opponent_missing_suit_safety_read(table, position, tile) * 0.7
}

fn opponent_missing_suit_safety_read(table: &AiPublicTable, position: usize, tile: i32) -> f64 {
    let suit = tile_suit(tile);
    if table.seats.iter().any(|(seat_position, seat)| {
        *seat_position != position && piao_threat_needs_suit(seat, suit)
    }) {
        return 0.0;
    }
    if closed_opponent_may_need_suit(table, position, suit) {
        return 0.0;
    }
    table
        .seats
        .iter()
        .filter(|(seat_position, _)| **seat_position != position)
        .map(|(_, seat)| {
            let discarded_in_suit = seat
                .discards
                .iter()
                .filter(|discard| is_suited(**discard) && tile_suit(**discard) == suit)
                .count();
            let exposed_in_suit = seat.melds.iter().any(|meld| {
                if !is_valid_meld(meld) {
                    return false;
                }
                meld.tiles
                    .iter()
                    .any(|meld_tile| is_suited(*meld_tile) && tile_suit(*meld_tile) == suit)
            });
            if exposed_in_suit {
                0.0
            } else if discarded_in_suit >= 3 {
                12.0 + (discarded_in_suit - 3) as f64 * 2.0
            } else if discarded_in_suit >= 2 {
                5.0
            } else {
                0.0
            }
        })
        .sum()
}

fn closed_opponent_may_need_suit(table: &AiPublicTable, position: usize, suit: i32) -> bool {
    table.seats.iter().any(|(seat_position, seat)| {
        *seat_position != position
            && !has_open_meld(&seat.melds)
            && (seat.hand_count >= 13
                || (seat.hand_count > 0 && has_concealed_gang_meld(&seat.melds)))
            && seat
                .discards
                .iter()
                .filter(|discard| is_suited(**discard) && tile_suit(**discard) == suit)
                .count()
                < 2
    })
}

fn piao_threat_needs_suit(seat: &AiSeatView, suit: i32) -> bool {
    piao_threat_level(&seat.melds) >= 3
        && !piao_threat_cannot_satisfy_three_suits(&seat.melds, seat.hand_count)
        && piao_missing_suits_from_melds(&seat.melds).contains(&suit)
}

fn opponent_threat_discard_bias(
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    own_tile_count: usize,
) -> f64 {
    let mut bias = 0.0;
    for (seat_position, seat) in &table.seats {
        if *seat_position == position {
            continue;
        }
        let threat_level = piao_threat_level(&seat.melds);
        if threat_level < 3 {
            continue;
        }
        if piao_threat_cannot_satisfy_three_suits(&seat.melds, seat.hand_count) {
            continue;
        }
        if seat.discards.contains(&tile) {
            bias += 4.5;
            continue;
        }
        let exposure_scale = piao_threat_exposure_scale(table, tile);
        let terminal_or_honor_need_penalty = piao_terminal_or_honor_need_penalty(&seat.melds, tile);
        if threat_level >= 4 && seat.hand_count <= 2 {
            let public_discount = (public_discard_count(table, tile) as f64 * 10.0
                + exposed_meld_tile_count(table, tile) as f64 * 8.0)
                .min(48.0);
            let single_wait_penalty = if is_dragon(tile) {
                86.0
            } else if is_honor(tile) || tile_is_terminal(tile) {
                80.0
            } else {
                72.0
            };
            let pair_penalty = piao_threat_pair_penalty(tile, own_tile_count);
            let late_multiplier = if is_late_round(table) { 1.25 } else { 1.0 };
            bias -= ((single_wait_penalty + pair_penalty + terminal_or_honor_need_penalty)
                - public_discount)
                .max(10.0)
                * late_multiplier;
            continue;
        }
        let piao_wait_suit_penalty = if is_suited(tile)
            && piao_missing_suits_from_melds(&seat.melds).contains(&tile_suit(tile))
        {
            if own_tile_count >= 2 { 7.0 } else { 5.0 }
        } else {
            0.0
        };
        let live_tile_penalty = if is_dragon(tile) {
            7.0
        } else if is_wind(tile) {
            5.0
        } else if tile_is_terminal(tile) {
            4.0
        } else {
            5.5
        };
        let pair_penalty = piao_threat_pair_penalty(tile, own_tile_count);
        let late_multiplier = if is_late_round(table) { 1.35 } else { 1.0 };
        bias -= (live_tile_penalty
            + pair_penalty
            + piao_wait_suit_penalty
            + terminal_or_honor_need_penalty)
            * late_multiplier
            * exposure_scale;
    }
    bias
}

fn piao_terminal_or_honor_need_penalty(melds: &[WsShenyangMahjongMeld], tile: i32) -> f64 {
    if !(is_honor(tile) || tile_is_terminal(tile))
        || !piao_needs_terminal_or_honor_from_melds(melds)
    {
        return 0.0;
    }
    if is_dragon(tile) {
        8.0
    } else if is_wind(tile) {
        7.0
    } else {
        6.0
    }
}

fn piao_needs_terminal_or_honor_from_melds(melds: &[WsShenyangMahjongMeld]) -> bool {
    piao_threat_level(melds) >= 3
        && !melds
            .iter()
            .filter(|meld| is_triplet_like_meld(meld))
            .flat_map(|meld| meld.tiles.iter().copied())
            .any(|tile| is_honor(tile) || tile_is_terminal(tile))
}

fn piao_threat_cannot_satisfy_three_suits(
    melds: &[WsShenyangMahjongMeld],
    hand_count: usize,
) -> bool {
    piao_threat_level(melds) >= 4
        && hand_count <= 2
        && piao_missing_suits_from_melds(melds).len() >= 2
}

fn piao_threat_exposure_scale(table: &AiPublicTable, tile: i32) -> f64 {
    match exposed_meld_tile_count(table, tile) {
        0 => 1.0,
        1 => 0.8,
        2 => 0.55,
        _ => 0.25,
    }
}

fn piao_threat_pair_penalty(tile: i32, own_tile_count: usize) -> f64 {
    if own_tile_count < 2 {
        return 0.0;
    }
    if is_honor(tile) || tile_is_terminal(tile) {
        6.0
    } else {
        4.0
    }
}

fn pure_one_suit_threat_discard_bias(
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    own_tile_count: usize,
) -> f64 {
    if table.wall_count > 52 || !is_suited(tile) || public_discard_count(table, tile) > 0 {
        return 0.0;
    }
    let suit = tile_suit(tile);
    table
        .seats
        .iter()
        .filter(|(seat_position, _)| **seat_position != position)
        .filter_map(|(_, seat)| {
            let (threat_suit, open_melds) = pure_one_suit_threat_suit(seat)?;
            (threat_suit == suit && !seat_has_open_meld_tile(seat, tile)).then_some((
                seat,
                open_melds,
                threat_suit,
            ))
        })
        .map(|(seat, open_melds, threat_suit)| {
            let base = if tile_is_terminal(tile) { 7.0 } else { 10.0 };
            let pair_penalty = if own_tile_count >= 2 {
                if tile_is_terminal(tile) { 5.0 } else { 7.0 }
            } else {
                0.0
            };
            let meld_pressure = pure_one_suit_threat_meld_pressure(open_melds);
            let late_pressure = if table.wall_count <= 20 {
                1.35
            } else if table.wall_count <= 42 {
                1.15
            } else {
                1.0
            };
            let hand_pressure = if seat.hand_count <= 4 {
                1.3
            } else if seat.hand_count <= 7 {
                1.15
            } else {
                1.0
            };
            let exposed_discount = (exposed_meld_tile_count(table, tile) as f64 * 4.0).min(8.0);
            let discard_scale = pure_one_suit_threat_discard_scale(seat, threat_suit);
            -((base + pair_penalty) * meld_pressure * late_pressure * hand_pressure * discard_scale
                - exposed_discount)
                .max(2.0)
        })
        .sum()
}

fn pure_one_suit_threat_discard_scale(seat: &AiSeatView, threat_suit: i32) -> f64 {
    let same_suit_discards = seat
        .discards
        .iter()
        .filter(|discard| is_suited(**discard) && tile_suit(**discard) == threat_suit)
        .count();
    match same_suit_discards {
        0 => {
            let off_suit_discards = seat
                .discards
                .iter()
                .filter(|discard| !is_suited(**discard) || tile_suit(**discard) != threat_suit)
                .count();
            if off_suit_discards >= 4 {
                1.25
            } else if off_suit_discards >= 2 {
                1.1
            } else {
                1.0
            }
        }
        1 => 0.7,
        2 => 0.45,
        _ => 0.25,
    }
}

fn pure_one_suit_threat_meld_pressure(open_melds: usize) -> f64 {
    if open_melds <= 1 {
        0.55
    } else {
        (open_melds as f64 - 1.0).min(2.0)
    }
}

fn pure_one_suit_threat_suit(seat: &AiSeatView) -> Option<(i32, usize)> {
    let mut open_meld_count = 0usize;
    let mut threat_suit = None;
    for meld in seat.melds.iter().filter(|meld| is_open_meld(meld)) {
        open_meld_count += 1;
        for tile in meld.tiles.iter().copied() {
            if !is_suited(tile) {
                return None;
            }
            let suit = tile_suit(tile);
            match threat_suit {
                Some(current) if current != suit => return None,
                Some(_) => {}
                None => threat_suit = Some(suit),
            }
        }
    }
    if open_meld_count == 0 {
        return pure_one_suit_closed_discard_threat_suit(seat).map(|suit| (suit, 0));
    }
    threat_suit.and_then(|suit| {
        (open_meld_count >= 2 || pure_one_suit_single_meld_discard_evidence(seat, suit))
            .then_some((suit, open_meld_count))
    })
}

fn pure_one_suit_closed_discard_threat_suit(seat: &AiSeatView) -> Option<i32> {
    if has_open_meld(&seat.melds) || seat.discards.len() < 5 {
        return None;
    }

    let mut suit_discards = [0usize; 3];
    for discard in seat
        .discards
        .iter()
        .copied()
        .filter(|tile| is_suited(*tile))
    {
        suit_discards[tile_suit(discard) as usize] += 1;
    }
    let untouched_suits = suit_discards
        .iter()
        .enumerate()
        .filter_map(|(suit, count)| (*count == 0).then_some(suit as i32))
        .collect::<Vec<_>>();
    if untouched_suits.len() != 1 {
        return None;
    }
    let threat_suit = untouched_suits[0];
    let off_suit_discards = seat
        .discards
        .iter()
        .filter(|discard| !is_suited(**discard) || tile_suit(**discard) != threat_suit)
        .count();
    (off_suit_discards >= 5).then_some(threat_suit)
}

fn pure_one_suit_single_meld_discard_evidence(seat: &AiSeatView, threat_suit: i32) -> bool {
    let same_suit_discards = seat
        .discards
        .iter()
        .filter(|discard| is_suited(**discard) && tile_suit(**discard) == threat_suit)
        .count();
    let off_suit_discards = seat
        .discards
        .iter()
        .filter(|discard| !is_suited(**discard) || tile_suit(**discard) != threat_suit)
        .count();
    same_suit_discards == 0 && off_suit_discards >= 4
}

fn pair_discard_bias(hand: &[i32]) -> f64 {
    if pair_count(hand) >= 4 { -4.4 } else { -1.8 }
}

fn basic_heng_seed_discard_bias(
    hand: &[i32],
    tile: i32,
    melds: &[WsShenyangMahjongMeld],
    win_rule: i32,
) -> f64 {
    if win_rule != WIN_RULE_SHENYANG_BASIC || has_triplet_or_dragon_pair(hand, melds) {
        return 0.0;
    }
    let count = hand.iter().filter(|item| **item == tile).count();
    if is_dragon(tile) && count == 1 {
        -7.0
    } else if count == 2 {
        -4.0
    } else {
        0.0
    }
}

fn piao_discard_bias(
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

fn piao_pair_liveness_discard_bias(
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

fn piao_dragon_pair_discard_bias(
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

fn piao_committed_group_count(hand: &[i32], melds: &[WsShenyangMahjongMeld]) -> usize {
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

fn piao_plan_score(hand: &[i32], melds: &[WsShenyangMahjongMeld]) -> f64 {
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

fn piao_plan_score_for_context(
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

fn piao_plan_is_capped(table: &AiPublicTable) -> bool {
    table.max_fan.is_some_and(|max_fan| max_fan <= 1)
}

fn has_piao_route_basics(hand: &[i32], melds: &[WsShenyangMahjongMeld]) -> bool {
    missing_suits(hand, melds).is_empty() && has_terminal_or_honor_with_extra(hand, melds, None)
}

fn piao_threat_level(melds: &[WsShenyangMahjongMeld]) -> usize {
    if melds.iter().any(is_sequence_meld) {
        return 0;
    }
    melds
        .iter()
        .filter(|meld| is_triplet_like_meld(meld))
        .count()
}

fn choose_piao_single_wait_discard(
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

fn piao_single_wait_tile_score(
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

fn piao_missing_suits_from_melds(melds: &[WsShenyangMahjongMeld]) -> Vec<i32> {
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

fn own_previous_discard_safety_bias(table: &AiPublicTable, position: usize, tile: i32) -> f64 {
    if !is_late_defense_round(table) {
        return 0.0;
    }
    own_previous_discard_count(table, position, tile) as f64 * 4.0
}

fn pure_one_suit_discard_bias(
    hand: &[i32],
    tile: i32,
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
) -> f64 {
    let current_score = pure_one_suit_plan_score_for_context(hand, melds, table, position);
    let after_discard = remove_n_tiles(hand, tile, 1);
    let after_score = if after_discard.len() + 1 == hand.len() {
        pure_one_suit_plan_score_for_context(&after_discard, melds, table, position)
    } else {
        0.0
    };
    if current_score <= 0.0 && after_score <= 0.0 {
        return 0.0;
    }
    if is_honor(tile) {
        return 72.0;
    }
    if is_main_pure_suit_tile(hand, melds, tile) {
        -26.0
    } else {
        64.0
    }
}

fn pure_one_suit_plan_score(hand: &[i32], melds: &[WsShenyangMahjongMeld]) -> f64 {
    let Some((main_suit, main_count, blockers)) = pure_one_suit_shape(hand, melds) else {
        return 0.0;
    };
    if melds.iter().any(|meld| {
        if !is_valid_meld(meld) {
            return false;
        }
        meld.tiles
            .iter()
            .any(|tile| !is_suited(*tile) || tile_suit(*tile) != main_suit)
    }) {
        return 0.0;
    }
    if main_count >= 8 && blockers <= 6 {
        12.0 + main_count as f64 * 2.0 - blockers as f64 * 3.0
    } else {
        0.0
    }
}

fn pure_one_suit_plan_score_for_context(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
) -> f64 {
    let score = pure_one_suit_plan_score(hand, melds);
    if score <= 0.0 || table.dealer_position != position {
        if score > 0.0
            && table.max_fan.is_some_and(|max_fan| max_fan <= 1)
            && missing_suits(hand, melds).is_empty()
        {
            return 0.0;
        }
        return score;
    }
    if table.max_fan.is_some_and(|max_fan| max_fan <= 1) && missing_suits(hand, melds).is_empty() {
        return 0.0;
    }
    pure_one_suit_shape(hand, melds)
        .filter(|(_, main_count, blockers)| *main_count >= 11 && *blockers <= 2)
        .map(|_| score)
        .unwrap_or(0.0)
}

fn pure_one_suit_shape(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
) -> Option<(i32, usize, usize)> {
    let all_tiles = hand
        .iter()
        .copied()
        .chain(valid_meld_tiles(melds))
        .collect::<Vec<_>>();
    let main_suit = dominant_pure_suit(hand, melds)?;
    let main_count = all_tiles
        .iter()
        .filter(|tile| is_suited(**tile) && tile_suit(**tile) == main_suit)
        .count();
    let blockers = all_tiles.len().saturating_sub(main_count);
    Some((main_suit, main_count, blockers))
}

fn ready_hand_visible_fan_reaches_cap(
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

fn ready_tile_score(
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

fn ready_has_pure_one_suit_win(
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

fn ready_visible_fan_reaches_cap(
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

fn self_gang_score(
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
    if pure_one_suit_score > 0.0 {
        if is_honor(tile) || !is_main_pure_suit_tile(hand, melds, tile) || !is_ready {
            return f64::NEG_INFINITY;
        }
    }
    if is_ready && ready_visible_fan_reaches_cap(hand, melds, table, position, win_rule) {
        return f64::NEG_INFINITY;
    }
    if !is_ready && table.max_fan.is_some_and(|max_fan| max_fan <= 1) {
        return f64::NEG_INFINITY;
    }
    if !is_added_gang
        && !is_ready
        && !is_dragon(tile)
        && piao_plan_score_for_context(hand, melds, table, position) >= 22.0
    {
        return f64::NEG_INFINITY;
    }
    if !is_added_gang
        && !is_ready
        && win_rule == WIN_RULE_SHENYANG_BASIC
        && (!is_dragon(tile) || !has_open_meld(melds))
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
    if pure_one_suit_score > 0.0 && after_ready_score <= 0.0 {
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
        let keeps_pure_one_suit_ready = pure_one_suit_score > 0.0
            && ready_has_pure_one_suit_win(&next, &next_melds, table, position, win_rule);
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
    if piao_plan_score_for_context(hand, melds, table, position) >= 22.0 {
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

fn seven_pairs_plan_discard_bias(
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

fn seven_pairs_pair_liveness_discard_bias(
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

fn should_keep_pairs_for_seven_pairs_discard(
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

fn should_chase_basic_missing_suit_four_pairs(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_rule: i32,
) -> bool {
    win_rule == WIN_RULE_SHENYANG_BASIC
        && pair_count(hand) == 4
        && melds.is_empty()
        && !missing_suits(hand, melds).is_empty()
}

fn should_chase_basic_missing_suit_pairs(
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

fn has_basic_normal_route_foundation(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_rule: i32,
) -> bool {
    win_rule == WIN_RULE_SHENYANG_BASIC
        && missing_suits(hand, melds).is_empty()
        && has_terminal_or_honor_with_extra(hand, melds, None)
        && has_triplet_or_dragon_pair(hand, melds)
}

fn should_lock_seven_pairs_plan(
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

fn seven_pairs_plan_score(
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

fn seven_pairs_wait_discard_bias(
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

fn choose_seven_pairs_wait_discard(
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
                seven_pairs_wait_tile_score(wait_tile, &next, table, position)
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

fn wait_setting_discard_safety_adjustment(
    table: &AiPublicTable,
    position: usize,
    discard_tile: i32,
    own_tile_count: usize,
) -> f64 {
    let piao_threat = opponent_threat_discard_bias(table, position, discard_tile, own_tile_count);
    let pure_one_suit_threat =
        pure_one_suit_threat_discard_bias(table, position, discard_tile, own_tile_count);
    let safety = late_defense_tile_safety_score(table, position, discard_tile, own_tile_count)
        + mid_round_public_discard_bias(table, position, discard_tile)
        + mid_round_open_meld_safety_bias(table, discard_tile)
        + mid_broken_opponent_missing_suit_safety_bias(table, position, discard_tile);
    safety.clamp(-36.0, 36.0) * 0.6
        + piao_threat.min(0.0) * 1.5
        + pure_one_suit_threat.min(0.0) * 1.0
}

fn seven_pairs_wait_tile_score(
    wait_tile: i32,
    hand_after_discard: &[i32],
    table: &AiPublicTable,
    position: usize,
) -> f64 {
    let public_discards = public_discard_count(table, wait_tile) as f64;
    let remaining = remaining_tile_count(hand_after_discard, table, position, wait_tile) as f64;
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

fn seven_pairs_wait_shape_tiebreaker(wait_tile: i32) -> f64 {
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

fn complete_sequence_discard_bias(
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

fn incomplete_sequence_discard_bias(
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

fn seven_pairs_regular_wait_reaches_cap(table: &AiPublicTable) -> bool {
    const SEVEN_PAIRS_VISIBLE_FAN: i32 = 4;
    const REGULAR_SINGLE_WAIT_FAN: i32 = 1;
    table.max_fan.is_some_and(|max_fan| {
        max_fan > 0 && SEVEN_PAIRS_VISIBLE_FAN + REGULAR_SINGLE_WAIT_FAN >= max_fan
    })
}

fn shenyang_rule_progress_score(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> f64 {
    if win_rule != WIN_RULE_SHENYANG_BASIC
        || should_lock_seven_pairs_plan(hand, melds, table, position, win_rule)
    {
        return 0.0;
    }
    let pure_score = pure_one_suit_plan_score_for_context(hand, melds, table, position);
    if pure_score > 0.0 {
        return pure_score;
    }
    let mut score = 0.0;
    let suits = suit_presence(hand, melds);
    let suit_count = suits.into_iter().filter(|present| *present).count();
    score += match suit_count {
        3 => 10.0,
        2 => -6.0,
        1 => -14.0,
        _ => -20.0,
    };
    if has_open_meld(melds) {
        score += 9.0;
    } else {
        score -= 8.0;
    }
    if has_terminal_or_honor_with_extra(hand, melds, None) {
        score += 7.0;
    } else {
        score -= 10.0;
    }
    if has_triplet_or_dragon_pair(hand, melds) {
        score += 8.0;
    } else {
        score -= 5.0;
    }
    score
}

fn should_claim_chi_to_open_broken_hand_for_defense(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> bool {
    if win_rule == WIN_RULE_SHENYANG_BASIC
        || has_open_meld(melds)
        || table.dealer_position == position
        || !is_mid_broken_hand_defense_round(table)
        || should_preserve_seven_pairs_plan_for_context(hand, melds, table, position, win_rule)
        || pure_one_suit_plan_score_for_context(hand, melds, table, position) > 0.0
        || piao_plan_score_for_context(hand, melds, table, position) >= 22.0
    {
        return false;
    }
    ready_tile_score(hand, melds, table, position, win_rule) <= 0.0
        && one_step_wait_potential(hand, melds, table, position, win_rule) <= 0.0
        && hand_power(hand) < 18.0
}

fn should_claim_gang_from_discard(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    tile: i32,
    from_position: usize,
) -> bool {
    if ready_visible_fan_reaches_cap(hand, current_melds, table, position, win_rule) {
        return false;
    }
    let current_ready_score = ready_tile_score(hand, current_melds, table, position, win_rule);
    let reaches_ready = claim_gang_from_discard_reaches_ready(
        hand,
        current_melds,
        table,
        position,
        win_rule,
        tile,
        from_position,
    );
    if current_ready_score > 0.0 {
        return reaches_ready;
    }
    if is_dragon(tile) {
        if table.max_fan.is_some_and(|max_fan| max_fan <= 1) {
            return reaches_ready;
        }
        return true;
    }
    if should_claim_opening_gang_for_basic_hand(
        hand,
        current_melds,
        table,
        position,
        win_rule,
        tile,
    ) {
        return true;
    }
    if should_open_broken_closed_hand_for_defense(hand, current_melds, table, position, win_rule) {
        return true;
    }

    if piao_plan_score_for_context(hand, current_melds, table, position) >= 22.0 {
        return reaches_ready;
    }
    reaches_ready
}

fn should_claim_opening_gang_for_basic_hand(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    tile: i32,
) -> bool {
    win_rule == WIN_RULE_SHENYANG_BASIC
        && !has_open_meld(current_melds)
        && can_gang(hand, tile)
        && !table.max_fan.is_some_and(|max_fan| max_fan <= 1)
        && !is_closed_early_piao_candidate(hand, current_melds, table, position)
        && !should_preserve_seven_pairs_plan_for_context(
            hand,
            current_melds,
            table,
            position,
            win_rule,
        )
        && pure_one_suit_plan_score_for_context(hand, current_melds, table, position) <= 0.0
        && piao_plan_score_for_context(hand, current_melds, table, position) < 22.0
}

fn is_closed_early_piao_candidate(
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

fn should_claim_peng_for_basic_heng_and_opening(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    tile: i32,
    from_position: usize,
) -> bool {
    if win_rule != WIN_RULE_SHENYANG_BASIC
        || has_open_meld(current_melds)
        || has_triplet_or_dragon_pair(hand, current_melds)
        || !can_peng(hand, tile)
        || !missing_suits(hand, current_melds).is_empty()
        || !has_terminal_or_honor_with_extra(hand, current_melds, Some(tile))
        || should_preserve_seven_pairs_plan_for_context(
            hand,
            current_melds,
            table,
            position,
            win_rule,
        )
        || pure_one_suit_plan_score_for_context(hand, current_melds, table, position) > 0.0
    {
        return false;
    }

    let mut next = remove_n_tiles(hand, tile, 2);
    if next.len() + 2 != hand.len() {
        return false;
    }
    sort_tiles(&mut next);
    let mut melds = current_melds.to_vec();
    melds.push(claim_peng_meld(tile, from_position));

    unique_tiles(&next).into_iter().any(|discard| {
        let after_discard = remove_n_tiles(&next, discard, 1);
        missing_suits(&after_discard, &melds).is_empty()
            && has_terminal_or_honor_with_extra(&after_discard, &melds, None)
            && has_triplet_or_dragon_pair(&after_discard, &melds)
    })
}

fn should_claim_peng_to_open_mid_basic_hand(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    tile: i32,
    from_position: usize,
) -> bool {
    if win_rule != WIN_RULE_SHENYANG_BASIC
        || has_open_meld(current_melds)
        || !is_mid_opening_round(table)
        || !can_peng(hand, tile)
        || !missing_suits(hand, current_melds).is_empty()
        || !has_terminal_or_honor_with_extra(hand, current_melds, Some(tile))
        || !has_triplet_or_dragon_pair(hand, current_melds)
        || should_preserve_seven_pairs_plan_for_context(
            hand,
            current_melds,
            table,
            position,
            win_rule,
        )
        || pure_one_suit_plan_score_for_context(hand, current_melds, table, position) > 0.0
    {
        return false;
    }

    let mut next = remove_n_tiles(hand, tile, 2);
    if next.len() + 2 != hand.len() {
        return false;
    }
    sort_tiles(&mut next);
    let mut melds = current_melds.to_vec();
    melds.push(claim_peng_meld(tile, from_position));

    unique_tiles(&next).into_iter().any(|discard| {
        let after_discard = remove_n_tiles(&next, discard, 1);
        missing_suits(&after_discard, &melds).is_empty()
            && has_terminal_or_honor_with_extra(&after_discard, &melds, None)
            && has_triplet_or_dragon_pair(&after_discard, &melds)
    })
}

fn should_pass_closed_basic_peng_to_preserve_sequence(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    tile: i32,
) -> bool {
    win_rule == WIN_RULE_SHENYANG_BASIC
        && !has_open_meld(current_melds)
        && table.dealer_position != position
        && !table.max_fan.is_some_and(|max_fan| max_fan <= 1)
        && !is_late_round(table)
        && can_peng(hand, tile)
        && is_suited(tile)
        && has_triplet_like_group(hand, current_melds)
        && tile_is_middle_of_sequence(hand, tile)
        && piao_plan_score_for_context(hand, current_melds, table, position) < 22.0
        && pure_one_suit_plan_score_for_context(hand, current_melds, table, position) <= 0.0
        && !should_open_broken_closed_hand_for_defense(
            hand,
            current_melds,
            table,
            position,
            win_rule,
        )
}

fn should_claim_peng_for_closed_early_piao_candidate(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    from_position: usize,
) -> bool {
    let pairs = pair_count(hand);
    if !current_melds.is_empty()
        || table.dealer_position == position
        || piao_plan_is_capped(table)
        || !(3..=4).contains(&pairs)
        || !can_peng(hand, tile)
        || !missing_suits(hand, current_melds).is_empty()
        || !has_terminal_or_honor_with_extra(hand, current_melds, None)
    {
        return false;
    }

    let mut next = remove_n_tiles(hand, tile, 2);
    if next.len() + 2 != hand.len() {
        return false;
    }
    sort_tiles(&mut next);
    let mut melds = current_melds.to_vec();
    melds.push(claim_peng_meld(tile, from_position));

    unique_tiles(&next).into_iter().any(|discard| {
        let after_discard = remove_n_tiles(&next, discard, 1);
        has_piao_route_basics(&after_discard, &melds)
            && piao_plan_score(&after_discard, &melds) > 0.0
    })
}

fn should_peng_to_preserve_four_gui_yi_from_discard(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    tile: i32,
    from_position: usize,
) -> bool {
    if !should_preserve_four_gui_yi(tile)
        || !can_gang(hand, tile)
        || pure_one_suit_plan_score_for_context(hand, current_melds, table, position) > 0.0
    {
        return false;
    }

    let mut gang_hand = remove_n_tiles(hand, tile, 3);
    if gang_hand.len() + 3 != hand.len() {
        return false;
    }
    sort_tiles(&mut gang_hand);
    let mut gang_melds = current_melds.to_vec();
    gang_melds.push(claim_gang_meld(tile, from_position));
    let gang_ready_score = ready_tile_score(&gang_hand, &gang_melds, table, position, win_rule);
    if gang_ready_score <= 0.0 {
        return false;
    }
    let gang_visible_fan = estimated_visible_bonus_fan(&gang_hand, &gang_melds);
    let gang_four_gui_yi = estimated_four_gui_yi_fan(&gang_hand, &gang_melds);

    let mut peng_hand = remove_n_tiles(hand, tile, 2);
    if peng_hand.len() + 2 != hand.len() {
        return false;
    }
    sort_tiles(&mut peng_hand);
    let mut peng_melds = current_melds.to_vec();
    peng_melds.push(claim_peng_meld(tile, from_position));

    unique_tiles(&peng_hand).into_iter().any(|discard| {
        if discard == tile {
            return false;
        }
        let after_discard = remove_n_tiles(&peng_hand, discard, 1);
        estimated_four_gui_yi_fan(&after_discard, &peng_melds) > gang_four_gui_yi
            && estimated_visible_bonus_fan(&after_discard, &peng_melds) >= gang_visible_fan
            && ready_tile_score(&after_discard, &peng_melds, table, position, win_rule)
                >= gang_ready_score
    })
}

fn claim_gang_from_discard_reaches_ready(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    tile: i32,
    from_position: usize,
) -> bool {
    let mut next = remove_n_tiles(hand, tile, 3);
    if next.len() + 3 != hand.len() {
        return false;
    }
    sort_tiles(&mut next);
    let mut melds = current_melds.to_vec();
    melds.push(claim_gang_meld(tile, from_position));
    ready_tile_score(&next, &melds, table, position, win_rule) > 0.0
}

fn should_claim_ready_pure_one_suit_gang_from_discard(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    tile: i32,
    from_position: usize,
) -> bool {
    if !can_gang(hand, tile)
        || !is_main_pure_suit_tile(hand, current_melds, tile)
        || ready_visible_fan_reaches_cap(hand, current_melds, table, position, win_rule)
    {
        return false;
    }

    let mut next = remove_n_tiles(hand, tile, 3);
    if next.len() + 3 != hand.len() {
        return false;
    }
    sort_tiles(&mut next);
    let mut melds = current_melds.to_vec();
    melds.push(claim_gang_meld(tile, from_position));
    ready_tile_score(&next, &melds, table, position, win_rule) > 0.0
}

fn should_claim_ready_open_pure_one_suit_peng_from_discard(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    tile: i32,
    from_position: usize,
    current_ready_score: f64,
) -> bool {
    if current_ready_score > 0.0
        || !has_open_meld(current_melds)
        || !can_peng(hand, tile)
        || !is_main_pure_suit_tile(hand, current_melds, tile)
        || should_preserve_seven_pairs_plan_for_context(
            hand,
            current_melds,
            table,
            position,
            win_rule,
        )
    {
        return false;
    }

    let mut next = remove_n_tiles(hand, tile, 2);
    if next.len() + 2 != hand.len() {
        return false;
    }
    sort_tiles(&mut next);
    let mut melds = current_melds.to_vec();
    melds.push(claim_peng_meld(tile, from_position));

    unique_tiles(&next).into_iter().any(|discard| {
        let mut after_discard = remove_n_tiles(&next, discard, 1);
        sort_tiles(&mut after_discard);
        ready_has_pure_one_suit_win(&after_discard, &melds, table, position, win_rule)
    })
}

fn should_claim_ready_dragon_peng_from_discard(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    tile: i32,
    from_position: usize,
    current_ready_score: f64,
) -> bool {
    if !is_dragon(tile)
        || !can_peng(hand, tile)
        || should_preserve_seven_pairs_plan_for_context(
            hand,
            current_melds,
            table,
            position,
            win_rule,
        )
        || pure_one_suit_plan_score_for_context(hand, current_melds, table, position) > 0.0
        || ready_visible_fan_reaches_cap(hand, current_melds, table, position, win_rule)
    {
        return false;
    }

    let mut next = remove_n_tiles(hand, tile, 2);
    if next.len() + 2 != hand.len() {
        return false;
    }
    sort_tiles(&mut next);
    let mut melds = current_melds.to_vec();
    melds.push(claim_peng_meld(tile, from_position));

    let after_ready_score =
        best_ready_score_after_discard(&next, &melds, table, position, win_rule);
    if after_ready_score <= 0.0 {
        return false;
    }
    let keep_ratio = if table.dealer_position == position || is_late_round(table) {
        0.75
    } else {
        0.45
    };
    after_ready_score >= current_ready_score * keep_ratio
}

fn should_open_broken_closed_hand_for_defense(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> bool {
    if has_open_meld(melds) || !is_mid_broken_hand_defense_round(table) {
        return false;
    }
    if should_preserve_seven_pairs_plan_for_context(hand, melds, table, position, win_rule)
        || pure_one_suit_plan_score_for_context(hand, melds, table, position) > 0.0
        || piao_plan_score_for_context(hand, melds, table, position) >= 22.0
    {
        return false;
    }
    if ready_tile_score(hand, melds, table, position, win_rule) > 0.0
        || one_step_wait_potential(hand, melds, table, position, win_rule) > 0.0
    {
        return false;
    }

    let (missing_rule_requirements, unrecoverable_rule_requirements) =
        if win_rule == WIN_RULE_SHENYANG_BASIC {
            let missing_rule_requirements = [
                !missing_suits(hand, melds).is_empty(),
                !has_terminal_or_honor_with_extra(hand, melds, None),
                !has_triplet_or_dragon_pair(hand, melds),
            ]
            .into_iter()
            .filter(|missing| *missing)
            .count();
            let unrecoverable_rule_requirements =
                unrecoverable_basic_rule_requirement_count(hand, melds, table);
            (missing_rule_requirements, unrecoverable_rule_requirements)
        } else {
            (0, 0)
        };
    let power = hand_power(hand);
    if !is_late_round(table) {
        return unrecoverable_rule_requirements >= 1
            || missing_rule_requirements >= 2
            || power < 14.0;
    }
    unrecoverable_rule_requirements >= 1 || missing_rule_requirements >= 1 || power < 18.0
}

fn unrecoverable_basic_rule_requirement_count(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
) -> usize {
    let missing_suits = missing_suits(hand, melds)
        .into_iter()
        .filter(|suit| live_tile_count_for_suit(hand, table, *suit) <= 0)
        .count();
    let missing_terminal_or_honor = !has_terminal_or_honor_with_extra(hand, melds, None)
        && live_terminal_or_honor_count(hand, table) <= 0;
    let missing_heng = !can_recover_basic_heng(hand, melds, table);
    missing_suits + usize::from(missing_terminal_or_honor) + usize::from(missing_heng)
}

fn can_recover_basic_heng(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
) -> bool {
    if has_triplet_or_dragon_pair(hand, melds) {
        return true;
    }

    let mut counts = HashMap::<i32, usize>::new();
    for tile in hand.iter().copied() {
        *counts.entry(tile).or_default() += 1;
    }

    SHENYANG_MAHJONG_TILE_KINDS.into_iter().any(|tile| {
        let count = counts.get(&tile).copied().unwrap_or(0);
        let remaining = remaining_tile_count(hand, table, 0, tile) as usize;
        let can_draw_triplet = count < 3 && remaining >= 3 - count;
        let can_draw_dragon_pair = is_dragon(tile) && count < 2 && remaining >= 2 - count;
        can_draw_triplet || can_draw_dragon_pair
    })
}

fn can_recover_basic_heng_after_discard(
    hand_after_discard: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    discarded_tile: i32,
) -> bool {
    if has_triplet_or_dragon_pair(hand_after_discard, melds) {
        return true;
    }

    let mut counts = HashMap::<i32, usize>::new();
    for tile in hand_after_discard.iter().copied() {
        *counts.entry(tile).or_default() += 1;
    }

    SHENYANG_MAHJONG_TILE_KINDS.into_iter().any(|tile| {
        let count = counts.get(&tile).copied().unwrap_or(0);
        let remaining =
            remaining_tile_count_after_discard(hand_after_discard, table, discarded_tile, tile)
                as usize;
        let can_draw_triplet = count < 3 && remaining >= 3 - count;
        let can_draw_dragon_pair = is_dragon(tile) && count < 2 && remaining >= 2 - count;
        can_draw_triplet || can_draw_dragon_pair
    })
}

fn loses_basic_heng_recovery_after_discard(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    tile: i32,
    win_rule: i32,
) -> bool {
    if win_rule != WIN_RULE_SHENYANG_BASIC
        || has_triplet_or_dragon_pair(hand, melds)
        || !can_recover_basic_heng(hand, melds, table)
    {
        return false;
    }

    let hand_after_discard = remove_n_tiles(hand, tile, 1);
    hand_after_discard.len() + 1 == hand.len()
        && !can_recover_basic_heng_after_discard(&hand_after_discard, melds, table, tile)
}

fn should_pass_late_unready_claim_for_defense(
    table: &AiPublicTable,
    current_ready_score: f64,
) -> bool {
    is_late_defense_round(table) && current_ready_score <= 0.0
}

fn should_use_broken_hand_public_defense_discard(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> bool {
    if is_late_defense_round(table)
        || !is_mid_broken_hand_defense_round(table)
        || !unique_tiles(hand).into_iter().any(|tile| {
            public_discard_count(table, tile) > 0
                || mid_round_open_meld_safety_bias(table, tile) > 0.0
                || mid_broken_opponent_missing_suit_safety_bias(table, position, tile) > 0.0
        })
    {
        return false;
    }
    if should_lock_seven_pairs_plan(hand, melds, table, position, win_rule)
        || pure_one_suit_plan_score_for_context(hand, melds, table, position) > 0.0
        || piao_plan_score_for_context(hand, melds, table, position) >= 22.0
    {
        return false;
    }
    if best_ready_score_after_discard(hand, melds, table, position, win_rule) > 0.0
        || best_one_step_wait_potential_after_discard(hand, melds, table, position, win_rule) > 0.0
    {
        return false;
    }

    let missing_rule_requirements = if win_rule == WIN_RULE_SHENYANG_BASIC {
        [
            !missing_suits(hand, melds).is_empty(),
            !has_terminal_or_honor_with_extra(hand, melds, None),
            !has_triplet_or_dragon_pair(hand, melds),
        ]
        .into_iter()
        .filter(|missing| *missing)
        .count()
    } else {
        0
    };
    let unrecoverable_rule_requirements = if win_rule == WIN_RULE_SHENYANG_BASIC {
        unrecoverable_basic_rule_requirement_count(hand, melds, table)
    } else {
        0
    };
    if table.dealer_position == position && unrecoverable_rule_requirements == 0 {
        return false;
    }
    let power_threshold = if is_late_round(table) { 18.0 } else { 16.0 };
    unrecoverable_rule_requirements >= 1
        || missing_rule_requirements >= 2
        || hand_power(hand) < power_threshold
}

fn should_preserve_four_gui_yi(tile: i32) -> bool {
    !is_dragon(tile)
}

fn should_pass_peng_for_relaxed_pure_defense(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    tile: i32,
) -> bool {
    if win_rule == WIN_RULE_SHENYANG_BASIC
        || is_dragon(tile)
        || has_open_meld(melds)
        || table.dealer_position == position
        || !is_late_round(table)
        || should_preserve_seven_pairs_plan_for_context(hand, melds, table, position, win_rule)
        || pure_one_suit_plan_score_for_context(hand, melds, table, position) > 0.0
        || piao_plan_score_for_context(hand, melds, table, position) >= 22.0
        || should_open_broken_closed_hand_for_defense(hand, melds, table, position, win_rule)
    {
        return false;
    }
    ready_tile_score(hand, melds, table, position, win_rule) <= 0.0
        && one_step_wait_potential(hand, melds, table, position, win_rule) <= 0.0
        && hand_power(hand) < 18.0
}

fn should_preserve_piao_plan_for_chi(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
) -> bool {
    if melds.iter().any(is_sequence_meld) {
        return false;
    }
    let score = piao_plan_score_for_context(hand, melds, table, position);
    let early_piao_candidate = melds.is_empty()
        && pair_count(hand) >= 3
        && table.dealer_position != position
        && !piao_plan_is_capped(table);
    if !early_piao_candidate && score <= 0.0 {
        return false;
    }
    has_piao_route_basics(hand, melds) && (score >= 20.0 || early_piao_candidate)
}

fn should_preserve_pinghu_sequence_over_peng(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    tile: i32,
) -> bool {
    if !is_suited(tile)
        || is_dragon(tile)
        || table.dealer_position == position
        || piao_plan_score_for_context(hand, melds, table, position) >= 22.0
        || !has_triplet_or_dragon_pair(hand, melds)
        || !tile_is_middle_of_sequence(hand, tile)
    {
        return false;
    }
    if win_rule == WIN_RULE_SHENYANG_BASIC && !has_open_meld(melds) {
        return false;
    }
    true
}

fn should_preserve_seven_pairs_for_self_gang(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> bool {
    should_lock_seven_pairs_plan(hand, melds, table, position, win_rule)
}

fn should_pass_late_unready_self_gang_for_defense(
    table: &AiPublicTable,
    current_ready_score: f64,
) -> bool {
    is_late_defense_round(table) && current_ready_score <= 0.0
}

fn should_preserve_seven_pairs_plan_for_context(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> bool {
    hand.len() == 13 && should_lock_seven_pairs_plan(hand, melds, table, position, win_rule)
}

fn terminal_or_honor_discard_bias(
    hand_after_discard: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    win_rule: i32,
) -> f64 {
    if violates_basic_terminal_or_honor_discard(
        hand_after_discard,
        melds,
        table,
        position,
        tile,
        win_rule,
    ) {
        -500.0
    } else {
        0.0
    }
}

fn three_suits_discard_bias(
    hand_after_discard: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    win_rule: i32,
) -> f64 {
    if !is_suited(tile) {
        return 0.0;
    }
    let suit = tile_suit(tile);
    let before = suit_presence_with_extra(hand_after_discard, melds, Some(tile));
    let after = suit_presence(hand_after_discard, melds);
    let was_missing_suit = before.iter().any(|present| !*present);
    if melds.is_empty() && pair_count(hand_after_discard) >= 4 && was_missing_suit {
        return 0.0;
    }
    let capped_three_suit_hand =
        table.max_fan.is_some_and(|max_fan| max_fan <= 1) && !was_missing_suit;
    if !capped_three_suit_hand
        && pure_one_suit_plan_score_for_context(hand_after_discard, melds, table, position) > 0.0
    {
        return 0.0;
    }
    let mut bias = 0.0;
    if before[suit as usize] && !after[suit as usize] {
        bias -= if win_rule == WIN_RULE_SHENYANG_BASIC {
            80.0
        } else {
            14.0
        };
    }
    if after.into_iter().filter(|present| *present).count() < 3 {
        bias -= 2.5;
    }
    bias
}

fn violates_basic_terminal_or_honor_discard(
    hand_after_discard: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    win_rule: i32,
) -> bool {
    if win_rule != WIN_RULE_SHENYANG_BASIC || !(is_honor(tile) || tile_is_terminal(tile)) {
        return false;
    }
    let before = has_terminal_or_honor_with_extra(hand_after_discard, melds, Some(tile));
    let after = has_terminal_or_honor_with_extra(hand_after_discard, melds, None);
    if !before || after {
        return false;
    }
    if should_preserve_seven_pairs_plan_for_context(
        hand_after_discard,
        melds,
        table,
        position,
        win_rule,
    ) {
        return false;
    }
    if pure_one_suit_plan_score_for_context(hand_after_discard, melds, table, position) > 0.0
        && (is_honor(tile) || !is_main_pure_suit_tile(hand_after_discard, melds, tile))
    {
        return false;
    }
    if table.max_fan.is_some_and(|max_fan| max_fan <= 1) {
        return true;
    }
    true
}

fn violates_basic_heng_discard(
    hand_after_discard: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    win_rule: i32,
) -> bool {
    if win_rule != WIN_RULE_SHENYANG_BASIC {
        return false;
    }
    let had_heng = has_triplet_or_dragon_pair_with_extra(hand_after_discard, melds, Some(tile));
    let has_heng_after = has_triplet_or_dragon_pair(hand_after_discard, melds);
    if has_heng_after {
        return false;
    }
    let mut hand_before_discard = hand_after_discard.to_vec();
    hand_before_discard.push(tile);
    sort_tiles(&mut hand_before_discard);
    let lost_recoverable_heng = !had_heng
        && loses_basic_heng_recovery_after_discard(
            &hand_before_discard,
            melds,
            table,
            tile,
            win_rule,
        );
    if !had_heng && !lost_recoverable_heng {
        return false;
    }
    if should_preserve_seven_pairs_plan_for_context(
        hand_after_discard,
        melds,
        table,
        position,
        win_rule,
    ) {
        return false;
    }
    pure_one_suit_plan_score_for_context(hand_after_discard, melds, table, position) <= 0.0
}

fn violates_basic_three_suits_discard(
    hand_after_discard: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    win_rule: i32,
) -> bool {
    if win_rule != WIN_RULE_SHENYANG_BASIC || !is_suited(tile) {
        return false;
    }
    let before = suit_presence_with_extra(hand_after_discard, melds, Some(tile));
    let after = suit_presence(hand_after_discard, melds);
    if before.into_iter().filter(|present| *present).count() < 3
        || after.into_iter().filter(|present| *present).count() >= 3
    {
        return false;
    }
    if table.max_fan.is_some_and(|max_fan| max_fan <= 1)
        && !is_seven_pairs_wait_shape(hand_after_discard)
        && pair_count(hand_after_discard) < 6
    {
        return true;
    }
    if should_preserve_seven_pairs_plan_for_context(
        hand_after_discard,
        melds,
        table,
        position,
        win_rule,
    ) {
        return false;
    }
    if table.max_fan.is_some_and(|max_fan| max_fan <= 1) {
        return true;
    }
    pure_one_suit_plan_score_for_context(hand_after_discard, melds, table, position) <= 0.0
}

#[cfg(test)]
mod tests;
