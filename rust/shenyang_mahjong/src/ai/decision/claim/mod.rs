mod chi_choice;
mod defensive_open;
mod gang;
mod gang_choice;
mod options;
mod peng;
mod peng_choice;
mod preserve;
mod requirements;

use super::*;

pub(super) use defensive_open::*;
pub(super) use gang::*;
pub(super) use options::*;
pub(super) use peng::*;
#[cfg(test)]
pub(super) use peng_choice::required_peng_gain;
pub(super) use preserve::*;
pub(super) use requirements::*;

use chi_choice::choose_chi_claim;
use gang_choice::choose_gang_claim;
use peng_choice::choose_peng_claim;

pub(in crate::ai::decision) const CAPPED_HU_CHASE_MIN_WALL_HIT_PROBABILITY: f64 = 2.0 / 3.0;
const EXPECTED_OPPONENT_COUNT: usize = 3;

const MIN_WALL_TILES_FOR_CAPPED_HU_CHASE: usize = 4;
const UNKNOWN_OPPONENT_HAND_COUNT: usize = 13;

pub(in crate::ai::decision) fn capped_hu_chase_wall_hit_probability(
    table: &AiPublicTable,
    position: usize,
    live_wait_copies: i32,
) -> f64 {
    if live_wait_copies <= 0 || table.wall_count == 0 {
        return 0.0;
    }
    let mut observed_opponents = 0usize;
    let mut opponent_hand_tiles = 0usize;
    for (_, seat) in table
        .seats
        .iter()
        .filter(|(seat_position, _)| **seat_position != position)
    {
        observed_opponents += 1;
        opponent_hand_tiles = opponent_hand_tiles.saturating_add(seat.hand_count);
    }
    let missing_opponents = EXPECTED_OPPONENT_COUNT.saturating_sub(observed_opponents);
    opponent_hand_tiles = opponent_hand_tiles
        .saturating_add(missing_opponents.saturating_mul(UNKNOWN_OPPONENT_HAND_COUNT));

    let unseen_tiles = table.wall_count.saturating_add(opponent_hand_tiles);
    let target_tiles = (live_wait_copies as usize).min(unseen_tiles);
    let wall_tiles = table.wall_count.min(unseen_tiles);
    if target_tiles == 0 || wall_tiles == 0 {
        return 0.0;
    }
    let non_target_tiles = unseen_tiles - target_tiles;
    if wall_tiles > non_target_tiles {
        return 1.0;
    }

    // Hypergeometric miss chance for exposing the whole remaining wall.
    let mut miss_probability = 1.0;
    for draw_index in 0..wall_tiles {
        miss_probability *=
            (non_target_tiles - draw_index) as f64 / (unseen_tiles - draw_index) as f64;
    }
    1.0 - miss_probability
}

pub fn choose_claim_from_view(
    hand: &[i32],
    claim: &AiClaimView,
    table: &AiPublicTable,
    position: usize,
) -> Option<AiClaimChoice> {
    if position == claim.from_position || !claim.eligible_positions.contains(&position) {
        return None;
    }
    let melds = table
        .seats
        .get(&position)
        .map(|seat| seat.melds.as_slice())
        .unwrap_or(&[]);
    if !has_virtual_tile_count(hand, melds, 13)
        || !claim_known_tile_counts_are_possible(hand, melds, claim, table)
    {
        return Some(AiClaimChoice::Pass);
    }
    let tile = claim.tile;
    let mut win_hand = hand.to_vec();
    win_hand.push(tile);
    win_hand.sort_unstable();
    if is_complete_win_for_table(&win_hand, melds, table) {
        if should_pass_hu_for_capped_live_wait(hand, &win_hand, melds, table, position, tile) {
            return Some(AiClaimChoice::Pass);
        }
        return Some(AiClaimChoice::Hu);
    }

    let current_melds = table
        .seats
        .get(&position)
        .map(|seat| seat.melds.clone())
        .unwrap_or_default();
    let current_ready_score = ready_tile_score(hand, &current_melds, table, position);
    if should_pass_late_unready_claim_for_defense(table, current_ready_score) {
        return Some(AiClaimChoice::Pass);
    }
    if ready_visible_fan_reaches_cap(hand, &current_melds, table, position) {
        return Some(AiClaimChoice::Pass);
    }
    if current_ready_score > 0.0
        && ready_visible_fan_exceeds_half_cap(hand, &current_melds, table, position)
    {
        return Some(AiClaimChoice::Pass);
    }

    if let Some(choice) = choose_gang_claim(
        hand,
        &current_melds,
        table,
        position,
        tile,
        claim.from_position,
    ) {
        return Some(choice);
    }

    let current_score = hand_progress_score(hand, &current_melds, table, position);
    if let Some(choice) = choose_peng_claim(
        hand,
        &current_melds,
        table,
        position,
        tile,
        claim.from_position,
        current_score,
        current_ready_score,
    ) {
        return Some(choice);
    }

    if let Some(choice) = choose_chi_claim(
        hand,
        &current_melds,
        table,
        position,
        tile,
        claim.from_position,
        current_ready_score,
    ) {
        return Some(choice);
    }

    Some(AiClaimChoice::Pass)
}

pub(in crate::ai::decision) fn should_pass_hu_for_capped_live_wait(
    hand: &[i32],
    win_hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
) -> bool {
    let Some(max_fan) = table.max_fan.filter(|max_fan| *max_fan > 1) else {
        return false;
    };
    if table.dealer_position == position
        || dealer_opponent_has_major_threat(table, position)
        || table.wall_count < MIN_WALL_TILES_FOR_CAPPED_HU_CHASE
        || hand.len() % 3 != 1
    {
        return false;
    }

    let current_known_unavailable = known_unavailable_tiles_for_claimed_win(table, position, tile);
    let current_fan = estimated_fan_with_known_unavailable_wait_for_table(
        win_hand,
        melds,
        tile,
        table,
        &current_known_unavailable,
    );
    if current_fan != max_fan - 1 {
        return false;
    }
    if current_fan * 2 > max_fan {
        return false;
    }

    let pass_simulated_discards = if claim_tile_already_visible(table, tile) {
        Vec::new()
    } else {
        vec![tile]
    };
    let pass_known_unavailable = known_unavailable_tiles_with_simulated_discards(
        table,
        position,
        melds,
        &pass_simulated_discards,
    );
    let capped_wait_copies = SHENYANG_MAHJONG_TILE_KINDS
        .into_iter()
        .map(|wait_tile| {
            let remaining = remaining_tile_count_with_melds_after_discards(
                hand,
                melds,
                table,
                position,
                wait_tile,
                &pass_simulated_discards,
            );
            if remaining <= 0 {
                return 0;
            }
            let mut next = hand.to_vec();
            next.push(wait_tile);
            next.sort_unstable();
            let reaches_cap = is_complete_win_for_table(&next, melds, table)
                && estimated_fan_with_known_unavailable_wait_for_table(
                    &next,
                    melds,
                    wait_tile,
                    table,
                    &pass_known_unavailable,
                ) >= max_fan;
            if reaches_cap { remaining } else { 0 }
        })
        .sum::<i32>();

    capped_wait_copies >= 3
        && capped_hu_chase_wall_hit_probability(table, position, capped_wait_copies)
            >= CAPPED_HU_CHASE_MIN_WALL_HIT_PROBABILITY
}

pub fn should_pass_self_draw_hu_from_view(
    win_hand: &[i32],
    table: &AiPublicTable,
    position: usize,
    win_tile: i32,
) -> bool {
    let melds = table
        .seats
        .get(&position)
        .map(|seat| seat.melds.as_slice())
        .unwrap_or(&[]);
    if !is_complete_win_for_table(win_hand, melds, table) {
        return false;
    }
    let Some(index) = win_hand.iter().position(|tile| *tile == win_tile) else {
        return false;
    };
    let mut hand_before_win = win_hand.to_vec();
    hand_before_win.remove(index);
    sort_tiles(&mut hand_before_win);

    should_pass_hu_for_capped_live_wait(
        &hand_before_win,
        win_hand,
        melds,
        table,
        position,
        win_tile,
    )
}
