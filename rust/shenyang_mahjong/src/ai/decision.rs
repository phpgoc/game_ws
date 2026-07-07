use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use share_type_public::games::shenyang_mahjong::{
    SHENYANG_MAHJONG_TILE_KINDS, ShenyangMahjongMeldKind, WsShenyangMahjongMeld,
};

use crate::rules::{
    WIN_RULE_SHENYANG_BASIC, can_chi, can_gang, can_peng, has_dragon_pair_as_standard_pair,
    has_triplet_in_standard_decomposition, is_complete_win, is_complete_win_with_melds,
    is_piao_hu_win, is_pure_one_suit_win, is_seven_pairs_win, is_single_wait_shape_with_rule,
    sort_tiles,
};

use super::observation::{AiClaimView, AiPublicTable, AiSeatView};

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
        if !is_mid_broken_hand_defense_round(table) {
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
        if let Some(tile) = choose_broken_hand_public_defense_discard(hand, table, position) {
            return Some(tile);
        }
    }

    let mut best_allowed: Option<(f64, i32)> = None;
    let mut best_any: Option<(f64, i32)> = None;
    for tile in hand.iter().copied() {
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
        let count = hand.iter().filter(|&&item| item == tile).count();
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
        ) + piao_discard_bias(hand, tile, melds, table, position)
            + early_piao_candidate_discard_bias(hand, tile, melds, table, position)
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
    table: &AiPublicTable,
    position: usize,
) -> Option<i32> {
    let candidates = unique_tiles(hand)
        .into_iter()
        .filter(|tile| public_discard_count(table, *tile) > 0)
        .collect::<Vec<_>>();
    choose_public_defense_discard_from_candidates(hand, table, position, candidates)
}

fn choose_public_defense_discard_from_candidates(
    hand: &[i32],
    table: &AiPublicTable,
    position: usize,
    candidates: Vec<i32>,
) -> Option<i32> {
    let mut best: Option<(f64, i32)> = None;
    for tile in candidates {
        let own_tile_count = hand.iter().filter(|item| **item == tile).count();
        let score = public_defense_tile_safety_score(table, position, tile, own_tile_count);
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

fn claim_gang_meld(tile: i32, from_position: usize) -> WsShenyangMahjongMeld {
    WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::GANG,
        tiles: vec![tile, tile, tile, tile],
        from_position: Some(from_position as i32),
    }
}

fn claim_peng_meld(tile: i32, from_position: usize) -> WsShenyangMahjongMeld {
    WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![tile, tile, tile],
        from_position: Some(from_position as i32),
    }
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
            **seat_position != position && !has_open_meld(&seat.melds) && seat.hand_count >= 10
        })
        .map(|(_, _)| {
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
            (base - pair_penalty) * pressure_scale * exposure_scale
        })
        .sum()
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
    for tile in hand
        .iter()
        .copied()
        .chain(melds.iter().flat_map(|meld| meld.tiles.iter().copied()))
    {
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
    if melds
        .iter()
        .any(|meld| meld.kind == ShenyangMahjongMeldKind::CHI)
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

fn exposed_meld_tile_count(table: &AiPublicTable, tile: i32) -> usize {
    table
        .seats
        .values()
        .flat_map(|seat| seat.melds.iter())
        .flat_map(|meld| meld.tiles.iter())
        .filter(|meld_tile| **meld_tile == tile)
        .count()
}

fn open_meld_tile_count(table: &AiPublicTable, tile: i32) -> usize {
    table
        .seats
        .values()
        .flat_map(|seat| seat.melds.iter())
        .filter(|meld| meld.from_position.is_some())
        .flat_map(|meld| meld.tiles.iter())
        .filter(|meld_tile| **meld_tile == tile)
        .count()
}

fn estimated_four_gui_yi_fan(hand: &[i32], melds: &[WsShenyangMahjongMeld]) -> i32 {
    let mut counts = HashMap::<i32, i32>::new();
    for tile in hand.iter().copied() {
        *counts.entry(tile).or_default() += 1;
    }
    for meld in melds
        .iter()
        .filter(|meld| meld.kind != ShenyangMahjongMeldKind::GANG)
    {
        for tile in meld.tiles.iter().copied() {
            *counts.entry(tile).or_default() += 1;
        }
    }
    counts.into_values().filter(|count| *count == 4).count() as i32
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
    let own_open_melds = melds
        .iter()
        .filter(|meld| meld.from_position.is_some())
        .count();
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
        || table.wall_count <= 30
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

fn hand_power(hand: &[i32]) -> f64 {
    let mut counts: HashMap<i32, usize> = HashMap::new();
    for &tile in hand {
        *counts.entry(tile).or_default() += 1;
    }

    let mut score = 0.0;
    let mut used = HashSet::new();
    for (&tile, &count) in &counts {
        if count >= 3 {
            score += 18.0;
            used.insert(tile);
        } else if count == 2 {
            score += 7.0;
        }
        if is_honor(tile) {
            score -= if count == 1 { 4.6 } else { 2.0 };
        } else {
            let rank = tile_rank(tile);
            let neigh = neighbor_count(hand, tile) as f64;
            if tile_is_terminal(tile) {
                score -= 0.6;
            }
            score += neigh * 1.2;
            if (2..=8).contains(&rank) {
                score += 0.4;
            }
            if count == 1 && neigh == 0.0 {
                score -= 3.8;
            } else if count == 1 && neigh == 1.0 {
                score -= 1.2;
            }
        }
    }

    let mut working = hand.to_vec();
    sort_tiles(&mut working);
    let mut i = 0usize;
    while i + 2 < working.len() {
        let a = working[i];
        let b = working[i + 1];
        let c = working[i + 2];
        if is_suited(a)
            && tile_suit(a) == tile_suit(b)
            && tile_suit(a) == tile_suit(c)
            && a + 1 == b
            && b + 1 == c
        {
            score += 10.0;
            i += 3;
        } else {
            i += 1;
        }
    }

    score -= used.len() as f64 * 0.2;
    score
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

fn has_open_meld(melds: &[WsShenyangMahjongMeld]) -> bool {
    melds.iter().any(|meld| meld.from_position.is_some())
}

fn has_peng_meld(melds: &[WsShenyangMahjongMeld], tile: i32) -> bool {
    melds.iter().any(|meld| {
        meld.kind == ShenyangMahjongMeldKind::PENG
            && meld.tiles.iter().all(|meld_tile| *meld_tile == tile)
    })
}

fn promoted_added_gang_melds(
    melds: &[WsShenyangMahjongMeld],
    tile: i32,
) -> Vec<WsShenyangMahjongMeld> {
    let mut next_melds = melds.to_vec();
    if let Some(meld) = next_melds.iter_mut().find(|meld| {
        meld.kind == ShenyangMahjongMeldKind::PENG
            && meld.tiles.iter().all(|meld_tile| *meld_tile == tile)
    }) {
        meld.kind = ShenyangMahjongMeldKind::GANG;
        meld.tiles = vec![tile, tile, tile, tile];
    }
    next_melds
}

fn has_terminal_or_honor_with_extra(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    extra: Option<i32>,
) -> bool {
    hand.iter()
        .copied()
        .chain(extra)
        .chain(melds.iter().flat_map(|meld| meld.tiles.iter().copied()))
        .any(|tile| is_honor(tile) || tile_is_terminal(tile))
}

fn has_triplet_or_dragon_pair(hand: &[i32], melds: &[WsShenyangMahjongMeld]) -> bool {
    has_triplet_or_dragon_pair_with_extra(hand, melds, None)
}

fn has_triplet_or_dragon_pair_with_extra(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    extra: Option<i32>,
) -> bool {
    let tiles = hand.iter().copied().chain(extra).collect::<Vec<_>>();
    if is_complete_win(&tiles, melds.len()) {
        return melds.iter().any(is_triplet_like_meld)
            || has_triplet_in_standard_decomposition(&tiles)
            || has_dragon_pair_as_standard_pair(&tiles);
    }

    let mut counts = HashMap::<i32, usize>::new();
    for tile in tiles {
        *counts.entry(tile).or_default() += 1;
    }
    melds.iter().any(is_triplet_like_meld)
        || counts.values().any(|count| *count >= 3)
        || [35, 36, 37]
            .into_iter()
            .any(|tile| counts.get(&tile).copied().unwrap_or(0) >= 2)
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

fn is_dragon(tile: i32) -> bool {
    matches!(tile, 35..=37)
}

fn is_honor(tile: i32) -> bool {
    tile >= 31
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

fn is_main_pure_suit_tile(hand: &[i32], melds: &[WsShenyangMahjongMeld], tile: i32) -> bool {
    dominant_pure_suit(hand, melds).is_some_and(|suit| is_suited(tile) && tile_suit(tile) == suit)
}

fn is_seven_pairs_wait_shape(hand: &[i32]) -> bool {
    if hand.len() != 13 {
        return false;
    }
    let mut counts: HashMap<i32, usize> = HashMap::new();
    for &tile in hand {
        *counts.entry(tile).or_default() += 1;
    }
    let pairs = counts.values().map(|count| count / 2).sum::<usize>();
    let singles = counts.values().filter(|&&count| count % 2 == 1).count();
    pairs == 6 && singles == 1
}

fn is_suited(tile: i32) -> bool {
    matches!(tile, 1..=9 | 11..=19 | 21..=29)
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

fn is_triplet_like_meld(meld: &WsShenyangMahjongMeld) -> bool {
    matches!(
        meld.kind,
        ShenyangMahjongMeldKind::PENG | ShenyangMahjongMeldKind::GANG
    ) && meld.tiles.len() >= 3
        && meld
            .tiles
            .first()
            .is_some_and(|tile| meld.tiles.iter().all(|item| item == tile))
}

fn is_wind(tile: i32) -> bool {
    matches!(tile, 31..=34)
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
        return 28.0
            + public_discards as f64 * 6.0
            + honor_bonus
            + suited_shape_bonus
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
        2 => 14.0,
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

fn live_tile_count_for_suit_after_discard(
    hand_after_discard: &[i32],
    table: &AiPublicTable,
    suit: i32,
    discarded_tile: i32,
) -> i32 {
    (1..=9)
        .map(|rank| {
            let tile = suit * 10 + rank;
            let visible = visible_tile_count(table, tile);
            let own_hand = hand_after_discard
                .iter()
                .filter(|item| **item == tile)
                .count() as i32;
            let own_discard = i32::from(discarded_tile == tile);
            (4 - visible - own_hand - own_discard).max(0)
        })
        .sum()
}

fn live_tile_count_for_suit(hand: &[i32], table: &AiPublicTable, suit: i32) -> i32 {
    (1..=9)
        .map(|rank| {
            let tile = suit * 10 + rank;
            let visible = visible_tile_count(table, tile);
            let own_hand = hand.iter().filter(|item| **item == tile).count() as i32;
            (4 - visible - own_hand).max(0)
        })
        .sum()
}

fn live_terminal_or_honor_count_after_discard(
    hand_after_discard: &[i32],
    table: &AiPublicTable,
    discarded_tile: i32,
) -> i32 {
    SHENYANG_MAHJONG_TILE_KINDS
        .into_iter()
        .filter(|tile| is_honor(*tile) || tile_is_terminal(*tile))
        .map(|tile| {
            let visible = visible_tile_count(table, tile);
            let own_hand = hand_after_discard
                .iter()
                .filter(|item| **item == tile)
                .count() as i32;
            let own_discard = i32::from(discarded_tile == tile);
            (4 - visible - own_hand - own_discard).max(0)
        })
        .sum()
}

fn live_terminal_or_honor_count(hand: &[i32], table: &AiPublicTable) -> i32 {
    SHENYANG_MAHJONG_TILE_KINDS
        .into_iter()
        .filter(|tile| is_honor(*tile) || tile_is_terminal(*tile))
        .map(|tile| {
            let visible = visible_tile_count(table, tile);
            let own_hand = hand.iter().filter(|item| **item == tile).count() as i32;
            (4 - visible - own_hand).max(0)
        })
        .sum()
}

fn meld_primary_tile(meld: &WsShenyangMahjongMeld) -> Option<i32> {
    let first = *meld.tiles.first()?;
    meld.tiles
        .iter()
        .all(|tile| *tile == first)
        .then_some(first)
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
    9.0 + public_discards as f64 * 4.0
        + shape_bonus
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
    open_risk + late_round_risk
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
    open_risk + late_round_risk
}

fn seat_has_open_meld_tile(seat: &AiSeatView, tile: i32) -> bool {
    seat.melds.iter().any(|meld| {
        meld.from_position.is_some() && meld.tiles.iter().any(|meld_tile| *meld_tile == tile)
    })
}

fn open_opponent_exists_for_tile(table: &AiPublicTable, position: usize, tile: i32) -> bool {
    table.seats.iter().any(|(seat_position, seat)| {
        *seat_position != position
            && has_open_meld(&seat.melds)
            && !seat.discards.contains(&tile)
            && !seat_has_open_meld_tile(seat, tile)
    })
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
    let own_open_melds = melds
        .iter()
        .filter(|meld| meld.from_position.is_some())
        .count();
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
    let own_open_melds = melds
        .iter()
        .filter(|meld| meld.from_position.is_some())
        .count();
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

fn missing_suits(hand: &[i32], melds: &[WsShenyangMahjongMeld]) -> Vec<i32> {
    suit_presence(hand, melds)
        .into_iter()
        .enumerate()
        .filter_map(|(suit, present)| (!present).then_some(suit as i32))
        .collect()
}

fn neighbor_count(hand: &[i32], tile: i32) -> i32 {
    if !is_suited(tile) {
        return 0;
    }
    let suit = tile_suit(tile);
    let rank = tile_rank(tile);
    let mut count = 0;
    for delta in [-2, -1, 1, 2] {
        let candidate = suit * 10 + rank + delta;
        if candidate > 0 && candidate < 40 && tile_suit(candidate) == suit {
            count += hand.iter().filter(|&&item| item == candidate).count() as i32;
        }
    }
    count
}

fn next_position_after(current: usize, table: &AiPublicTable) -> usize {
    let mut positions: Vec<usize> = table.seats.keys().copied().collect();
    positions.sort_unstable();
    if positions.is_empty() {
        return current;
    }
    let idx = positions
        .iter()
        .position(|pos| *pos == current)
        .unwrap_or(0);
    positions[(idx + 1) % positions.len()]
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
    if hand_power(hand) < 50.0 && pair_count(hand) < 4 {
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
            && seat.hand_count >= 13
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

fn pair_count(hand: &[i32]) -> usize {
    let mut counts: HashMap<i32, usize> = HashMap::new();
    for &tile in hand {
        *counts.entry(tile).or_default() += 1;
    }
    counts.values().map(|count| count / 2).sum()
}

fn pair_discard_bias(hand: &[i32]) -> f64 {
    if pair_count(hand) >= 4 { -4.4 } else { -1.8 }
}

fn piao_discard_bias(
    hand: &[i32],
    tile: i32,
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
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
        if committed_groups >= 2 { -24.0 } else { -16.0 }
    } else if only_terminal_or_honor || only_suit_tile {
        -40.0
    } else if is_honor(tile) || tile_is_terminal(tile) {
        1.0
    } else if neighbor_count(hand, tile) >= 2 {
        3.0
    } else {
        0.0
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
    if melds
        .iter()
        .any(|meld| meld.kind == ShenyangMahjongMeldKind::CHI)
    {
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
    if open_triplets + triplets >= 2 || pairs >= 4 || (open_triplets >= 1 && pairs >= 2) {
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
    if melds
        .iter()
        .any(|meld| meld.kind == ShenyangMahjongMeldKind::CHI)
    {
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
            Some((
                piao_single_wait_tile_score(wait_tile, &next, melds, table, position, win_rule),
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
    let speed_first = table.dealer_position == position || is_late_round(table);
    let remaining_weight = if speed_first { 14.0 } else { 9.0 };
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

fn public_discard_count(table: &AiPublicTable, tile: i32) -> usize {
    table
        .seats
        .values()
        .map(|seat| {
            seat.discards
                .iter()
                .filter(|discard| **discard == tile)
                .count()
        })
        .sum()
}

fn own_previous_discard_count(table: &AiPublicTable, position: usize, tile: i32) -> usize {
    table
        .seats
        .get(&position)
        .map(|seat| {
            seat.discards
                .iter()
                .filter(|discard| **discard == tile)
                .count()
        })
        .unwrap_or(0)
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
    if pure_one_suit_plan_score_for_context(hand, melds, table, position) <= 0.0 {
        return 0.0;
    }
    if is_honor(tile) {
        return 18.0;
    }
    if is_main_pure_suit_tile(hand, melds, tile) {
        -5.0
    } else {
        16.0
    }
}

fn pure_one_suit_plan_score(hand: &[i32], melds: &[WsShenyangMahjongMeld]) -> f64 {
    let Some((main_suit, main_count, blockers)) = pure_one_suit_shape(hand, melds) else {
        return 0.0;
    };
    if melds.iter().any(|meld| {
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
        .chain(melds.iter().flat_map(|meld| meld.tiles.iter().copied()))
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

fn remaining_tile_count(hand: &[i32], table: &AiPublicTable, _position: usize, tile: i32) -> i32 {
    let visible = visible_tile_count(table, tile);
    let own = hand.iter().filter(|&&item| item == tile).count() as i32;
    (4 - visible - own).max(0)
}

fn remove_n_tiles(hand: &[i32], tile: i32, count: usize) -> Vec<i32> {
    let mut removed = 0usize;
    let mut next = Vec::with_capacity(hand.len().saturating_sub(count));
    for &item in hand {
        if item == tile && removed < count {
            removed += 1;
        } else {
            next.push(item);
        }
    }
    next
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
        return if is_honor(tile) {
            -18.0
        } else if tile_is_terminal(tile) {
            -15.0
        } else {
            -12.0
        };
    }
    if is_honor(tile) {
        5.0
    } else if tile_is_terminal(tile) {
        0.0
    } else {
        3.0
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
    18.0 + seven_pairs_wait_tile_score(wait_tile, &next, table, position)
}

fn choose_seven_pairs_wait_discard(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> Option<i32> {
    if !should_keep_pairs_for_seven_pairs_discard(hand, melds, table, position, win_rule)
        || table.max_fan.is_some_and(|max_fan| max_fan <= 1)
        || pair_count(hand) != 6
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
            Some((
                seven_pairs_wait_tile_score(wait_tile, &next, table, position),
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
        return pure_score + if has_open_meld(melds) { 9.0 } else { -8.0 };
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
    missing_suits + usize::from(missing_terminal_or_honor)
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
        || !unique_tiles(hand)
            .into_iter()
            .any(|tile| public_discard_count(table, tile) > 0)
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
    unrecoverable_rule_requirements >= 1
        || missing_rule_requirements >= 2
        || hand_power(hand) < 16.0
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
    if melds
        .iter()
        .any(|meld| meld.kind == ShenyangMahjongMeldKind::CHI)
    {
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

fn single_tile(hand: &[i32]) -> Option<i32> {
    let mut counts: HashMap<i32, usize> = HashMap::new();
    for &tile in hand {
        *counts.entry(tile).or_default() += 1;
    }
    counts
        .into_iter()
        .find_map(|(tile, count)| (count % 2 == 1).then_some(tile))
}

fn suit_presence(hand: &[i32], melds: &[WsShenyangMahjongMeld]) -> [bool; 3] {
    suit_presence_with_extra(hand, melds, None)
}

fn suit_presence_with_extra(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    extra: Option<i32>,
) -> [bool; 3] {
    let mut suits = [false; 3];
    for tile in hand.iter().copied().chain(extra) {
        if is_suited(tile) {
            suits[tile_suit(tile) as usize] = true;
        }
    }
    for tile in melds.iter().flat_map(|meld| meld.tiles.iter().copied()) {
        if is_suited(tile) {
            suits[tile_suit(tile) as usize] = true;
        }
    }
    suits
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

fn terminal_or_honor_count(hand: &[i32], melds: &[WsShenyangMahjongMeld]) -> usize {
    hand.iter()
        .copied()
        .chain(melds.iter().flat_map(|meld| meld.tiles.iter().copied()))
        .filter(|tile| is_honor(*tile) || tile_is_terminal(*tile))
        .count()
}

fn suited_tile_count_for_suit(hand: &[i32], melds: &[WsShenyangMahjongMeld], suit: i32) -> usize {
    hand.iter()
        .copied()
        .chain(melds.iter().flat_map(|meld| meld.tiles.iter().copied()))
        .filter(|tile| is_suited(*tile) && tile_suit(*tile) == suit)
        .count()
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
    if win_rule == WIN_RULE_SHENYANG_BASIC
        && !capped_three_suit_hand
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

fn tile_is_middle_of_sequence(hand: &[i32], tile: i32) -> bool {
    if !is_suited(tile) || !(2..=8).contains(&tile_rank(tile)) {
        return false;
    }
    let left = tile - 1;
    let right = tile + 1;
    hand.iter().any(|item| *item == left) && hand.iter().any(|item| *item == right)
}

fn tile_is_part_of_complete_sequence(hand: &[i32], tile: i32) -> bool {
    if !is_suited(tile) {
        return false;
    }
    let rank = tile_rank(tile);
    let suit = tile_suit(tile);
    let min_start = (rank - 2).max(1);
    let max_start = rank.min(7);
    (min_start..=max_start).any(|start| {
        (start..start + 3).all(|sequence_rank| {
            let sequence_tile = suit * 10 + sequence_rank;
            hand.iter().any(|item| *item == sequence_tile)
        })
    })
}

fn tile_is_core_two_sided_wait_member(hand: &[i32], tile: i32) -> bool {
    if !is_suited(tile) {
        return false;
    }
    [-1, 1].into_iter().any(|offset| {
        let other = tile + offset;
        is_suited(other)
            && tile_suit(other) == tile_suit(tile)
            && hand.iter().any(|item| *item == other)
            && {
                let low_rank = tile_rank(tile).min(tile_rank(other));
                let high_rank = tile_rank(tile).max(tile_rank(other));
                matches!((low_rank, high_rank), (3, 4) | (4, 5) | (5, 6) | (6, 7))
            }
    })
}

fn tile_is_core_closed_middle_wait_member(hand: &[i32], tile: i32) -> bool {
    if !is_suited(tile) {
        return false;
    }
    [-2, 2].into_iter().any(|offset| {
        let other = tile + offset;
        is_suited(other)
            && tile_suit(other) == tile_suit(tile)
            && hand.iter().any(|item| *item == other)
            && {
                let low_rank = tile_rank(tile).min(tile_rank(other));
                let high_rank = tile_rank(tile).max(tile_rank(other));
                matches!((low_rank, high_rank), (3, 5) | (4, 6) | (5, 7))
            }
    })
}

fn tile_is_weak_edge_wait_terminal(hand: &[i32], tile: i32) -> bool {
    if !is_suited(tile) {
        return false;
    }
    match tile_rank(tile) {
        1 => hand.iter().any(|item| *item == tile + 1),
        9 => hand.iter().any(|item| *item == tile - 1),
        _ => false,
    }
}

fn tile_is_terminal(tile: i32) -> bool {
    is_suited(tile) && matches!(tile_rank(tile), 1 | 9)
}

fn tile_rank(tile: i32) -> i32 {
    tile % 10
}

fn tile_suit(tile: i32) -> i32 {
    tile / 10
}

fn unique_tiles(hand: &[i32]) -> Vec<i32> {
    let mut tiles = hand.to_vec();
    tiles.sort_unstable();
    tiles.dedup();
    tiles
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
    if !had_heng || has_heng_after {
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

fn visible_tile_count(table: &AiPublicTable, tile: i32) -> i32 {
    table
        .seats
        .values()
        .map(|seat| {
            let discard_count = seat.discards.iter().filter(|&&item| item == tile).count();
            let meld_count = seat
                .melds
                .iter()
                .flat_map(|meld| meld.tiles.iter())
                .filter(|&&item| item == tile)
                .count();
            discard_count + meld_count
        })
        .sum::<usize>() as i32
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::ai::observation::{AiClaimView, AiSeatView};
    use crate::rules::{WIN_RULE_RELAXED, WIN_RULE_SHENYANG_BASIC};

    #[test]
    fn basic_three_suits_filter_allows_locked_seven_pairs_route() {
        let table = table_with_discards(1, Vec::new());
        let hand_after_discard = vec![1, 1, 2, 2, 3, 3, 4, 4, 11, 11, 12, 12, 31];

        assert!(!violates_basic_three_suits_discard(
            &hand_after_discard,
            &[],
            &table,
            0,
            21,
            WIN_RULE_SHENYANG_BASIC
        ));
    }

    #[test]
    fn basic_heng_filter_ignores_chi_tile_plus_hand_pair() {
        let table = table_with_discards(1, Vec::new());
        let melds = vec![test_chi_meld(1)];
        let hand_after_discard = vec![1, 1, 11, 12, 13, 21, 22, 23, 31, 35];

        assert!(violates_basic_heng_discard(
            &hand_after_discard,
            &melds,
            &table,
            0,
            35,
            WIN_RULE_SHENYANG_BASIC
        ));
    }

    #[test]
    fn basic_heng_filter_ignores_short_triplet_like_meld() {
        let melds = vec![WsShenyangMahjongMeld {
            kind: ShenyangMahjongMeldKind::PENG,
            tiles: vec![1, 1],
            from_position: Some(1),
        }];
        let hand = vec![11, 12, 13, 21, 22, 23, 31, 35];

        assert!(!is_triplet_like_meld(&melds[0]));
        assert!(!has_triplet_or_dragon_pair(&hand, &melds));
        assert_eq!(piao_threat_level(&melds), 0);
    }

    #[test]
    fn basic_heng_heuristic_uses_complete_decomposition_for_fake_triplet() {
        let melds = vec![test_chi_meld(11)];
        let hand = vec![1, 2, 2, 3, 3, 3, 4, 4, 5, 26, 26];

        assert!(is_complete_win(&hand, melds.len()));
        assert!(hand.iter().filter(|tile| **tile == 3).count() >= 3);
        assert!(!has_triplet_in_standard_decomposition(&hand));
        assert!(!has_triplet_or_dragon_pair(&hand, &melds));
    }

    #[test]
    fn broken_closed_defense_uses_basic_rule_instead_of_relaxed_near_ready_shape() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 40;
        let hand = vec![2, 2, 3, 4, 5, 11, 12, 13, 21, 22, 23, 31, 35];

        assert!(
            ready_tile_score(&hand, &[], &table, 0, WIN_RULE_RELAXED) > 0.0
                || one_step_wait_potential(&hand, &[], &table, 0, WIN_RULE_RELAXED) > 0.0
        );
        assert_eq!(
            ready_tile_score(&hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC),
            0.0
        );
        assert_eq!(
            one_step_wait_potential(&hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC),
            0.0
        );
        assert!(should_open_broken_closed_hand_for_defense(
            &hand,
            &[],
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC
        ));
    }

    #[test]
    fn broken_closed_defense_preserves_seven_pairs_route() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 40;
        let hand = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 12, 31, 35, 36];

        assert!(!should_open_broken_closed_hand_for_defense(
            &hand,
            &[],
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC
        ));
    }

    #[test]
    fn broken_closed_defense_opens_mid_severely_broken_hand() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 52;
        let hand = vec![2, 5, 8, 11, 14, 17, 19, 31, 31, 33, 35, 36, 37];

        assert!(should_open_broken_closed_hand_for_defense(
            &hand,
            &[],
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC
        ));
    }

    #[test]
    fn broken_closed_defense_waits_mid_when_basic_requirements_are_intact() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 52;
        let hand = vec![1, 1, 1, 2, 3, 4, 11, 12, 13, 21, 22, 23, 35];

        assert!(!should_open_broken_closed_hand_for_defense(
            &hand,
            &[],
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC
        ));
    }

    #[test]
    fn capped_discard_does_not_chase_pure_one_suit_when_three_suits_remain() {
        let mut table = table_with_discards(1, Vec::new());
        table.max_fan = Some(1);
        let hand = vec![1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 7, 8, 11, 21];

        assert!(!matches!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(11 | 21)
        ));
    }

    #[test]
    fn capped_pure_one_suit_route_can_discard_last_honor_when_suits_are_missing() {
        let mut table = table_with_discards(1, Vec::new());
        table.max_fan = Some(1);
        let hand = vec![2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 8, 8, 12, 31];

        assert!(
            pure_one_suit_plan_score_for_context(&remove_n_tiles(&hand, 31, 1), &[], &table, 0)
                > 0.0
        );
        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(31)
        );
    }

    #[test]
    fn capped_locked_seven_pairs_route_can_discard_last_honor() {
        let mut table = table_with_discards(1, Vec::new());
        table.max_fan = Some(1);
        let hand = vec![2, 2, 3, 3, 4, 4, 12, 12, 13, 13, 14, 14, 5, 31];
        let after_discard = remove_n_tiles(&hand, 31, 1);

        assert!(should_preserve_seven_pairs_plan_for_context(
            &after_discard,
            &[],
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC
        ));
        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(31)
        );
    }

    #[test]
    fn capped_locked_seven_pairs_route_can_break_three_suits_requirement() {
        let mut table = table_with_discards(1, Vec::new());
        table.max_fan = Some(1);
        let hand_after_discard = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 12, 13, 13, 5];

        assert!(should_preserve_seven_pairs_plan_for_context(
            &hand_after_discard,
            &[],
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC
        ));
        assert!(!violates_basic_three_suits_discard(
            &hand_after_discard,
            &[],
            &table,
            0,
            21,
            WIN_RULE_SHENYANG_BASIC
        ));
    }

    #[test]
    fn uncapped_room_keeps_piao_plan_biases() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 1, 2, 2, 11, 11, 12, 13, 21, 21, 22, 31, 32];

        assert!(piao_plan_score_for_context(&hand, &[], &table, 0) >= 20.0);
        assert!(piao_discard_bias(&hand, 1, &[], &table, 0) < 0.0);
        assert!(early_piao_candidate_discard_bias(&hand, 1, &[], &table, 0) < 0.0);
    }

    #[test]
    fn dealer_ignores_marginal_piao_discard_bias_for_speed() {
        let mut table = table_with_discards(1, Vec::new());
        table.dealer_position = 0;
        let hand = vec![1, 1, 2, 2, 11, 11, 12, 13, 21, 21, 22, 31, 32];

        assert!(piao_plan_score(&hand, &[]) >= 20.0);
        assert!(piao_plan_score_for_context(&hand, &[], &table, 0) < 20.0);
        assert_eq!(piao_discard_bias(&hand, 1, &[], &table, 0), 0.0);
        assert_eq!(
            early_piao_candidate_discard_bias(&hand, 1, &[], &table, 0),
            0.0
        );
    }

    #[test]
    fn one_fan_capped_room_disables_piao_plan_biases() {
        let mut table = table_with_discards(1, Vec::new());
        table.max_fan = Some(1);
        let hand = vec![1, 1, 2, 2, 11, 11, 12, 13, 21, 21, 22, 31, 32];

        assert!(piao_plan_score(&hand, &[]) >= 20.0);
        assert_eq!(piao_plan_score_for_context(&hand, &[], &table, 0), 0.0);
        assert_eq!(piao_discard_bias(&hand, 1, &[], &table, 0), 0.0);
        assert_eq!(
            early_piao_candidate_discard_bias(&hand, 1, &[], &table, 0),
            0.0
        );
    }

    #[test]
    fn piao_plan_counts_open_triplet_with_two_pairs_as_route() {
        let hand = vec![11, 11, 12, 13, 14, 21, 21, 23, 24, 31];
        let melds = vec![test_peng_meld(1)];

        assert!(piao_plan_score(&hand, &melds) >= 20.0);
    }

    #[test]
    fn piao_context_requires_terminal_or_honor() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![2, 2, 3, 3, 12, 12, 13, 13, 22, 22, 23, 23, 24];

        assert!(piao_plan_score(&hand, &[]) >= 20.0);
        assert_eq!(piao_plan_score_for_context(&hand, &[], &table, 0), 0.0);
    }

    #[test]
    fn piao_context_requires_three_suits() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 1, 2, 2, 3, 3, 4, 4, 11, 11, 12, 13, 31];

        assert!(piao_plan_score(&hand, &[]) >= 20.0);
        assert_eq!(piao_plan_score_for_context(&hand, &[], &table, 0), 0.0);
    }

    #[test]
    fn claim_peng_passes_raw_piao_shape_without_terminal_or_honor() {
        let mut table = table_with_discards(1, Vec::new());
        table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(2)];
        table.claim_window = Some(AiClaimView {
            tile: 13,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![12, 13, 13, 14, 15, 22, 22, 23, 25, 25];
        let melds = table.seats.get(&0).unwrap().melds.as_slice();

        assert!(piao_plan_score(&hand, melds) >= 32.0);
        assert_eq!(piao_plan_score_for_context(&hand, melds, &table, 0), 0.0);
        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn one_fan_capped_room_does_not_lock_five_pairs_when_basic_route_is_viable() {
        let mut table = table_with_discards(1, Vec::new());
        table.max_fan = Some(1);
        let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 22, 31, 35, 35];

        assert!(has_basic_normal_route_foundation(
            &hand,
            &[],
            WIN_RULE_SHENYANG_BASIC
        ));
        assert!(!should_lock_seven_pairs_plan(
            &hand,
            &[],
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC
        ));
    }

    #[test]
    fn capped_non_dealer_prefers_wider_wait_over_single_wait_fan() {
        let mut table = table_with_discards(1, Vec::new());
        table.max_fan = Some(1);
        table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
        let hand = vec![2, 2, 4, 5, 7, 11, 12, 13, 21, 22, 23];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(7)
        );
    }

    #[test]
    fn claim_chi_can_fill_missing_third_suit() {
        let mut table = table_with_discards(3, Vec::new());
        table.wall_count = 40;
        table.claim_window = Some(AiClaimView {
            tile: 22,
            from_position: 3,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 23, 31, 35];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
            Some(AiClaimChoice::Chi {
                consume_tiles: vec![21, 23]
            })
        );
    }

    #[test]
    fn claim_chi_takes_mid_round_when_it_reaches_ready() {
        let mut table = table_with_discards(3, Vec::new());
        table.wall_count = 40;
        table.claim_window = Some(AiClaimView {
            tile: 3,
            from_position: 3,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 2, 4, 5, 6, 7, 8, 9, 11, 12, 13, 31, 35];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
            Some(AiClaimChoice::Chi {
                consume_tiles: vec![1, 2]
            })
        );
    }

    #[test]
    fn claim_chi_can_use_claim_tile_as_low_edge() {
        let mut table = table_with_discards(3, Vec::new());
        table.wall_count = 40;
        table.claim_window = Some(AiClaimView {
            tile: 1,
            from_position: 3,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![2, 2, 3, 5, 8, 11, 14, 17, 21, 24, 31, 32, 33];

        assert!(should_claim_chi_to_open_broken_hand_for_defense(
            &hand,
            &[],
            &table,
            0,
            WIN_RULE_RELAXED
        ));
        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
            Some(AiClaimChoice::Chi {
                consume_tiles: vec![2, 3]
            })
        );
    }

    #[test]
    fn claim_chi_passes_late_ready_hand() {
        let mut table = table_with_discards(3, Vec::new());
        table.wall_count = 36;
        table.claim_window = Some(AiClaimView {
            tile: 3,
            from_position: 3,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 31, 31];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_chi_preserves_pure_one_suit_plan_from_off_suit_chi() {
        let mut table = table_with_discards(3, Vec::new());
        table.wall_count = 40;
        table.claim_window = Some(AiClaimView {
            tile: 13,
            from_position: 3,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 11, 12, 35];

        assert!(pure_one_suit_plan_score_for_context(&hand, &[], &table, 0) > 0.0);
        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_chi_opens_late_broken_hand_for_defense() {
        let mut table = table_with_discards(3, Vec::new());
        table.wall_count = 40;
        table.claim_window = Some(AiClaimView {
            tile: 3,
            from_position: 3,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 1, 2, 5, 8, 11, 14, 17, 21, 24, 31, 32, 33];

        assert!(should_claim_chi_to_open_broken_hand_for_defense(
            &hand,
            &[],
            &table,
            0,
            WIN_RULE_RELAXED
        ));
        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
            Some(AiClaimChoice::Chi {
                consume_tiles: vec![1, 2]
            })
        );
    }

    #[test]
    fn claim_chi_opens_mid_broken_hand_for_defense() {
        let mut table = table_with_discards(3, Vec::new());
        table.wall_count = 52;
        table.claim_window = Some(AiClaimView {
            tile: 3,
            from_position: 3,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 1, 2, 5, 8, 11, 14, 17, 21, 24, 31, 32, 33];

        assert!(should_claim_chi_to_open_broken_hand_for_defense(
            &hand,
            &[],
            &table,
            0,
            WIN_RULE_RELAXED
        ));
        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
            Some(AiClaimChoice::Chi {
                consume_tiles: vec![1, 2]
            })
        );
    }

    #[test]
    fn claim_chi_does_not_rush_opening_closed_basic_hand_early() {
        let mut table = table_with_discards(3, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 3,
            from_position: 3,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 2, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_chi_passes_early_even_when_it_can_fill_missing_third_suit() {
        let mut table = table_with_discards(3, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 22,
            from_position: 3,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 23, 31, 35];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_chi_passes_for_shenyang_basic_rule_even_late() {
        let mut table = table_with_discards(3, Vec::new());
        table.wall_count = 40;
        table.claim_window = Some(AiClaimView {
            tile: 3,
            from_position: 3,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 2, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_chi_passes_mid_round_when_it_does_not_make_ready_or_defensive_open() {
        let mut table = table_with_discards(3, Vec::new());
        table.wall_count = 40;
        table.claim_window = Some(AiClaimView {
            tile: 3,
            from_position: 3,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 2, 5, 5, 5, 9, 9, 9, 11, 14, 17, 21, 24];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_chi_passes_when_five_pairs_missing_suit_can_chase_seven_pairs() {
        let mut table = table_with_discards(3, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 23,
            from_position: 3,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 1, 2, 2, 3, 3, 11, 11, 21, 21, 22, 31, 32];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_chi_passes_when_piao_plan_is_stronger() {
        let mut table = table_with_discards(3, Vec::new());
        table.seats.get_mut(&0).unwrap().melds = vec![WsShenyangMahjongMeld {
            kind: ShenyangMahjongMeldKind::PENG,
            tiles: vec![1, 1, 1],
            from_position: Some(2),
        }];
        table.claim_window = Some(AiClaimView {
            tile: 22,
            from_position: 3,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![11, 11, 21, 23, 31, 31, 35, 35, 36, 37];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_chi_passes_for_open_triplet_two_pair_piao_route_even_when_chi_reaches_ready() {
        let mut table = table_with_discards(3, Vec::new());
        table.wall_count = 40;
        table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
        table.claim_window = Some(AiClaimView {
            tile: 22,
            from_position: 3,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![11, 11, 12, 13, 14, 21, 21, 23, 24, 31];

        assert!(should_preserve_piao_plan_for_chi(
            &hand,
            table.seats.get(&0).unwrap().melds.as_slice(),
            &table,
            0
        ));
        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_chi_passes_for_four_pair_piao_candidate_in_relaxed_rule() {
        let mut table = table_with_discards(3, Vec::new());
        table.wall_count = 40;
        table.claim_window = Some(AiClaimView {
            tile: 7,
            from_position: 3,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 1, 2, 2, 3, 3, 4, 4, 5, 6, 11, 12, 21];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn piao_chi_preservation_uses_dealer_and_cap_context() {
        let table = table_with_discards(3, Vec::new());
        let hand = vec![1, 1, 2, 2, 3, 3, 4, 4, 5, 6, 11, 12, 21];

        assert!(should_preserve_piao_plan_for_chi(&hand, &[], &table, 0));

        let mut dealer_table = table.clone();
        dealer_table.dealer_position = 0;
        assert!(!should_preserve_piao_plan_for_chi(
            &hand,
            &[],
            &dealer_table,
            0
        ));

        let mut capped_table = table.clone();
        capped_table.max_fan = Some(1);
        assert!(!should_preserve_piao_plan_for_chi(
            &hand,
            &[],
            &capped_table,
            0
        ));
    }

    #[test]
    fn claim_chi_passes_for_three_pair_piao_candidate_even_when_chi_reaches_ready() {
        let mut table = table_with_discards(3, Vec::new());
        table.wall_count = 40;
        table.claim_window = Some(AiClaimView {
            tile: 27,
            from_position: 3,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 1, 5, 5, 11, 12, 13, 22, 23, 24, 24, 28, 29];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_gang_beats_peng_when_not_winning() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 35,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 35, 35, 35];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
            Some(AiClaimChoice::Gang)
        );
    }

    #[test]
    fn claim_gang_takes_dragon_gang_to_open_basic_hand_before_ready() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 35,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 2, 3, 4, 5, 7, 11, 12, 14, 21, 35, 35, 35];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Gang)
        );
    }

    #[test]
    fn one_fan_capped_claim_gang_penges_dragon_for_speed_before_ready() {
        let mut table = table_with_discards(1, Vec::new());
        table.max_fan = Some(1);
        table.claim_window = Some(AiClaimView {
            tile: 35,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 2, 3, 4, 5, 7, 11, 12, 14, 21, 35, 35, 35];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Peng)
        );
    }

    #[test]
    fn claim_gang_delays_open_piao_plain_gang_until_ready_and_pengs() {
        let mut table = table_with_discards(1, Vec::new());
        table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
        table.claim_window = Some(AiClaimView {
            tile: 21,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![11, 11, 21, 21, 21, 31, 31, 35, 35, 36];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Peng)
        );
    }

    #[test]
    fn claim_gang_delays_open_plain_gang_when_not_ready() {
        let mut table = table_with_discards(1, Vec::new());
        table.seats.get_mut(&0).unwrap().melds = vec![test_chi_meld(1)];
        table.claim_window = Some(AiClaimView {
            tile: 9,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![4, 5, 6, 9, 9, 9, 11, 12, 14, 21];

        assert_ne!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Gang)
        );
    }

    #[test]
    fn claim_gang_opens_closed_plain_basic_hand_before_ready() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 3,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![3, 3, 3, 4, 5, 7, 8, 11, 12, 14, 21, 22, 31];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Gang)
        );
    }

    #[test]
    fn claim_gang_penges_closed_early_piao_candidate() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 1,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 1, 1, 4, 5, 6, 11, 11, 12, 13, 21, 21, 22];

        assert!(is_closed_early_piao_candidate(&hand, &[], &table, 0));
        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Peng)
        );
    }

    #[test]
    fn claim_gang_opens_broken_closed_hand_late_for_defense() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 40;
        table.claim_window = Some(AiClaimView {
            tile: 2,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![2, 2, 2, 4, 7, 12, 14, 17, 31, 32, 33, 34, 35];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Gang)
        );
    }

    #[test]
    fn dealer_claim_gang_opens_broken_closed_hand_for_speed() {
        let mut table = table_with_discards(1, Vec::new());
        table.dealer_position = 0;
        table.wall_count = 40;
        table.claim_window = Some(AiClaimView {
            tile: 2,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![2, 2, 2, 4, 7, 12, 14, 17, 31, 32, 33, 34, 35];

        assert!(should_open_broken_closed_hand_for_defense(
            &hand,
            &[],
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC
        ));
        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Gang)
        );
    }

    #[test]
    fn claim_gang_passes_final_unready_broken_hand_for_defense() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 16;
        table.claim_window = Some(AiClaimView {
            tile: 2,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![2, 2, 2, 4, 7, 12, 14, 17, 31, 32, 33, 34, 35];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_gang_opens_broken_closed_hand_for_defense_in_relaxed_rule() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 40;
        table.claim_window = Some(AiClaimView {
            tile: 31,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![2, 5, 8, 12, 14, 17, 21, 24, 27, 31, 31, 31, 34];

        assert!(should_open_broken_closed_hand_for_defense(
            &hand,
            &[],
            &table,
            0,
            WIN_RULE_RELAXED
        ));
        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
            Some(AiClaimChoice::Gang)
        );
    }

    #[test]
    fn claim_gang_opens_mid_missing_suit_no_terminal_hand_for_defense() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 52;
        table.claim_window = Some(AiClaimView {
            tile: 5,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![2, 3, 5, 5, 5, 7, 8, 12, 14, 15, 16, 17, 18];

        assert!(should_open_broken_closed_hand_for_defense(
            &hand,
            &[],
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC
        ));
        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Gang)
        );
    }

    #[test]
    fn claim_gang_passes_when_it_breaks_locked_pure_one_suit_plan() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 11,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 11, 11, 11];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_gang_passes_dragon_when_pure_one_suit_plan_starts_at_eight_tiles() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 35,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 11, 21, 35, 35, 35];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_gang_passes_closed_pure_one_suit_plan_before_ready() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 1,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 1, 1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 11];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_gang_takes_ready_main_suit_pure_one_suit_when_not_capped() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 1,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 1, 1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 9];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Gang)
        );
    }

    #[test]
    fn claim_gang_passes_ready_pure_one_suit_when_visible_fan_capped() {
        let mut table = table_with_discards(1, Vec::new());
        table.max_fan = Some(4);
        table.seats.get_mut(&0).unwrap().melds = vec![test_chi_meld(2)];
        table.claim_window = Some(AiClaimView {
            tile: 1,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 1, 1, 5, 6, 7, 8, 8, 9, 9];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_gang_passes_capped_closed_pure_one_suit_wait() {
        let mut table = table_with_discards(1, Vec::new());
        table.max_fan = Some(4);
        table.claim_window = Some(AiClaimView {
            tile: 1,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 1, 1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 9];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_gang_passes_when_it_breaks_locked_seven_pairs_plan() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 1,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 31];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_gang_preserves_five_pairs_even_for_dragon_gang() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 35,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 22, 35, 35, 35];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_gang_skips_plain_gang_when_ready_fan_already_capped() {
        let mut table = table_with_discards(1, Vec::new());
        table.max_fan = Some(3);
        table.seats.get_mut(&0).unwrap().melds = vec![test_gang_meld(35)];
        table.claim_window = Some(AiClaimView {
            tile: 9,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 2, 3, 9, 9, 9, 11, 12, 13, 21];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_gang_takes_open_plain_gang_when_it_reaches_ready() {
        let mut table = table_with_discards(1, Vec::new());
        table.seats.get_mut(&0).unwrap().melds = vec![test_chi_meld(1)];
        table.claim_window = Some(AiClaimView {
            tile: 9,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![4, 5, 6, 9, 9, 9, 11, 12, 13, 21];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Gang)
        );
    }

    #[test]
    fn claim_gang_penges_to_preserve_four_gui_yi_when_peng_stays_ready() {
        let mut table = table_with_discards(1, Vec::new());
        table.dealer_position = 0;
        table.seats.get_mut(&0).unwrap().melds = vec![test_chi_meld(11)];
        table.claim_window = Some(AiClaimView {
            tile: 4,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![2, 2, 2, 4, 4, 4, 5, 21, 21, 21];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Peng)
        );
    }

    #[test]
    fn claim_gang_takes_late_ready_dragon_gang_when_it_keeps_ready() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 36;
        table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
        table.claim_window = Some(AiClaimView {
            tile: 35,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 2, 3, 11, 12, 13, 21, 35, 35, 35];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Gang)
        );
    }

    #[test]
    fn claim_gang_passes_late_ready_hand_when_gang_breaks_ready() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 36;
        table.claim_window = Some(AiClaimView {
            tile: 6,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![2, 4, 6, 6, 6, 7, 8, 13, 14, 15, 23, 24, 25];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_hu_accepts_open_meld_remainder() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 35,
            from_position: 1,
            eligible_positions: vec![0],
        });
        table.seats.get_mut(&0).unwrap().melds = vec![
            share_type_public::games::shenyang_mahjong::WsShenyangMahjongMeld {
                kind: share_type_public::games::shenyang_mahjong::ShenyangMahjongMeldKind::PENG,
                tiles: vec![1, 1, 1],
                from_position: Some(2),
            },
        ];
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
            Some(AiClaimChoice::Hu)
        );
    }

    #[test]
    fn claim_hu_accepts_seven_pairs() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 35,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
            Some(AiClaimChoice::Hu)
        );
    }

    #[test]
    fn claim_hu_beats_other_claims() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 35,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
            Some(AiClaimChoice::Hu)
        );
    }

    #[test]
    fn claim_hu_still_wins_during_final_defense() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 16;
        table.claim_window = Some(AiClaimView {
            tile: 35,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
            Some(AiClaimChoice::Hu)
        );
    }

    #[test]
    fn claim_peng_allows_dragon_when_missing_suit_can_still_be_recovered() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 35,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 31, 35, 35, 36];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Peng)
        );
    }

    #[test]
    fn claim_peng_passes_main_suit_when_closed_pure_one_suit_plan_is_strong() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 1,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 11, 12];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_peng_passes_nine_tile_pure_one_suit_plan() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 1,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 1, 2, 3, 4, 5, 6, 7, 8, 11, 12, 31, 35];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_peng_passes_main_suit_pure_one_suit_when_opening_is_not_required() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 1,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 11, 12];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_peng_takes_open_main_suit_pure_one_suit_when_it_reaches_ready() {
        let mut table = table_with_discards(1, Vec::new());
        table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
        table.claim_window = Some(AiClaimView {
            tile: 2,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 2, 2, 3, 3, 3, 3, 4, 4, 7];
        let melds = table.seats.get(&0).unwrap().melds.as_slice();

        assert!(pure_one_suit_plan_score_for_context(&hand, melds, &table, 0) > 0.0);
        assert_eq!(
            ready_tile_score(&hand, melds, &table, 0, WIN_RULE_SHENYANG_BASIC),
            0.0
        );
        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Peng)
        );
    }

    #[test]
    fn claim_peng_passes_weak_main_suit_pure_one_suit_start() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 1,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 1, 2, 3, 4, 5, 6, 7, 11, 12, 21, 31, 35];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_peng_preserves_pure_one_suit_seven_pairs_wait() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 1,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 8];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_peng_opens_broken_closed_hand_late_for_defense() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 40;
        table.claim_window = Some(AiClaimView {
            tile: 2,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![2, 2, 4, 7, 12, 14, 17, 31, 32, 33, 34, 35, 36];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Peng)
        );
    }

    #[test]
    fn claim_peng_passes_final_unready_broken_hand_for_defense() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 16;
        table.claim_window = Some(AiClaimView {
            tile: 2,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![2, 2, 4, 7, 12, 14, 17, 31, 32, 33, 34, 35, 36];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_peng_opens_mid_severely_broken_closed_hand_for_defense() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 52;
        table.claim_window = Some(AiClaimView {
            tile: 31,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![2, 5, 8, 11, 14, 17, 19, 31, 31, 33, 35, 36, 37];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Peng)
        );
    }

    #[test]
    fn claim_peng_opens_missing_suit_basic_hand_despite_relaxed_near_ready_shape() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 52;
        table.claim_window = Some(AiClaimView {
            tile: 1,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 1, 2, 3, 4, 5, 6, 11, 12, 13, 14, 15, 31];

        assert!(
            one_step_wait_potential(&hand, &[], &table, 0, WIN_RULE_RELAXED) > 0.0,
            "the relaxed shape is close enough that it used to block defensive opening"
        );
        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Peng)
        );
    }

    #[test]
    fn claim_peng_opens_broken_closed_hand_for_defense_in_relaxed_rule() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 40;
        table.claim_window = Some(AiClaimView {
            tile: 31,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![2, 5, 8, 12, 14, 17, 21, 24, 27, 31, 31, 33, 34];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
            Some(AiClaimChoice::Peng)
        );
    }

    #[test]
    fn relaxed_near_ready_hand_does_not_use_defensive_opening() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 40;
        let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 31, 31, 35];

        assert!(
            ready_tile_score(&hand, &[], &table, 0, WIN_RULE_RELAXED) > 0.0
                || one_step_wait_potential(&hand, &[], &table, 0, WIN_RULE_RELAXED) > 0.0
        );
        assert!(!should_open_broken_closed_hand_for_defense(
            &hand,
            &[],
            &table,
            0,
            WIN_RULE_RELAXED
        ));
    }

    #[test]
    fn claim_peng_passes_dragon_when_pure_one_suit_plan_is_strong() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 35,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 35, 35, 36];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_peng_passes_dragon_when_pure_one_suit_plan_starts_at_eight_tiles() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 35,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 11, 21, 35, 35, 36];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn dealer_claim_peng_can_ignore_early_eight_tile_pure_one_suit_plan() {
        let mut table = table_with_discards(1, Vec::new());
        table.dealer_position = 0;
        table.claim_window = Some(AiClaimView {
            tile: 35,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 11, 21, 35, 35, 36];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Peng)
        );
    }

    #[test]
    fn claim_peng_passes_when_five_pairs_missing_suit_can_chase_seven_pairs() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 21,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 12, 22, 31, 32];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_peng_passes_when_it_breaks_locked_pure_one_suit_plan() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 11,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 11, 11];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_peng_passes_when_it_breaks_seven_pairs_shape() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 6,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 31];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_peng_passes_when_missing_suit_is_unrecoverable_even_for_dragon() {
        let dead_bamboo = (21..=29)
            .flat_map(|tile| std::iter::repeat_n(tile, 4))
            .collect::<Vec<_>>();
        let mut table = table_with_discards(1, dead_bamboo);
        table.claim_window = Some(AiClaimView {
            tile: 35,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 31, 35, 35, 36];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_peng_opens_late_broken_missing_suit_hand_even_for_dragon() {
        let dead_bamboo = (21..=29)
            .flat_map(|tile| std::iter::repeat_n(tile, 4))
            .collect::<Vec<_>>();
        let mut table = table_with_discards(1, dead_bamboo);
        table.wall_count = 40;
        table.claim_window = Some(AiClaimView {
            tile: 35,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 2, 4, 6, 8, 11, 13, 16, 19, 31, 35, 35, 36];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Peng)
        );
    }

    #[test]
    fn claim_peng_passes_when_terminal_or_honor_is_unrecoverable_for_basic() {
        let mut table = table_with_discards(1, dead_terminal_or_honor_discards());
        table.claim_window = Some(AiClaimView {
            tile: 5,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![2, 4, 5, 5, 12, 13, 14, 15, 16, 22, 23, 24, 25];

        assert!(claim_leaves_unrecoverable_terminal_or_honor(
            &hand,
            &[],
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC,
            ShenyangMahjongMeldKind::PENG,
            5,
            1
        ));
        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_gang_passes_when_terminal_or_honor_is_unrecoverable_for_basic() {
        let mut table = table_with_discards(1, dead_terminal_or_honor_discards());
        table.claim_window = Some(AiClaimView {
            tile: 5,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![2, 4, 5, 5, 5, 12, 13, 14, 15, 16, 22, 23, 24];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_peng_opens_late_broken_no_terminal_hand_for_defense() {
        let mut table = table_with_discards(1, dead_terminal_or_honor_discards());
        table.wall_count = 40;
        table.claim_window = Some(AiClaimView {
            tile: 5,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![2, 4, 5, 5, 7, 12, 14, 16, 18, 22, 24, 26, 28];

        assert!(should_open_broken_closed_hand_for_defense(
            &hand,
            &[],
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC
        ));
        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Peng)
        );
    }

    #[test]
    fn claim_peng_opens_mid_unrecoverable_no_terminal_hand_for_defense() {
        let mut table = table_with_discards(1, dead_terminal_or_honor_discards());
        table.wall_count = 52;
        table.claim_window = Some(AiClaimView {
            tile: 5,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![2, 3, 4, 5, 5, 6, 7, 12, 13, 14, 22, 23, 24];

        assert_eq!(
            unrecoverable_basic_rule_requirement_count(&hand, &[], &table),
            1
        );
        assert!(should_open_broken_closed_hand_for_defense(
            &hand,
            &[],
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC
        ));
        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Peng)
        );
    }

    #[test]
    fn broken_closed_defense_waits_mid_recoverable_no_terminal_hand() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 52;
        let hand = vec![2, 2, 2, 5, 5, 6, 7, 12, 13, 14, 22, 23, 24];

        assert_eq!(
            unrecoverable_basic_rule_requirement_count(&hand, &[], &table),
            0
        );
        assert!(!should_open_broken_closed_hand_for_defense(
            &hand,
            &[],
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC
        ));
    }

    #[test]
    fn claim_peng_passes_late_ready_hand_even_for_dragon() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 36;
        table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
        table.claim_window = Some(AiClaimView {
            tile: 35,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 35, 35];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_peng_passes_ready_hand_even_for_dragon_before_late_round() {
        let mut table = table_with_discards(1, Vec::new());
        table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
        table.claim_window = Some(AiClaimView {
            tile: 35,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 35, 35];
        let melds = table.seats.get(&0).unwrap().melds.as_slice();

        assert!(ready_tile_score(&hand, melds, &table, 0, WIN_RULE_SHENYANG_BASIC) > 0.0);
        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_peng_preserves_five_pairs_even_with_three_suits() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 21,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 31, 32];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_peng_preserves_pinghu_sequence_when_open_and_heng_is_ready() {
        let mut table = table_with_discards(1, Vec::new());
        table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
        table.claim_window = Some(AiClaimView {
            tile: 5,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![4, 5, 5, 6, 11, 12, 21, 22, 35, 35];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_peng_pursues_piao_plan_after_open_triplet() {
        let mut table = table_with_discards(1, Vec::new());
        table.seats.get_mut(&0).unwrap().melds = vec![WsShenyangMahjongMeld {
            kind: ShenyangMahjongMeldKind::PENG,
            tiles: vec![1, 1, 1],
            from_position: Some(2),
        }];
        table.claim_window = Some(AiClaimView {
            tile: 21,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![11, 11, 21, 21, 31, 31, 35, 35, 36, 37];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Peng)
        );
    }

    #[test]
    fn claim_peng_still_opens_closed_basic_hand_despite_sequence_shape() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 5,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![4, 5, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35, 35];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Peng)
        );
    }

    #[test]
    fn claim_peng_still_preserves_locked_seven_pairs_over_dragon_pair() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 35,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 12, 35, 35, 36];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn claim_peng_takes_dragon_pair_for_open_and_fan() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 35,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 2, 4, 5, 7, 9, 11, 12, 14, 17, 21, 35, 35];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Peng)
        );
    }

    #[test]
    fn claim_peng_takes_four_pair_three_suit_piao_start() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 21,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 1, 2, 2, 11, 11, 12, 13, 21, 21, 22, 31, 32];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Peng)
        );
    }

    #[test]
    fn relaxed_claim_peng_takes_closed_early_piao_candidate_over_sequence_shape() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 5,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 1, 4, 5, 5, 6, 11, 12, 13, 21, 21, 35, 35];

        assert!(tile_is_middle_of_sequence(&hand, 5));
        assert!(should_claim_peng_for_closed_early_piao_candidate(
            &hand,
            &[],
            &table,
            0,
            5,
            1
        ));
        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
            Some(AiClaimChoice::Peng)
        );
    }

    #[test]
    fn claim_peng_takes_fourth_piao_meld_to_set_up_shou_ba_yi() {
        let mut table = table_with_discards(1, Vec::new());
        table.seats.get_mut(&0).unwrap().melds =
            vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];
        table.claim_window = Some(AiClaimView {
            tile: 35,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![35, 35, 36, 37];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Peng)
        );
    }

    #[test]
    fn claim_peng_takes_three_pair_three_suit_piao_start() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 1,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 1, 4, 5, 6, 11, 11, 12, 13, 21, 21, 22, 23];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Peng)
        );
    }

    #[test]
    fn claim_peng_takes_basic_heng_and_opening_when_no_heng() {
        let mut table = table_with_discards(1, Vec::new());
        table.claim_window = Some(AiClaimView {
            tile: 5,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 2, 4, 5, 5, 7, 8, 11, 13, 15, 21, 24, 31];

        assert!(!has_open_meld(
            table.seats.get(&0).unwrap().melds.as_slice()
        ));
        assert!(!has_triplet_or_dragon_pair(&hand, &[]));
        assert_eq!(
            ready_tile_score(&hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC),
            0.0
        );
        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Peng)
        );
    }

    #[test]
    fn closed_opponent_threat_does_not_penalize_public_safe_tile() {
        let mut table = table_with_discards(1, vec![31]);
        table.wall_count = 16;
        table.seats.get_mut(&1).unwrap().hand_count = 13;

        assert_eq!(closed_opponent_threat_discard_bias(&table, 0, 31, 1), 0.0);
        assert!(closed_opponent_threat_discard_bias(&table, 0, 32, 1) < 0.0);
    }

    #[test]
    fn closed_opponent_threat_discounts_exposed_meld_tiles() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 16;
        table.seats.get_mut(&1).unwrap().hand_count = 13;
        table.seats.insert(
            2,
            AiSeatView {
                position: 2,
                hand_count: 10,
                discards: Vec::new(),
                melds: vec![test_peng_meld(9)],
            },
        );

        let exposed_terminal_bias = closed_opponent_threat_discard_bias(&table, 0, 9, 1);
        let cold_honor_bias = closed_opponent_threat_discard_bias(&table, 0, 31, 1);

        assert!(exposed_terminal_bias < 0.0);
        assert!(exposed_terminal_bias > cold_honor_bias);
    }

    #[test]
    fn closed_opponent_threat_ignores_fully_exposed_tile() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 16;
        table.seats.get_mut(&1).unwrap().hand_count = 13;
        table.seats.insert(
            2,
            AiSeatView {
                position: 2,
                hand_count: 10,
                discards: Vec::new(),
                melds: vec![test_gang_meld(9)],
            },
        );

        assert_eq!(closed_opponent_threat_discard_bias(&table, 0, 9, 1), 0.0);
    }

    #[test]
    fn closed_opponent_threat_counts_ai_controlled_table_seat() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 16;
        table.seats.get_mut(&1).unwrap().hand_count = 13;

        assert!(closed_opponent_threat_discard_bias(&table, 0, 32, 1) < 0.0);
    }

    #[test]
    fn closed_opponent_threat_counts_concealed_gang_as_closed() {
        let mut concealed = table_with_discards(1, Vec::new());
        concealed.wall_count = 16;
        concealed.seats.get_mut(&1).unwrap().melds = vec![test_concealed_gang_meld(9)];

        let mut open = table_with_discards(1, Vec::new());
        open.wall_count = 16;
        open.seats.get_mut(&1).unwrap().melds = vec![test_gang_meld(9)];

        assert!(closed_opponent_threat_discard_bias(&concealed, 0, 32, 1) < 0.0);
        assert_eq!(closed_opponent_threat_discard_bias(&open, 0, 32, 1), 0.0);
    }

    #[test]
    fn closed_opponent_threat_penalizes_cold_pair_more_than_singleton() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 16;
        table.seats.get_mut(&1).unwrap().hand_count = 13;

        assert!(
            closed_opponent_threat_discard_bias(&table, 0, 9, 2)
                < closed_opponent_threat_discard_bias(&table, 0, 19, 1)
        );
    }

    #[test]
    fn closed_opponent_threat_starts_before_final_defense() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 37;
        table.seats.get_mut(&1).unwrap().hand_count = 13;
        let mid_round_bias = closed_opponent_threat_discard_bias(&table, 0, 32, 1);
        table.wall_count = 16;
        let late_defense_bias = closed_opponent_threat_discard_bias(&table, 0, 32, 1);

        assert!(mid_round_bias < 0.0);
        assert!(mid_round_bias > late_defense_bias);
    }

    #[test]
    fn late_defense_can_follow_exposed_terminal_over_live_wind() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 16;
        table.seats.get_mut(&1).unwrap().hand_count = 13;
        table.seats.insert(
            2,
            AiSeatView {
                position: 2,
                hand_count: 10,
                discards: Vec::new(),
                melds: vec![test_peng_meld(9)],
            },
        );
        let hand = vec![2, 4, 6, 8, 9, 12, 14, 16, 18, 22, 24, 26, 28, 31];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(9)
        );
    }

    #[test]
    fn late_defense_avoids_breaking_cold_terminal_pair_against_closed_opponent() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 16;
        table.seats.get_mut(&1).unwrap().hand_count = 13;
        let hand = vec![2, 4, 6, 8, 9, 9, 12, 14, 16, 18, 19, 22, 24, 26];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(19)
        );
    }

    #[test]
    fn dealer_claim_chi_passes_for_shenyang_basic_rule() {
        let mut table = table_with_discards(3, Vec::new());
        table.dealer_position = 0;
        table.claim_window = Some(AiClaimView {
            tile: 3,
            from_position: 3,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 2, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn dealer_claim_peng_does_not_chase_early_pure_one_suit_plan() {
        let mut table = table_with_discards(1, Vec::new());
        table.dealer_position = 0;
        table.claim_window = Some(AiClaimView {
            tile: 1,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 1, 2, 3, 4, 5, 6, 7, 8, 11, 12, 31, 35];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Peng)
        );
    }

    #[test]
    fn dealer_claim_peng_preserves_five_pairs_when_basic_hand_is_missing_suit() {
        let mut table = table_with_discards(1, Vec::new());
        table.dealer_position = 0;
        table.claim_window = Some(AiClaimView {
            tile: 11,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 12, 31, 32, 33];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn dealer_claim_peng_uses_dragon_pair_for_speed_when_basic_route_is_viable() {
        let mut table = table_with_discards(1, Vec::new());
        table.dealer_position = 0;
        table.claim_window = Some(AiClaimView {
            tile: 35,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 1, 2, 2, 11, 11, 12, 21, 21, 22, 31, 35, 35];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Peng)
        );
    }

    #[test]
    fn one_fan_capped_claim_peng_uses_dragon_pair_for_speed_over_five_pairs() {
        let mut table = table_with_discards(1, Vec::new());
        table.max_fan = Some(1);
        table.claim_window = Some(AiClaimView {
            tile: 35,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 22, 31, 35, 35];

        assert!(!should_lock_seven_pairs_plan(
            &hand,
            &[],
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC
        ));
        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Peng)
        );
    }

    #[test]
    fn dealer_does_not_lock_five_pairs_when_basic_route_is_viable() {
        let mut table = table_with_discards(1, Vec::new());
        table.dealer_position = 0;
        let hand = vec![1, 1, 2, 2, 11, 11, 12, 21, 21, 22, 31, 35, 35, 36];

        assert!(!should_lock_seven_pairs_plan(
            &hand,
            &[],
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC
        ));
    }

    #[test]
    fn dealer_claim_peng_preserves_six_pairs_seven_pairs_plan() {
        let mut table = table_with_discards(1, Vec::new());
        table.dealer_position = 0;
        table.claim_window = Some(AiClaimView {
            tile: 35,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 31, 35, 35];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn dealer_claim_peng_preserves_four_pairs_when_basic_hand_is_missing_suit() {
        let mut table = table_with_discards(1, Vec::new());
        table.dealer_position = 0;
        table.claim_window = Some(AiClaimView {
            tile: 11,
            from_position: 1,
            eligible_positions: vec![0],
        });
        let claim = table.claim_window.clone().unwrap();
        let hand = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 13, 14, 31, 35];

        assert_eq!(
            choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(AiClaimChoice::Pass)
        );
    }

    #[test]
    fn dealer_discard_does_not_chase_early_pure_one_suit_by_breaking_second_suit() {
        let mut table = table_with_discards(1, Vec::new());
        table.dealer_position = 0;
        let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 11, 12, 31, 35];

        assert!(!matches!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(11 | 12)
        ));
    }

    #[test]
    fn dealer_does_not_start_pure_one_suit_plan_at_eight_main_suit_tiles() {
        let mut table = table_with_discards(1, Vec::new());
        table.dealer_position = 0;
        let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 11, 12, 21, 22, 31, 35];

        assert!(!matches!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(11 | 12 | 21 | 22)
        ));
    }

    #[test]
    fn dealer_can_chase_overwhelming_pure_one_suit_shape() {
        let mut table = table_with_discards(1, Vec::new());
        table.dealer_position = 0;
        let hand = vec![1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 11, 35];

        assert!(matches!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(11 | 35)
        ));
    }

    #[test]
    fn dealer_prefers_wider_wait_over_single_wait_fan() {
        let mut table = table_with_discards(1, Vec::new());
        table.dealer_position = 0;
        table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
        let hand = vec![2, 2, 4, 5, 7, 11, 12, 13, 21, 22, 23];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(7)
        );
    }

    #[test]
    fn dealer_self_gang_delays_closed_plain_gang_before_opening_basic_hand() {
        let mut table = table_with_discards(1, Vec::new());
        table.dealer_position = 0;
        let hand = vec![3, 3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31];

        assert_eq!(
            choose_self_gang_from_view(&hand, &[3], &table, 0, WIN_RULE_SHENYANG_BASIC),
            None
        );
    }

    #[test]
    fn discard_after_four_piao_melds_keeps_live_single_wait() {
        let mut table = table_with_discards(1, vec![36, 36, 36]);
        table.seats.get_mut(&0).unwrap().melds = vec![
            test_peng_meld(1),
            test_peng_meld(11),
            test_peng_meld(21),
            test_peng_meld(31),
        ];
        let hand = vec![36, 37];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(36)
        );
    }

    #[test]
    fn discard_after_four_piao_melds_rejects_dead_exposed_wind_wait() {
        let mut table = table_with_discards(1, Vec::new());
        table.seats.get_mut(&0).unwrap().melds = vec![
            test_peng_meld(1),
            test_peng_meld(11),
            test_peng_meld(21),
            test_peng_meld(31),
        ];
        let hand = vec![5, 31];

        assert_eq!(remaining_tile_count(&[31], &table, 0, 31), 0);
        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(31)
        );
    }

    #[test]
    fn dealer_four_piao_melds_prefers_live_middle_over_low_live_wind_wait() {
        let mut table = table_with_discards(1, vec![31, 31]);
        table.dealer_position = 0;
        table.seats.get_mut(&0).unwrap().melds = vec![
            test_peng_meld(1),
            test_peng_meld(11),
            test_peng_meld(21),
            test_peng_meld(32),
        ];
        let hand = vec![5, 31];
        let melds = table.seats.get(&0).unwrap().melds.as_slice();

        assert!(
            piao_single_wait_tile_score(5, &[5], melds, &table, 0, WIN_RULE_SHENYANG_BASIC)
                > piao_single_wait_tile_score(31, &[31], melds, &table, 0, WIN_RULE_SHENYANG_BASIC)
        );
        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(31)
        );
    }

    #[test]
    fn capped_four_piao_melds_prefers_wider_wait_over_honor_shape() {
        let mut table = table_with_discards(1, vec![31]);
        table.max_fan = Some(4);
        table.seats.get_mut(&0).unwrap().melds = vec![
            test_peng_meld(1),
            test_peng_meld(11),
            test_peng_meld(21),
            test_peng_meld(32),
        ];
        let hand = vec![5, 31];
        let melds = table.seats.get(&0).unwrap().melds.as_slice();

        assert!(
            piao_single_wait_tile_score(5, &[5], melds, &table, 0, WIN_RULE_SHENYANG_BASIC)
                > piao_single_wait_tile_score(31, &[31], melds, &table, 0, WIN_RULE_SHENYANG_BASIC)
        );
        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(31)
        );
    }

    #[test]
    fn discard_avoids_live_pair_against_piao_threat() {
        let mut table = table_with_discards(1, vec![31]);
        table.wall_count = 32;
        table.seats.get_mut(&1).unwrap().melds =
            vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];
        let hand = vec![2, 3, 4, 5, 5, 6, 7, 11, 12, 13, 21, 22, 23, 31];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(31)
        );
    }

    #[test]
    fn discard_follows_public_tile_over_live_pair_against_piao_threat() {
        let mut table = table_with_discards(1, vec![14]);
        table.wall_count = 32;
        table.seats.get_mut(&1).unwrap().melds =
            vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];
        let hand = vec![2, 3, 4, 5, 5, 6, 7, 11, 12, 13, 14, 21, 22, 23];

        assert!(
            opponent_threat_discard_bias(&table, 0, 5, 2)
                < opponent_threat_discard_bias(&table, 0, 14, 1)
        );
        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(14)
        );
    }

    #[test]
    fn discard_can_pursue_pure_one_suit_when_shape_is_strong() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 7, 8, 9, 11];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(11)
        );
    }

    #[test]
    fn discard_clears_honor_when_early_pure_one_suit_plan_is_available() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 11, 12, 31, 35];

        assert!(matches!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(31 | 35)
        ));
    }

    #[test]
    fn discard_clears_honor_before_off_suit_singleton_for_pure_one_suit_plan() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 11, 31, 35, 36];

        assert!(matches!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(31 | 35 | 36)
        ));
    }

    #[test]
    fn discard_clears_last_honor_for_pure_one_suit_without_terminal_need() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 8, 8, 12, 31];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(31)
        );
    }

    #[test]
    fn discard_keeps_pairs_for_basic_seven_pairs_plan_when_missing_suit() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 12, 31, 35, 36, 37];

        let discard = choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC);

        assert!(matches!(discard, Some(31 | 36 | 37)));
    }

    #[test]
    fn discard_keeps_four_pairs_for_basic_seven_pairs_when_missing_suit() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 13, 14, 15, 31, 35];

        assert!(!matches!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(1 | 2 | 3 | 11)
        ));
    }

    #[test]
    fn dealer_discard_keeps_four_pairs_when_basic_hand_is_missing_suit() {
        let mut table = table_with_discards(1, Vec::new());
        table.dealer_position = 0;
        let hand = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 13, 14, 15, 31, 35];

        assert!(!matches!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(1 | 2 | 3 | 11)
        ));
    }

    #[test]
    fn discard_keeps_pairs_when_many_pairs_can_chase_seven_pairs() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 22, 23, 31, 35, 36];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(31)
        );
    }

    #[test]
    fn seven_pairs_plan_protects_honor_and_terminal_pairs_more() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 1, 5, 5, 11, 11, 12, 12, 21, 21, 31, 31, 35, 36];

        let middle_pair =
            seven_pairs_plan_discard_bias(&hand, 5, &[], &table, 0, WIN_RULE_SHENYANG_BASIC);
        let terminal_pair =
            seven_pairs_plan_discard_bias(&hand, 1, &[], &table, 0, WIN_RULE_SHENYANG_BASIC);
        let honor_pair =
            seven_pairs_plan_discard_bias(&hand, 31, &[], &table, 0, WIN_RULE_SHENYANG_BASIC);

        assert!(honor_pair < terminal_pair);
        assert!(terminal_pair < middle_pair);
    }

    #[test]
    fn discard_locked_five_pairs_prefers_honor_singleton_first() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 1, 2, 2, 5, 9, 11, 11, 12, 12, 14, 21, 21, 31];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(31)
        );
    }

    #[test]
    fn discard_locked_five_pairs_prefers_non_terminal_singleton_over_terminal() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 1, 2, 2, 5, 9, 11, 11, 12, 12, 14, 19, 21, 21];

        assert!(matches!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(5 | 14)
        ));
    }

    #[test]
    fn late_defense_preserves_locked_five_pairs_over_public_pair_tile() {
        let mut table = table_with_discards(1, vec![1]);
        table.wall_count = 20;
        let hand = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 14, 21, 21, 31, 35];

        assert!(!matches!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(1 | 2 | 11 | 12 | 21)
        ));
    }

    #[test]
    fn late_defense_locked_five_pairs_follows_public_singleton_without_breaking_pairs() {
        let mut table = table_with_discards(1, vec![5]);
        table.wall_count = 20;
        let hand = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 14, 21, 21, 31, 35];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(5)
        );
    }

    #[test]
    fn discard_prefers_isolated_honor() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(31)
        );
    }

    #[test]
    fn discard_clears_isolated_edge_before_core_middle() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![5, 8, 11, 11, 11, 19, 19, 19, 21, 21, 21, 22, 22, 22];

        assert!(
            isolated_suited_singleton_discard_bias(8) > isolated_suited_singleton_discard_bias(5)
        );
        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(8)
        );
    }

    #[test]
    fn discard_breaks_weak_edge_wait_before_core_two_sided_wait() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 2, 4, 5, 11, 12, 13, 21, 22, 23, 24, 25, 35, 35];

        assert!(
            incomplete_sequence_discard_bias(&hand, 1, &[], &table, 0, WIN_RULE_SHENYANG_BASIC)
                > incomplete_sequence_discard_bias(
                    &hand,
                    4,
                    &[],
                    &table,
                    0,
                    WIN_RULE_SHENYANG_BASIC
                )
        );
        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(1)
        );
    }

    #[test]
    fn discard_breaks_weak_edge_wait_before_core_closed_middle_wait() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 2, 4, 6, 11, 12, 13, 21, 22, 23, 24, 25, 35, 35];

        assert!(tile_is_core_closed_middle_wait_member(&hand, 4));
        assert!(
            incomplete_sequence_discard_bias(&hand, 1, &[], &table, 0, WIN_RULE_SHENYANG_BASIC)
                > incomplete_sequence_discard_bias(
                    &hand,
                    4,
                    &[],
                    &table,
                    0,
                    WIN_RULE_SHENYANG_BASIC
                )
        );
        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(1)
        );
    }

    #[test]
    fn incomplete_sequence_bias_does_not_override_piao_pair_plan() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 2, 4, 5, 11, 11, 12, 13, 21, 21, 22, 23, 35, 35];

        assert_eq!(
            incomplete_sequence_discard_bias(&hand, 1, &[], &table, 0, WIN_RULE_SHENYANG_BASIC),
            0.0
        );
        assert!(!matches!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(11 | 21 | 35)
        ));
    }

    #[test]
    fn discard_preserves_middle_of_complete_sequence() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![4, 5, 6, 8, 11, 11, 11, 19, 19, 19, 21, 21, 22, 22];

        assert!(
            complete_sequence_discard_bias(&hand, 5, &[], &table, 0)
                < complete_sequence_discard_bias(&hand, 8, &[], &table, 0)
        );
        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(8)
        );
    }

    #[test]
    fn discard_preserves_edge_of_complete_sequence() {
        let mut table = table_with_discards(1, Vec::new());
        table.dealer_position = 0;
        let hand = vec![4, 5, 6, 8, 11, 11, 11, 19, 19, 19, 21, 21, 22, 22];

        assert!(tile_is_part_of_complete_sequence(&hand, 4));
        assert!(
            complete_sequence_discard_bias(&hand, 4, &[], &table, 0)
                < complete_sequence_discard_bias(&hand, 8, &[], &table, 0)
        );
        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(8)
        );
    }

    #[test]
    fn discard_prefers_wind_before_single_dragon() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(31)
        );
    }

    #[test]
    fn discard_can_clear_single_dragon_when_pairs_are_many() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 23, 24, 26, 35];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(35)
        );
    }

    #[test]
    fn discard_preserves_four_pair_piao_candidate_over_public_pair_tile() {
        let mut table = table_with_discards(1, vec![11, 11]);
        table.wall_count = 36;
        let hand = vec![1, 1, 4, 5, 11, 11, 12, 13, 21, 21, 22, 23, 31, 31];

        assert!(!matches!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(1 | 11 | 21 | 31)
        ));
    }

    #[test]
    fn discard_preserves_open_piao_pairs_over_public_pair_tile() {
        let mut table = table_with_discards(1, vec![11, 11]);
        table.wall_count = 36;
        table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
        let hand = vec![11, 11, 12, 21, 21, 22, 23, 24, 31, 35, 36];

        assert!(!matches!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(11 | 21)
        ));
    }

    #[test]
    fn piao_discard_bias_locks_pairs_after_two_triplet_groups() {
        let table = table_with_discards(1, Vec::new());
        let one_group_melds = vec![test_peng_meld(1)];
        let one_group_hand = vec![11, 11, 21, 21, 22, 23, 31, 35, 36, 37];
        let two_group_melds = vec![test_peng_meld(1), test_peng_meld(11)];
        let two_group_hand = vec![21, 21, 22, 23, 31, 35, 36, 37];

        assert_eq!(
            piao_discard_bias(&one_group_hand, 21, &one_group_melds, &table, 0),
            -16.0
        );
        assert_eq!(
            piao_discard_bias(&two_group_hand, 21, &two_group_melds, &table, 0),
            -24.0
        );
    }

    #[test]
    fn discard_preserves_committed_piao_pair_over_public_pair_tile() {
        let mut table = table_with_discards(1, vec![21, 21]);
        table.wall_count = 36;
        table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1), test_peng_meld(11)];
        let hand = vec![21, 21, 22, 23, 24, 31, 35, 36];

        assert!(!matches!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(21)
        ));
    }

    #[test]
    fn discard_preserves_only_terminal_or_honor_for_piao_plan_even_relaxed() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![2, 2, 5, 5, 8, 8, 12, 12, 15, 16, 18, 22, 24, 31];

        assert_ne!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(31)
        );
    }

    #[test]
    fn discard_preserves_only_third_suit_for_piao_plan_even_relaxed() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![2, 2, 2, 5, 5, 8, 8, 12, 12, 12, 15, 15, 22, 31];

        assert_ne!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(22)
        );
    }

    #[test]
    fn discard_preserves_last_honor_for_basic_rule() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![2, 3, 4, 5, 6, 7, 12, 13, 14, 22, 23, 24, 31, 5];

        assert_ne!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(31)
        );
    }

    #[test]
    fn discard_preserves_only_dragon_pair_for_basic_heng() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 35, 35];

        assert_ne!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(35)
        );
    }

    #[test]
    fn discard_preserves_only_pair_as_basic_heng_seed() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 2, 3, 5, 5, 6, 7, 8, 11, 12, 13, 21, 22, 23];

        assert!(!has_triplet_or_dragon_pair(&hand, &[]));
        assert_ne!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(5)
        );
    }

    #[test]
    fn discard_preserves_ready_four_gui_yi_route() {
        let mut table = table_with_discards(1, Vec::new());
        table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(2)];
        let melds = table.seats.get(&0).unwrap().melds.as_slice();
        let hand = vec![2, 3, 4, 11, 12, 13, 21, 22, 23, 35, 36];
        let after_safe_discard = remove_n_tiles(&hand, 36, 1);
        let after_four_gui_yi_discard = remove_n_tiles(&hand, 2, 1);

        assert_eq!(estimated_four_gui_yi_fan(&hand, melds), 1);
        assert_eq!(
            estimated_four_gui_yi_fan(&after_four_gui_yi_discard, melds),
            0
        );
        assert!(
            ready_tile_score(
                &after_safe_discard,
                melds,
                &table,
                0,
                WIN_RULE_SHENYANG_BASIC
            ) > 0.0
        );
        assert!(
            four_gui_yi_discard_bias(&hand, 2, melds, &table, 0, WIN_RULE_SHENYANG_BASIC)
                < four_gui_yi_discard_bias(&hand, 36, melds, &table, 0, WIN_RULE_SHENYANG_BASIC)
        );
        assert_ne!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(2)
        );
    }

    #[test]
    fn discard_preserves_only_triplet_for_basic_heng() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 1, 1, 2, 3, 4, 11, 12, 13, 21, 22, 23, 35, 36];

        assert_ne!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(1)
        );
    }

    #[test]
    fn discard_preserves_last_suit_for_basic_rule() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 12, 13, 21, 31];

        assert_ne!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(21)
        );
    }

    #[test]
    fn discard_preserves_last_tile_of_a_suit_for_three_suits() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 11, 12, 13, 14, 15, 16, 21, 22, 23, 24, 25, 26, 31];

        assert_ne!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(1)
        );
    }

    #[test]
    fn discard_preserves_only_terminal_or_honor_for_basic_rule() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 5, 6];

        assert_ne!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(1)
        );
    }

    #[test]
    fn discard_preserves_ready_hand_instead_of_breaking_wait() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 31, 31, 32];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(32)
        );
    }

    #[test]
    fn discard_preserves_three_pair_three_suit_piao_candidate() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 1, 4, 5, 6, 11, 11, 12, 13, 14, 21, 21, 22, 23];

        assert!(!matches!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(1 | 11 | 21)
        ));
    }

    #[test]
    fn discard_preserves_only_terminal_or_honor_for_three_pair_piao_candidate_even_relaxed() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![2, 2, 5, 5, 8, 8, 12, 14, 15, 16, 22, 24, 26, 31];

        assert_ne!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(31)
        );
    }

    #[test]
    fn discard_preserves_only_third_suit_for_three_pair_piao_candidate_even_relaxed() {
        let mut table = table_with_discards(1, vec![24]);
        table.wall_count = 36;
        let hand = vec![2, 2, 5, 5, 8, 8, 12, 14, 15, 16, 24, 31, 35, 37];

        assert_ne!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(24)
        );
    }

    #[test]
    fn discard_returns_none_for_seven_pairs_win() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            None
        );
    }

    #[test]
    fn discard_sets_seven_pairs_wait_on_live_terminal_over_dead_wind() {
        let table = table_with_discards(1, vec![31, 31]);
        let hand = vec![1, 1, 2, 2, 9, 11, 11, 12, 12, 21, 21, 22, 22, 31];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(31)
        );
    }

    #[test]
    fn discard_sets_seven_pairs_wait_away_from_public_middle_tile() {
        let table = table_with_discards(1, vec![5]);
        let hand = vec![1, 1, 2, 2, 5, 9, 11, 11, 12, 12, 21, 21, 22, 22];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(5)
        );
    }

    #[test]
    fn discard_sets_seven_pairs_wait_on_live_wind_before_middle_tile() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 21, 21, 22, 22, 31];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(5)
        );
    }

    #[test]
    fn discard_sets_seven_pairs_wait_on_live_terminal_before_middle_tile() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 1, 2, 2, 5, 9, 11, 11, 12, 12, 21, 21, 22, 22];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(5)
        );
    }

    #[test]
    fn discard_sets_seven_pairs_wait_by_breaking_dead_triplet_wait() {
        let table = table_with_discards(1, vec![31]);
        let hand = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 21, 21, 31, 31, 31];
        let dead_wind_wait = remove_n_tiles(&hand, 5, 1);
        let live_middle_wait = remove_n_tiles(&hand, 31, 1);

        assert_eq!(remaining_tile_count(&dead_wind_wait, &table, 0, 31), 0);
        assert!(
            seven_pairs_wait_tile_score(5, &live_middle_wait, &table, 0)
                > seven_pairs_wait_tile_score(31, &dead_wind_wait, &table, 0)
        );
        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(31)
        );
    }

    #[test]
    fn ready_score_values_live_wind_over_middle_for_dealer_seven_pairs() {
        let mut table = table_with_discards(1, Vec::new());
        table.dealer_position = 0;
        let wind_wait = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 31];
        let middle_wait = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 21, 21, 22, 22];

        assert!(
            ready_tile_score(&wind_wait, &[], &table, 0, WIN_RULE_SHENYANG_BASIC)
                > ready_tile_score(&middle_wait, &[], &table, 0, WIN_RULE_SHENYANG_BASIC)
        );
    }

    #[test]
    fn capped_ready_score_keeps_wind_shape_as_seven_pairs_tiebreaker() {
        let mut table = table_with_discards(1, Vec::new());
        table.max_fan = Some(4);
        let wind_wait = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 31];
        let middle_wait = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 21, 21, 22, 22];

        assert!(
            ready_tile_score(&wind_wait, &[], &table, 0, WIN_RULE_SHENYANG_BASIC)
                > ready_tile_score(&middle_wait, &[], &table, 0, WIN_RULE_SHENYANG_BASIC)
        );
    }

    #[test]
    fn capped_ready_score_prefers_live_middle_over_public_wind_wait() {
        let mut table = table_with_discards(1, vec![31]);
        table.max_fan = Some(4);
        let wind_wait = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 31];
        let middle_wait = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 21, 21, 22, 22];

        assert!(
            ready_tile_score(&middle_wait, &[], &table, 0, WIN_RULE_SHENYANG_BASIC)
                > ready_tile_score(&wind_wait, &[], &table, 0, WIN_RULE_SHENYANG_BASIC)
        );
    }

    #[test]
    fn seven_pairs_wait_score_prefers_live_middle_over_public_wind() {
        let table = table_with_discards(1, vec![31]);
        let wind_wait = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 31];
        let middle_wait = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 21, 21, 22, 22];

        assert!(
            seven_pairs_wait_tile_score(5, &middle_wait, &table, 0)
                > seven_pairs_wait_tile_score(31, &wind_wait, &table, 0)
        );
    }

    #[test]
    fn seven_pairs_wait_score_rejects_dead_exposed_wind_wait() {
        let mut table = table_with_discards(1, Vec::new());
        table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(31)];
        let wind_wait = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 31];
        let middle_wait = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 21, 21, 22, 22];
        let hand = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 21, 21, 22, 22, 31];

        assert_eq!(remaining_tile_count(&wind_wait, &table, 0, 31), 0);
        assert!(
            seven_pairs_wait_tile_score(5, &middle_wait, &table, 0)
                > seven_pairs_wait_tile_score(31, &wind_wait, &table, 0)
        );
        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(31)
        );
    }

    #[test]
    fn capped_discard_sets_seven_pairs_wait_on_live_wind_tiebreaker() {
        let mut table = table_with_discards(1, Vec::new());
        table.max_fan = Some(4);
        let hand = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 21, 21, 22, 22, 31];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(5)
        );
    }

    #[test]
    fn discard_starts_pure_one_suit_plan_at_eight_main_suit_tiles() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 11, 12, 21, 22, 31, 35];

        assert!(matches!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(11 | 12 | 21 | 22 | 31 | 35)
        ));
    }

    #[test]
    fn discard_uses_public_discard_safety() {
        let table = table_with_discards(1, vec![31]);
        let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 32];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(31)
        );
    }

    #[test]
    fn mid_round_discard_follows_public_honor_over_live_dragon() {
        let mut table = table_with_discards(1, vec![31]);
        table.wall_count = 46;
        let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 36];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(31)
        );
    }

    #[test]
    fn mid_round_discard_follows_public_dragon_over_multiple_public_terminal() {
        let mut table = table_with_discards(1, vec![9, 9, 35]);
        table.wall_count = 46;
        let hand = vec![1, 2, 3, 9, 11, 12, 14, 16, 18, 21, 22, 24, 26, 35];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(35)
        );
    }

    #[test]
    fn mid_round_discard_follows_public_middle_before_late_round() {
        let mut table = table_with_discards(1, vec![14]);
        table.wall_count = 55;
        let hand = vec![1, 2, 3, 9, 11, 12, 14, 16, 18, 21, 22, 24, 26, 35];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(14)
        );
    }

    #[test]
    fn mid_round_live_dragon_risk_grows_when_opponents_are_open() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 42;
        let base = mid_round_live_honor_risk_bias(&table, 0, 35, 1);
        table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(9)];
        table.seats.insert(
            2,
            AiSeatView {
                position: 2,
                hand_count: 10,
                discards: Vec::new(),
                melds: vec![test_peng_meld(16)],
            },
        );

        assert!(mid_round_live_honor_risk_bias(&table, 0, 35, 1) < base);
    }

    #[test]
    fn mid_round_live_dragon_risk_ignores_concealed_gang_opponent() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 42;
        let base = mid_round_live_honor_risk_bias(&table, 0, 35, 1);

        table.seats.get_mut(&1).unwrap().melds = vec![test_concealed_gang_meld(9)];
        assert_eq!(mid_round_live_honor_risk_bias(&table, 0, 35, 1), base);

        table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(9)];
        assert!(mid_round_live_honor_risk_bias(&table, 0, 35, 1) < base);
    }

    #[test]
    fn mid_round_open_dragon_meld_does_not_add_live_dragon_pressure() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 42;
        let base = mid_round_live_honor_risk_bias(&table, 0, 35, 1);

        table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(35)];
        assert_eq!(open_opponent_live_dragon_risk(&table, 0, 35), 0.0);
        assert_eq!(mid_round_live_honor_risk_bias(&table, 0, 35, 1), base);

        table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(9)];
        assert!(open_opponent_live_dragon_risk(&table, 0, 35) > 0.0);
        assert!(mid_round_live_honor_risk_bias(&table, 0, 35, 1) < base);
    }

    #[test]
    fn mid_round_open_honor_meld_tile_is_safer_than_live_dragon() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 42;
        table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(35)];

        let exposed_dragon_safety = mid_round_open_meld_safety_bias(&table, 35);
        let live_dragon_safety = mid_round_open_meld_safety_bias(&table, 36);
        assert!(exposed_dragon_safety > 0.0);
        assert_eq!(live_dragon_safety, 0.0);

        let exposed_dragon_score =
            exposed_dragon_safety + mid_round_live_honor_risk_bias(&table, 0, 35, 1);
        let live_dragon_score =
            live_dragon_safety + mid_round_live_honor_risk_bias(&table, 0, 36, 1);
        assert!(exposed_dragon_score > live_dragon_score);
    }

    #[test]
    fn mid_round_discard_avoids_live_dragon_against_open_opponent() {
        let mut table = table_with_discards(1, vec![14]);
        table.wall_count = 42;
        table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(9)];
        let hand = vec![1, 2, 3, 11, 12, 13, 14, 16, 18, 21, 22, 23, 31, 35];

        assert_ne!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(35)
        );
    }

    #[test]
    fn mid_round_discard_follows_public_middle_over_live_terminal() {
        let mut table = table_with_discards(1, vec![14]);
        table.wall_count = 37;
        let hand = vec![1, 2, 3, 9, 11, 12, 14, 16, 18, 21, 22, 24, 26, 31];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(14)
        );
    }

    #[test]
    fn mid_round_discard_follows_public_middle_over_cold_wind_against_closed_opponent() {
        let mut table = table_with_discards(1, vec![14]);
        table.wall_count = 37;
        table.seats.get_mut(&1).unwrap().hand_count = 13;
        let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 14, 21, 22, 23, 31];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(14)
        );
    }

    #[test]
    fn mid_round_live_suited_risk_grows_when_opponents_are_open() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 37;
        let hand = vec![1, 2, 3, 9, 11, 12, 14, 16, 18, 21, 22, 24, 26, 31];
        let base = mid_round_live_suited_risk_bias(&hand, &[], &table, 0, 9, 1, WIN_RULE_RELAXED);
        table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(16)];

        assert!(
            mid_round_live_suited_risk_bias(&hand, &[], &table, 0, 9, 1, WIN_RULE_RELAXED) < base
        );
    }

    #[test]
    fn mid_round_live_suited_risk_ignores_concealed_gang_opponent() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 37;
        let hand = vec![1, 2, 3, 9, 11, 12, 14, 16, 18, 21, 22, 24, 26, 31];
        let base = mid_round_live_suited_risk_bias(&hand, &[], &table, 0, 9, 1, WIN_RULE_RELAXED);

        table.seats.get_mut(&1).unwrap().melds = vec![test_concealed_gang_meld(16)];
        assert_eq!(
            mid_round_live_suited_risk_bias(&hand, &[], &table, 0, 9, 1, WIN_RULE_RELAXED),
            base
        );

        table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(16)];
        assert!(
            mid_round_live_suited_risk_bias(&hand, &[], &table, 0, 9, 1, WIN_RULE_RELAXED) < base
        );
    }

    #[test]
    fn mid_round_open_meld_tile_is_safer_than_live_suited_tile() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 37;
        table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(14)];
        let hand = vec![1, 2, 3, 9, 11, 12, 14, 16, 18, 21, 22, 24, 26, 28];

        assert!(mid_round_open_meld_safety_bias(&table, 14) > 0.0);
        assert_eq!(
            open_opponent_live_suited_risk(&table, 0, 14),
            0.0,
            "an opponent who already opened this tile should not add live-tile pressure for it"
        );
        assert!(
            mid_round_open_meld_safety_bias(&table, 14)
                > mid_round_open_meld_safety_bias(&table, 9)
        );
        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(14)
        );
    }

    #[test]
    fn open_opponent_exists_ignores_tile_from_its_open_meld() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 37;
        table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(14)];

        assert!(!open_opponent_exists_for_tile(&table, 0, 14));
        assert!(open_opponent_exists_for_tile(&table, 0, 15));
    }

    #[test]
    fn own_open_live_suited_pressure_ignores_opponent_open_meld_tile() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 37;
        table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(14)];
        let melds = vec![test_peng_meld(1), test_peng_meld(11)];

        assert_eq!(own_open_live_suited_pressure(&melds, &table, 0, 14), 0.0);
        assert!(own_open_live_suited_pressure(&melds, &table, 0, 15) > 0.0);
    }

    #[test]
    fn mid_round_discard_avoids_live_terminal_against_open_opponent() {
        let mut table = table_with_discards(1, vec![14]);
        table.wall_count = 37;
        table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(16)];
        let hand = vec![1, 2, 3, 9, 11, 12, 14, 16, 18, 21, 22, 24, 26, 31];

        assert_ne!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(9)
        );
    }

    #[test]
    fn mid_round_open_hand_does_not_chase_wait_fan_with_live_terminal_discard() {
        let mut seats = HashMap::new();
        seats.insert(
            0,
            AiSeatView {
                position: 0,
                hand_count: 1,
                discards: vec![31, 33, 16, 1, 31, 21, 8, 12, 32, 3, 4, 2, 15],
                melds: vec![
                    test_peng_meld(37),
                    test_peng_meld(5),
                    test_peng_meld(6),
                    test_peng_meld(25),
                ],
            },
        );
        seats.insert(
            1,
            AiSeatView {
                position: 1,
                hand_count: 10,
                discards: vec![21, 4, 15, 35, 37, 11, 12, 16, 5, 33, 33, 35],
                melds: vec![test_peng_meld(19)],
            },
        );
        seats.insert(
            2,
            AiSeatView {
                position: 2,
                hand_count: 13,
                discards: vec![34, 1, 22, 33, 12, 23, 5, 3, 28, 1],
                melds: Vec::new(),
            },
        );
        seats.insert(
            3,
            AiSeatView {
                position: 3,
                hand_count: 8,
                discards: vec![34, 32, 22, 8, 35, 16, 11, 12, 25, 17, 3],
                melds: vec![test_peng_meld(7), test_peng_meld(26)],
            },
        );
        let table = AiPublicTable {
            current_position: 3,
            dealer_position: 0,
            wall_count: 37,
            max_fan: Some(4),
            claim_window: None,
            seats,
        };
        let hand = vec![9, 13, 14, 15, 24, 24, 28, 29];

        assert_ne!(
            choose_discard_from_view(&hand, &table, 3, WIN_RULE_SHENYANG_BASIC),
            Some(9)
        );
    }

    #[test]
    fn late_open_hand_avoids_live_tile_against_four_piao_melds() {
        let mut seats = HashMap::new();
        seats.insert(
            0,
            AiSeatView {
                position: 0,
                hand_count: 1,
                discards: vec![31, 33, 19, 16, 1, 31, 21, 8, 12, 32, 3, 4, 2, 15, 22, 4],
                melds: vec![
                    test_peng_meld(37),
                    test_peng_meld(5),
                    test_peng_meld(6),
                    test_peng_meld(25),
                ],
            },
        );
        seats.insert(
            1,
            AiSeatView {
                position: 1,
                hand_count: 11,
                discards: vec![21, 4, 15, 35, 11, 12, 16, 34, 33, 33, 35, 35],
                melds: vec![test_peng_meld(19)],
            },
        );
        seats.insert(
            2,
            AiSeatView {
                position: 2,
                hand_count: 13,
                discards: vec![34, 1, 22, 33, 12, 23, 5, 3, 25, 28, 1, 29],
                melds: Vec::new(),
            },
        );
        seats.insert(
            3,
            AiSeatView {
                position: 3,
                hand_count: 8,
                discards: vec![34, 32, 22, 8, 35, 16, 11, 12, 17, 3, 28, 28],
                melds: vec![test_peng_meld(7), test_peng_meld(26)],
            },
        );
        let table = AiPublicTable {
            current_position: 1,
            dealer_position: 0,
            wall_count: 31,
            max_fan: Some(4),
            claim_window: None,
            seats,
        };
        let hand = vec![7, 8, 9, 9, 9, 13, 22, 23, 24, 36, 36];

        assert_ne!(
            choose_discard_from_view(&hand, &table, 1, WIN_RULE_SHENYANG_BASIC),
            Some(13)
        );
    }

    #[test]
    fn discard_uses_own_previous_discard_as_public_safety() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 16;
        table.seats.get_mut(&0).unwrap().discards = vec![5];
        let hand = vec![1, 1, 4, 5, 7, 9, 12, 14, 17, 21, 23, 25, 31, 35];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(5)
        );
    }

    #[test]
    fn estimated_visible_fan_counts_four_gui_yi_before_wait_fan() {
        let win_hand = vec![2, 3, 4, 11, 12, 13, 21, 22, 23, 35, 35];
        let melds = vec![test_peng_meld(2)];

        assert_eq!(
            estimated_visible_fan_without_wait(&win_hand, &melds, WIN_RULE_SHENYANG_BASIC),
            2
        );
    }

    #[test]
    fn estimated_visible_fan_counts_concealed_dragon_triplet() {
        let win_hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 35, 35, 35];

        assert_eq!(
            estimated_visible_fan_without_wait(&win_hand, &[], WIN_RULE_RELAXED),
            2
        );
    }

    #[test]
    fn estimated_visible_fan_counts_four_concealed_dragons_as_triplet_and_four_gui_yi() {
        let win_hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 35, 35, 35, 35];

        assert_eq!(
            estimated_visible_fan_without_wait(&win_hand, &[], WIN_RULE_RELAXED),
            6
        );
    }

    #[test]
    fn estimated_visible_fan_counts_piao_shou_ba_yi_before_wait_fan() {
        let win_hand = vec![35, 35];
        let melds = vec![
            test_peng_meld(1),
            test_peng_meld(11),
            test_peng_meld(21),
            test_peng_meld(31),
        ];

        assert_eq!(
            estimated_visible_fan_without_wait(&win_hand, &melds, WIN_RULE_SHENYANG_BASIC),
            4
        );
    }

    #[test]
    fn estimated_visible_fan_uses_win_rule_for_closed_pure_one_suit() {
        let win_hand = vec![1, 2, 3, 2, 3, 4, 4, 5, 6, 7, 7, 7, 9, 9];

        assert_eq!(
            estimated_visible_fan_without_wait(&win_hand, &[], WIN_RULE_RELAXED),
            4
        );
        assert_eq!(
            estimated_visible_fan_without_wait(&win_hand, &[], WIN_RULE_SHENYANG_BASIC),
            4
        );
    }

    #[test]
    fn estimated_visible_fan_does_not_add_closed_winner_fan() {
        let closed_pure_one_suit = vec![1, 2, 3, 2, 3, 4, 4, 5, 6, 7, 7, 7, 9, 9];
        let closed_seven_pairs = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35];

        assert_eq!(
            estimated_visible_fan_without_wait(&closed_pure_one_suit, &[], WIN_RULE_SHENYANG_BASIC),
            4
        );
        assert_eq!(
            estimated_visible_fan_without_wait(&closed_seven_pairs, &[], WIN_RULE_SHENYANG_BASIC),
            4
        );
    }

    #[test]
    fn estimated_fan_counts_single_yaojiu_terminal_wait_extra() {
        let win_hand = vec![11, 11, 14, 15, 15, 16, 16, 17, 17, 17, 17];
        let melds = vec![test_chi_meld(12)];

        assert_eq!(
            estimated_fan_with_wait(&win_hand, &melds, 11, WIN_RULE_SHENYANG_BASIC),
            7
        );
    }

    #[test]
    fn estimated_fan_counts_single_yaojiu_honor_wait_extra() {
        let win_hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35];

        assert_eq!(
            estimated_fan_with_wait(&win_hand, &[], 35, WIN_RULE_RELAXED),
            3
        );
    }

    #[test]
    fn fan_wait_bias_uses_win_rule_for_closed_basic_hand() {
        let table = table_with_discards(1, Vec::new());
        let win_hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35];

        assert!(fan_wait_bias(&win_hand, &[], &table, 0, WIN_RULE_RELAXED, 35, 2) > 0.0);
        assert_eq!(
            fan_wait_bias(&win_hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC, 35, 2),
            0.0
        );
    }

    #[test]
    fn fan_wait_bias_counts_middle_tile_seven_pairs_single_wait() {
        let mut table = table_with_discards(1, Vec::new());
        table.max_fan = Some(5);
        let win_hand = vec![1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 11, 11, 21, 21];

        assert!(fan_wait_bias(&win_hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC, 5, 3) > 0.0);

        table.dealer_position = 0;
        assert_eq!(
            fan_wait_bias(&win_hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC, 5, 3),
            0.0
        );
    }

    #[test]
    fn fan_wait_bias_counts_single_yaojiu_terminal_wait_extra_for_cap() {
        let mut table = table_with_discards(1, Vec::new());
        table.max_fan = Some(7);
        let win_hand = vec![11, 11, 14, 15, 15, 16, 16, 17, 17, 17, 17];
        let melds = vec![test_chi_meld(12)];

        assert_eq!(
            estimated_visible_fan_without_wait(&win_hand, &melds, WIN_RULE_SHENYANG_BASIC),
            5
        );
        assert_eq!(
            estimated_fan_with_wait(&win_hand, &melds, 11, WIN_RULE_SHENYANG_BASIC),
            7
        );
        assert_eq!(
            fan_wait_bias(&win_hand, &melds, &table, 0, WIN_RULE_SHENYANG_BASIC, 11, 2),
            0.0
        );
        assert_eq!(
            fan_wait_bias(&win_hand, &melds, &table, 0, WIN_RULE_SHENYANG_BASIC, 11, 3),
            14.0
        );
    }

    #[test]
    fn late_defense_avoids_cold_honor_against_closed_opponent() {
        let mut table = table_with_discards(1, vec![11, 14, 19]);
        table.wall_count = 16;
        table.seats.get_mut(&1).unwrap().hand_count = 13;
        let hand = vec![2, 3, 4, 5, 6, 7, 8, 12, 13, 15, 16, 17, 18, 31];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(12)
        );
    }

    #[test]
    fn late_defense_does_not_mark_exposed_suit_as_missing() {
        let mut table = table_with_discards(1, vec![11, 14, 19]);
        table.wall_count = 16;
        table.seats.get_mut(&1).unwrap().melds = vec![WsShenyangMahjongMeld {
            kind: ShenyangMahjongMeldKind::PENG,
            tiles: vec![12, 12, 12],
            from_position: Some(0),
        }];

        assert_eq!(opponent_missing_suit_safety_bias(&table, 0, 12), 0.0);
    }

    #[test]
    fn late_defense_does_not_mark_piao_needed_suit_as_missing() {
        let mut table = table_with_discards(1, vec![1, 4, 9]);
        table.wall_count = 16;
        table.seats.get_mut(&1).unwrap().melds =
            vec![test_peng_meld(11), test_peng_meld(21), test_peng_meld(31)];

        assert_eq!(
            piao_missing_suits_from_melds(&table.seats.get(&1).unwrap().melds),
            vec![0]
        );
        assert_eq!(opponent_missing_suit_safety_bias(&table, 0, 5), 0.0);
    }

    #[test]
    fn late_defense_piao_needed_suit_blocks_other_missing_suit_reads() {
        let mut table = table_with_discards(1, vec![1, 4, 9]);
        table.wall_count = 16;
        table.seats.get_mut(&1).unwrap().melds =
            vec![test_peng_meld(11), test_peng_meld(21), test_peng_meld(31)];
        for position in [2, 3] {
            table.seats.insert(
                position,
                AiSeatView {
                    position,
                    hand_count: 10,
                    discards: vec![1, 4, 9],
                    melds: Vec::new(),
                },
            );
        }
        let mut no_piao_table = table.clone();
        no_piao_table.seats.get_mut(&1).unwrap().melds.clear();

        assert!(opponent_missing_suit_safety_bias(&no_piao_table, 0, 5) > 0.0);
        assert_eq!(opponent_missing_suit_safety_bias(&table, 0, 5), 0.0);
    }

    #[test]
    fn late_defense_closed_opponent_blocks_other_missing_suit_reads() {
        let mut table = table_with_discards(1, vec![1, 4, 9]);
        table.wall_count = 16;
        table.seats.insert(
            2,
            AiSeatView {
                position: 2,
                hand_count: 10,
                discards: vec![1, 4, 9],
                melds: Vec::new(),
            },
        );
        let mut closed_threat_table = table.clone();
        closed_threat_table.seats.insert(
            3,
            AiSeatView {
                position: 3,
                hand_count: 13,
                discards: Vec::new(),
                melds: Vec::new(),
            },
        );

        assert!(opponent_missing_suit_safety_bias(&table, 0, 5) > 0.0);
        assert_eq!(
            opponent_missing_suit_safety_bias(&closed_threat_table, 0, 5),
            0.0
        );
    }

    #[test]
    fn late_defense_candidates_avoid_piao_needed_suit_over_missing_suit_read() {
        let mut table = table_with_discards(1, vec![1, 4, 9]);
        table.wall_count = 16;
        table.seats.get_mut(&1).unwrap().melds =
            vec![test_peng_meld(11), test_peng_meld(21), test_peng_meld(31)];
        let hand = vec![5, 12];

        assert_eq!(
            choose_late_defense_discard_from_candidates(&hand, &table, 0, vec![5, 12]),
            Some(12)
        );
    }

    #[test]
    fn late_defense_follows_public_tile_before_live_missing_suit_read() {
        let missing_suit_discards = vec![11, 13, 14, 19, 11, 13, 14, 19, 11, 13];
        let mut table = table_with_discards(1, {
            let mut discards = missing_suit_discards.clone();
            discards.push(5);
            discards
        });
        table.wall_count = 16;
        table.seats.insert(
            2,
            AiSeatView {
                position: 2,
                hand_count: 10,
                discards: missing_suit_discards.clone(),
                melds: Vec::new(),
            },
        );
        table.seats.insert(
            3,
            AiSeatView {
                position: 3,
                hand_count: 10,
                discards: missing_suit_discards,
                melds: Vec::new(),
            },
        );
        let hand = vec![2, 5, 7, 9, 12, 16, 18, 21, 23, 25, 27, 31, 33, 35];

        assert!(
            late_defense_tile_safety_score(&table, 0, 12, 1)
                > late_defense_tile_safety_score(&table, 0, 5, 1)
        );
        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(5)
        );
    }

    #[test]
    fn late_defense_prefers_opponent_missing_suit_tile() {
        let mut table = table_with_discards(1, vec![11, 14, 19]);
        table.wall_count = 16;
        let hand = vec![2, 3, 4, 5, 6, 7, 8, 12, 13, 15, 16, 17, 18, 22];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(12)
        );
    }

    #[test]
    fn late_defense_missing_suit_read_can_beat_live_wind() {
        let mut table = table_with_discards(1, vec![11, 14, 19]);
        table.wall_count = 16;
        let hand = vec![2, 3, 4, 5, 6, 7, 8, 12, 13, 15, 16, 17, 18, 31];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(12)
        );
    }

    #[test]
    fn late_defense_prefers_public_honor_over_multiple_public_suited_tile() {
        let mut table = table_with_discards(1, vec![5, 5, 31]);
        table.wall_count = 16;

        assert!(
            late_defense_tile_safety_score(&table, 0, 31, 1)
                > late_defense_tile_safety_score(&table, 0, 5, 1)
        );
    }

    #[test]
    fn late_defense_bias_keeps_public_honor_above_four_public_middle_tiles() {
        let mut table = table_with_discards(1, vec![5, 5, 5, 5, 31]);
        table.wall_count = 16;

        assert!(late_defense_discard_bias(&table, 0, 31) > late_defense_discard_bias(&table, 0, 5));
    }

    #[test]
    fn late_defense_prefers_public_middle_tile_over_public_terminal() {
        let mut table = table_with_discards(1, vec![5, 9]);
        table.wall_count = 16;

        assert!(
            late_defense_tile_safety_score(&table, 0, 5, 1)
                > late_defense_tile_safety_score(&table, 0, 9, 1)
        );
    }

    #[test]
    fn late_defense_prefers_own_previous_middle_discard_over_other_public_middle() {
        let mut table = table_with_discards(1, vec![5]);
        table.wall_count = 16;
        table.seats.get_mut(&0).unwrap().discards = vec![8];
        let hand = vec![2, 3, 5, 7, 8, 12, 14, 16, 18, 21, 23, 25, 31, 35];

        assert!(
            late_defense_tile_safety_score(&table, 0, 8, 1)
                > late_defense_tile_safety_score(&table, 0, 5, 1)
        );
        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(8)
        );
    }

    #[test]
    fn late_defense_prefers_public_middle_tile_over_live_wind() {
        let mut table = table_with_discards(1, vec![5]);
        table.wall_count = 16;

        assert!(
            late_defense_tile_safety_score(&table, 0, 5, 1)
                > late_defense_tile_safety_score(&table, 0, 31, 1)
        );
    }

    #[test]
    fn late_defense_prefers_live_wind_then_terminal_then_middle() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 16;

        assert!(
            late_defense_tile_safety_score(&table, 0, 31, 1)
                > late_defense_tile_safety_score(&table, 0, 9, 1)
        );
        assert!(
            late_defense_tile_safety_score(&table, 0, 9, 1)
                > late_defense_tile_safety_score(&table, 0, 5, 1)
        );
    }

    #[test]
    fn late_defense_values_three_exposed_meld_tiles_over_live_wind() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 16;
        table.seats.insert(
            2,
            AiSeatView {
                position: 2,
                hand_count: 10,
                discards: Vec::new(),
                melds: vec![test_peng_meld(6)],
            },
        );

        assert!(
            late_defense_tile_safety_score(&table, 0, 6, 1)
                > late_defense_tile_safety_score(&table, 0, 31, 1)
        );
    }

    #[test]
    fn late_defense_discards_three_exposed_meld_tile_before_live_wind() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 16;
        table.seats.insert(
            2,
            AiSeatView {
                position: 2,
                hand_count: 10,
                discards: Vec::new(),
                melds: vec![test_peng_meld(6)],
            },
        );
        let hand = vec![2, 4, 6, 8, 12, 14, 16, 18, 22, 24, 26, 28, 31, 35];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(6)
        );
    }

    #[test]
    fn late_defense_prefers_lone_wind_before_breaking_wind_pair() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 16;
        let hand = vec![1, 2, 4, 6, 8, 11, 13, 15, 17, 21, 23, 31, 31, 32];

        assert!(
            late_defense_tile_safety_score(&table, 0, 32, 1)
                > late_defense_tile_safety_score(&table, 0, 31, 2)
        );
        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(32)
        );
    }

    #[test]
    fn late_defense_prefers_live_middle_before_breaking_terminal_pair() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 16;

        assert!(
            late_defense_tile_safety_score(&table, 0, 5, 1)
                > late_defense_tile_safety_score(&table, 0, 9, 2)
        );
    }

    #[test]
    fn late_defense_discards_live_middle_before_breaking_terminal_pair() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 16;
        let hand = vec![2, 4, 5, 6, 8, 9, 9, 12, 14, 16, 18, 22, 24, 26];

        assert_ne!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(9)
        );
    }

    #[test]
    fn late_discard_follows_safe_tile_over_hand_efficiency() {
        let mut table = table_with_discards(1, vec![5]);
        table.wall_count = 16;
        let hand = vec![3, 4, 5, 5, 6, 7, 11, 12, 13, 21, 22, 23, 31, 35];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(5)
        );
    }

    #[test]
    fn late_ready_discard_still_preserves_wait_over_safe_tile() {
        let mut table = table_with_discards(1, vec![5]);
        table.wall_count = 16;
        let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 31, 31, 32];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(32)
        );
    }

    #[test]
    fn late_unready_discard_uses_defense_before_hand_progress() {
        let mut table = table_with_discards(1, vec![14]);
        table.wall_count = 16;
        let hand = vec![1, 1, 4, 7, 9, 12, 14, 14, 17, 21, 23, 25, 31, 35];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(14)
        );
    }

    #[test]
    fn mid_round_discard_follows_multiple_public_terminal_over_live_wind() {
        let mut table = table_with_discards(1, vec![9, 9]);
        table.wall_count = 36;
        let hand = vec![1, 2, 4, 6, 8, 9, 11, 12, 14, 16, 21, 23, 25, 31];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(9)
        );
    }

    #[test]
    fn mid_round_public_discard_prefers_own_previous_middle_over_other_public_middle() {
        let mut table = table_with_discards(1, vec![5]);
        table.wall_count = 36;
        table.seats.get_mut(&0).unwrap().discards = vec![8];
        let hand = vec![2, 3, 5, 7, 8, 12, 14, 16, 18, 21, 23, 25, 31, 35];

        assert!(
            mid_round_public_discard_bias(&table, 0, 8)
                > mid_round_public_discard_bias(&table, 0, 5)
        );
        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(8)
        );
    }

    #[test]
    fn mid_broken_basic_discard_follows_public_tile_before_hand_shape() {
        let mut table = table_with_discards(1, vec![5]);
        table.wall_count = 40;
        let hand = vec![2, 5, 8, 12, 14, 17, 22, 24, 27, 31, 32, 33, 35, 37];

        assert!(should_use_broken_hand_public_defense_discard(
            &hand,
            &[],
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC
        ));
        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(5)
        );
    }

    #[test]
    fn mid_broken_relaxed_discard_follows_public_tile_before_hand_shape() {
        let mut table = table_with_discards(1, vec![5]);
        table.wall_count = 40;
        let hand = vec![2, 5, 8, 12, 14, 17, 22, 24, 27, 31, 32, 33, 35, 37];

        assert!(should_use_broken_hand_public_defense_discard(
            &hand,
            &[],
            &table,
            0,
            WIN_RULE_RELAXED
        ));
        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(5)
        );
    }

    #[test]
    fn mid_broken_public_defense_preserves_dragon_pair_over_public_singleton() {
        let mut table = table_with_discards(1, vec![5, 35]);
        table.wall_count = 40;
        let hand = vec![2, 5, 8, 12, 14, 17, 22, 24, 27, 31, 32, 33, 35, 35];

        assert!(should_use_broken_hand_public_defense_discard(
            &hand,
            &[],
            &table,
            0,
            WIN_RULE_RELAXED
        ));
        assert!(
            public_defense_tile_safety_score(&table, 0, 5, 1)
                > public_defense_tile_safety_score(&table, 0, 35, 2)
        );
        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(5)
        );
    }

    #[test]
    fn mid_broken_public_defense_preserves_triplet_over_public_pair() {
        let mut table = table_with_discards(1, vec![5, 7]);
        table.wall_count = 40;
        let hand = vec![2, 5, 5, 5, 7, 7, 12, 14, 17, 22, 24, 27, 31, 33];

        assert!(
            public_defense_tile_safety_score(&table, 0, 7, 2)
                > public_defense_tile_safety_score(&table, 0, 5, 3)
        );
        assert_eq!(
            choose_public_defense_discard_from_candidates(&hand, &table, 0, vec![5, 7]),
            Some(7)
        );
    }

    #[test]
    fn dealer_mid_unrecoverable_basic_hand_uses_public_defense_discard() {
        let mut discards = dead_terminal_or_honor_discards();
        discards.push(5);
        let mut table = table_with_discards(1, discards);
        table.dealer_position = 0;
        table.wall_count = 52;
        let hand = vec![2, 3, 4, 5, 5, 6, 7, 12, 13, 14, 22, 23, 24, 25];

        assert_eq!(
            unrecoverable_basic_rule_requirement_count(&hand, &[], &table),
            1
        );
        assert!(should_use_broken_hand_public_defense_discard(
            &hand,
            &[],
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC
        ));
        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(5)
        );
    }

    #[test]
    fn mid_broken_public_defense_preserves_locked_seven_pairs_route() {
        let mut table = table_with_discards(1, vec![11]);
        table.wall_count = 40;
        let hand = vec![1, 1, 2, 2, 3, 3, 4, 4, 11, 11, 12, 12, 21, 31];

        assert!(!should_use_broken_hand_public_defense_discard(
            &hand,
            &[],
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC
        ));
        assert_ne!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(11)
        );
    }

    #[test]
    fn missing_suits_tracks_three_suits_need() {
        let hand = vec![1, 2, 3, 11, 18, 19, 21, 22, 23, 24, 25, 26, 35, 36];

        assert!(missing_suits(&hand, &[]).is_empty());
        assert_eq!(missing_suits(&hand[0..6], &[]), vec![2]);
    }

    #[test]
    fn near_capped_non_dealer_prefers_wider_wait_over_single_wait_fan() {
        let mut table = table_with_discards(1, Vec::new());
        table.max_fan = Some(4);
        table.seats.get_mut(&0).unwrap().melds = vec![test_gang_meld(35)];
        let hand = vec![2, 2, 4, 5, 7, 11, 12, 13, 21, 22, 23];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(7)
        );
    }

    #[test]
    fn late_non_dealer_prefers_wider_wait_over_single_wait_fan() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 30;
        table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
        let hand = vec![2, 2, 4, 5, 7, 11, 12, 13, 21, 22, 23];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(7)
        );
    }

    #[test]
    fn non_dealer_can_choose_single_wait_for_extra_fan_before_late_round() {
        let mut table = table_with_discards(1, Vec::new());
        table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
        let hand = vec![2, 2, 4, 5, 7, 11, 12, 13, 21, 22, 23];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(4)
        );
    }

    #[test]
    fn non_dealer_avoids_nearly_dead_single_wait_before_late_round() {
        let mut table = table_with_discards(1, vec![6, 6, 6]);
        table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
        let hand = vec![2, 2, 4, 5, 7, 11, 12, 13, 21, 22, 23];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(7)
        );
    }

    #[test]
    fn one_step_wait_potential_values_near_ready_shape() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 31, 35];

        assert!(
            one_step_wait_potential(&hand, &[], &table, 0, WIN_RULE_RELAXED) > 0.0,
            "near-ready hand should see useful draws"
        );
    }

    #[test]
    fn opponent_threat_starts_after_three_triplet_melds() {
        let mut table = table_with_discards(1, Vec::new());
        table.seats.get_mut(&1).unwrap().melds =
            vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];

        assert!(opponent_threat_discard_bias(&table, 0, 5, 2) < -9.0);

        table.seats.get_mut(&1).unwrap().melds.pop();
        assert_eq!(opponent_threat_discard_bias(&table, 0, 5, 2), 0.0);
    }

    #[test]
    fn opponent_piao_threat_ignores_player_after_chi_meld() {
        let mut table = table_with_discards(1, Vec::new());
        table.seats.get_mut(&1).unwrap().melds = vec![
            test_chi_meld(2),
            test_peng_meld(1),
            test_peng_meld(11),
            test_peng_meld(21),
        ];

        assert_eq!(piao_threat_level(&table.seats.get(&1).unwrap().melds), 0);
        assert_eq!(opponent_threat_discard_bias(&table, 0, 5, 2), 0.0);
    }

    #[test]
    fn opponent_four_piao_threat_penalizes_live_pair_more_than_singleton() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 16;
        table.seats.get_mut(&1).unwrap().hand_count = 2;
        table.seats.get_mut(&1).unwrap().melds = vec![
            test_peng_meld(1),
            test_peng_meld(11),
            test_peng_meld(21),
            test_peng_meld(31),
        ];

        assert!(
            opponent_threat_discard_bias(&table, 0, 5, 2)
                < opponent_threat_discard_bias(&table, 0, 6, 1)
        );
    }

    #[test]
    fn opponent_four_piao_threat_ignores_impossible_two_missing_suits() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 16;
        table.seats.get_mut(&1).unwrap().hand_count = 2;
        table.seats.get_mut(&1).unwrap().melds = vec![
            test_peng_meld(2),
            test_peng_meld(3),
            test_peng_meld(4),
            test_peng_meld(5),
        ];

        assert_eq!(
            piao_missing_suits_from_melds(&table.seats.get(&1).unwrap().melds),
            vec![1, 2]
        );
        assert!(piao_threat_cannot_satisfy_three_suits(
            &table.seats.get(&1).unwrap().melds,
            table.seats.get(&1).unwrap().hand_count
        ));
        assert_eq!(opponent_threat_discard_bias(&table, 0, 31, 1), 0.0);

        table.seats.get_mut(&1).unwrap().melds = vec![
            test_peng_meld(2),
            test_peng_meld(3),
            test_peng_meld(12),
            test_peng_meld(13),
        ];
        assert_eq!(
            piao_missing_suits_from_melds(&table.seats.get(&1).unwrap().melds),
            vec![2]
        );
        assert!(!piao_threat_cannot_satisfy_three_suits(
            &table.seats.get(&1).unwrap().melds,
            table.seats.get(&1).unwrap().hand_count
        ));
        assert!(opponent_threat_discard_bias(&table, 0, 21, 1) < 0.0);
    }

    #[test]
    fn piao_threat_penalizes_live_wind_pair_more_than_terminal_singleton() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 16;
        table.seats.get_mut(&1).unwrap().melds =
            vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];

        assert!(
            opponent_threat_discard_bias(&table, 0, 31, 2)
                < opponent_threat_discard_bias(&table, 0, 9, 1)
        );
    }

    #[test]
    fn piao_threat_needing_yaojiu_penalizes_live_terminal_over_middle() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 32;
        table.seats.get_mut(&1).unwrap().melds =
            vec![test_peng_meld(2), test_peng_meld(12), test_peng_meld(22)];

        assert!(piao_needs_terminal_or_honor_from_melds(
            &table.seats.get(&1).unwrap().melds
        ));
        assert!(
            opponent_threat_discard_bias(&table, 0, 9, 1)
                < opponent_threat_discard_bias(&table, 0, 5, 1)
        );
        assert!(
            opponent_threat_discard_bias(&table, 0, 31, 1)
                < opponent_threat_discard_bias(&table, 0, 5, 1)
        );
    }

    #[test]
    fn piao_threat_discounts_exposed_meld_tiles() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 16;
        table.seats.get_mut(&1).unwrap().melds =
            vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];
        table.seats.insert(
            2,
            AiSeatView {
                position: 2,
                hand_count: 10,
                discards: Vec::new(),
                melds: vec![test_peng_meld(6)],
            },
        );

        assert!(
            opponent_threat_discard_bias(&table, 0, 6, 1)
                > opponent_threat_discard_bias(&table, 0, 5, 1)
        );
    }

    #[test]
    fn late_defense_can_follow_exposed_middle_against_piao_threat() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 16;
        table.seats.get_mut(&1).unwrap().melds =
            vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];
        table.seats.insert(
            2,
            AiSeatView {
                position: 2,
                hand_count: 10,
                discards: Vec::new(),
                melds: vec![test_peng_meld(6)],
            },
        );
        let hand = vec![2, 3, 4, 5, 6, 7, 8, 12, 14, 16, 18, 22, 24, 26];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(6)
        );
    }

    #[test]
    fn late_defense_avoids_breaking_wind_pair_against_piao_threat() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 16;
        table.seats.get_mut(&1).unwrap().melds =
            vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];
        let hand = vec![2, 4, 6, 8, 9, 12, 14, 16, 18, 22, 24, 26, 31, 31];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(9)
        );
    }

    #[test]
    fn late_defense_avoids_piao_threat_missing_suit_wait_tiles() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 16;
        table.seats.get_mut(&1).unwrap().melds =
            vec![test_peng_meld(11), test_peng_meld(21), test_peng_meld(31)];
        let hand = vec![2, 3, 5, 6, 8, 12, 13, 15, 16, 18, 22, 23, 25, 28];

        assert!(!matches!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(2 | 3 | 5 | 6 | 8)
        ));
    }

    #[test]
    fn opponent_threat_counts_ai_controlled_table_seat() {
        let mut table = table_with_discards(1, Vec::new());
        table.seats.get_mut(&1).unwrap().melds =
            vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];

        assert!(opponent_threat_discard_bias(&table, 0, 5, 1) < 0.0);
    }

    #[test]
    fn opponent_missing_suit_read_counts_ai_controlled_table_seat() {
        let mut table = table_with_discards(1, vec![11, 12, 13]);
        table.wall_count = 16;

        assert!(opponent_missing_suit_safety_bias(&table, 0, 14) > 0.0);
    }

    #[test]
    fn ready_visible_cap_counts_four_gui_yi() {
        let mut table = table_with_discards(1, Vec::new());
        table.max_fan = Some(2);
        let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 9, 9, 21, 21];

        assert!(ready_visible_fan_reaches_cap(
            &hand,
            &[],
            &table,
            0,
            WIN_RULE_RELAXED
        ));
    }

    #[test]
    fn ready_visible_cap_counts_concealed_dragon_triplet() {
        let mut table = table_with_discards(1, Vec::new());
        table.max_fan = Some(2);
        let hand = vec![1, 2, 3, 11, 12, 13, 22, 23, 31, 31, 35, 35, 35];

        assert!(ready_visible_fan_reaches_cap(
            &hand,
            &[],
            &table,
            0,
            WIN_RULE_RELAXED
        ));
    }

    #[test]
    fn ready_cap_counts_single_wait_fan() {
        let mut table = table_with_discards(1, Vec::new());
        table.max_fan = Some(2);
        let hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35];

        assert!(ready_visible_fan_reaches_cap(
            &hand,
            &[],
            &table,
            0,
            WIN_RULE_RELAXED
        ));
    }

    #[test]
    fn ready_visible_cap_counts_piao_shou_ba_yi() {
        let mut table = table_with_discards(1, Vec::new());
        table.max_fan = Some(4);
        table.seats.get_mut(&0).unwrap().melds = vec![
            test_peng_meld(1),
            test_peng_meld(11),
            test_peng_meld(21),
            test_peng_meld(31),
        ];
        let hand = vec![35];
        let melds = table.seats.get(&0).unwrap().melds.as_slice();

        assert!(ready_visible_fan_reaches_cap(
            &hand,
            melds,
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC
        ));
    }

    #[test]
    fn remaining_tile_count_counts_own_public_tiles() {
        let mut table = table_with_discards(1, Vec::new());
        table.seats.get_mut(&0).unwrap().discards = vec![31];
        table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];

        assert_eq!(remaining_tile_count(&[], &table, 0, 31), 0);
    }

    #[test]
    fn self_gang_allows_dragon_gang_after_opening_basic_hand() {
        let mut table = table_with_discards(1, Vec::new());
        table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
        let hand = vec![11, 12, 13, 21, 22, 23, 31, 35, 35, 35, 35];

        assert_eq!(
            choose_self_gang_from_view(&hand, &[35], &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(35)
        );
    }

    #[test]
    fn one_fan_capped_self_gang_delays_dragon_before_ready() {
        let mut table = table_with_discards(1, Vec::new());
        table.max_fan = Some(1);
        table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
        let hand = vec![2, 5, 8, 11, 14, 17, 21, 35, 35, 35, 35];

        assert_eq!(
            choose_self_gang_from_view(&hand, &[35], &table, 0, WIN_RULE_SHENYANG_BASIC),
            None
        );
    }

    #[test]
    fn one_fan_capped_self_gang_delays_added_dragon_before_ready() {
        let mut table = table_with_discards(1, Vec::new());
        table.max_fan = Some(1);
        table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(35)];
        let hand = vec![2, 5, 8, 11, 14, 17, 21, 31, 32, 33, 35];

        assert_eq!(
            choose_self_gang_from_view(&hand, &[35], &table, 0, WIN_RULE_SHENYANG_BASIC),
            None
        );
    }

    #[test]
    fn one_fan_capped_self_gang_delays_added_plain_before_ready() {
        let mut table = table_with_discards(1, Vec::new());
        table.max_fan = Some(1);
        table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(9)];
        let hand = vec![1, 2, 4, 6, 8, 9, 11, 13, 16, 21, 24];

        assert_eq!(
            choose_self_gang_from_view(&hand, &[9], &table, 0, WIN_RULE_SHENYANG_BASIC),
            None
        );
    }

    #[test]
    fn self_gang_allows_open_plain_gang_when_ready() {
        let mut table = table_with_discards(1, Vec::new());
        table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
        let hand = vec![1, 2, 3, 9, 9, 9, 9, 11, 12, 13, 21];

        assert_eq!(
            choose_self_gang_from_view(&hand, &[9], &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(9)
        );
    }

    #[test]
    fn self_gang_allows_final_ready_hand_when_gang_keeps_ready() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 16;
        table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
        let hand = vec![1, 2, 3, 9, 9, 9, 9, 11, 12, 13, 21];

        assert!(
            best_ready_score_after_discard(
                &hand,
                table.seats.get(&0).unwrap().melds.as_slice(),
                &table,
                0,
                WIN_RULE_SHENYANG_BASIC
            ) > 0.0
        );
        assert_eq!(
            choose_self_gang_from_view(&hand, &[9], &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(9)
        );
    }

    #[test]
    fn self_gang_allows_ready_main_suit_added_gang_for_pure_one_suit_plan() {
        let mut table = table_with_discards(1, Vec::new());
        table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
        let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 9];

        assert_eq!(
            choose_self_gang_from_view(&hand, &[1], &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(1)
        );
    }

    #[test]
    fn self_gang_delays_main_suit_added_gang_when_pure_one_suit_plan_not_ready() {
        let mut table = table_with_discards(1, Vec::new());
        table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
        let hand = vec![1, 2, 4, 5, 7, 8, 9, 11, 12, 21, 31];

        assert_eq!(
            choose_self_gang_from_view(&hand, &[1], &table, 0, WIN_RULE_SHENYANG_BASIC),
            None
        );
    }

    #[test]
    fn self_gang_delays_closed_dragon_gang_before_opening_basic_hand() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 35, 35, 35, 35, 31];

        assert_eq!(
            choose_self_gang_from_view(&hand, &[35], &table, 0, WIN_RULE_SHENYANG_BASIC),
            None
        );
    }

    #[test]
    fn self_gang_delays_closed_plain_gang_before_opening_basic_hand() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![3, 3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31];

        assert_eq!(
            choose_self_gang_from_view(&hand, &[3], &table, 0, WIN_RULE_SHENYANG_BASIC),
            None
        );
    }

    #[test]
    fn self_gang_delays_closed_pure_one_suit_gang_before_opening_basic_hand() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 1, 1, 1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 9];

        assert_eq!(
            choose_self_gang_from_view(&hand, &[1], &table, 0, WIN_RULE_SHENYANG_BASIC),
            None
        );
    }

    #[test]
    fn self_gang_skips_ready_pure_one_suit_when_visible_fan_capped() {
        let mut table = table_with_discards(1, Vec::new());
        table.max_fan = Some(4);
        let hand = vec![1, 1, 1, 1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 9];

        assert!(ready_visible_fan_reaches_cap(
            &hand,
            &[],
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC
        ));
        assert_eq!(
            choose_self_gang_from_view(&hand, &[1], &table, 0, WIN_RULE_SHENYANG_BASIC),
            None
        );
    }

    #[test]
    fn self_gang_allows_same_closed_plain_gang_when_opening_is_not_required() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![3, 3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31];

        assert_eq!(
            choose_self_gang_from_view(&hand, &[3], &table, 0, WIN_RULE_RELAXED),
            Some(3)
        );
    }

    #[test]
    fn one_fan_capped_self_gang_delays_closed_plain_before_ready() {
        let mut table = table_with_discards(1, Vec::new());
        table.max_fan = Some(1);
        let hand = vec![3, 3, 3, 3, 4, 6, 8, 11, 13, 15, 21, 24, 27, 31];

        assert_eq!(
            choose_self_gang_from_view(&hand, &[3], &table, 0, WIN_RULE_RELAXED),
            None
        );
    }

    #[test]
    fn self_gang_skips_plain_gang_when_concealed_dragon_triplet_caps_ready_hand() {
        let mut table = table_with_discards(1, Vec::new());
        table.max_fan = Some(2);
        table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(11)];
        let hand = vec![9, 9, 9, 9, 22, 23, 31, 31, 35, 35, 35];

        assert!(ready_visible_fan_reaches_cap(
            &remove_n_tiles(&hand, 9, 1),
            table.seats.get(&0).unwrap().melds.as_slice(),
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC
        ));
        assert_eq!(
            choose_self_gang_from_view(&hand, &[9], &table, 0, WIN_RULE_SHENYANG_BASIC),
            None
        );
    }

    #[test]
    fn self_gang_delays_open_piao_plain_gang_until_ready() {
        let mut table = table_with_discards(1, Vec::new());
        table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
        let hand = vec![1, 2, 4, 5, 7, 9, 9, 9, 9, 11, 21];

        assert_eq!(
            choose_self_gang_from_view(&hand, &[9], &table, 0, WIN_RULE_SHENYANG_BASIC),
            None
        );
    }

    #[test]
    fn self_gang_delays_open_piao_added_plain_gang_until_ready() {
        let mut table = table_with_discards(1, Vec::new());
        table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(9)];
        let hand = vec![1, 2, 4, 5, 7, 9, 11, 11, 21, 21, 31];

        assert_eq!(
            choose_self_gang_from_view(&hand, &[9], &table, 0, WIN_RULE_SHENYANG_BASIC),
            None
        );
    }

    #[test]
    fn self_gang_delays_relaxed_piao_plain_gang_until_ready() {
        let mut table = table_with_discards(1, Vec::new());
        table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
        let hand = vec![1, 2, 4, 5, 7, 9, 9, 9, 9, 11, 21];

        assert_eq!(
            choose_self_gang_from_view(&hand, &[9], &table, 0, WIN_RULE_RELAXED),
            None
        );
    }

    #[test]
    fn self_gang_prefers_dragon_gang_over_plain_gang() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 2, 3, 3, 3, 3, 11, 12, 21, 22, 35, 35, 35, 35];

        assert_eq!(
            choose_self_gang_from_view(&hand, &[3, 35], &table, 0, WIN_RULE_RELAXED),
            Some(35)
        );
    }

    #[test]
    fn self_gang_passes_final_unready_hand_for_defense() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 16;
        let hand = vec![1, 2, 3, 3, 3, 3, 11, 12, 21, 22, 35, 35, 35, 35];

        assert_eq!(
            best_ready_score_after_discard(&hand, &[], &table, 0, WIN_RULE_RELAXED),
            0.0
        );
        assert_eq!(
            choose_self_gang_from_view(&hand, &[3, 35], &table, 0, WIN_RULE_RELAXED),
            None
        );
    }

    #[test]
    fn self_gang_preserves_basic_four_pairs_missing_suit_seven_pairs_plan() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 1, 1, 1, 2, 2, 3, 3, 11, 11, 12, 31, 35, 36];

        assert_eq!(
            choose_self_gang_from_view(&hand, &[1], &table, 0, WIN_RULE_SHENYANG_BASIC),
            None
        );
    }

    #[test]
    fn dealer_self_gang_preserves_basic_four_pairs_missing_suit_seven_pairs_plan() {
        let mut table = table_with_discards(1, Vec::new());
        table.dealer_position = 0;
        let hand = vec![1, 1, 1, 1, 2, 2, 3, 3, 11, 11, 12, 31, 35, 36];

        assert_eq!(
            choose_self_gang_from_view(&hand, &[1], &table, 0, WIN_RULE_SHENYANG_BASIC),
            None
        );
    }

    #[test]
    fn self_gang_preserves_five_pairs_even_for_dragon_gang() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 35, 35, 35, 35];

        assert_eq!(
            choose_self_gang_from_view(&hand, &[35], &table, 0, WIN_RULE_SHENYANG_BASIC),
            None
        );
    }

    #[test]
    fn self_gang_preserves_four_gui_yi_when_gang_breaks_ready_hand() {
        let mut table = table_with_discards(1, Vec::new());
        table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
        let hand = vec![1, 2, 3, 3, 3, 3, 11, 12, 13, 21, 35];

        assert_eq!(
            choose_self_gang_from_view(&hand, &[3], &table, 0, WIN_RULE_SHENYANG_BASIC),
            None
        );
    }

    #[test]
    fn self_gang_preserves_added_four_gui_yi_when_gang_breaks_ready_hand() {
        let mut table = table_with_discards(1, Vec::new());
        table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(3)];
        let hand = vec![1, 2, 3, 4, 5, 7, 11, 12, 13, 21, 21];

        assert_eq!(
            choose_self_gang_from_view(&hand, &[3], &table, 0, WIN_RULE_SHENYANG_BASIC),
            None
        );
    }

    #[test]
    fn self_gang_preserves_added_four_gui_yi_when_added_gang_has_no_fan_gain() {
        let mut table = table_with_discards(1, Vec::new());
        table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(3)];
        let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 21];

        assert_eq!(
            choose_self_gang_from_view(&hand, &[3], &table, 0, WIN_RULE_SHENYANG_BASIC),
            None
        );
    }

    #[test]
    fn self_gang_preserves_locked_seven_pairs_plan() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 1, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 31];

        assert_eq!(
            choose_self_gang_from_view(&hand, &[1], &table, 0, WIN_RULE_RELAXED),
            None
        );
    }

    #[test]
    fn self_gang_refuses_honor_gang_when_pure_one_suit_plan_is_strong() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 35, 35, 35, 35];

        assert_eq!(
            choose_self_gang_from_view(&hand, &[35], &table, 0, WIN_RULE_SHENYANG_BASIC),
            None
        );
    }

    #[test]
    fn self_gang_skips_plain_gang_when_ready_fan_already_capped() {
        let mut table = table_with_discards(1, Vec::new());
        table.max_fan = Some(1);
        table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
        let hand = vec![1, 2, 3, 9, 9, 9, 9, 11, 12, 13, 21];

        assert_eq!(
            choose_self_gang_from_view(&hand, &[9], &table, 0, WIN_RULE_SHENYANG_BASIC),
            None
        );
    }

    #[test]
    fn self_gang_skips_plain_gang_when_single_wait_fan_caps_ready_hand() {
        let mut table = table_with_discards(1, Vec::new());
        table.max_fan = Some(2);
        table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
        let hand = vec![1, 2, 3, 9, 9, 9, 9, 11, 12, 13, 21];

        assert_eq!(
            choose_self_gang_from_view(&hand, &[9], &table, 0, WIN_RULE_SHENYANG_BASIC),
            None
        );
    }

    fn table_with_discards(position: usize, discards: Vec<i32>) -> AiPublicTable {
        let mut seats = HashMap::new();
        seats.insert(
            0,
            AiSeatView {
                position: 0,
                hand_count: 14,
                discards: Vec::new(),
                melds: Vec::new(),
            },
        );
        seats.insert(
            position,
            AiSeatView {
                position,
                hand_count: 10,
                discards,
                melds: Vec::new(),
            },
        );
        AiPublicTable {
            current_position: 0,
            dealer_position: 1,
            wall_count: 60,
            max_fan: None,
            claim_window: None,
            seats,
        }
    }

    fn dead_terminal_or_honor_discards() -> Vec<i32> {
        SHENYANG_MAHJONG_TILE_KINDS
            .into_iter()
            .filter(|tile| is_honor(*tile) || tile_is_terminal(*tile))
            .flat_map(|tile| std::iter::repeat_n(tile, 4))
            .collect()
    }

    fn test_chi_meld(start_tile: i32) -> WsShenyangMahjongMeld {
        WsShenyangMahjongMeld {
            kind: ShenyangMahjongMeldKind::CHI,
            tiles: vec![start_tile, start_tile + 1, start_tile + 2],
            from_position: Some(1),
        }
    }

    fn test_gang_meld(tile: i32) -> WsShenyangMahjongMeld {
        WsShenyangMahjongMeld {
            kind: ShenyangMahjongMeldKind::GANG,
            tiles: vec![tile, tile, tile, tile],
            from_position: Some(1),
        }
    }

    fn test_concealed_gang_meld(tile: i32) -> WsShenyangMahjongMeld {
        WsShenyangMahjongMeld {
            kind: ShenyangMahjongMeldKind::GANG,
            tiles: vec![tile, tile, tile, tile],
            from_position: None,
        }
    }

    fn test_peng_meld(tile: i32) -> WsShenyangMahjongMeld {
        WsShenyangMahjongMeld {
            kind: ShenyangMahjongMeldKind::PENG,
            tiles: vec![tile, tile, tile],
            from_position: Some(1),
        }
    }
}
