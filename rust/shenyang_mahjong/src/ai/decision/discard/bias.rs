use super::*;

pub(in crate::ai::decision) fn capped_spare_dragon_discard_bias(
    hand: &[i32],
    tile: i32,
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
) -> f64 {
    if !is_dragon(tile)
        || hand.iter().filter(|item| **item == tile).count() != 1
        || !has_triplet_or_dragon_pair(hand, melds)
        || terminal_or_honor_count(hand, melds) <= 1
    {
        return 0.0;
    }
    if table.max_fan.is_some_and(|max_fan| max_fan <= 1) {
        return 5.0;
    }

    let after_discard = remove_n_tiles(hand, tile, 1);
    if after_discard.len() + 1 == hand.len()
        && capped_normal_route_visible_fan_reaches_cap(&after_discard, melds, table)
    {
        return 6.0;
    }
    0.0
}

pub(in crate::ai::decision) fn dragon_value_bias(hand: &[i32], tile: i32) -> f64 {
    if !is_dragon(tile) {
        return 0.0;
    }
    let pairs = pair_count(hand);
    if pairs >= 4 { 10.4 } else { -3.0 }
}

pub(in crate::ai::decision) fn honor_discard_bias(hand: &[i32], tile: i32) -> f64 {
    if is_wind(tile) {
        8.0
    } else if is_dragon(tile) {
        4.8 + dragon_value_bias(hand, tile)
    } else {
        6.0
    }
}

pub(in crate::ai::decision) fn isolated_suited_singleton_discard_bias(tile: i32) -> f64 {
    if !is_suited(tile) {
        return 4.0;
    }
    match tile_rank(tile) {
        2 | 8 => 4.6,
        3 | 7 => 4.25,
        _ => 4.0,
    }
}

pub(in crate::ai::decision) fn pair_discard_bias(hand: &[i32]) -> f64 {
    if pair_count(hand) >= 4 { -4.4 } else { -1.8 }
}
