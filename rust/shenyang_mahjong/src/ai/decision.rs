use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use share_type_public::games::shenyang_mahjong::{
    SHENYANG_MAHJONG_TILE_KINDS, ShenyangMahjongMeldKind, WsShenyangMahjongMeld,
};

use crate::rules::{
    WIN_RULE_RELAXED, WIN_RULE_SHENYANG_BASIC, can_chi, can_gang, can_peng,
    is_complete_win_with_melds, is_piao_hu_win, is_pure_one_suit_win, is_seven_pairs_win,
    is_single_wait_shape_with_rule, sort_tiles,
};

use super::observation::{AiClaimView, AiPublicTable};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiClaimChoice {
    Pass,
    Peng,
    Gang,
    Chi { consume_tiles: Vec<i32> },
    Hu,
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

fn best_chi_option(hand: &[i32], tile: i32) -> Option<Vec<i32>> {
    let mut best: Option<(f64, Vec<i32>)> = None;
    for consume_tiles in [
        [tile - 2, tile - 1],
        [tile - 1, tile + 1],
        [tile + 1, tile + 2],
    ] {
        if !can_chi(hand, tile, &consume_tiles) {
            continue;
        }
        let mut next = hand.to_vec();
        for consume in consume_tiles {
            if let Some(index) = next.iter().position(|item| *item == consume) {
                next.remove(index);
            }
        }
        next.push(tile);
        next.sort_unstable();
        let score = hand_power(&next);
        match &best {
            None => best = Some((score, consume_tiles.to_vec())),
            Some((best_score, best_tiles)) => {
                if score > *best_score
                    || (score == *best_score && consume_tiles.to_vec() < *best_tiles)
                {
                    best = Some((score, consume_tiles.to_vec()));
                }
            }
        }
    }
    best.map(|(_, tiles)| tiles)
}

fn claim_meld(
    kind: ShenyangMahjongMeldKind,
    tile: i32,
    from_position: usize,
) -> WsShenyangMahjongMeld {
    let tiles = match kind {
        ShenyangMahjongMeldKind::CHI => vec![tile - 1, tile, tile + 1],
        ShenyangMahjongMeldKind::PENG => vec![tile, tile, tile],
        ShenyangMahjongMeldKind::GANG => vec![tile, tile, tile, tile],
    };
    WsShenyangMahjongMeld {
        kind,
        tiles,
        from_position: Some(from_position as i32),
    }
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
    if pure_one_suit_plan_score_for_context(hand, &current_melds, table, position) > 0.0
        && (is_honor(tile) || !is_main_pure_suit_tile(hand, &current_melds, tile))
    {
        return Some(AiClaimChoice::Pass);
    }
    if ready_visible_fan_reaches_cap(hand, &current_melds, table, position, win_rule) {
        return Some(AiClaimChoice::Pass);
    }

    if can_gang(hand, tile) {
        if should_preserve_seven_pairs_plan_for_context(
            hand,
            &current_melds,
            table,
            position,
            win_rule,
        ) {
            return Some(AiClaimChoice::Pass);
        }
        if claim_leaves_unrecoverable_missing_suit(
            hand,
            &current_melds,
            table,
            position,
            win_rule,
            ShenyangMahjongMeldKind::GANG,
            tile,
            claim.from_position,
        ) {
            return Some(AiClaimChoice::Pass);
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
        if should_preserve_seven_pairs_plan_for_context(
            hand,
            &current_melds,
            table,
            position,
            win_rule,
        ) {
            return Some(AiClaimChoice::Pass);
        }
        if claim_leaves_unrecoverable_missing_suit(
            hand,
            &current_melds,
            table,
            position,
            win_rule,
            ShenyangMahjongMeldKind::PENG,
            tile,
            claim.from_position,
        ) {
            return Some(AiClaimChoice::Pass);
        }
        if is_dragon(tile) {
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
        melds.push(claim_meld(
            ShenyangMahjongMeldKind::PENG,
            tile,
            claim.from_position,
        ));
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
        if piao_plan_score(hand, &current_melds) >= 22.0 {
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
        if piao_plan_score(hand, &current_melds) >= 22.0 {
            return Some(AiClaimChoice::Pass);
        }
        if !is_late_round(table) {
            return Some(AiClaimChoice::Pass);
        }
        if let Some(consume_tiles) = best_chi_option(hand, tile) {
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
            if after_ready <= 0.0
                && !should_claim_chi_to_open_broken_hand_for_defense(
                    hand,
                    &current_melds,
                    table,
                    position,
                    win_rule,
                )
            {
                return Some(AiClaimChoice::Pass);
            }
            let mut required_gain = if is_suited(tile) && missing_suits.contains(&tile_suit(tile)) {
                3.0
            } else {
                7.0
            };
            if win_rule == WIN_RULE_SHENYANG_BASIC && !has_open_meld(&current_melds) {
                required_gain -= 3.0;
            }
            if piao_plan_score(hand, &current_melds) >= 22.0 {
                required_gain += 12.0;
            }
            if current_melds.is_empty() && pair_count(hand) >= 4 {
                required_gain += 8.0;
            }
            if after >= current_score + required_gain {
                return Some(AiClaimChoice::Chi { consume_tiles });
            }
        }
    }

    Some(AiClaimChoice::Pass)
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
    if is_dragon(tile) {
        return true;
    }
    if win_rule == WIN_RULE_SHENYANG_BASIC && !has_open_meld(current_melds) {
        return true;
    }
    if should_open_broken_closed_hand_for_defense(hand, current_melds, table, position, win_rule) {
        return true;
    }

    let mut next = remove_n_tiles(hand, tile, 3);
    if next.len() + 3 != hand.len() {
        return false;
    }
    sort_tiles(&mut next);
    let mut melds = current_melds.to_vec();
    melds.push(claim_meld(
        ShenyangMahjongMeldKind::GANG,
        tile,
        from_position,
    ));
    let reaches_ready = ready_tile_score(&next, &melds, table, position, win_rule) > 0.0;
    if piao_plan_score_for_context(hand, current_melds, table, position) >= 22.0 {
        return reaches_ready;
    }
    reaches_ready
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
        || !is_late_round(table)
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
        || piao_plan_score(hand, melds) >= 22.0
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

fn claim_leaves_unrecoverable_missing_suit(
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

    let remove_count = match kind {
        ShenyangMahjongMeldKind::PENG => 2,
        ShenyangMahjongMeldKind::GANG => 3,
        ShenyangMahjongMeldKind::CHI => return false,
    };
    let mut next = remove_n_tiles(hand, tile, remove_count);
    if next.len() + remove_count != hand.len() || next.is_empty() {
        return false;
    }
    sort_tiles(&mut next);
    let mut melds = current_melds.to_vec();
    melds.push(claim_meld(kind, tile, from_position));

    !unique_tiles(&next).into_iter().any(|discard| {
        let after_discard = remove_n_tiles(&next, discard, 1);
        let missing = missing_suits(&after_discard, &melds);
        missing.is_empty()
            || missing.iter().all(|suit| {
                live_tile_count_for_suit_after_discard(
                    &after_discard,
                    &melds,
                    table,
                    position,
                    *suit,
                    discard,
                ) > 0
            })
    })
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
    if is_late_defense_round(table)
        && best_ready_score_after_discard(hand, melds, table, position, win_rule) <= 0.0
    {
        return choose_late_defense_discard(hand, table, position);
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
                );
        let score = hand_progress_score(&next, melds, table, position, win_rule);
        let pressure = estimate_pressure_for_tile(table, position, tile);
        let count = hand.iter().filter(|&&item| item == tile).count();
        let neigh = neighbor_count(hand, tile);
        let discard_bias =
            match (count, is_honor(tile), tile_is_terminal(tile), neigh) {
                (c, true, _, _) if c == 1 => honor_discard_bias(hand, tile),
                (1, _, true, 0) => 4.8,
                (1, _, _, 0) => 4.0,
                (2, _, _, _) => pair_discard_bias(hand),
                (c, _, _, neigh) if c >= 3 => -4.5 - neigh as f64,
                _ => 0.0,
            } + three_suits_discard_bias(&next, melds, table, position, tile, win_rule)
                + terminal_or_honor_discard_bias(&next, melds, table, position, tile, win_rule)
                + piao_discard_bias(hand, tile, melds)
                + early_piao_candidate_discard_bias(hand, tile, melds)
                + seven_pairs_plan_discard_bias(hand, tile, melds)
                + seven_pairs_wait_discard_bias(hand, tile, melds, table, position)
                + pure_one_suit_discard_bias(hand, tile, melds, table, position)
                + mid_round_public_discard_bias(table, position, tile)
                + opponent_threat_discard_bias(table, position, tile, count)
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
    let mut best: Option<(f64, i32)> = None;
    let candidates = unique_tiles(hand);
    let public_candidates = candidates
        .iter()
        .copied()
        .filter(|tile| public_discard_count(table, position, *tile) > 0)
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

fn late_defense_tile_safety_score(
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    own_tile_count: usize,
) -> f64 {
    late_defense_discard_bias(table, position, tile)
        + opponent_threat_discard_bias(table, position, tile, own_tile_count)
        + opponent_missing_suit_safety_bias(table, position, tile)
        + closed_opponent_threat_discard_bias(table, position, tile)
        + estimate_pressure_for_tile(table, position, tile)
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

fn self_gang_score(
    tile: i32,
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    current_score: f64,
) -> f64 {
    if pure_one_suit_plan_score_for_context(hand, melds, table, position) > 0.0
        && (is_honor(tile) || !is_main_pure_suit_tile(hand, melds, tile))
    {
        return f64::NEG_INFINITY;
    }

    let is_added_gang = has_peng_meld(melds, tile);
    let is_ready = best_ready_score_after_discard(hand, melds, table, position, win_rule) > 0.0;
    if is_ready && ready_visible_fan_reaches_cap(hand, melds, table, position, win_rule) {
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
    let mut next_melds = melds.to_vec();
    if !is_added_gang {
        next_melds.push(WsShenyangMahjongMeld {
            kind: ShenyangMahjongMeldKind::GANG,
            tiles: vec![tile, tile, tile, tile],
            from_position: None,
        });
    }
    let after_ready_score = ready_tile_score(&next, &next_melds, table, position, win_rule);
    if is_ready && after_ready_score <= 0.0 && should_preserve_four_gui_yi(tile, is_added_gang) {
        return f64::NEG_INFINITY;
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
    if piao_plan_score(hand, melds) >= 22.0 {
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
                && estimated_visible_fan_without_wait(&next, melds) >= max_fan
        }
    })
}

fn should_preserve_seven_pairs_for_self_gang(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> bool {
    if !melds.is_empty() {
        return false;
    }
    let pairs = pair_count(hand);
    if pairs >= 5 {
        return true;
    }
    if table.dealer_position == position {
        return false;
    }
    if win_rule == WIN_RULE_SHENYANG_BASIC {
        return pairs >= 4 && !missing_suits(hand, melds).is_empty();
    }
    false
}

fn has_peng_meld(melds: &[WsShenyangMahjongMeld], tile: i32) -> bool {
    melds.iter().any(|meld| {
        meld.kind == ShenyangMahjongMeldKind::PENG
            && meld.tiles.iter().all(|meld_tile| *meld_tile == tile)
    })
}

fn should_preserve_four_gui_yi(tile: i32, is_added_gang: bool) -> bool {
    !is_added_gang && !is_dragon(tile)
}

fn should_open_broken_closed_hand_for_defense(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> bool {
    if win_rule != WIN_RULE_SHENYANG_BASIC
        || has_open_meld(melds)
        || table.dealer_position == position
        || !is_late_round(table)
    {
        return false;
    }
    if should_preserve_seven_pairs_plan_for_context(hand, melds, table, position, win_rule)
        || pure_one_suit_plan_score_for_context(hand, melds, table, position) > 0.0
        || piao_plan_score_for_context(hand, melds, table, position) >= 22.0
    {
        return false;
    }
    if ready_tile_score(hand, melds, table, position, WIN_RULE_RELAXED) > 0.0
        || one_step_wait_potential(hand, melds, table, position, WIN_RULE_RELAXED) > 0.0
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
    missing_rule_requirements >= 1 || hand_power(hand) < 18.0
}

fn estimate_pressure_for_tile(table: &AiPublicTable, position: usize, tile: i32) -> f64 {
    let mut pressure = 0.0;
    for (seat_position, seat) in &table.seats {
        if *seat_position == position || seat.is_away || seat.is_ai {
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

fn late_defense_discard_bias(table: &AiPublicTable, position: usize, tile: i32) -> f64 {
    if !is_late_defense_round(table) {
        return 0.0;
    }
    let public_discards = public_discard_count(table, position, tile);
    if public_discards > 0 {
        let honor_bonus = if is_honor(tile) { 14.0 } else { 0.0 };
        let suited_shape_bonus = if is_suited(tile) {
            if tile_is_terminal(tile) { -1.0 } else { 2.0 }
        } else {
            0.0
        };
        return 28.0 + public_discards as f64 * 6.0 + honor_bonus + suited_shape_bonus;
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

fn mid_round_public_discard_bias(table: &AiPublicTable, position: usize, tile: i32) -> f64 {
    if !is_late_round(table) || is_late_defense_round(table) {
        return 0.0;
    }
    let public_discards = public_discard_count(table, position, tile);
    if public_discards == 0 {
        return 0.0;
    }
    let shape_bonus = if is_honor(tile) {
        3.0
    } else if tile_is_terminal(tile) {
        1.5
    } else {
        2.0
    };
    9.0 + public_discards as f64 * 4.0 + shape_bonus
}

fn opponent_threat_discard_bias(
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    own_tile_count: usize,
) -> f64 {
    let mut bias = 0.0;
    for (seat_position, seat) in &table.seats {
        if *seat_position == position || seat.is_away || seat.is_ai {
            continue;
        }
        if piao_threat_level(&seat.melds) < 3 {
            continue;
        }
        if seat.discards.contains(&tile) {
            bias += 4.5;
            continue;
        }
        let live_tile_penalty = if is_dragon(tile) {
            7.0
        } else if is_wind(tile) {
            5.0
        } else if tile_is_terminal(tile) {
            4.0
        } else {
            5.5
        };
        let pair_penalty = if own_tile_count >= 2 { 4.0 } else { 0.0 };
        let late_multiplier = if is_late_round(table) { 1.35 } else { 1.0 };
        bias -= (live_tile_penalty + pair_penalty) * late_multiplier;
    }
    bias
}

fn opponent_missing_suit_safety_bias(table: &AiPublicTable, position: usize, tile: i32) -> f64 {
    if !is_late_defense_round(table) || !is_suited(tile) {
        return 0.0;
    }
    let suit = tile_suit(tile);
    table
        .seats
        .iter()
        .filter(|(seat_position, seat)| **seat_position != position && !seat.is_away && !seat.is_ai)
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

fn closed_opponent_threat_discard_bias(table: &AiPublicTable, position: usize, tile: i32) -> f64 {
    if !is_late_defense_round(table) || public_discard_count(table, position, tile) > 0 {
        return 0.0;
    }

    table
        .seats
        .iter()
        .filter(|(seat_position, seat)| {
            **seat_position != position
                && !seat.is_away
                && !seat.is_ai
                && seat.melds.is_empty()
                && seat.hand_count >= 10
        })
        .map(|(_, _)| {
            if is_dragon(tile) {
                -13.0
            } else if is_wind(tile) {
                -12.0
            } else if tile_is_terminal(tile) {
                -9.0
            } else {
                -5.0
            }
        })
        .sum()
}

fn piao_threat_level(melds: &[WsShenyangMahjongMeld]) -> usize {
    melds
        .iter()
        .filter(|meld| is_triplet_like_meld(meld))
        .count()
}

fn public_discard_count(table: &AiPublicTable, position: usize, tile: i32) -> usize {
    table
        .seats
        .iter()
        .filter(|(seat_position, _)| **seat_position != position)
        .map(|(_, seat)| {
            seat.discards
                .iter()
                .filter(|discard| **discard == tile)
                .count()
        })
        .sum()
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

fn should_preserve_seven_pairs_plan(hand: &[i32]) -> bool {
    is_seven_pairs_wait_shape(hand) || (hand.len() == 13 && pair_count(hand) >= 5)
}

fn should_preserve_seven_pairs_plan_for_context(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> bool {
    if !melds.is_empty() || hand.len() != 13 {
        return false;
    }
    if is_seven_pairs_wait_shape(hand) {
        return true;
    }
    let pairs = pair_count(hand);
    if pairs >= 5 {
        return true;
    }
    if pairs < 4 || table.dealer_position == position {
        return false;
    }
    if win_rule == WIN_RULE_SHENYANG_BASIC {
        !missing_suits(hand, melds).is_empty()
    } else {
        false
    }
}

fn dragon_value_bias(hand: &[i32], tile: i32) -> f64 {
    if !is_dragon(tile) {
        return 0.0;
    }
    let pairs = pair_count(hand);
    if pairs >= 4 { 0.4 } else { -3.0 }
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

fn is_wind(tile: i32) -> bool {
    matches!(tile, 31..=34)
}

fn pair_count(hand: &[i32]) -> usize {
    let mut counts: HashMap<i32, usize> = HashMap::new();
    for &tile in hand {
        *counts.entry(tile).or_default() += 1;
    }
    counts.values().map(|count| count / 2).sum()
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

fn pair_discard_bias(hand: &[i32]) -> f64 {
    if pair_count(hand) >= 4 { -4.4 } else { -1.8 }
}

fn seven_pairs_plan_discard_bias(hand: &[i32], tile: i32, melds: &[WsShenyangMahjongMeld]) -> f64 {
    if !melds.is_empty() || hand.len() != 14 || pair_count(hand) != 5 {
        return 0.0;
    }
    let count = hand.iter().filter(|item| **item == tile).count();
    if count >= 2 {
        return -12.0;
    }
    if is_honor(tile) {
        5.0
    } else if tile_is_terminal(tile) {
        0.0
    } else {
        3.0
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
    if hand.iter().filter(|item| **item == tile).count() != 1 {
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

fn seven_pairs_wait_tile_score(
    wait_tile: i32,
    hand_after_discard: &[i32],
    table: &AiPublicTable,
    position: usize,
) -> f64 {
    let public_discards = public_discard_count(table, position, wait_tile) as f64;
    let remaining = remaining_tile_count(hand_after_discard, table, position, wait_tile) as f64;
    let shape = if is_wind(wait_tile) {
        10.0
    } else if is_dragon(wait_tile) {
        7.0
    } else if tile_is_terminal(wait_tile) {
        8.0
    } else {
        -4.0
    };
    shape + remaining * 3.0 - public_discards * 9.0
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
    if table.dealer_position == position && pairs < 6 {
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

fn piao_plan_score_for_context(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
) -> f64 {
    let score = piao_plan_score(hand, melds);
    if table.dealer_position == position && score < 40.0 {
        score * 0.35
    } else {
        score
    }
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
    if open_triplets + triplets >= 2 || pairs >= 4 || (open_triplets >= 1 && pairs >= 3) {
        score
    } else {
        0.0
    }
}

fn piao_discard_bias(hand: &[i32], tile: i32, melds: &[WsShenyangMahjongMeld]) -> f64 {
    if piao_plan_score(hand, melds) < 20.0 {
        return 0.0;
    }
    let count = hand.iter().filter(|item| **item == tile).count();
    if count >= 3 {
        -16.0
    } else if count == 2 {
        -9.0
    } else if is_honor(tile) || tile_is_terminal(tile) {
        1.0
    } else if neighbor_count(hand, tile) >= 2 {
        3.0
    } else {
        0.0
    }
}

fn early_piao_candidate_discard_bias(
    hand: &[i32],
    tile: i32,
    melds: &[WsShenyangMahjongMeld],
) -> f64 {
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
    if count >= 3 {
        -10.0
    } else if count == 2 {
        -6.5
    } else {
        0.0
    }
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
        }
    }
    if wait_kinds >= 2 {
        score += wait_kinds as f64 * 3.0;
    }
    score
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
    if let Some(max_fan) = table.max_fan {
        let visible_fan = estimated_visible_fan_without_wait(win_hand, melds);
        if visible_fan >= max_fan {
            return 0.0;
        }
        if visible_fan + 1 >= max_fan {
            return if remaining >= 3 { 14.0 } else { 0.0 };
        }
    }

    let terminal_or_honor_bonus = if tile_is_terminal(win_tile) || is_honor(win_tile) {
        14.0
    } else {
        0.0
    };
    62.0 + terminal_or_honor_bonus
}

fn estimated_visible_fan_without_wait(win_hand: &[i32], melds: &[WsShenyangMahjongMeld]) -> i32 {
    let is_piao = is_piao_hu_win(win_hand, melds);
    let base = if is_piao {
        3
    } else if is_seven_pairs_win(win_hand) || is_pure_one_suit_win(win_hand, melds) {
        4
    } else {
        1
    };
    let meld_fan = melds
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
        .sum::<i32>();
    let shou_ba_yi_fan = if is_piao && melds.len() == 4 && win_hand.len() == 2 {
        1
    } else {
        0
    };
    base + meld_fan + shou_ba_yi_fan + estimated_four_gui_yi_fan(win_hand, melds)
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
    true
}

fn shenyang_rule_progress_score(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> f64 {
    if win_rule != WIN_RULE_SHENYANG_BASIC || should_preserve_seven_pairs_plan(hand) {
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

fn is_main_pure_suit_tile(hand: &[i32], melds: &[WsShenyangMahjongMeld], tile: i32) -> bool {
    dominant_pure_suit(hand, melds).is_some_and(|suit| is_suited(tile) && tile_suit(tile) == suit)
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

fn has_open_meld(melds: &[WsShenyangMahjongMeld]) -> bool {
    melds.iter().any(|meld| meld.from_position.is_some())
}

fn is_late_round(table: &AiPublicTable) -> bool {
    table.wall_count <= 42
}

fn is_late_defense_round(table: &AiPublicTable) -> bool {
    table.wall_count <= 20
}

fn is_triplet_like_meld(meld: &WsShenyangMahjongMeld) -> bool {
    matches!(
        meld.kind,
        ShenyangMahjongMeldKind::PENG | ShenyangMahjongMeldKind::GANG
    ) && meld
        .tiles
        .first()
        .is_some_and(|tile| meld.tiles.iter().all(|item| item == tile))
}

fn meld_primary_tile(meld: &WsShenyangMahjongMeld) -> Option<i32> {
    let first = *meld.tiles.first()?;
    meld.tiles
        .iter()
        .all(|tile| *tile == first)
        .then_some(first)
}

fn has_triplet_or_dragon_pair(hand: &[i32], melds: &[WsShenyangMahjongMeld]) -> bool {
    let mut counts = HashMap::<i32, usize>::new();
    for tile in hand
        .iter()
        .copied()
        .chain(melds.iter().flat_map(|meld| meld.tiles.iter().copied()))
    {
        *counts.entry(tile).or_default() += 1;
    }
    counts.values().any(|count| *count >= 3)
        || [35, 36, 37]
            .into_iter()
            .any(|tile| counts.get(&tile).copied().unwrap_or(0) >= 2)
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

fn missing_suits(hand: &[i32], melds: &[WsShenyangMahjongMeld]) -> Vec<i32> {
    suit_presence(hand, melds)
        .into_iter()
        .enumerate()
        .filter_map(|(suit, present)| (!present).then_some(suit as i32))
        .collect()
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

fn live_tile_count_for_suit_after_discard(
    hand_after_discard: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    suit: i32,
    discarded_tile: i32,
) -> i32 {
    (1..=9)
        .map(|rank| {
            let tile = suit * 10 + rank;
            let visible = visible_tile_count(table, position, tile);
            let own_hand = hand_after_discard
                .iter()
                .filter(|item| **item == tile)
                .count() as i32;
            let own_melds = melds
                .iter()
                .flat_map(|meld| meld.tiles.iter())
                .filter(|item| **item == tile)
                .count() as i32;
            let own_discard = i32::from(discarded_tile == tile);
            (4 - visible - own_hand - own_melds - own_discard).max(0)
        })
        .sum()
}

fn remaining_tile_count(hand: &[i32], table: &AiPublicTable, position: usize, tile: i32) -> i32 {
    let visible = visible_tile_count(table, position, tile);
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

fn unique_tiles(hand: &[i32]) -> Vec<i32> {
    let mut tiles = hand.to_vec();
    tiles.sort_unstable();
    tiles.dedup();
    tiles
}

fn visible_tile_count(table: &AiPublicTable, position: usize, tile: i32) -> i32 {
    table
        .seats
        .iter()
        .filter(|(seat_position, _)| **seat_position != position)
        .map(|(_, seat)| {
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

fn is_honor(tile: i32) -> bool {
    tile >= 31
}

fn is_suited(tile: i32) -> bool {
    matches!(tile, 1..=9 | 11..=19 | 21..=29)
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

fn tile_is_middle_of_sequence(hand: &[i32], tile: i32) -> bool {
    if !is_suited(tile) || !(2..=8).contains(&tile_rank(tile)) {
        return false;
    }
    let left = tile - 1;
    let right = tile + 1;
    hand.iter().any(|item| *item == left) && hand.iter().any(|item| *item == right)
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

fn tile_is_terminal(tile: i32) -> bool {
    matches!(tile_rank(tile), 1 | 9)
}

fn tile_rank(tile: i32) -> i32 {
    tile % 10
}

fn tile_suit(tile: i32) -> i32 {
    tile / 10
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::ai::observation::{AiClaimView, AiSeatView};
    use crate::rules::{WIN_RULE_RELAXED, WIN_RULE_SHENYANG_BASIC};

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
    fn dealer_claim_peng_preserves_five_pairs_seven_pairs_plan() {
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
    fn claim_peng_can_open_locked_pure_one_suit_plan_with_main_suit() {
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
            Some(AiClaimChoice::Peng)
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
    fn broken_closed_defense_does_not_override_near_ready_hand() {
        let mut table = table_with_discards(1, Vec::new());
        table.wall_count = 40;
        let hand = vec![2, 2, 3, 4, 5, 11, 12, 13, 21, 22, 23, 31, 35];

        assert!(!should_open_broken_closed_hand_for_defense(
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
    fn self_gang_prefers_dragon_gang_over_plain_gang() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 2, 3, 3, 3, 3, 11, 12, 21, 22, 35, 35, 35, 35];

        assert_eq!(
            choose_self_gang_from_view(&hand, &[3, 35], &table, 0, WIN_RULE_RELAXED),
            Some(35)
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
    fn self_gang_preserves_five_pairs_even_for_dragon_gang() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 35, 35, 35, 35];

        assert_eq!(
            choose_self_gang_from_view(&hand, &[35], &table, 0, WIN_RULE_SHENYANG_BASIC),
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
    fn self_gang_delays_closed_plain_gang_before_opening_basic_hand() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![3, 3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31];

        assert_eq!(
            choose_self_gang_from_view(&hand, &[3], &table, 0, WIN_RULE_SHENYANG_BASIC),
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
    fn self_gang_refuses_honor_gang_when_pure_one_suit_plan_is_strong() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 35, 35, 35, 35];

        assert_eq!(
            choose_self_gang_from_view(&hand, &[35], &table, 0, WIN_RULE_SHENYANG_BASIC),
            None
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
    fn discard_preserves_last_tile_of_a_suit_for_three_suits() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 11, 12, 13, 14, 15, 16, 21, 22, 23, 24, 25, 26, 31];

        assert_ne!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
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
    fn discard_can_pursue_pure_one_suit_when_shape_is_strong() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 7, 8, 9, 11];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(11)
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
    fn discard_clears_honor_when_early_pure_one_suit_plan_is_available() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 11, 12, 31, 35];

        assert!(matches!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(31 | 35 | 11 | 12)
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
    fn discard_preserves_only_terminal_or_honor_for_basic_rule() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 5, 6];

        assert_ne!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(1)
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
    fn discard_preserves_three_pair_three_suit_piao_candidate() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 1, 4, 5, 6, 11, 11, 12, 13, 14, 21, 21, 22, 23];

        assert!(!matches!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(1 | 11 | 21)
        ));
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
    fn missing_suits_tracks_three_suits_need() {
        let hand = vec![1, 2, 3, 11, 18, 19, 21, 22, 23, 24, 25, 26, 35, 36];

        assert!(missing_suits(&hand, &[]).is_empty());
        assert_eq!(missing_suits(&hand[0..6], &[]), vec![2]);
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
    fn discard_preserves_ready_hand_instead_of_breaking_wait() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 31, 31, 32];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(32)
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
    fn estimated_visible_fan_counts_piao_shou_ba_yi_before_wait_fan() {
        let win_hand = vec![35, 35];
        let melds = vec![
            test_peng_meld(1),
            test_peng_meld(11),
            test_peng_meld(21),
            test_peng_meld(31),
        ];

        assert_eq!(estimated_visible_fan_without_wait(&win_hand, &melds), 4);
    }

    #[test]
    fn estimated_visible_fan_counts_four_gui_yi_before_wait_fan() {
        let win_hand = vec![2, 3, 4, 11, 12, 13, 21, 22, 23, 35, 35];
        let melds = vec![test_peng_meld(2)];

        assert_eq!(estimated_visible_fan_without_wait(&win_hand, &melds), 2);
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
    fn one_step_wait_potential_values_near_ready_shape() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 31, 35];

        assert!(
            one_step_wait_potential(&hand, &[], &table, 0, WIN_RULE_RELAXED) > 0.0,
            "near-ready hand should see useful draws"
        );
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
    fn discard_keeps_pairs_for_basic_seven_pairs_plan_when_missing_suit() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 12, 31, 35, 36, 37];

        let discard = choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC);

        assert!(matches!(discard, Some(31 | 36 | 37)));
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
    fn discard_locked_five_pairs_prefers_honor_singleton_first() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 1, 2, 2, 5, 9, 11, 11, 12, 12, 14, 21, 21, 31];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(31)
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
    fn discard_sets_seven_pairs_wait_on_live_terminal_over_dead_wind() {
        let table = table_with_discards(1, vec![31, 31]);
        let hand = vec![1, 1, 2, 2, 9, 11, 11, 12, 12, 21, 21, 22, 22, 31];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(31)
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
    fn discard_uses_public_discard_safety() {
        let table = table_with_discards(1, vec![31]);
        let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 32];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
            Some(31)
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
    fn late_defense_prefers_public_honor_over_multiple_public_suited_tile() {
        let mut table = table_with_discards(1, vec![5, 5, 31]);
        table.wall_count = 16;

        assert!(
            late_defense_tile_safety_score(&table, 0, 31, 1)
                > late_defense_tile_safety_score(&table, 0, 5, 1)
        );
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
                is_ai: false,
                is_away: false,
                hand_count: 10,
                discards: missing_suit_discards.clone(),
                melds: Vec::new(),
            },
        );
        table.seats.insert(
            3,
            AiSeatView {
                position: 3,
                is_ai: false,
                is_away: false,
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
    fn closed_opponent_threat_does_not_penalize_public_safe_tile() {
        let mut table = table_with_discards(1, vec![31]);
        table.wall_count = 16;
        table.seats.get_mut(&1).unwrap().hand_count = 13;

        assert_eq!(closed_opponent_threat_discard_bias(&table, 0, 31), 0.0);
        assert!(closed_opponent_threat_discard_bias(&table, 0, 32) < 0.0);
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

    fn test_peng_meld(tile: i32) -> WsShenyangMahjongMeld {
        WsShenyangMahjongMeld {
            kind: ShenyangMahjongMeldKind::PENG,
            tiles: vec![tile, tile, tile],
            from_position: Some(1),
        }
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

    fn table_with_discards(position: usize, discards: Vec<i32>) -> AiPublicTable {
        let mut seats = HashMap::new();
        seats.insert(
            0,
            AiSeatView {
                position: 0,
                is_ai: true,
                is_away: false,
                hand_count: 14,
                discards: Vec::new(),
                melds: Vec::new(),
            },
        );
        seats.insert(
            position,
            AiSeatView {
                position,
                is_ai: false,
                is_away: false,
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
}
