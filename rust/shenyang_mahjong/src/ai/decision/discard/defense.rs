use super::*;

pub(in crate::ai::decision) fn choose_late_defense_discard(
    hand: &[i32],
    table: &AiPublicTable,
    position: usize,
) -> Option<i32> {
    choose_late_defense_discard_from_candidates(hand, table, position, unique_tiles(hand))
}

pub(in crate::ai::decision) fn choose_late_defense_discard_preserving_pairs(
    hand: &[i32],
    table: &AiPublicTable,
    position: usize,
) -> Option<i32> {
    let public_candidates = unique_tiles(hand)
        .into_iter()
        .filter(|tile| public_discard_count(table, *tile) > 0)
        .collect::<Vec<_>>();
    if !public_candidates.is_empty() {
        return choose_late_defense_discard_from_candidates(
            hand,
            table,
            position,
            public_candidates,
        );
    }

    let singletons = unique_tiles(hand)
        .into_iter()
        .filter(|tile| hand.iter().filter(|item| **item == *tile).count() == 1)
        .collect::<Vec<_>>();
    if singletons.is_empty() {
        choose_late_defense_discard(hand, table, position)
    } else {
        choose_late_defense_discard_from_candidates(hand, table, position, singletons)
    }
}

pub(in crate::ai::decision) fn choose_late_defense_discard_from_candidates(
    hand: &[i32],
    table: &AiPublicTable,
    position: usize,
    candidates: Vec<i32>,
) -> Option<i32> {
    let mut best: Option<(f64, i32)> = None;
    let public_candidates = candidates
        .iter()
        .copied()
        .filter(|tile| public_discard_count(table, *tile) > 0)
        .collect::<Vec<_>>();
    let candidates = if public_candidates.is_empty() {
        candidates
    } else {
        public_candidates
    };

    for tile in candidates {
        let own_tile_count = hand.iter().filter(|item| **item == tile).count();
        let score = late_defense_tile_safety_score(table, position, tile, own_tile_count);
        match best {
            None => best = Some((score, tile)),
            Some((best_score, best_tile)) => {
                if score > best_score || (score == best_score && tile < best_tile) {
                    best = Some((score, tile));
                }
            }
        }
    }
    best.map(|(_, tile)| tile)
}

pub(in crate::ai::decision) fn choose_broken_hand_public_defense_discard(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> Option<i32> {
    let public_candidates = unique_tiles(hand)
        .into_iter()
        .filter(|tile| public_discard_count(table, *tile) > 0)
        .collect::<Vec<_>>();
    if !public_candidates.is_empty() {
        return choose_public_defense_discard_from_candidates(
            hand,
            melds,
            table,
            position,
            win_rule,
            public_candidates,
        );
    }

    let open_meld_candidates = unique_tiles(hand)
        .into_iter()
        .filter(|tile| mid_round_open_meld_safety_bias(table, *tile) > 0.0)
        .collect::<Vec<_>>();
    if !open_meld_candidates.is_empty() {
        return choose_public_defense_discard_from_candidates(
            hand,
            melds,
            table,
            position,
            win_rule,
            open_meld_candidates,
        );
    }

    let missing_suit_candidates = unique_tiles(hand)
        .into_iter()
        .filter(|tile| mid_broken_opponent_missing_suit_safety_bias(table, position, *tile) > 0.0)
        .collect::<Vec<_>>();
    choose_public_defense_discard_from_candidates(
        hand,
        melds,
        table,
        position,
        win_rule,
        missing_suit_candidates,
    )
}

pub(in crate::ai::decision) fn choose_public_defense_discard_from_candidates(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    candidates: Vec<i32>,
) -> Option<i32> {
    let mut best: Option<(f64, i32)> = None;
    for tile in candidates {
        let own_tile_count = hand.iter().filter(|item| **item == tile).count();
        let score = public_defense_tile_safety_score(table, position, tile, own_tile_count)
            + basic_heng_recovery_public_defense_bias(hand, melds, table, tile, win_rule);
        match best {
            None => best = Some((score, tile)),
            Some((best_score, best_tile)) => {
                if score > best_score || (score == best_score && tile < best_tile) {
                    best = Some((score, tile));
                }
            }
        }
    }
    best.map(|(_, tile)| tile)
}
