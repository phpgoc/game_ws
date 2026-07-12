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
pub(super) use preserve::*;
pub(super) use requirements::*;

use chi_choice::choose_chi_claim;
use gang_choice::choose_gang_claim;
use peng_choice::choose_peng_claim;

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
    if is_complete_win_for_table(&win_hand, melds, table, win_rule) {
        if should_pass_hu_for_capped_live_wait(
            hand, &win_hand, melds, table, position, win_rule, tile,
        ) {
            return Some(AiClaimChoice::Pass);
        }
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
    if current_ready_score > 0.0
        && ready_visible_fan_exceeds_half_cap(hand, &current_melds, table, position, win_rule)
    {
        return Some(AiClaimChoice::Pass);
    }

    if let Some(choice) = choose_gang_claim(
        hand,
        &current_melds,
        table,
        position,
        win_rule,
        tile,
        claim.from_position,
    ) {
        return Some(choice);
    }

    let current_score = hand_progress_score(hand, &current_melds, table, position, win_rule);
    if let Some(choice) = choose_peng_claim(
        hand,
        &current_melds,
        table,
        position,
        win_rule,
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
        win_rule,
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
    win_rule: i32,
    tile: i32,
) -> bool {
    let Some(max_fan) = table.max_fan.filter(|max_fan| *max_fan > 1) else {
        return false;
    };
    if table.dealer_position == position || is_late_defense_round(table) || hand.len() % 3 != 1 {
        return false;
    }

    let current_known_unavailable =
        known_unavailable_tiles_with_simulated_discards(table, position, melds, &[]);
    let current_fan = estimated_fan_with_known_unavailable_wait_and_open_rule(
        win_hand,
        melds,
        tile,
        win_rule,
        table.chi_opens_door,
        &current_known_unavailable,
    );
    if current_fan != max_fan - 1 {
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
            let reaches_cap = is_complete_win_for_table(&next, melds, table, win_rule)
                && estimated_fan_with_known_unavailable_wait_and_open_rule(
                    &next,
                    melds,
                    wait_tile,
                    win_rule,
                    table.chi_opens_door,
                    &pass_known_unavailable,
                ) >= max_fan;
            if reaches_cap { remaining } else { 0 }
        })
        .sum::<i32>();

    capped_wait_copies >= 3
}

pub fn should_pass_self_draw_hu_from_view(
    win_hand: &[i32],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    win_tile: i32,
) -> bool {
    let melds = table
        .seats
        .get(&position)
        .map(|seat| seat.melds.as_slice())
        .unwrap_or(&[]);
    if !is_complete_win_for_table(win_hand, melds, table, win_rule) {
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
        win_rule,
        win_tile,
    )
}
