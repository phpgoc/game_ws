use std::collections::HashMap;

use share_type_public::games::shenyang_mahjong::{
    SHENYANG_MAHJONG_TILE_KINDS, ShenyangMahjongMeldKind, ShenyangMahjongWinPattern,
    WsShenyangMahjongMeld,
};

pub const XI_GANG_DRAGONS: [i32; 3] = [35, 36, 37];
pub const XI_GANG_WINDS: [i32; 4] = [31, 32, 33, 34];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShenyangMahjongWinContext {
    allow_first_chi: bool,
}

fn all_tiles_with_melds(tiles: &[i32], melds: &[WsShenyangMahjongMeld]) -> Vec<i32> {
    let mut all_tiles = tiles.to_vec();
    for meld in melds {
        all_tiles.extend(meld.tiles.iter().copied());
    }
    all_tiles
}

pub fn can_chi(hand: &[i32], target_tile: i32, consume_tiles: &[i32]) -> bool {
    if consume_tiles.len() != 2 || !is_suited_tile(target_tile) {
        return false;
    }
    if !has_valid_tile_multiplicity(hand)
        || hand.iter().filter(|&&tile| tile == target_tile).count() >= 4
    {
        return false;
    }
    if !tiles_in_hand(hand, consume_tiles) {
        return false;
    }
    let mut sequence = vec![target_tile, consume_tiles[0], consume_tiles[1]];
    sequence.sort_unstable();
    is_valid_sequence(&sequence)
}

pub fn can_concealed_gang(hand: &[i32], target_tile: i32) -> bool {
    if !is_valid_tile(target_tile) || !has_valid_tile_multiplicity(hand) {
        return false;
    }
    hand.iter().filter(|&&tile| tile == target_tile).count() == 4
}

fn can_form_sequences(counts: &mut [u8; 38]) -> bool {
    let Some(tile) = SHENYANG_MAHJONG_TILE_KINDS
        .into_iter()
        .find(|tile| counts[*tile as usize] > 0)
    else {
        return true;
    };
    if !is_suited_tile(tile) {
        return false;
    }
    let tile2 = tile + 1;
    let tile3 = tile + 2;
    if !same_suit(tile, tile2)
        || !same_suit(tile, tile3)
        || counts[tile2 as usize] == 0
        || counts[tile3 as usize] == 0
    {
        return false;
    }
    counts[tile as usize] -= 1;
    counts[tile2 as usize] -= 1;
    counts[tile3 as usize] -= 1;
    let complete = can_form_sequences(counts);
    counts[tile as usize] += 1;
    counts[tile2 as usize] += 1;
    counts[tile3 as usize] += 1;
    complete
}

fn can_form_sequences_with_dragon_pair(tiles: &[i32]) -> bool {
    if tiles.len() % 3 != 2 || !has_valid_tile_multiplicity(tiles) {
        return false;
    }
    dragon_pair_tiles().into_iter().any(|pair_tile| {
        let mut counts = tile_counts(tiles);
        let pair_index = pair_tile as usize;
        if counts[pair_index] < 2 {
            return false;
        }
        counts[pair_index] -= 2;
        can_form_sequences(&mut counts)
    })
}

fn can_form_sets(counts: &mut [u8; 38]) -> bool {
    let Some(tile) = SHENYANG_MAHJONG_TILE_KINDS
        .into_iter()
        .find(|tile| counts[*tile as usize] > 0)
    else {
        return true;
    };
    let index = tile as usize;

    if counts[index] >= 3 {
        counts[index] -= 3;
        if can_form_sets(counts) {
            counts[index] += 3;
            return true;
        }
        counts[index] += 3;
    }

    if is_suited_tile(tile) {
        let tile2 = tile + 1;
        let tile3 = tile + 2;
        if same_suit(tile, tile2)
            && same_suit(tile, tile3)
            && counts[tile2 as usize] > 0
            && counts[tile3 as usize] > 0
        {
            counts[index] -= 1;
            counts[tile2 as usize] -= 1;
            counts[tile3 as usize] -= 1;
            if can_form_sets(counts) {
                counts[index] += 1;
                counts[tile2 as usize] += 1;
                counts[tile3 as usize] += 1;
                return true;
            }
            counts[index] += 1;
            counts[tile2 as usize] += 1;
            counts[tile3 as usize] += 1;
        }
    }

    false
}

fn can_form_sets_with_one_pair(counts: &[u8; 38]) -> bool {
    for tile in SHENYANG_MAHJONG_TILE_KINDS {
        let index = tile as usize;
        if counts[index] < 2 {
            continue;
        }
        let mut working = *counts;
        working[index] -= 2;
        if can_form_sets(&mut working) {
            return true;
        }
    }
    false
}

fn can_form_sets_with_triplet(counts: &mut [u8; 38], has_triplet: bool) -> bool {
    let Some(tile) = SHENYANG_MAHJONG_TILE_KINDS
        .into_iter()
        .find(|tile| counts[*tile as usize] > 0)
    else {
        return has_triplet;
    };
    let index = tile as usize;

    if counts[index] >= 3 {
        counts[index] -= 3;
        if can_form_sets_with_triplet(counts, true) {
            counts[index] += 3;
            return true;
        }
        counts[index] += 3;
    }

    if is_suited_tile(tile) {
        let tile2 = tile + 1;
        let tile3 = tile + 2;
        if same_suit(tile, tile2)
            && same_suit(tile, tile3)
            && counts[tile2 as usize] > 0
            && counts[tile3 as usize] > 0
        {
            counts[index] -= 1;
            counts[tile2 as usize] -= 1;
            counts[tile3 as usize] -= 1;
            if can_form_sets_with_triplet(counts, has_triplet) {
                counts[index] += 1;
                counts[tile2 as usize] += 1;
                counts[tile3 as usize] += 1;
                return true;
            }
            counts[index] += 1;
            counts[tile2 as usize] += 1;
            counts[tile3 as usize] += 1;
        }
    }

    false
}

fn can_form_triplets_with_dragon_pair(tiles: &[i32]) -> bool {
    if tiles.len() % 3 != 2 {
        return false;
    }
    let counts = tile_counts(tiles);
    dragon_pair_tiles().into_iter().any(|pair_tile| {
        counts[pair_tile as usize] >= 2
            && SHENYANG_MAHJONG_TILE_KINDS.into_iter().all(|tile| {
                let mut count = counts[tile as usize];
                if tile == pair_tile {
                    count -= 2;
                }
                count % 3 == 0
            })
    })
}

fn can_form_triplets_with_pair(tiles: &[i32]) -> bool {
    if tiles.len() % 3 != 2 {
        return false;
    }
    let counts = tile_counts(tiles);
    SHENYANG_MAHJONG_TILE_KINDS.into_iter().any(|pair_tile| {
        let pair_count = counts[pair_tile as usize];
        pair_count >= 2
            && SHENYANG_MAHJONG_TILE_KINDS.into_iter().all(|tile| {
                let mut count = counts[tile as usize];
                if tile == pair_tile {
                    count -= 2;
                }
                count % 3 == 0
            })
    })
}

pub fn can_gang(hand: &[i32], target_tile: i32) -> bool {
    if !is_valid_tile(target_tile) || !has_valid_tile_multiplicity(hand) {
        return false;
    }
    hand.iter().filter(|&&tile| tile == target_tile).count() == 3
}

pub fn can_peng(hand: &[i32], target_tile: i32) -> bool {
    if !is_valid_tile(target_tile) || !has_valid_tile_multiplicity(hand) {
        return false;
    }
    matches!(
        hand.iter().filter(|&&tile| tile == target_tile).count(),
        2 | 3
    )
}

fn counts_after_removing(tiles: &[i32], remove_tiles: &[i32]) -> Option<[u8; 38]> {
    let mut counts = tile_counts(tiles);
    for tile in remove_tiles {
        let index = *tile as usize;
        if counts[index] == 0 {
            return None;
        }
        counts[index] -= 1;
    }
    Some(counts)
}

fn dragon_pair_tiles() -> [i32; 3] {
    [35, 36, 37]
}

fn has_available_wait_copy(
    base_tiles: &[i32],
    melds: &[WsShenyangMahjongMeld],
    known_unavailable_counts: &[u8; 38],
    tile: i32,
) -> bool {
    let base_count = base_tiles.iter().filter(|item| **item == tile).count();
    let meld_count = melds
        .iter()
        .filter(|meld| is_valid_meld(meld))
        .flat_map(|meld| meld.tiles.iter())
        .filter(|item| **item == tile)
        .count();
    let known_count = known_unavailable_counts[tile as usize] as usize;
    base_count + meld_count + known_count < 4
}

pub(crate) fn has_dragon_pair_as_standard_pair(tiles: &[i32]) -> bool {
    if !has_valid_tile_multiplicity(tiles) {
        return false;
    }
    for pair_tile in dragon_pair_tiles() {
        let mut counts = tile_counts(tiles);
        let index = pair_tile as usize;
        if counts[index] < 2 {
            continue;
        }
        counts[index] -= 2;
        if can_form_sets(&mut counts) {
            return true;
        }
    }
    false
}

pub(crate) fn has_edge_wait_decomposition(tiles: &[i32], win_tile: i32) -> bool {
    if !is_suited_tile(win_tile) {
        return false;
    }
    let rank = win_tile % 10;
    let sequence = match rank {
        3 => [win_tile - 2, win_tile - 1, win_tile],
        7 => [win_tile, win_tile + 1, win_tile + 2],
        _ => return false,
    };
    counts_after_removing(tiles, &sequence)
        .is_some_and(|counts| can_form_sets_with_one_pair(&counts))
}

fn has_open_meld(melds: &[WsShenyangMahjongMeld]) -> bool {
    melds.iter().any(is_door_opening_meld)
}

fn has_terminal_or_honor(tiles: &[i32]) -> bool {
    tiles.iter().copied().any(|tile| {
        matches!(
            tile,
            1 | 9 | 11 | 19 | 21 | 29 | 31 | 32 | 33 | 34 | 35 | 36 | 37
        )
    })
}

fn has_three_suits(tiles: &[i32]) -> bool {
    let mut suits = [false; 3];
    for tile in tiles.iter().copied() {
        match tile {
            1..=9 => suits[0] = true,
            11..=19 => suits[1] = true,
            21..=29 => suits[2] = true,
            _ => {}
        }
    }
    suits.into_iter().all(|present| present)
}

pub(crate) fn has_triplet_in_standard_decomposition(tiles: &[i32]) -> bool {
    if tiles.len() % 3 != 2 {
        return false;
    }
    if !has_valid_tile_multiplicity(tiles) {
        return false;
    }
    let mut counts = [0u8; 38];
    for &tile in tiles {
        if !is_valid_tile(tile) {
            return false;
        }
        counts[tile as usize] += 1;
    }

    for tile in SHENYANG_MAHJONG_TILE_KINDS {
        let index = tile as usize;
        if counts[index] < 2 {
            continue;
        }
        counts[index] -= 2;
        if can_form_sets_with_triplet(&mut counts, false) {
            return true;
        }
        counts[index] += 2;
    }
    false
}

fn has_valid_tile_multiplicity(tiles: &[i32]) -> bool {
    let mut counts = [0u8; 38];
    for &tile in tiles {
        if !is_valid_tile(tile) {
            return false;
        }
        counts[tile as usize] += 1;
        if counts[tile as usize] > 4 {
            return false;
        }
    }
    true
}

fn is_closed_middle_wait(tiles: &[i32], win_tile: i32) -> bool {
    is_suited_tile(win_tile)
        && same_suit(win_tile - 1, win_tile)
        && same_suit(win_tile + 1, win_tile)
        && counts_after_removing(tiles, &[win_tile - 1, win_tile, win_tile + 1])
            .is_some_and(|counts| can_form_sets_with_one_pair(&counts))
}

pub fn is_complete_win(tiles: &[i32], meld_count: usize) -> bool {
    if !has_valid_tile_multiplicity(tiles) {
        return false;
    }
    if meld_count == 0 {
        return tiles.len() == 14 && is_win(tiles);
    }
    if tiles.len() + meld_count * 3 != 14 {
        return false;
    }
    is_standard_win(tiles)
}

#[cfg(test)]
pub fn is_complete_win_with_melds(tiles: &[i32], melds: &[WsShenyangMahjongMeld]) -> bool {
    is_complete_win_with_melds_with_context(tiles, melds, ShenyangMahjongWinContext::new())
}

fn is_complete_shape_with_melds(tiles: &[i32], melds: &[WsShenyangMahjongMeld]) -> bool {
    melds.iter().all(is_valid_meld)
        && is_complete_win(tiles, melds.len())
        && has_valid_tile_multiplicity(&all_tiles_with_melds(tiles, melds))
}

pub fn is_complete_win_with_melds_with_context(
    tiles: &[i32],
    melds: &[WsShenyangMahjongMeld],
    context: ShenyangMahjongWinContext,
) -> bool {
    if !is_complete_shape_with_melds(tiles, melds) {
        return false;
    }
    satisfies_shenyang_win_with_context(tiles, melds, context)
}

pub(crate) fn is_door_opening_meld(meld: &WsShenyangMahjongMeld) -> bool {
    meld.from_position.is_some() && (is_triplet_meld(meld) || is_sequence_meld(meld))
}

fn is_dragon_tile(tile: i32) -> bool {
    matches!(tile, 35..=37)
}

fn is_heng_meld(meld: &WsShenyangMahjongMeld) -> bool {
    is_triplet_meld(meld) || is_xi_gang_meld(meld)
}

fn is_honor_tile(tile: i32) -> bool {
    matches!(tile, 31..=37)
}

fn is_pair_single_wait(tiles: &[i32], win_tile: i32) -> bool {
    if tiles.iter().filter(|tile| **tile == win_tile).count() != 2 {
        return false;
    }
    let Some(mut counts) = counts_after_removing(tiles, &[win_tile, win_tile]) else {
        return false;
    };
    can_form_sets(&mut counts)
}

pub fn is_piao_hu_win(tiles: &[i32], melds: &[WsShenyangMahjongMeld]) -> bool {
    if !is_complete_win(tiles, melds.len()) || melds.iter().any(|meld| !is_heng_meld(meld)) {
        return false;
    }
    let all_tiles = all_tiles_with_melds(tiles, melds);
    if !has_valid_tile_multiplicity(&all_tiles) {
        return false;
    }
    let has_required_opening = has_open_meld(melds) || can_form_triplets_with_dragon_pair(tiles);
    has_required_opening
        && has_three_suits(&all_tiles)
        && has_terminal_or_honor(&all_tiles)
        && can_form_triplets_with_pair(tiles)
}

fn is_pure_one_suit_tiles(tiles: &[i32]) -> bool {
    let mut suit = None;
    for tile in tiles.iter().copied() {
        if !is_suited_tile(tile) {
            return false;
        }
        let tile_suit = tile / 10;
        match suit {
            Some(suit) if suit != tile_suit => return false,
            None => suit = Some(tile_suit),
            _ => {}
        }
    }
    suit.is_some()
}

pub fn is_pure_one_suit_win(tiles: &[i32], melds: &[WsShenyangMahjongMeld]) -> bool {
    if !melds.iter().all(is_valid_meld) || !is_complete_win(tiles, melds.len()) {
        return false;
    }
    let all_tiles = all_tiles_with_melds(tiles, melds);
    has_valid_tile_multiplicity(&all_tiles) && is_pure_one_suit_tiles(&all_tiles)
}

fn is_sequence_meld(meld: &WsShenyangMahjongMeld) -> bool {
    if meld.kind != ShenyangMahjongMeldKind::CHI || meld.tiles.len() != 3 {
        return false;
    }
    let mut tiles = meld.tiles.clone();
    tiles.sort_unstable();
    is_valid_sequence(&tiles)
}

fn is_seven_pairs_single_wait(tiles: &[i32], win_tile: i32) -> bool {
    if !is_seven_pairs_win(tiles) {
        return false;
    }
    matches!(
        tiles.iter().filter(|tile| **tile == win_tile).count(),
        2 | 4
    )
}

pub fn is_seven_pairs_win(tiles: &[i32]) -> bool {
    if tiles.len() != 14 {
        return false;
    }
    let mut counts = [0u8; 38];
    for &tile in tiles {
        if !is_valid_tile(tile) {
            return false;
        }
        counts[tile as usize] += 1;
    }

    SHENYANG_MAHJONG_TILE_KINDS.into_iter().all(|tile| {
        let count = counts[tile as usize];
        count == 0 || count == 2 || count == 4
    })
}

#[cfg(test)]
pub fn is_single_wait_shape(tiles: &[i32], melds: &[WsShenyangMahjongMeld], win_tile: i32) -> bool {
    if !is_complete_shape_with_melds(tiles, melds)
        || !tiles.contains(&win_tile)
        || !is_unique_complete_wait(tiles, melds, win_tile)
    {
        return false;
    }
    is_seven_pairs_single_wait(tiles, win_tile)
        || is_pair_single_wait(tiles, win_tile)
        || is_closed_middle_wait(tiles, win_tile)
        || has_edge_wait_decomposition(tiles, win_tile)
        || is_terminal_tile(win_tile)
        || is_honor_tile(win_tile)
}

#[cfg(test)]
pub fn is_single_wait_shape_with_known_unavailable_tiles(
    tiles: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_tile: i32,
    known_unavailable_tiles: &[i32],
) -> bool {
    is_single_wait_shape_with_known_unavailable_tiles_with_context(
        tiles,
        melds,
        win_tile,
        ShenyangMahjongWinContext::new(),
        known_unavailable_tiles,
    )
}

pub fn is_single_wait_shape_with_known_unavailable_tiles_with_context(
    tiles: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_tile: i32,
    context: ShenyangMahjongWinContext,
    known_unavailable_tiles: &[i32],
) -> bool {
    if !is_complete_win_with_melds_with_context(tiles, melds, context) || !tiles.contains(&win_tile)
    {
        return false;
    }
    if !is_unique_complete_wait_with_known_unavailable_tiles(
        tiles,
        melds,
        win_tile,
        context,
        known_unavailable_tiles,
    ) {
        return false;
    }
    is_seven_pairs_single_wait(tiles, win_tile)
        || is_pair_single_wait(tiles, win_tile)
        || is_closed_middle_wait(tiles, win_tile)
        || has_edge_wait_decomposition(tiles, win_tile)
        || is_terminal_tile(win_tile)
        || is_honor_tile(win_tile)
}

#[cfg(test)]
pub fn is_legal_single_wait_shape(
    tiles: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_tile: i32,
) -> bool {
    is_single_wait_shape_with_known_unavailable_tiles(tiles, melds, win_tile, &[])
}

pub fn is_standard_win(tiles: &[i32]) -> bool {
    if tiles.len() % 3 != 2 {
        return false;
    }
    if !has_valid_tile_multiplicity(tiles) {
        return false;
    }
    let mut counts = [0u8; 38];
    for &tile in tiles {
        if !is_valid_tile(tile) {
            return false;
        }
        counts[tile as usize] += 1;
    }

    for tile in SHENYANG_MAHJONG_TILE_KINDS {
        let index = tile as usize;
        if counts[index] < 2 {
            continue;
        }
        counts[index] -= 2;
        if can_form_sets(&mut counts) {
            return true;
        }
        counts[index] += 2;
    }
    false
}

fn is_suited_tile(tile: i32) -> bool {
    matches!(tile, 1..=9 | 11..=19 | 21..=29)
}

fn is_terminal_tile(tile: i32) -> bool {
    matches!(tile, 1 | 9 | 11 | 19 | 21 | 29)
}

fn is_triplet_meld(meld: &WsShenyangMahjongMeld) -> bool {
    let expected_len = match meld.kind {
        ShenyangMahjongMeldKind::PENG => 3,
        ShenyangMahjongMeldKind::GANG => 4,
        ShenyangMahjongMeldKind::CHI | ShenyangMahjongMeldKind::XI_GANG => return false,
    };
    meld.tiles.len() == expected_len
        && is_valid_tile(meld.tiles[0])
        && meld.tiles.iter().all(|tile| *tile == meld.tiles[0])
}

#[cfg(test)]
fn is_unique_complete_wait(tiles: &[i32], melds: &[WsShenyangMahjongMeld], win_tile: i32) -> bool {
    let Some(index) = tiles.iter().position(|tile| *tile == win_tile) else {
        return false;
    };
    let mut base = tiles.to_vec();
    base.remove(index);
    let waits = SHENYANG_MAHJONG_TILE_KINDS
        .into_iter()
        .filter(|tile| base.iter().filter(|item| *item == tile).count() < 4)
        .filter(|tile| {
            let mut test = base.clone();
            test.push(*tile);
            test.sort_unstable();
            is_complete_shape_with_melds(&test, melds)
        })
        .collect::<Vec<_>>();
    waits.len() == 1 && waits[0] == win_tile
}

fn is_unique_complete_wait_with_known_unavailable_tiles(
    tiles: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_tile: i32,
    context: ShenyangMahjongWinContext,
    known_unavailable_tiles: &[i32],
) -> bool {
    let Some(index) = tiles.iter().position(|tile| *tile == win_tile) else {
        return false;
    };
    let mut base = tiles.to_vec();
    base.remove(index);
    let known_counts = tile_counts(known_unavailable_tiles);
    let waits = SHENYANG_MAHJONG_TILE_KINDS
        .into_iter()
        .filter(|tile| has_available_wait_copy(&base, melds, &known_counts, *tile))
        .filter(|tile| {
            let mut test = base.clone();
            test.push(*tile);
            test.sort_unstable();
            is_complete_win_with_melds_with_context(&test, melds, context)
        })
        .collect::<Vec<_>>();
    waits.len() == 1 && waits[0] == win_tile
}

pub(crate) fn is_valid_meld(meld: &WsShenyangMahjongMeld) -> bool {
    is_heng_meld(meld) || is_sequence_meld(meld)
}

fn is_valid_sequence(sequence: &[i32]) -> bool {
    if sequence.len() != 3 {
        return false;
    }
    let [a, b, c] = [sequence[0], sequence[1], sequence[2]];
    is_suited_tile(a) && is_suited_tile(b) && is_suited_tile(c) && a + 1 == b && b + 1 == c
}

fn is_valid_tile(tile: i32) -> bool {
    SHENYANG_MAHJONG_TILE_KINDS.contains(&tile)
}

pub fn is_win(tiles: &[i32]) -> bool {
    is_standard_win(tiles) || is_seven_pairs_win(tiles)
}

pub(crate) fn is_xi_gang_meld(meld: &WsShenyangMahjongMeld) -> bool {
    meld.kind == ShenyangMahjongMeldKind::XI_GANG
        && meld.from_position.is_none()
        && is_xi_gang_tiles(&meld.tiles)
}

pub fn is_xi_gang_tiles(tiles: &[i32]) -> bool {
    let mut tiles = tiles.to_vec();
    tiles.sort_unstable();
    tiles == XI_GANG_WINDS || tiles == XI_GANG_DRAGONS
}

pub fn remove_tiles(hand: &mut Vec<i32>, tiles: &[i32]) -> bool {
    if !tiles_in_hand(hand, tiles) {
        return false;
    }
    for tile in tiles {
        if let Some(index) = hand.iter().position(|item| item == tile) {
            hand.remove(index);
        }
    }
    hand.sort_unstable();
    true
}

fn same_suit(a: i32, b: i32) -> bool {
    a / 10 == b / 10
}

#[cfg(test)]
pub fn satisfies_shenyang_win(tiles: &[i32], melds: &[WsShenyangMahjongMeld]) -> bool {
    satisfies_shenyang_win_with_context(tiles, melds, ShenyangMahjongWinContext::new())
}

pub fn satisfies_shenyang_win_with_context(
    tiles: &[i32],
    melds: &[WsShenyangMahjongMeld],
    context: ShenyangMahjongWinContext,
) -> bool {
    if melds.iter().any(|meld| !is_valid_meld(meld)) {
        return false;
    }
    if !is_complete_win(tiles, melds.len()) {
        return false;
    }
    if is_seven_pairs_win(tiles) {
        return true;
    }
    let all_tiles = all_tiles_with_melds(tiles, melds);
    if !has_valid_tile_multiplicity(&all_tiles) {
        return false;
    }
    if is_pure_one_suit_tiles(&all_tiles) {
        return true;
    }
    if is_piao_hu_win(tiles, melds) {
        return true;
    }
    if !context.allow_first_chi
        && melds.iter().all(is_xi_gang_meld)
        && can_form_sequences_with_dragon_pair(tiles)
    {
        return has_three_suits(&all_tiles) && has_terminal_or_honor(&all_tiles);
    }
    if !has_open_meld(melds) {
        return false;
    }
    let has_triplet_or_dragon_pair = melds.iter().any(is_heng_meld)
        || has_triplet_in_standard_decomposition(tiles)
        || has_dragon_pair_as_standard_pair(tiles);
    has_three_suits(&all_tiles) && has_terminal_or_honor(&all_tiles) && has_triplet_or_dragon_pair
}

pub(crate) fn shenyang_score_concealed_dragon_triplet_fan(hand_tiles: &[i32]) -> i32 {
    [35, 36, 37]
        .into_iter()
        .filter(|dragon| hand_tiles.iter().filter(|tile| **tile == *dragon).count() >= 3)
        .count() as i32
}

pub(crate) fn shenyang_score_four_gui_yi_fan(
    hand_tiles: &[i32],
    melds: &[WsShenyangMahjongMeld],
) -> i32 {
    let mut counts = HashMap::<i32, i32>::new();
    for tile in hand_tiles
        .iter()
        .copied()
        .filter(|tile| is_valid_tile(*tile))
    {
        *counts.entry(tile).or_default() += 1;
    }
    for meld in melds.iter().filter(|meld| match meld.kind {
        ShenyangMahjongMeldKind::PENG => is_triplet_meld(meld),
        ShenyangMahjongMeldKind::CHI => is_sequence_meld(meld),
        ShenyangMahjongMeldKind::XI_GANG => is_xi_gang_meld(meld),
        ShenyangMahjongMeldKind::GANG => false,
    }) {
        for tile in meld.tiles.iter().copied() {
            *counts.entry(tile).or_default() += 1;
        }
    }
    counts.into_values().filter(|count| *count == 4).count() as i32
}

pub(crate) fn shenyang_score_meld_fan(melds: &[WsShenyangMahjongMeld]) -> i32 {
    melds
        .iter()
        .map(|meld| match meld.kind {
            ShenyangMahjongMeldKind::PENG
                if triplet_meld_primary_tile(meld).is_some_and(is_dragon_tile) =>
            {
                1
            }
            ShenyangMahjongMeldKind::GANG => {
                let concealed = meld.from_position.is_none();
                match triplet_meld_primary_tile(meld) {
                    Some(tile) if is_dragon_tile(tile) && concealed => 4,
                    Some(tile) if is_dragon_tile(tile) => 2,
                    Some(_) if concealed => 2,
                    Some(_) => 1,
                    None => 0,
                }
            }
            ShenyangMahjongMeldKind::XI_GANG if is_xi_gang_meld(meld) => 1,
            _ => 0,
        })
        .sum()
}

pub(crate) fn shenyang_score_for_fan(fan: i32) -> i32 {
    match u32::try_from(fan) {
        Ok(exponent) if exponent < i32::BITS - 1 => 1_i32 << exponent,
        Ok(_) => i32::MAX,
        Err(_) => 0,
    }
}

pub(crate) fn shenyang_score_for_fan_with_cap(fan: i32, score_cap: Option<i32>) -> i32 {
    let score = shenyang_score_for_fan(fan);
    score_cap
        .filter(|score_cap| *score_cap > 0)
        .map(|score_cap| score.min(score_cap))
        .unwrap_or(score)
}

pub(crate) fn shenyang_payment_fan(
    winner_fan: i32,
    winner_is_dealer: bool,
    payer_is_dealer: bool,
    payer_is_closed: bool,
    all_losers_closed: bool,
) -> i32 {
    winner_fan
        + i32::from(winner_is_dealer)
        + i32::from(payer_is_dealer)
        + if payer_is_closed {
            if all_losers_closed { 2 } else { 1 }
        } else {
            0
        }
}

pub(crate) fn shenyang_fan_reaches_score_cap(fan: i32, score_cap: i32) -> bool {
    score_cap > 0 && shenyang_score_for_fan(fan) >= score_cap
}

pub(crate) fn shenyang_fan_score_exceeds_half_cap(fan: i32, score_cap: i32) -> bool {
    score_cap > 0 && shenyang_score_for_fan(fan) > score_cap / 2
}

pub(crate) fn shenyang_fan_needed_for_score_cap(score_cap: i32) -> i32 {
    if score_cap <= 1 {
        return 0;
    }
    i32::try_from(i32::BITS - (score_cap - 1).leading_zeros()).unwrap_or(i32::MAX)
}

pub(crate) fn shenyang_score_visible_win_fan(
    hand_tiles: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_tile: Option<i32>,
    context: ShenyangMahjongWinContext,
    known_unavailable_tiles: &[i32],
) -> i32 {
    if !is_complete_win_with_melds_with_context(hand_tiles, melds, context) {
        return 0;
    }

    shenyang_win_pattern_base_fan(shenyang_win_pattern(hand_tiles, melds))
        + shenyang_score_meld_fan(melds)
        + shenyang_score_concealed_dragon_triplet_fan(hand_tiles)
        + shenyang_score_four_gui_yi_fan(hand_tiles, melds)
        + shenyang_score_wait_fan(
            hand_tiles,
            melds,
            win_tile,
            context,
            known_unavailable_tiles,
        )
}

pub(crate) fn shenyang_score_wait_fan(
    hand_tiles: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_tile: Option<i32>,
    context: ShenyangMahjongWinContext,
    known_unavailable_tiles: &[i32],
) -> i32 {
    let Some(win_tile) = win_tile else {
        return 0;
    };
    if !is_single_wait_shape_with_known_unavailable_tiles_with_context(
        hand_tiles,
        melds,
        win_tile,
        context,
        known_unavailable_tiles,
    ) {
        return 0;
    }

    1 + i32::from(
        shenyang_win_pattern(hand_tiles, melds) == ShenyangMahjongWinPattern::PiaoHu
            && melds.len() == 4
            && hand_tiles.len() == 2,
    )
}

pub(crate) fn shenyang_win_pattern(
    hand_tiles: &[i32],
    melds: &[WsShenyangMahjongMeld],
) -> ShenyangMahjongWinPattern {
    if melds.is_empty() && is_seven_pairs_win(hand_tiles) {
        ShenyangMahjongWinPattern::SevenPairs
    } else if is_pure_one_suit_win(hand_tiles, melds) {
        ShenyangMahjongWinPattern::PureOneSuit
    } else if is_piao_hu_win(hand_tiles, melds) {
        ShenyangMahjongWinPattern::PiaoHu
    } else {
        ShenyangMahjongWinPattern::Standard
    }
}

pub(crate) fn shenyang_win_pattern_base_fan(pattern: ShenyangMahjongWinPattern) -> i32 {
    match pattern {
        ShenyangMahjongWinPattern::Standard => 1,
        ShenyangMahjongWinPattern::PiaoHu => 3,
        ShenyangMahjongWinPattern::SevenPairs | ShenyangMahjongWinPattern::PureOneSuit => 4,
    }
}

pub fn sort_tiles(hand: &mut [i32]) {
    hand.sort_unstable();
}

fn tile_counts(tiles: &[i32]) -> [u8; 38] {
    let mut counts = [0u8; 38];
    for &tile in tiles {
        if is_valid_tile(tile) {
            counts[tile as usize] += 1;
        }
    }
    counts
}

pub fn tiles_in_hand(hand: &[i32], tiles: &[i32]) -> bool {
    let mut working = hand.to_vec();
    for tile in tiles {
        let Some(index) = working.iter().position(|item| item == tile) else {
            return false;
        };
        working.remove(index);
    }
    true
}

fn triplet_meld_primary_tile(meld: &WsShenyangMahjongMeld) -> Option<i32> {
    is_triplet_meld(meld).then(|| meld.tiles[0])
}

pub fn xi_gang_options(hand: &[i32]) -> Vec<Vec<i32>> {
    [XI_GANG_WINDS.as_slice(), XI_GANG_DRAGONS.as_slice()]
        .into_iter()
        .filter(|tiles| tiles_in_hand(hand, tiles))
        .map(<[i32]>::to_vec)
        .collect()
}

impl ShenyangMahjongWinContext {
    pub fn from_configs(configs: &HashMap<String, i32>) -> Self {
        Self::from_allow_first_chi(configs.get("allow_first_chi").copied().unwrap_or(1) == 1)
    }

    pub const fn from_allow_first_chi(allow_first_chi: bool) -> Self {
        Self { allow_first_chi }
    }

    pub const fn allows_first_chi(self) -> bool {
        self.allow_first_chi
    }

    #[cfg(test)]
    pub const fn new() -> Self {
        Self::from_allow_first_chi(true)
    }
}

#[cfg(test)]
mod tests {
    use share_type_public::games::shenyang_mahjong::{
        ShenyangMahjongMeldKind, ShenyangMahjongWinPattern, WsShenyangMahjongMeld,
    };

    use super::{
        ShenyangMahjongWinContext, can_chi, can_concealed_gang, can_gang, can_peng,
        has_triplet_in_standard_decomposition, is_complete_win, is_complete_win_with_melds,
        is_complete_win_with_melds_with_context, is_door_opening_meld, is_legal_single_wait_shape,
        is_piao_hu_win, is_pure_one_suit_win, is_seven_pairs_win, is_single_wait_shape,
        is_single_wait_shape_with_known_unavailable_tiles, is_standard_win,
        is_unique_complete_wait, is_win, satisfies_shenyang_win,
        satisfies_shenyang_win_with_context, shenyang_fan_needed_for_score_cap,
        shenyang_fan_reaches_score_cap, shenyang_fan_score_exceeds_half_cap, shenyang_payment_fan,
        shenyang_score_for_fan, shenyang_score_for_fan_with_cap, shenyang_score_visible_win_fan,
        shenyang_score_wait_fan, shenyang_win_pattern, shenyang_win_pattern_base_fan,
    };

    #[test]
    fn fan_scores_double_before_the_per_payer_cap() {
        assert_eq!(shenyang_score_for_fan(0), 1);
        assert_eq!(shenyang_score_for_fan(1), 2);
        assert_eq!(shenyang_score_for_fan(2), 4);
        assert_eq!(shenyang_score_for_fan(5), 32);
        assert_eq!(shenyang_score_for_fan_with_cap(5, Some(50)), 32);
        for fan in [6, 7, 8] {
            assert_eq!(shenyang_score_for_fan_with_cap(fan, Some(50)), 50);
        }
        assert!(shenyang_fan_score_exceeds_half_cap(5, 50));
        assert!(!shenyang_fan_reaches_score_cap(5, 50));
        assert!(shenyang_fan_reaches_score_cap(6, 50));
        assert_eq!(shenyang_fan_needed_for_score_cap(50), 6);

        assert_eq!(shenyang_score_for_fan(30), 1 << 30);
        assert_eq!(shenyang_score_for_fan(31), i32::MAX);
        assert_eq!(shenyang_score_for_fan(32), i32::MAX);
        assert_eq!(shenyang_score_for_fan_with_cap(31, Some(500)), 500);
        assert!(!shenyang_fan_score_exceeds_half_cap(29, i32::MAX));
        assert!(shenyang_fan_score_exceeds_half_cap(30, i32::MAX));
        assert!(!shenyang_fan_reaches_score_cap(30, i32::MAX));
        assert!(shenyang_fan_reaches_score_cap(31, i32::MAX));
        assert_eq!(shenyang_fan_needed_for_score_cap(i32::MAX), 31);
    }

    #[test]
    fn fan_score_cap_invariants_cover_the_configured_range() {
        for score_cap in 1..=500 {
            let needed_fan = shenyang_fan_needed_for_score_cap(score_cap);
            assert!(shenyang_fan_reaches_score_cap(needed_fan, score_cap));
            if needed_fan > 0 {
                assert!(!shenyang_fan_reaches_score_cap(needed_fan - 1, score_cap));
            }

            let mut previous_payment = 0;
            for fan in 0..=31 {
                let uncapped_score = if fan < 31 { 1_i32 << fan } else { i32::MAX };
                let payment = shenyang_score_for_fan_with_cap(fan, Some(score_cap));

                assert_eq!(payment, uncapped_score.min(score_cap));
                assert!(payment >= previous_payment);
                assert_eq!(
                    shenyang_fan_reaches_score_cap(fan, score_cap),
                    uncapped_score >= score_cap
                );
                assert_eq!(
                    shenyang_fan_score_exceeds_half_cap(fan, score_cap),
                    uncapped_score > score_cap / 2
                );
                previous_payment = payment;
            }
        }
    }

    #[test]
    fn payment_fan_includes_dealer_and_payer_closed_state() {
        assert_eq!(shenyang_payment_fan(1, false, false, false, false), 1);
        assert_eq!(shenyang_payment_fan(1, true, false, false, false), 2);
        assert_eq!(shenyang_payment_fan(1, false, true, false, false), 2);
        assert_eq!(shenyang_payment_fan(1, false, false, true, false), 2);
        assert_eq!(shenyang_payment_fan(1, false, true, true, true), 4);
    }

    #[test]
    fn chi_requires_real_sequence() {
        let hand = vec![1, 2, 4, 5, 6];
        assert!(can_chi(&hand, 3, &[1, 2]));
        assert!(can_chi(&hand, 3, &[2, 4]));
        assert!(!can_chi(&hand, 3, &[4, 6]));
        assert!(!can_chi(&hand, 31, &[32, 33]));
        assert!(!can_chi(&[1, 2, 3, 3, 3, 3], 3, &[1, 2]));
        assert!(!can_chi(&[1, 2, 99], 3, &[1, 2]));
    }

    #[test]
    fn closed_sequence_dragon_pair_exception_ignores_only_xi_gang_melds() {
        let tiles = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 35, 35];
        let xi_gang = vec![meld(
            ShenyangMahjongMeldKind::XI_GANG,
            vec![31, 32, 33, 34],
            None,
        )];
        let concealed_gang = vec![meld(
            ShenyangMahjongMeldKind::GANG,
            vec![31, 31, 31, 31],
            None,
        )];
        let context = ShenyangMahjongWinContext::from_allow_first_chi(false);

        assert!(satisfies_shenyang_win_with_context(
            &tiles, &xi_gang, context
        ));
        assert!(is_complete_win_with_melds_with_context(
            &tiles, &xi_gang, context
        ));
        assert!(!satisfies_shenyang_win_with_context(
            &tiles,
            &concealed_gang,
            context
        ));
    }

    #[test]
    fn complete_win_accepts_open_meld_remainder() {
        let tiles = vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35];
        assert!(is_complete_win(&tiles, 1));
    }

    #[test]
    fn complete_win_rejects_short_hand_without_melds() {
        assert!(is_standard_win(&[35, 35]));
        assert!(!is_complete_win(&[35, 35], 0));
    }

    #[test]
    fn complete_win_with_melds_enforces_shenyang_requirements() {
        let tiles = vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35];
        let chi_meld = vec![meld(ShenyangMahjongMeldKind::CHI, vec![1, 2, 3], Some(1))];
        let gang_meld = vec![meld(ShenyangMahjongMeldKind::GANG, vec![1, 1, 1, 1], None)];

        assert!(is_complete_win_with_melds(&tiles, &chi_meld));
        assert!(!is_complete_win_with_melds(&tiles, &gang_meld));
    }

    #[test]
    fn complete_win_with_melds_rejects_malformed_melds() {
        let tiles = vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35];
        let malformed_melds = [
            vec![meld(ShenyangMahjongMeldKind::PENG, vec![1, 1], Some(1))],
            vec![meld(ShenyangMahjongMeldKind::CHI, vec![1, 1, 1], Some(1))],
            vec![meld(
                ShenyangMahjongMeldKind::PENG,
                vec![99, 99, 99],
                Some(1),
            )],
            vec![meld(ShenyangMahjongMeldKind::GANG, vec![1, 1, 1], None)],
        ];

        for melds in malformed_melds {
            assert!(!is_complete_win_with_melds(&tiles, &melds));
        }
    }

    #[test]
    fn complete_win_with_melds_rejects_more_than_four_visible_copies() {
        let tiles = vec![1, 1, 11, 12, 13, 21, 22, 23, 31, 31, 31];
        let melds = vec![meld(ShenyangMahjongMeldKind::PENG, vec![1, 1, 1], Some(2))];

        assert!(is_complete_win(&tiles, melds.len()));
        assert!(!is_complete_win_with_melds(&tiles, &melds));
        assert!(!satisfies_shenyang_win(&tiles, &melds));
    }

    #[test]
    fn concealed_gang_requires_four_copies() {
        let hand = vec![31, 31, 31, 31, 32, 33];
        assert!(can_concealed_gang(&hand, 31));
        assert!(!can_concealed_gang(&hand, 32));
        assert!(!can_concealed_gang(&[99, 99, 99, 99], 99));
        assert!(!can_concealed_gang(&[31, 31, 31, 31, 31], 31));
    }

    #[test]
    fn disabled_first_chi_allows_closed_sequence_dragon_pair_win() {
        let default_context =
            ShenyangMahjongWinContext::from_configs(&std::collections::HashMap::new());
        let first_chi_disabled_context = ShenyangMahjongWinContext::from_configs(
            &std::collections::HashMap::from([("allow_first_chi".to_owned(), 0)]),
        );
        let invalid_value_context = ShenyangMahjongWinContext::from_configs(
            &std::collections::HashMap::from([("allow_first_chi".to_owned(), 2)]),
        );

        assert!(default_context.allows_first_chi());
        assert!(!first_chi_disabled_context.allows_first_chi());
        assert!(!invalid_value_context.allows_first_chi());
    }

    #[test]
    fn each_xi_gang_adds_one_meld_fan() {
        let melds = vec![
            meld(ShenyangMahjongMeldKind::XI_GANG, vec![35, 36, 37], None),
            meld(ShenyangMahjongMeldKind::XI_GANG, vec![31, 32, 33, 34], None),
        ];

        assert_eq!(super::shenyang_score_meld_fan(&melds), 2);
    }

    #[test]
    fn four_gui_yi_counts_tile_exposed_by_xi_gang() {
        let melds = vec![
            meld(ShenyangMahjongMeldKind::XI_GANG, vec![35, 36, 37], None),
            meld(ShenyangMahjongMeldKind::CHI, vec![1, 2, 3], Some(1)),
            meld(ShenyangMahjongMeldKind::CHI, vec![11, 12, 13], Some(2)),
        ];
        let hand = vec![21, 21, 37, 37, 37];

        assert!(is_complete_win_with_melds(&hand, &melds));
        assert_eq!(super::shenyang_score_meld_fan(&melds), 1);
        assert_eq!(super::shenyang_score_four_gui_yi_fan(&hand, &melds), 1);
    }

    #[test]
    fn gang_requires_three_copies_for_discard_claim() {
        let hand = vec![31, 31, 31, 32, 33];
        assert!(can_gang(&hand, 31));
        assert!(!can_gang(&hand, 32));
        assert!(!can_gang(&[99, 99, 99], 99));
        assert!(!can_gang(&[31, 31, 31, 31], 31));
        assert!(!can_gang(&[31, 31, 31, 99], 31));
    }

    fn meld(
        kind: ShenyangMahjongMeldKind,
        tiles: Vec<i32>,
        from_position: Option<i32>,
    ) -> WsShenyangMahjongMeld {
        WsShenyangMahjongMeld {
            kind,
            tiles,
            from_position,
        }
    }

    #[test]
    fn legacy_win_rule_config_is_ignored() {
        let configs = std::collections::HashMap::from([("win_rule".to_owned(), 0)]);

        assert_eq!(
            ShenyangMahjongWinContext::from_configs(&configs),
            ShenyangMahjongWinContext::new()
        );
    }

    #[test]
    fn peng_requires_two_copies() {
        let hand = vec![31, 31, 32, 33];
        assert!(can_peng(&hand, 31));
        assert!(can_peng(&[31, 31, 31, 32], 31));
        assert!(!can_peng(&hand, 32));
        assert!(!can_peng(&[99, 99], 99));
        assert!(!can_peng(&[31, 31, 31, 31], 31));
        assert!(!can_peng(&[31, 31, 99], 31));
    }

    #[test]
    fn piao_hu_accepts_closed_triplet_hand_with_dragon_pair() {
        let tiles = vec![1, 1, 1, 11, 11, 11, 21, 21, 21, 35, 35];
        let melds = vec![meld(
            ShenyangMahjongMeldKind::GANG,
            vec![31, 31, 31, 31],
            None,
        )];

        assert!(is_complete_win(&tiles, melds.len()));
        assert!(is_piao_hu_win(&tiles, &melds));
        assert!(satisfies_shenyang_win(&tiles, &melds));
        assert!(is_complete_win_with_melds(&tiles, &melds));
    }

    #[test]
    fn piao_hu_accepts_concealed_gang_as_triplet_group() {
        let tiles = vec![1, 1, 35, 35, 35];
        let melds = vec![
            meld(ShenyangMahjongMeldKind::GANG, vec![11, 11, 11, 11], None),
            meld(ShenyangMahjongMeldKind::PENG, vec![21, 21, 21], Some(2)),
            meld(ShenyangMahjongMeldKind::PENG, vec![31, 31, 31], Some(3)),
        ];

        assert!(is_piao_hu_win(&tiles, &melds));
    }

    #[test]
    fn piao_hu_accepts_gang_meld_as_triplet_group() {
        let tiles = vec![1, 1, 35, 35, 35];
        let melds = vec![
            meld(ShenyangMahjongMeldKind::GANG, vec![11, 11, 11, 11], Some(0)),
            meld(ShenyangMahjongMeldKind::PENG, vec![21, 21, 21], Some(2)),
            meld(ShenyangMahjongMeldKind::PENG, vec![31, 31, 31], Some(3)),
        ];

        assert!(is_piao_hu_win(&tiles, &melds));
    }

    #[test]
    fn piao_hu_rejects_closed_triplet_hand_with_non_dragon_pair() {
        let tiles = vec![1, 1, 1, 11, 11, 11, 21, 21, 21, 31, 31, 35, 35, 35];

        assert!(is_complete_win(&tiles, 0));
        assert!(!is_piao_hu_win(&tiles, &[]));
        assert!(!satisfies_shenyang_win(&tiles, &[]));
    }

    #[test]
    fn piao_hu_rejects_concealed_sequence_remainder() {
        let tiles = vec![1, 1, 2, 3, 4, 35, 35, 35];
        let melds = vec![
            meld(ShenyangMahjongMeldKind::PENG, vec![11, 11, 11], Some(0)),
            meld(ShenyangMahjongMeldKind::PENG, vec![21, 21, 21], Some(2)),
        ];

        assert!(is_complete_win(&tiles, melds.len()));
        assert!(!is_piao_hu_win(&tiles, &melds));
    }

    #[test]
    fn piao_hu_rejects_missing_suit_triplet_hand() {
        let tiles = vec![1, 1, 35, 35, 35];
        let melds = vec![
            meld(ShenyangMahjongMeldKind::PENG, vec![11, 11, 11], Some(0)),
            meld(ShenyangMahjongMeldKind::PENG, vec![12, 12, 12], Some(2)),
            meld(ShenyangMahjongMeldKind::PENG, vec![31, 31, 31], Some(3)),
        ];

        assert!(!is_piao_hu_win(&tiles, &melds));
    }

    #[test]
    fn piao_hu_rejects_more_than_four_visible_copies() {
        let tiles = vec![1, 1, 35, 35, 35];
        let melds = vec![
            meld(ShenyangMahjongMeldKind::PENG, vec![1, 1, 1], Some(0)),
            meld(ShenyangMahjongMeldKind::PENG, vec![11, 11, 11], Some(2)),
            meld(ShenyangMahjongMeldKind::PENG, vec![21, 21, 21], Some(3)),
        ];

        assert!(is_complete_win(&tiles, melds.len()));
        assert!(!is_complete_win_with_melds(&tiles, &melds));
        assert!(!is_piao_hu_win(&tiles, &melds));
    }

    #[test]
    fn piao_hu_rejects_short_gang_meld() {
        let tiles = vec![1, 1, 35, 35, 35];
        let melds = vec![
            meld(ShenyangMahjongMeldKind::GANG, vec![11, 11, 11], Some(0)),
            meld(ShenyangMahjongMeldKind::PENG, vec![21, 21, 21], Some(2)),
            meld(ShenyangMahjongMeldKind::PENG, vec![31, 31, 31], Some(3)),
        ];

        assert!(is_complete_win(&tiles, melds.len()));
        assert!(!is_piao_hu_win(&tiles, &melds));
    }

    #[test]
    fn piao_hu_rejects_triplet_hand_without_terminal_or_honor() {
        let tiles = vec![2, 2, 5, 5, 5];
        let melds = vec![
            meld(ShenyangMahjongMeldKind::PENG, vec![12, 12, 12], Some(0)),
            meld(ShenyangMahjongMeldKind::PENG, vec![15, 15, 15], Some(2)),
            meld(ShenyangMahjongMeldKind::PENG, vec![22, 22, 22], Some(3)),
        ];

        assert!(!is_piao_hu_win(&tiles, &melds));
    }

    #[test]
    fn piao_hu_rejects_triplet_tiles_with_chi_kind() {
        let tiles = vec![1, 1, 35, 35, 35];
        let melds = vec![
            meld(ShenyangMahjongMeldKind::PENG, vec![11, 11, 11], Some(0)),
            meld(ShenyangMahjongMeldKind::CHI, vec![21, 21, 21], Some(2)),
            meld(ShenyangMahjongMeldKind::PENG, vec![31, 31, 31], Some(3)),
        ];

        assert!(!is_piao_hu_win(&tiles, &melds));
    }

    #[test]
    fn piao_hu_requires_triplets_three_suits_and_terminal_or_honor() {
        let tiles = vec![1, 1, 35, 35, 35];
        let melds = vec![
            meld(ShenyangMahjongMeldKind::PENG, vec![11, 11, 11], Some(0)),
            meld(ShenyangMahjongMeldKind::PENG, vec![21, 21, 21], Some(2)),
            meld(ShenyangMahjongMeldKind::PENG, vec![31, 31, 31], Some(3)),
        ];

        assert!(is_piao_hu_win(&tiles, &melds));
    }

    #[test]
    fn pure_one_suit_rejects_honor_chi_meld() {
        let tiles = vec![1, 2, 3, 4, 5, 6, 7, 7, 7, 9, 9];
        let melds = vec![meld(
            ShenyangMahjongMeldKind::CHI,
            vec![31, 32, 33],
            Some(1),
        )];

        assert!(is_complete_win(&tiles, melds.len()));
        assert!(!is_pure_one_suit_win(&tiles, &melds));
        assert!(!satisfies_shenyang_win(&tiles, &melds));
    }

    #[test]
    fn pure_one_suit_rejects_honor_meld() {
        let tiles = vec![1, 2, 3, 4, 5, 6, 7, 7, 9, 9, 9];
        let melds = vec![meld(
            ShenyangMahjongMeldKind::PENG,
            vec![31, 31, 31],
            Some(1),
        )];

        assert!(!is_pure_one_suit_win(&tiles, &melds));
    }

    #[test]
    fn pure_one_suit_rejects_honor_tiles() {
        let tiles = vec![1, 2, 3, 4, 5, 6, 7, 7, 7, 31, 31];
        let melds = vec![meld(ShenyangMahjongMeldKind::CHI, vec![2, 3, 4], Some(1))];

        assert!(!is_pure_one_suit_win(&tiles, &melds));
    }

    #[test]
    fn pure_one_suit_rejects_malformed_same_suit_meld() {
        let tiles = vec![1, 2, 3, 4, 5, 6, 7, 7, 7, 9, 9];
        let melds = vec![meld(ShenyangMahjongMeldKind::PENG, vec![2, 3, 4], Some(1))];

        assert!(is_complete_win(&tiles, melds.len()));
        assert!(!is_pure_one_suit_win(&tiles, &melds));
        assert!(!satisfies_shenyang_win(&tiles, &melds));
    }

    #[test]
    fn pure_one_suit_rejects_more_than_four_visible_copies() {
        let tiles = vec![1, 1, 2, 3, 4, 4, 5, 6, 7, 7, 7];
        let melds = vec![meld(ShenyangMahjongMeldKind::PENG, vec![1, 1, 1], Some(2))];

        assert!(is_complete_win(&tiles, melds.len()));
        assert!(!is_complete_win_with_melds(&tiles, &melds));
        assert!(!is_pure_one_suit_win(&tiles, &melds));
    }

    #[test]
    fn seven_pairs_allows_four_of_a_kind_as_two_pairs() {
        let tiles = vec![1, 1, 1, 1, 11, 11, 12, 12, 21, 21, 22, 22, 31, 31];
        assert!(is_seven_pairs_win(&tiles));
        assert!(is_win(&tiles));
    }

    #[test]
    fn seven_pairs_hand_is_recognized() {
        let tiles = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 31, 31];
        assert!(is_seven_pairs_win(&tiles));
        assert!(is_win(&tiles));
    }

    #[test]
    fn seven_pairs_rejects_unpaired_tile() {
        let tiles = vec![1, 1, 2, 3, 11, 11, 12, 12, 21, 21, 22, 22, 31, 31];
        assert!(!is_seven_pairs_win(&tiles));
        assert!(!is_win(&tiles));
    }

    #[test]
    fn shared_door_opening_predicate_excludes_concealed_and_xi_gangs() {
        let chi = meld(ShenyangMahjongMeldKind::CHI, vec![1, 2, 3], Some(0));
        let peng = meld(ShenyangMahjongMeldKind::PENG, vec![11, 11, 11], Some(1));
        let open_gang = meld(ShenyangMahjongMeldKind::GANG, vec![21, 21, 21, 21], Some(2));
        let concealed_gang = meld(ShenyangMahjongMeldKind::GANG, vec![31, 31, 31, 31], None);
        let xi_gang = meld(ShenyangMahjongMeldKind::XI_GANG, vec![35, 36, 37], None);

        assert!(is_door_opening_meld(&chi));
        assert!(is_door_opening_meld(&peng));
        assert!(is_door_opening_meld(&open_gang));
        assert!(!is_door_opening_meld(&concealed_gang));
        assert!(!is_door_opening_meld(&xi_gang));
    }

    #[test]
    fn shared_visible_win_fan_combines_pattern_meld_and_wait_scoring() {
        let melds = vec![
            meld(ShenyangMahjongMeldKind::PENG, vec![1, 1, 1], Some(0)),
            meld(ShenyangMahjongMeldKind::GANG, vec![11, 11, 11, 11], Some(1)),
            meld(ShenyangMahjongMeldKind::GANG, vec![21, 21, 21, 21], None),
            meld(ShenyangMahjongMeldKind::PENG, vec![31, 31, 31], Some(2)),
        ];
        let hand = vec![35, 35];
        let context = ShenyangMahjongWinContext::new();

        assert_eq!(
            shenyang_score_visible_win_fan(&hand, &melds, Some(35), context, &[]),
            8
        );
        assert_eq!(
            shenyang_score_visible_win_fan(&hand, &melds, None, context, &[]),
            6
        );
        assert_eq!(
            shenyang_score_visible_win_fan(&[1, 2, 3], &[], Some(3), context, &[]),
            0
        );
    }

    #[test]
    fn shared_wait_fan_stacks_shou_ba_yi_only_for_piao_single_wait() {
        let piao_melds = vec![
            meld(ShenyangMahjongMeldKind::PENG, vec![1, 1, 1], Some(0)),
            meld(ShenyangMahjongMeldKind::PENG, vec![11, 11, 11], Some(1)),
            meld(ShenyangMahjongMeldKind::PENG, vec![21, 21, 21], Some(2)),
            meld(ShenyangMahjongMeldKind::PENG, vec![31, 31, 31], Some(3)),
        ];
        let piao_pair = vec![35, 35];
        let standard_melds = vec![meld(ShenyangMahjongMeldKind::CHI, vec![1, 2, 3], Some(0))];
        let standard = vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35];
        let context = ShenyangMahjongWinContext::new();

        assert_eq!(
            shenyang_score_wait_fan(&piao_pair, &piao_melds, Some(35), context, &[]),
            2
        );
        assert_eq!(
            shenyang_score_wait_fan(&standard, &standard_melds, Some(35), context, &[]),
            1
        );
        assert_eq!(
            shenyang_score_wait_fan(&standard, &standard_melds, None, context, &[]),
            0
        );
    }

    #[test]
    fn shared_win_pattern_uses_server_base_fan_priority() {
        let seven_pairs = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35];
        let pure_one_suit = vec![1, 2, 2, 3, 3, 4, 4, 5, 6, 7, 7, 7, 9, 9];
        let piao_hand = vec![1, 1, 1, 35, 35];
        let piao_melds = vec![
            meld(ShenyangMahjongMeldKind::PENG, vec![11, 11, 11], Some(0)),
            meld(ShenyangMahjongMeldKind::PENG, vec![21, 21, 21], Some(2)),
            meld(ShenyangMahjongMeldKind::PENG, vec![31, 31, 31], Some(3)),
        ];
        let standard = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35];

        let cases = [
            (
                seven_pairs.as_slice(),
                &[][..],
                ShenyangMahjongWinPattern::SevenPairs,
                4,
            ),
            (
                pure_one_suit.as_slice(),
                &[][..],
                ShenyangMahjongWinPattern::PureOneSuit,
                4,
            ),
            (
                piao_hand.as_slice(),
                piao_melds.as_slice(),
                ShenyangMahjongWinPattern::PiaoHu,
                3,
            ),
            (
                standard.as_slice(),
                &[][..],
                ShenyangMahjongWinPattern::Standard,
                1,
            ),
        ];

        for (hand, melds, expected_pattern, expected_base_fan) in cases {
            let pattern = shenyang_win_pattern(hand, melds);
            assert_eq!(pattern, expected_pattern);
            assert_eq!(shenyang_win_pattern_base_fan(pattern), expected_base_fan);
        }
    }

    #[test]
    fn shenyang_accepts_closed_pure_one_suit_without_open_meld() {
        let tiles = vec![1, 2, 3, 2, 3, 4, 4, 5, 6, 7, 7, 7, 9, 9];

        assert!(is_pure_one_suit_win(&tiles, &[]));
        assert!(satisfies_shenyang_win(&tiles, &[]));
    }

    #[test]
    fn shenyang_accepts_concealed_gang_as_heng_after_open_meld() {
        let tiles = vec![4, 5, 6, 8, 8, 21, 22, 23];
        let melds = vec![
            meld(ShenyangMahjongMeldKind::GANG, vec![1, 1, 1, 1], None),
            meld(ShenyangMahjongMeldKind::CHI, vec![11, 12, 13], Some(1)),
        ];

        assert!(!has_triplet_in_standard_decomposition(&tiles));
        assert!(satisfies_shenyang_win(&tiles, &melds));
    }

    #[test]
    fn shenyang_accepts_concealed_triplet_decomposition() {
        let tiles = vec![1, 1, 1, 2, 3, 4, 21, 22, 23, 8, 8];
        let melds = vec![meld(
            ShenyangMahjongMeldKind::CHI,
            vec![11, 12, 13],
            Some(1),
        )];

        assert!(satisfies_shenyang_win(&tiles, &melds));
    }

    #[test]
    fn shenyang_accepts_dragon_pair_without_triplet() {
        let tiles = vec![11, 12, 13, 21, 22, 23, 4, 5, 6, 35, 35];
        let melds = vec![meld(ShenyangMahjongMeldKind::CHI, vec![1, 2, 3], Some(1))];

        assert!(satisfies_shenyang_win(&tiles, &melds));
    }

    #[test]
    fn shenyang_accepts_open_pure_one_suit_without_terminal_or_triplet() {
        let tiles = vec![3, 4, 5, 4, 5, 6, 5, 6, 7, 8, 8];
        let melds = vec![meld(ShenyangMahjongMeldKind::CHI, vec![2, 3, 4], Some(1))];

        assert!(is_pure_one_suit_win(&tiles, &melds));
        assert!(!has_triplet_in_standard_decomposition(&tiles));
        assert!(satisfies_shenyang_win(&tiles, &melds));
    }

    #[test]
    fn shenyang_accepts_open_triplet_with_three_suits_and_terminal() {
        let tiles = vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35];
        let melds = vec![meld(ShenyangMahjongMeldKind::CHI, vec![1, 2, 3], Some(1))];

        assert!(satisfies_shenyang_win(&tiles, &melds));
    }

    #[test]
    fn shenyang_accepts_pure_one_suit_without_three_suits() {
        let tiles = vec![1, 2, 3, 4, 5, 6, 7, 7, 7, 9, 9];
        let melds = vec![meld(ShenyangMahjongMeldKind::CHI, vec![2, 3, 4], Some(1))];

        assert!(is_pure_one_suit_win(&tiles, &melds));
        assert!(satisfies_shenyang_win(&tiles, &melds));
    }

    #[test]
    fn shenyang_allows_closed_sequence_hand_with_dragon_pair_when_configured() {
        let tiles = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 35, 35];
        let context = ShenyangMahjongWinContext::from_allow_first_chi(false);

        assert!(!has_triplet_in_standard_decomposition(&tiles));
        assert!(!satisfies_shenyang_win(&tiles, &[]));
        assert!(satisfies_shenyang_win_with_context(&tiles, &[], context));
        assert!(is_complete_win_with_melds_with_context(
            &tiles,
            &[],
            context
        ));
    }

    #[test]
    fn shenyang_always_counts_chi_as_opening_meld() {
        let tiles = vec![1, 1, 1, 2, 3, 4, 21, 22, 23, 8, 8];
        let melds = vec![meld(
            ShenyangMahjongMeldKind::CHI,
            vec![11, 12, 13],
            Some(1),
        )];

        assert!(satisfies_shenyang_win(&tiles, &melds));
        assert!(is_complete_win_with_melds(&tiles, &melds));
    }

    #[test]
    fn shenyang_closed_sequence_dragon_pair_exception_rejects_actual_triplet() {
        let tiles = vec![1, 1, 1, 4, 5, 6, 11, 12, 13, 21, 22, 23, 35, 35];
        let context = ShenyangMahjongWinContext::from_allow_first_chi(false);

        assert!(has_triplet_in_standard_decomposition(&tiles));
        assert!(!satisfies_shenyang_win_with_context(&tiles, &[], context));
    }

    #[test]
    fn shenyang_closed_sequence_dragon_pair_exception_rejects_ordinary_pair() {
        let tiles = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31];
        let context = ShenyangMahjongWinContext::from_allow_first_chi(false);

        assert!(!satisfies_shenyang_win_with_context(&tiles, &[], context));
    }

    #[test]
    fn shenyang_rejects_concealed_gang_as_open_meld() {
        let tiles = vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35];
        let melds = vec![meld(ShenyangMahjongMeldKind::GANG, vec![1, 1, 1, 1], None)];

        assert!(!is_complete_win_with_melds(&tiles, &melds));
        assert!(!satisfies_shenyang_win(&tiles, &melds));
    }

    #[test]
    fn shenyang_rejects_incomplete_pure_one_suit() {
        let tiles = vec![1, 2, 3, 2, 3, 4, 4, 5, 6, 7, 7, 7, 9];

        assert!(!is_complete_win(&tiles, 0));
        assert!(!is_pure_one_suit_win(&tiles, &[]));
        assert!(!satisfies_shenyang_win(&tiles, &[]));
    }

    #[test]
    fn shenyang_rejects_malformed_meld_supplying_required_suit() {
        let tiles = vec![11, 12, 13, 31, 31, 31, 35, 35];
        let melds = vec![
            meld(ShenyangMahjongMeldKind::PENG, vec![2, 2, 2], Some(1)),
            meld(ShenyangMahjongMeldKind::CHI, vec![21, 22, 24], Some(2)),
        ];

        assert!(!is_complete_win_with_melds(&tiles, &melds));
        assert!(!satisfies_shenyang_win(&tiles, &melds));
    }

    #[test]
    fn shenyang_rejects_malformed_open_meld_as_opening() {
        let tiles = vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35];
        let short_peng = vec![meld(ShenyangMahjongMeldKind::PENG, vec![1, 1], Some(1))];
        let invalid_chi = vec![meld(ShenyangMahjongMeldKind::CHI, vec![1, 1, 1], Some(1))];

        assert!(!satisfies_shenyang_win(&tiles, &short_peng));
        assert!(!satisfies_shenyang_win(&tiles, &invalid_chi));
    }

    #[test]
    fn shenyang_rejects_non_dragon_pair_without_triplet() {
        let tiles = vec![11, 12, 13, 21, 22, 23, 4, 5, 6, 8, 8];
        let melds = vec![meld(ShenyangMahjongMeldKind::CHI, vec![1, 2, 3], Some(1))];

        assert!(!satisfies_shenyang_win(&tiles, &melds));
    }

    #[test]
    fn shenyang_rejects_open_all_simples_standard_without_terminal() {
        let tiles = vec![2, 3, 4, 12, 13, 14, 22, 23, 24, 6, 6];
        let melds = vec![meld(ShenyangMahjongMeldKind::PENG, vec![5, 5, 5], Some(1))];

        assert!(!is_complete_win_with_melds(&tiles, &melds));
        assert!(!satisfies_shenyang_win(&tiles, &melds));
    }

    #[test]
    fn shenyang_rejects_open_win_without_three_suits() {
        let tiles = vec![11, 12, 13, 11, 12, 13, 31, 31, 31, 35, 35];
        let melds = vec![meld(ShenyangMahjongMeldKind::CHI, vec![1, 2, 3], Some(1))];

        assert!(!satisfies_shenyang_win(&tiles, &melds));
    }

    #[test]
    fn shenyang_rejects_sequence_reuse_as_fake_triplet() {
        let tiles = vec![1, 2, 2, 3, 3, 3, 4, 4, 5, 26, 26];
        let melds = vec![meld(
            ShenyangMahjongMeldKind::CHI,
            vec![11, 12, 13],
            Some(1),
        )];

        assert!(!is_complete_win_with_melds(&tiles, &melds));
        assert!(!satisfies_shenyang_win(&tiles, &melds));
    }

    #[test]
    fn shenyang_rejects_short_gang_as_heng() {
        let tiles = vec![4, 5, 6, 8, 8, 21, 22, 23];
        let melds = vec![
            meld(ShenyangMahjongMeldKind::GANG, vec![1, 1, 1], None),
            meld(ShenyangMahjongMeldKind::CHI, vec![11, 12, 13], Some(1)),
        ];

        assert!(!is_complete_win_with_melds(&tiles, &melds));
        assert!(!has_triplet_in_standard_decomposition(&tiles));
        assert!(!satisfies_shenyang_win(&tiles, &melds));
    }

    #[test]
    fn shenyang_rejects_standard_win_without_open_meld() {
        let tiles = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35];

        assert!(!satisfies_shenyang_win(&tiles, &[]));
    }

    #[test]
    fn shenyang_seven_pairs_ignores_normal_hand_requirements() {
        let tiles = vec![2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8];

        assert!(is_seven_pairs_win(&tiles));
        assert!(satisfies_shenyang_win(&tiles, &[]));
    }

    #[test]
    fn single_wait_shape_accepts_closed_middle_wait() {
        let tiles = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 12, 13, 21, 21];

        assert!(is_single_wait_shape(&tiles, &[], 5));
    }

    #[test]
    fn single_wait_shape_accepts_edge_wait() {
        let tiles = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 35, 35];

        assert!(is_single_wait_shape(&tiles, &[], 3));
    }

    #[test]
    fn single_wait_shape_accepts_four_copy_seven_pairs_wait() {
        let tiles = vec![5, 5, 5, 5, 31, 31, 32, 32, 33, 33, 34, 34, 35, 35];

        assert!(is_seven_pairs_win(&tiles));
        assert!(is_single_wait_shape(&tiles, &[], 5));
    }

    #[test]
    fn single_wait_shape_accepts_middle_tile_seven_pairs_wait() {
        let tiles = vec![1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 11, 11, 21, 21];

        assert!(is_seven_pairs_win(&tiles));
        assert!(is_single_wait_shape(&tiles, &[], 5));
        assert!(is_legal_single_wait_shape(&tiles, &[], 5));
    }

    #[test]
    fn single_wait_shape_accepts_pair_wait() {
        let tiles = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 35, 35];

        assert!(is_single_wait_shape(&tiles, &[], 35));
    }

    #[test]
    fn single_wait_shape_accepts_pair_wait_with_honor_triplet() {
        let tiles = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35];

        assert!(is_single_wait_shape(&tiles, &[], 35));
    }

    #[test]
    fn single_wait_shape_accepts_unique_terminal_wait() {
        let tiles = vec![1, 1, 1, 13, 14, 15, 16, 17, 17, 17, 17, 18, 18, 19];

        assert!(is_standard_win(&tiles));
        assert!(is_unique_complete_wait(&tiles, &[], 1));
        assert!(is_single_wait_shape(&tiles, &[], 1));
    }

    #[test]
    fn single_wait_shape_counts_chi_as_opening_meld() {
        let tiles = vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35];
        let melds = vec![meld(ShenyangMahjongMeldKind::CHI, vec![1, 2, 3], Some(1))];

        assert!(is_single_wait_shape_with_known_unavailable_tiles(
            &tiles,
            &melds,
            35,
            &[],
        ));
    }

    #[test]
    fn single_wait_shape_ignores_waits_blocked_by_exposed_four_copies() {
        let tiles = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 35, 35];
        let melds = vec![meld(
            ShenyangMahjongMeldKind::GANG,
            vec![4, 4, 4, 4],
            Some(2),
        )];

        assert!(is_complete_win_with_melds(&tiles, &melds));
        assert!(is_unique_complete_wait(&tiles, &melds, 1));
        assert!(is_single_wait_shape(&tiles, &melds, 1));
    }

    #[test]
    fn single_wait_shape_ignores_waits_blocked_by_known_unavailable_tiles() {
        let tiles = vec![1, 2, 3, 11, 12, 13, 25, 25, 31, 31, 31];
        let melds = vec![meld(
            ShenyangMahjongMeldKind::CHI,
            vec![21, 22, 23],
            Some(2),
        )];

        assert!(!is_legal_single_wait_shape(&tiles, &melds, 1));
        assert!(is_single_wait_shape_with_known_unavailable_tiles(
            &tiles,
            &melds,
            1,
            &[4, 4, 4, 4]
        ));
    }

    #[test]
    fn single_wait_shape_rejects_closed_middle_shape_with_multiple_waits() {
        let tiles = vec![6, 7, 7, 8, 8, 9, 11, 12, 13, 15, 15, 15, 22, 22];

        assert!(is_standard_win(&tiles));
        assert!(!is_single_wait_shape(&tiles, &[], 8));
    }

    #[test]
    fn single_wait_shape_rejects_invalid_meld() {
        let tiles = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 35, 35];
        let invalid_meld = vec![meld(
            ShenyangMahjongMeldKind::PENG,
            vec![99, 99, 99],
            Some(0),
        )];

        assert!(is_complete_win(&tiles, invalid_meld.len()));
        assert!(!is_single_wait_shape(&tiles, &invalid_meld, 35));
        assert!(!is_legal_single_wait_shape(&tiles, &invalid_meld, 35));
    }

    #[test]
    fn single_wait_shape_rejects_open_two_sided_wait() {
        let tiles = vec![2, 3, 4, 5, 6, 7, 11, 12, 13, 21, 22, 23, 31, 31];

        assert!(!is_single_wait_shape(&tiles, &[], 4));
    }

    #[test]
    fn single_wait_shape_rejects_terminal_triplet_completion_with_multiple_waits() {
        let tiles = vec![1, 1, 1, 2, 3, 4, 5, 6, 7, 8, 9, 35, 35, 35];

        assert!(is_standard_win(&tiles));
        assert!(!is_single_wait_shape(&tiles, &[], 1));
    }

    #[test]
    fn single_wait_shape_rejects_closed_standard_basic_win() {
        let tiles = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35];

        assert!(is_single_wait_shape(&tiles, &[], 35));
        assert!(!is_legal_single_wait_shape(&tiles, &[], 35));
    }

    #[test]
    fn standard_hand_is_recognized() {
        let tiles = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35];
        assert!(is_standard_win(&tiles));
        assert!(is_win(&tiles));
    }

    #[test]
    fn standard_win_rejects_more_than_four_copies() {
        let tiles = vec![1, 1, 1, 1, 1, 11, 12, 13, 21, 22, 23, 31, 31, 31];

        assert!(!is_standard_win(&tiles));
        assert!(!is_complete_win(&tiles, 0));
        assert!(!is_win(&tiles));
    }

    #[test]
    fn xi_gang_can_participate_in_closed_dragon_pair_piao() {
        let tiles = vec![1, 1, 1, 11, 11, 11, 21, 21, 21, 35, 35];
        let melds = vec![meld(
            ShenyangMahjongMeldKind::XI_GANG,
            vec![35, 36, 37],
            None,
        )];

        assert!(is_piao_hu_win(&tiles, &melds));
        assert!(is_complete_win_with_melds(&tiles, &melds));
        assert_eq!(
            shenyang_score_visible_win_fan(
                &tiles,
                &melds,
                Some(35),
                ShenyangMahjongWinContext::new(),
                &[],
            ),
            5,
        );
    }

    #[test]
    fn xi_gang_counts_as_heng_without_opening_the_hand() {
        let xi_gang = meld(ShenyangMahjongMeldKind::XI_GANG, vec![35, 36, 37], None);
        let closed_remainder = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31];
        assert!(is_complete_win(&closed_remainder, 1));
        assert!(!is_complete_win_with_melds(
            &closed_remainder,
            std::slice::from_ref(&xi_gang),
        ));

        let open_melds = vec![
            xi_gang,
            meld(ShenyangMahjongMeldKind::CHI, vec![1, 2, 3], Some(1)),
        ];
        let open_remainder = vec![11, 12, 13, 21, 22, 23, 31, 31];
        assert!(is_complete_win_with_melds(&open_remainder, &open_melds));
    }

    #[test]
    fn xi_gang_honors_satisfy_terminal_or_honor_requirement() {
        let melds = vec![
            meld(ShenyangMahjongMeldKind::XI_GANG, vec![35, 36, 37], None),
            meld(ShenyangMahjongMeldKind::PENG, vec![5, 5, 5], Some(1)),
            meld(ShenyangMahjongMeldKind::CHI, vec![12, 13, 14], Some(2)),
        ];
        let remainder = vec![22, 23, 24, 6, 6];

        assert!(is_complete_win_with_melds(&remainder, &melds));
    }

    #[test]
    fn xi_gang_honors_do_not_supply_a_missing_suit() {
        let melds = vec![
            meld(ShenyangMahjongMeldKind::XI_GANG, vec![31, 32, 33, 34], None),
            meld(ShenyangMahjongMeldKind::CHI, vec![2, 3, 4], Some(1)),
        ];
        let remainder = vec![5, 6, 7, 12, 13, 14, 8, 8];

        assert!(!is_complete_win_with_melds(&remainder, &melds));
    }

    #[test]
    fn xi_gang_is_valid_only_as_a_source_less_special_meld() {
        let dragons = meld(ShenyangMahjongMeldKind::XI_GANG, vec![35, 36, 37], None);
        let winds = meld(ShenyangMahjongMeldKind::XI_GANG, vec![34, 31, 33, 32], None);
        let sourced = meld(ShenyangMahjongMeldKind::XI_GANG, vec![35, 36, 37], Some(1));
        let malformed = meld(ShenyangMahjongMeldKind::XI_GANG, vec![31, 32, 33], None);

        assert!(super::is_valid_meld(&dragons));
        assert!(super::is_valid_meld(&winds));
        assert!(!super::is_valid_meld(&sourced));
        assert!(!super::is_valid_meld(&malformed));
    }

    #[test]
    fn xi_gang_options_require_one_of_each_honor() {
        assert_eq!(
            super::xi_gang_options(&[31, 32, 33, 34, 35, 36, 37]),
            vec![vec![31, 32, 33, 34], vec![35, 36, 37]]
        );
        assert_eq!(
            super::xi_gang_options(&[31, 32, 33, 35, 36]),
            Vec::<Vec<i32>>::new()
        );
        assert_eq!(
            super::xi_gang_options(&[35, 35, 36, 37]),
            vec![vec![35, 36, 37]]
        );
    }
}
