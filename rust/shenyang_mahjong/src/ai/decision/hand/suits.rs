use super::*;

pub(in crate::ai::decision) fn missing_suits(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
) -> Vec<i32> {
    suit_presence(hand, melds)
        .into_iter()
        .enumerate()
        .filter_map(|(suit, present)| (!present).then_some(suit as i32))
        .collect()
}

pub(in crate::ai::decision) fn suit_presence(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
) -> [bool; 3] {
    suit_presence_with_extra(hand, melds, None)
}

pub(in crate::ai::decision) fn suit_presence_with_extra(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    extra: Option<i32>,
) -> [bool; 3] {
    let mut suits = [false; 3];
    for tile in hand.iter().copied().chain(extra) {
        if is_suited(tile) {
            suits[tile_suit(tile) as usize] = true;
        }
    }
    for tile in valid_meld_tiles(melds) {
        if is_suited(tile) {
            suits[tile_suit(tile) as usize] = true;
        }
    }
    suits
}

pub(in crate::ai::decision) fn suited_tile_count_for_suit(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    suit: i32,
) -> usize {
    hand.iter()
        .copied()
        .chain(valid_meld_tiles(melds))
        .filter(|tile| is_suited(*tile) && tile_suit(*tile) == suit)
        .count()
}

pub(in crate::ai::decision) fn terminal_or_honor_count(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
) -> usize {
    hand.iter()
        .copied()
        .chain(valid_meld_tiles(melds))
        .filter(|tile| is_honor(*tile) || tile_is_terminal(*tile))
        .count()
}
