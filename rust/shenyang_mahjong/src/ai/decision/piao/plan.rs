use super::*;

pub(in crate::ai::decision) fn piao_committed_group_count(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
) -> usize {
    let open_triplets = melds
        .iter()
        .filter(|meld| is_triplet_like_meld(meld))
        .count();
    let mut counts: HashMap<i32, usize> = HashMap::new();
    for &tile in hand.iter().filter(|tile| is_valid_tile(**tile)) {
        *counts.entry(tile).or_default() += 1;
    }
    open_triplets + counts.values().filter(|count| **count >= 3).count()
}

pub(in crate::ai::decision) fn piao_plan_score(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
) -> f64 {
    if melds.iter().any(is_sequence_meld) {
        return 0.0;
    }
    let open_triplets = melds
        .iter()
        .filter(|meld| is_triplet_like_meld(meld))
        .count();
    let mut counts: HashMap<i32, usize> = HashMap::new();
    for &tile in hand.iter().filter(|tile| is_valid_tile(**tile)) {
        *counts.entry(tile).or_default() += 1;
    }
    let triplets = counts.values().filter(|count| **count >= 3).count();
    let pairs = counts.values().filter(|count| **count >= 2).count();
    let score = open_triplets as f64 * 18.0 + triplets as f64 * 14.0 + pairs as f64 * 5.0;
    if open_triplets + triplets >= 2 || pairs >= 3 || (open_triplets >= 1 && pairs >= 2) {
        score
    } else {
        0.0
    }
}

pub(in crate::ai::decision) fn piao_plan_score_for_context(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
) -> f64 {
    let score = piao_plan_score(hand, melds);
    if score <= 0.0
        || piao_plan_is_capped(table)
        || !has_piao_route_basics(hand, melds)
        || capped_open_basic_route_visible_fan_reaches_cap(hand, melds, table)
    {
        return 0.0;
    }
    if table.dealer_position == position && score < 40.0 {
        score * 0.35
    } else {
        score
    }
}

pub(in crate::ai::decision) fn piao_plan_is_capped(table: &AiPublicTable) -> bool {
    table.max_fan.is_some_and(|max_fan| max_fan <= 1)
}

pub(in crate::ai::decision) fn has_piao_route_basics(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
) -> bool {
    missing_suits(hand, melds).is_empty() && has_terminal_or_honor_with_extra(hand, melds, None)
}

pub(in crate::ai::decision) fn piao_threat_level(melds: &[WsShenyangMahjongMeld]) -> usize {
    if melds.iter().any(is_sequence_meld) {
        return 0;
    }
    melds
        .iter()
        .filter(|meld| is_triplet_like_meld(meld))
        .count()
}

pub(in crate::ai::decision) fn piao_missing_suits_from_melds(
    melds: &[WsShenyangMahjongMeld],
) -> Vec<i32> {
    if piao_threat_level(melds) < 3 {
        return Vec::new();
    }
    let mut suits = [false; 3];
    for meld in melds.iter().filter(|meld| is_triplet_like_meld(meld)) {
        if let Some(tile) = meld_primary_tile(meld)
            && is_suited(tile)
        {
            suits[tile_suit(tile) as usize] = true;
        }
    }
    suits
        .into_iter()
        .enumerate()
        .filter_map(|(suit, present)| (!present).then_some(suit as i32))
        .collect()
}

pub(in crate::ai::decision) fn is_closed_early_piao_candidate(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
) -> bool {
    melds.is_empty()
        && pair_count(hand) >= 3
        && table.dealer_position != position
        && !piao_plan_is_capped(table)
        && has_piao_route_basics(hand, melds)
}
