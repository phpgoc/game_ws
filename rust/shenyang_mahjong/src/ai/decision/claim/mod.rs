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
