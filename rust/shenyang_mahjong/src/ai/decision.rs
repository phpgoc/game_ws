use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use share_type_public::games::shenyang_mahjong::{
    SHENYANG_MAHJONG_TILE_KINDS, ShenyangMahjongMeldKind, WsShenyangMahjongMeld,
};

use crate::rules::{
    WIN_RULE_SHENYANG_BASIC, can_chi, can_gang, can_peng, is_complete_win_with_melds, sort_tiles,
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

    if can_gang(hand, tile) {
        return Some(AiClaimChoice::Gang);
    }

    let current_melds = table
        .seats
        .get(&position)
        .map(|seat| seat.melds.clone())
        .unwrap_or_default();
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
        if win_rule == WIN_RULE_SHENYANG_BASIC && table.dealer_position == position {
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

    if position == next_position_after(claim.from_position, table) {
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
        if win_rule == WIN_RULE_SHENYANG_BASIC
            && !has_open_meld(&current_melds)
            && !is_late_round(table)
            && table.dealer_position != position
        {
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

    let mut best: Option<(f64, i32)> = None;
    for tile in hand.iter().copied() {
        let mut next = hand.to_vec();
        if let Some(index) = next.iter().position(|item| *item == tile) {
            next.remove(index);
        }
        let score = hand_progress_score(&next, melds, table, position, win_rule);
        let pressure = estimate_pressure_for_tile(table, position, tile);
        let count = hand.iter().filter(|&&item| item == tile).count();
        let neigh = neighbor_count(hand, tile);
        let discard_bias = match (count, is_honor(tile), tile_is_terminal(tile), neigh) {
            (c, true, _, _) if c == 1 => honor_discard_bias(hand, tile),
            (1, _, true, 0) => 4.8,
            (1, _, _, 0) => 4.0,
            (2, _, _, _) => pair_discard_bias(hand),
            (c, _, _, neigh) if c >= 3 => -4.5 - neigh as f64,
            _ => 0.0,
        } + three_suits_discard_bias(&next, melds, tile, win_rule)
            + terminal_or_honor_discard_bias(&next, melds, tile, win_rule)
            + piao_discard_bias(hand, tile, melds);
        let combined = score + discard_bias + pressure;
        match best {
            None => best = Some((combined, tile)),
            Some((best_score, best_tile)) => {
                let better = combined.partial_cmp(&best_score) == Some(Ordering::Greater);
                if better || (combined == best_score && tile < best_tile) {
                    best = Some((combined, tile));
                }
            }
        }
    }
    best.map(|(_, tile)| tile)
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
        + shenyang_rule_progress_score(hand, melds, win_rule)
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
    if pairs < 4 || table.dealer_position == position {
        return false;
    }
    if win_rule == WIN_RULE_SHENYANG_BASIC {
        !missing_suits(hand, melds).is_empty()
    } else {
        pairs >= 5
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

fn pair_discard_bias(hand: &[i32]) -> f64 {
    if pair_count(hand) >= 4 { -4.4 } else { -1.8 }
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
    if piao_plan_score(hand, melds) < 22.0 {
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
        }
    }
    if wait_kinds >= 2 {
        score += wait_kinds as f64 * 3.0;
    }
    score
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
    tile: i32,
    win_rule: i32,
) -> f64 {
    if !is_suited(tile) {
        return 0.0;
    }
    if melds.is_empty() && pair_count(hand_after_discard) >= 4 {
        return 0.0;
    }
    if win_rule == WIN_RULE_SHENYANG_BASIC
        && pure_one_suit_plan_score(hand_after_discard, melds) > 0.0
    {
        return 0.0;
    }
    let mut bias = 0.0;
    let suit = tile_suit(tile);
    let before = suit_presence_with_extra(hand_after_discard, melds, Some(tile));
    let after = suit_presence(hand_after_discard, melds);
    if before[suit as usize] && !after[suit as usize] {
        bias -= 14.0;
    }
    if after.into_iter().filter(|present| *present).count() < 3 {
        bias -= 2.5;
    }
    bias
}

fn terminal_or_honor_discard_bias(
    hand_after_discard: &[i32],
    melds: &[WsShenyangMahjongMeld],
    tile: i32,
    win_rule: i32,
) -> f64 {
    if win_rule != WIN_RULE_SHENYANG_BASIC || !(is_honor(tile) || tile_is_terminal(tile)) {
        return 0.0;
    }
    let before = has_terminal_or_honor_with_extra(hand_after_discard, melds, Some(tile));
    let after = has_terminal_or_honor_with_extra(hand_after_discard, melds, None);
    if before && !after { -12.0 } else { 0.0 }
}

fn shenyang_rule_progress_score(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_rule: i32,
) -> f64 {
    if win_rule != WIN_RULE_SHENYANG_BASIC || should_preserve_seven_pairs_plan(hand) {
        return 0.0;
    }
    let mut score = pure_one_suit_plan_score(hand, melds);
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
    if melds
        .iter()
        .any(|meld| meld.tiles.iter().any(|tile| is_honor(*tile)))
    {
        return 0.0;
    }
    let all_tiles = hand
        .iter()
        .copied()
        .chain(melds.iter().flat_map(|meld| meld.tiles.iter().copied()))
        .collect::<Vec<_>>();
    let suited = all_tiles
        .iter()
        .copied()
        .filter(|tile| is_suited(*tile))
        .collect::<Vec<_>>();
    if suited.len() < all_tiles.len().saturating_sub(1) {
        return 0.0;
    }
    let mut suit_counts = [0usize; 3];
    for tile in suited {
        suit_counts[tile_suit(tile) as usize] += 1;
    }
    let best = suit_counts.into_iter().max().unwrap_or(0);
    let off_suit = all_tiles
        .iter()
        .filter(|tile| is_suited(**tile) && suit_counts[tile_suit(**tile) as usize] != best)
        .count();
    if best >= 10 && off_suit <= 2 {
        18.0 + best as f64 * 1.5 - off_suit as f64 * 4.0
    } else {
        0.0
    }
}

fn has_open_meld(melds: &[WsShenyangMahjongMeld]) -> bool {
    melds.iter().any(|meld| meld.from_position.is_some())
}

fn is_late_round(table: &AiPublicTable) -> bool {
    table.wall_count <= 42
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
    fn dealer_claim_peng_does_not_chase_seven_pairs_with_five_pairs() {
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
            Some(AiClaimChoice::Peng)
        );
    }

    #[test]
    fn claim_peng_takes_pair_when_five_pairs_have_three_suits_for_piao() {
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
    fn claim_chi_opens_closed_basic_hand_late() {
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
            Some(AiClaimChoice::Chi {
                consume_tiles: vec![1, 2]
            })
        );
    }

    #[test]
    fn dealer_claim_chi_can_open_closed_basic_hand_early() {
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
            Some(AiClaimChoice::Chi {
                consume_tiles: vec![1, 2]
            })
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
    fn discard_can_pursue_pure_one_suit_when_shape_is_strong() {
        let table = table_with_discards(1, Vec::new());
        let hand = vec![1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 7, 8, 9, 11];

        assert_eq!(
            choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
            Some(11)
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
            claim_window: None,
            seats,
        }
    }
}
