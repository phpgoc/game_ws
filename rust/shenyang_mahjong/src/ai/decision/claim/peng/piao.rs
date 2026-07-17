use super::*;

pub(in crate::ai::decision) fn should_claim_peng_for_closed_early_piao_candidate(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    from_position: usize,
) -> bool {
    let pairs = pair_count(hand);
    if valid_meld_count(current_melds) > 0
        || table.dealer_position == position
        || dealer_opponent_has_major_threat(table, position)
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

pub(in crate::ai::decision) fn should_claim_ready_piao_peng_for_shou_ba_yi(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    tile: i32,
    from_position: usize,
    current_ready_score: f64,
) -> bool {
    if current_ready_score <= 0.0
        || table.dealer_position == position
        || dealer_opponent_has_major_threat(table, position)
        || is_late_defense_round(table)
        || piao_plan_is_capped(table)
        || !can_peng(hand, tile)
        || piao_threat_level(current_melds) != 3
        || !has_piao_route_basics(hand, current_melds)
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
    if piao_threat_level(&melds) != 4 {
        return false;
    }

    unique_tiles(&next).into_iter().any(|discard| {
        let after_discard = remove_n_tiles(&next, discard, 1);
        after_discard.len() == 1
            && ready_tile_score_after_discard(
                &after_discard,
                &melds,
                table,
                position,
                win_rule,
                discard,
            ) > 0.0
    })
}
