use super::*;

mod defensive_open;
mod gang;
mod options;
mod peng;
mod preserve;
mod requirements;

pub(super) use defensive_open::*;
pub(super) use gang::*;
pub(super) use options::*;
pub(super) use peng::*;
pub(super) use preserve::*;
pub(super) use requirements::*;

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
            if should_claim_ready_piao_peng_for_shou_ba_yi(
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
