use super::super::*;

fn can_draw_required_tiles(
    current_count: usize,
    required_count: usize,
    remaining_count: usize,
    wall_count: usize,
) -> bool {
    if current_count >= required_count {
        return false;
    }
    let needed = required_count - current_count;
    remaining_count >= needed && wall_count >= needed
}

pub(in crate::ai::decision) fn can_recover_basic_heng(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
) -> bool {
    if has_triplet_or_dragon_pair(hand, melds) {
        return true;
    }

    let mut counts = HashMap::<i32, usize>::new();
    for tile in hand.iter().copied() {
        *counts.entry(tile).or_default() += 1;
    }

    SHENYANG_MAHJONG_TILE_KINDS.into_iter().any(|tile| {
        let count = counts.get(&tile).copied().unwrap_or(0);
        let remaining =
            remaining_tile_count_with_melds_after_discards(hand, melds, table, position, tile, &[])
                as usize;
        let can_draw_triplet = can_draw_required_tiles(count, 3, remaining, table.wall_count);
        let can_draw_dragon_pair =
            is_dragon(tile) && can_draw_required_tiles(count, 2, remaining, table.wall_count);
        can_draw_triplet || can_draw_dragon_pair
    })
}

pub(in crate::ai::decision) fn can_recover_basic_heng_after_discard(
    hand_after_discard: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    discarded_tile: i32,
) -> bool {
    if has_triplet_or_dragon_pair(hand_after_discard, melds) {
        return true;
    }

    let mut counts = HashMap::<i32, usize>::new();
    for tile in hand_after_discard.iter().copied() {
        *counts.entry(tile).or_default() += 1;
    }

    SHENYANG_MAHJONG_TILE_KINDS.into_iter().any(|tile| {
        let count = counts.get(&tile).copied().unwrap_or(0);
        let remaining = remaining_tile_count_with_melds_after_discards(
            hand_after_discard,
            melds,
            table,
            position,
            tile,
            &[discarded_tile],
        ) as usize;
        let can_draw_triplet = can_draw_required_tiles(count, 3, remaining, table.wall_count);
        let can_draw_dragon_pair =
            is_dragon(tile) && can_draw_required_tiles(count, 2, remaining, table.wall_count);
        can_draw_triplet || can_draw_dragon_pair
    })
}

pub(in crate::ai::decision) fn loses_basic_heng_recovery_after_discard(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    win_rule: i32,
) -> bool {
    if win_rule != WIN_RULE_SHENYANG_BASIC
        || has_triplet_or_dragon_pair(hand, melds)
        || !can_recover_basic_heng(hand, melds, table, position)
    {
        return false;
    }

    let hand_after_discard = remove_n_tiles(hand, tile, 1);
    hand_after_discard.len() + 1 == hand.len()
        && !can_recover_basic_heng_after_discard(&hand_after_discard, melds, table, position, tile)
}
