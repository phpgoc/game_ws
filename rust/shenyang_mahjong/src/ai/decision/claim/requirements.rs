use super::*;

pub(in crate::ai::decision) fn claim_leaves_unrecoverable_basic_requirement(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    kind: ShenyangMahjongMeldKind,
    tile: i32,
    from_position: usize,
) -> bool {
    claim_leaves_unrecoverable_missing_suit(hand, current_melds, table, kind, tile, from_position)
        || claim_leaves_unrecoverable_terminal_or_honor(
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

pub(in crate::ai::decision) fn claim_leaves_unrecoverable_missing_suit(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    kind: ShenyangMahjongMeldKind,
    tile: i32,
    from_position: usize,
) -> bool {
    let (remove_count, claimed_meld) = match kind {
        ShenyangMahjongMeldKind::PENG => (2, claim_peng_meld(tile, from_position)),
        ShenyangMahjongMeldKind::GANG => (3, claim_gang_meld(tile, from_position)),
        ShenyangMahjongMeldKind::CHI | ShenyangMahjongMeldKind::XI_GANG => return false,
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

pub(in crate::ai::decision) fn claim_leaves_unrecoverable_terminal_or_honor(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    kind: ShenyangMahjongMeldKind,
    tile: i32,
    from_position: usize,
) -> bool {
    let (remove_count, claimed_meld) = match kind {
        ShenyangMahjongMeldKind::PENG => (2, claim_peng_meld(tile, from_position)),
        ShenyangMahjongMeldKind::GANG => (3, claim_gang_meld(tile, from_position)),
        ShenyangMahjongMeldKind::CHI | ShenyangMahjongMeldKind::XI_GANG => return false,
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
            || pure_one_suit_plan_score_for_context(
                &after_discard,
                &melds,
                table,
                position,
                win_rule,
            ) > 0.0
    })
}
