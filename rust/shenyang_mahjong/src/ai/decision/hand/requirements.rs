use super::*;

pub(in crate::ai::decision) fn has_terminal_or_honor_with_extra(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    extra: Option<i32>,
) -> bool {
    hand.iter()
        .copied()
        .chain(extra)
        .chain(valid_meld_tiles(melds))
        .any(|tile| is_honor(tile) || tile_is_terminal(tile))
}

pub(in crate::ai::decision) fn has_triplet_like_group(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
) -> bool {
    melds.iter().any(is_triplet_like_meld)
        || unique_tiles(hand)
            .into_iter()
            .filter(|tile| is_valid_tile(*tile))
            .any(|tile| hand.iter().filter(|item| **item == tile).count() >= 3)
}

pub(in crate::ai::decision) fn has_triplet_or_dragon_pair(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
) -> bool {
    has_triplet_or_dragon_pair_with_extra(hand, melds, None)
}

pub(in crate::ai::decision) fn has_triplet_or_dragon_pair_with_extra(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    extra: Option<i32>,
) -> bool {
    let tiles = hand
        .iter()
        .copied()
        .chain(extra)
        .filter(|tile| is_valid_tile(*tile))
        .collect::<Vec<_>>();
    if is_complete_win(&tiles, melds.len()) {
        return melds.iter().any(is_triplet_like_meld)
            || has_triplet_in_standard_decomposition(&tiles)
            || has_dragon_pair_as_standard_pair(&tiles);
    }

    let mut counts = HashMap::<i32, usize>::new();
    for tile in tiles {
        *counts.entry(tile).or_default() += 1;
    }
    melds.iter().any(is_triplet_like_meld)
        || counts.values().any(|count| *count >= 3)
        || [35, 36, 37]
            .into_iter()
            .any(|tile| counts.get(&tile).copied().unwrap_or(0) >= 2)
}
