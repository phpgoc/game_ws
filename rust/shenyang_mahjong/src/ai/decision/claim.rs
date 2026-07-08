use super::*;

pub(super) fn chi_options(hand: &[i32], tile: i32) -> Vec<Vec<i32>> {
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

pub(super) fn claim_leaves_unrecoverable_basic_requirement(
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

pub(super) fn claim_leaves_unrecoverable_missing_suit(
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

pub(super) fn claim_leaves_unrecoverable_terminal_or_honor(
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

pub(super) fn should_claim_chi_to_open_broken_hand_for_defense(
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

pub(super) fn should_claim_gang_from_discard(
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

pub(super) fn should_claim_opening_gang_for_basic_hand(
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

pub(super) fn should_claim_peng_for_basic_heng_and_opening(
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

pub(super) fn should_claim_peng_to_open_mid_basic_hand(
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

pub(super) fn should_pass_closed_basic_peng_to_preserve_sequence(
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

pub(super) fn should_claim_peng_for_closed_early_piao_candidate(
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

pub(super) fn should_peng_to_preserve_four_gui_yi_from_discard(
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

pub(super) fn claim_gang_from_discard_reaches_ready(
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

pub(super) fn should_claim_ready_pure_one_suit_gang_from_discard(
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

pub(super) fn should_claim_ready_open_pure_one_suit_peng_from_discard(
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

pub(super) fn should_claim_ready_dragon_peng_from_discard(
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

pub(super) fn should_pass_peng_for_relaxed_pure_defense(
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

pub(super) fn should_preserve_piao_plan_for_chi(
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

pub(super) fn should_preserve_pinghu_sequence_over_peng(
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
