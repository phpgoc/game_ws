use super::*;

pub(in crate::ai::decision) fn should_claim_peng_for_basic_heng_and_opening(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    from_position: usize,
) -> bool {
    if has_door_opening_meld(current_melds, table)
        || has_triplet_or_dragon_pair(hand, current_melds)
        || !can_peng(hand, tile)
        || !missing_suits(hand, current_melds).is_empty()
        || !has_terminal_or_honor_with_extra(hand, current_melds, Some(tile))
        || should_preserve_seven_pairs_plan_for_context(hand, current_melds, table, position)
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

pub(in crate::ai::decision) fn should_claim_peng_to_open_capped_basic_route(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    from_position: usize,
) -> bool {
    if table.max_fan.is_none_or(|max_fan| max_fan <= 0)
        || has_door_opening_meld(current_melds, table)
        || !can_peng(hand, tile)
        || !missing_suits(hand, current_melds).is_empty()
        || !has_terminal_or_honor_with_extra(hand, current_melds, Some(tile))
        || !has_triplet_or_dragon_pair(hand, current_melds)
        || should_preserve_seven_pairs_plan_for_context(hand, current_melds, table, position)
        || pure_one_suit_plan_score_for_context(hand, current_melds, table, position) > 0.0
        || piao_plan_score_for_context(hand, current_melds, table, position) >= 22.0
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
            && capped_open_normal_route_visible_fan_reaches_cap(&after_discard, &melds, table)
    })
}

pub(in crate::ai::decision) fn should_claim_peng_to_open_mid_basic_hand(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    from_position: usize,
) -> bool {
    if has_door_opening_meld(current_melds, table)
        || !is_mid_opening_round(table)
        || !can_peng(hand, tile)
        || !missing_suits(hand, current_melds).is_empty()
        || !has_terminal_or_honor_with_extra(hand, current_melds, Some(tile))
        || !has_triplet_or_dragon_pair(hand, current_melds)
        || should_preserve_seven_pairs_plan_for_context(hand, current_melds, table, position)
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

pub(in crate::ai::decision) fn should_pass_closed_basic_peng_to_preserve_sequence(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
) -> bool {
    !has_door_opening_meld(current_melds, table)
        && table.dealer_position != position
        && !dealer_opponent_has_major_threat(table, position)
        && table.max_fan.is_none_or(|max_fan| max_fan > 1)
        && !is_late_round(table)
        && can_peng(hand, tile)
        && is_suited(tile)
        && has_triplet_like_group(hand, current_melds)
        && tile_is_middle_of_sequence(hand, tile)
        && piao_plan_score_for_context(hand, current_melds, table, position) < 22.0
        && pure_one_suit_plan_score_for_context(hand, current_melds, table, position) <= 0.0
        && !should_open_broken_closed_hand_for_defense(hand, current_melds, table, position)
}
