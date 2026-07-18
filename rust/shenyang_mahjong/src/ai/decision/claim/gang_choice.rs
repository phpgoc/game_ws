use super::*;

pub(super) fn choose_gang_claim(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    from_position: usize,
) -> Option<AiClaimChoice> {
    if table.wall_count == 0 || !can_gang(hand, tile) {
        return None;
    }
    if pure_one_suit_plan_score_for_context(hand, current_melds, table, position) > 0.0
        && !should_claim_ready_pure_one_suit_gang_from_discard(
            hand,
            current_melds,
            table,
            position,
            tile,
            from_position,
        )
        && !claim_gang_projects_capped_pure_one_suit_fan(
            hand,
            current_melds,
            table,
            position,
            tile,
            from_position,
        )
    {
        return Some(AiClaimChoice::Pass);
    }
    if should_claim_capped_dragon_peng_over_five_pairs(
        hand,
        current_melds,
        table,
        position,
        tile,
        from_position,
    ) {
        return Some(AiClaimChoice::Peng);
    }
    if should_claim_dragon_peng_over_live_five_pairs(
        hand,
        current_melds,
        table,
        position,
        tile,
        from_position,
    ) {
        return Some(AiClaimChoice::Peng);
    }
    if should_preserve_seven_pairs_plan_for_context(hand, current_melds, table, position) {
        return Some(AiClaimChoice::Pass);
    }
    if claim_leaves_unrecoverable_basic_requirement(
        hand,
        current_melds,
        table,
        position,
        ShenyangMahjongMeldKind::GANG,
        tile,
        from_position,
    ) && !should_open_broken_closed_hand_for_defense(hand, current_melds, table, position)
    {
        return Some(AiClaimChoice::Pass);
    }
    if should_peng_to_preserve_four_gui_yi_from_discard(
        hand,
        current_melds,
        table,
        position,
        tile,
        from_position,
    ) {
        return Some(AiClaimChoice::Peng);
    }
    should_claim_gang_from_discard(hand, current_melds, table, position, tile, from_position)
        .then_some(AiClaimChoice::Gang)
}
