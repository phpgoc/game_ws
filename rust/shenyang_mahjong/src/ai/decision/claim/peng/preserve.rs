use super::*;

pub(in crate::ai::decision) fn should_peng_to_preserve_four_gui_yi_from_discard(
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
        || dealer_opponent_has_major_threat(table, position, win_rule)
        || pure_one_suit_plan_score_for_context(hand, current_melds, table, position, win_rule)
            > 0.0
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
    if let Some(max_fan) = table.max_fan.filter(|max_fan| *max_fan > 0)
        && ready_hand_visible_fan_reaches_cap(
            &gang_hand,
            &gang_melds,
            table,
            position,
            win_rule,
            max_fan,
        )
    {
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
            && ready_tile_score_after_discard(
                &after_discard,
                &peng_melds,
                table,
                position,
                win_rule,
                discard,
            ) >= gang_ready_score
    })
}
