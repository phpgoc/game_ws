use super::*;

pub(super) fn choose_peng_claim(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    tile: i32,
    from_position: usize,
    current_score: f64,
    current_ready_score: f64,
) -> Option<AiClaimChoice> {
    if !can_peng(hand, tile) {
        return None;
    }
    if pure_one_suit_plan_score_for_context(hand, current_melds, table, position) > 0.0 {
        if should_claim_ready_open_pure_one_suit_peng_from_discard(
            hand,
            current_melds,
            table,
            position,
            win_rule,
            tile,
            from_position,
            current_ready_score,
        ) {
            return Some(AiClaimChoice::Peng);
        }
        return Some(AiClaimChoice::Pass);
    }
    if should_preserve_seven_pairs_plan_for_context(hand, current_melds, table, position, win_rule)
    {
        return Some(AiClaimChoice::Pass);
    }
    if claim_leaves_unrecoverable_basic_requirement(
        hand,
        current_melds,
        table,
        position,
        win_rule,
        ShenyangMahjongMeldKind::PENG,
        tile,
        from_position,
    ) && !should_open_broken_closed_hand_for_defense(
        hand,
        current_melds,
        table,
        position,
        win_rule,
    ) {
        return Some(AiClaimChoice::Pass);
    }
    if should_pass_peng_for_relaxed_pure_defense(
        hand,
        current_melds,
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
            current_melds,
            table,
            position,
            win_rule,
            tile,
            from_position,
            current_ready_score,
        ) {
            return Some(AiClaimChoice::Peng);
        }
        if should_claim_ready_dragon_peng_from_discard(
            hand,
            current_melds,
            table,
            position,
            win_rule,
            tile,
            from_position,
            current_ready_score,
        ) {
            return Some(AiClaimChoice::Peng);
        }
        return Some(AiClaimChoice::Pass);
    }
    if should_claim_peng_for_closed_early_piao_candidate(
        hand,
        current_melds,
        table,
        position,
        tile,
        from_position,
    ) {
        return Some(AiClaimChoice::Peng);
    }
    if is_dragon(tile) {
        return Some(AiClaimChoice::Peng);
    }
    if should_claim_peng_for_basic_heng_and_opening(
        hand,
        current_melds,
        table,
        position,
        win_rule,
        tile,
        from_position,
    ) {
        return Some(AiClaimChoice::Peng);
    }
    if should_pass_closed_basic_peng_to_preserve_sequence(
        hand,
        current_melds,
        table,
        position,
        win_rule,
        tile,
    ) {
        return Some(AiClaimChoice::Pass);
    }
    if should_claim_peng_to_open_mid_basic_hand(
        hand,
        current_melds,
        table,
        position,
        win_rule,
        tile,
        from_position,
    ) {
        return Some(AiClaimChoice::Peng);
    }
    if win_rule == WIN_RULE_SHENYANG_BASIC && !has_open_meld(current_melds) && can_gang(hand, tile)
    {
        return Some(AiClaimChoice::Peng);
    }
    if win_rule == WIN_RULE_SHENYANG_BASIC && table.dealer_position == position {
        return Some(AiClaimChoice::Peng);
    }
    if piao_plan_score_for_context(hand, current_melds, table, position) >= 32.0 {
        return Some(AiClaimChoice::Peng);
    }

    let mut next = remove_n_tiles(hand, tile, 2);
    let mut melds = current_melds.to_vec();
    melds.push(claim_peng_meld(tile, from_position));
    sort_tiles(&mut next);
    let after = best_score_after_forced_discard(&next, &melds, table, position, win_rule);
    if should_open_broken_closed_hand_for_defense(hand, current_melds, table, position, win_rule) {
        return Some(AiClaimChoice::Peng);
    }
    if should_preserve_pinghu_sequence_over_peng(
        hand,
        current_melds,
        table,
        position,
        win_rule,
        tile,
    ) {
        return Some(AiClaimChoice::Pass);
    }
    let required_gain = required_peng_gain(hand, current_melds, table, position, win_rule, tile);
    (after >= current_score + required_gain).then_some(AiClaimChoice::Peng)
}

fn required_peng_gain(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    tile: i32,
) -> f64 {
    let mut required_gain = if is_honor(tile) || tile_is_terminal(tile) {
        6.0
    } else {
        10.0
    };
    if is_suited(tile) && neighbor_count(hand, tile) >= 2 {
        required_gain += 8.0;
    }
    let missing_suits = missing_suits(hand, current_melds);
    if is_suited(tile) && missing_suits.contains(&tile_suit(tile)) {
        required_gain -= 5.0;
    }
    if win_rule == WIN_RULE_SHENYANG_BASIC && !has_open_meld(current_melds) {
        required_gain -= 4.0;
    }
    if win_rule == WIN_RULE_SHENYANG_BASIC && !has_triplet_or_dragon_pair(hand, current_melds) {
        required_gain -= 3.0;
    }
    if piao_plan_score_for_context(hand, current_melds, table, position) >= 22.0 {
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
    required_gain
}
