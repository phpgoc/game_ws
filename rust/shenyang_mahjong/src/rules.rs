use std::collections::HashMap;

use share_type_public::games::shenyang_mahjong::{
    SHENYANG_MAHJONG_TILE_KINDS, ShenyangMahjongMeldKind, WsShenyangMahjongMeld,
};

pub const WIN_RULE_RELAXED: i32 = 0;
pub const WIN_RULE_SHENYANG_BASIC: i32 = 1;

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
    if !tiles_in_hand(hand, consume_tiles) {
        return false;
    }
    let mut sequence = vec![target_tile, consume_tiles[0], consume_tiles[1]];
    sequence.sort_unstable();
    is_valid_sequence(&sequence)
}

pub fn can_concealed_gang(hand: &[i32], target_tile: i32) -> bool {
    hand.iter().filter(|&&tile| tile == target_tile).count() >= 4
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

pub fn can_gang(hand: &[i32], target_tile: i32) -> bool {
    hand.iter().filter(|&&tile| tile == target_tile).count() >= 3
}

pub fn can_peng(hand: &[i32], target_tile: i32) -> bool {
    hand.iter().filter(|&&tile| tile == target_tile).count() >= 2
}

fn dragon_pair_tiles() -> [i32; 3] {
    [35, 36, 37]
}

fn has_dragon_pair_as_standard_pair(tiles: &[i32]) -> bool {
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

fn has_open_meld(melds: &[WsShenyangMahjongMeld]) -> bool {
    melds.iter().any(|meld| meld.from_position.is_some())
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

fn has_triplet_in_standard_decomposition(tiles: &[i32]) -> bool {
    if tiles.len() % 3 != 2 {
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

pub fn is_complete_win(tiles: &[i32], meld_count: usize) -> bool {
    if meld_count == 0 {
        return tiles.len() == 14 && is_win(tiles);
    }
    if tiles.len() + meld_count * 3 != 14 {
        return false;
    }
    is_standard_win(tiles)
}

pub fn is_complete_win_with_melds(
    tiles: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_rule: i32,
) -> bool {
    if !is_complete_win(tiles, melds.len()) {
        return false;
    }
    if win_rule != WIN_RULE_SHENYANG_BASIC {
        return true;
    }
    satisfies_shenyang_basic_win(tiles, melds)
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

pub fn is_pure_one_suit_win(tiles: &[i32], melds: &[WsShenyangMahjongMeld]) -> bool {
    is_complete_win(tiles, melds.len())
        && is_pure_one_suit_tiles(&all_tiles_with_melds(tiles, melds))
}

pub fn is_piao_hu_win(tiles: &[i32], melds: &[WsShenyangMahjongMeld]) -> bool {
    if !is_complete_win(tiles, melds.len()) || melds.iter().any(|meld| !is_triplet_meld(meld)) {
        return false;
    }
    let all_tiles = all_tiles_with_melds(tiles, melds);
    has_three_suits(&all_tiles)
        && has_terminal_or_honor(&all_tiles)
        && can_form_triplets_with_pair(tiles)
}

pub fn is_single_wait_shape(tiles: &[i32], melds: &[WsShenyangMahjongMeld], win_tile: i32) -> bool {
    if !is_complete_win(tiles, melds.len()) || !tiles.contains(&win_tile) {
        return false;
    }
    if !is_unique_complete_wait(tiles, melds, win_tile) {
        return false;
    }
    is_pair_single_wait(tiles, win_tile)
        || is_closed_middle_wait(tiles, win_tile)
        || is_edge_wait(tiles, win_tile)
        || is_terminal_tile(win_tile)
        || is_honor_tile(win_tile)
}

pub fn is_single_wait_shape_with_rule(
    tiles: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_tile: i32,
    win_rule: i32,
) -> bool {
    if win_rule != WIN_RULE_SHENYANG_BASIC {
        return is_single_wait_shape(tiles, melds, win_tile);
    }
    if !is_complete_win_with_melds(tiles, melds, win_rule) || !tiles.contains(&win_tile) {
        return false;
    }
    if !is_unique_complete_wait_with_rule(tiles, melds, win_tile, win_rule) {
        return false;
    }
    is_pair_single_wait(tiles, win_tile)
        || is_closed_middle_wait(tiles, win_tile)
        || is_edge_wait(tiles, win_tile)
        || is_terminal_tile(win_tile)
        || is_honor_tile(win_tile)
}

pub fn is_standard_win(tiles: &[i32]) -> bool {
    if tiles.len() % 3 != 2 {
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

fn is_honor_tile(tile: i32) -> bool {
    matches!(tile, 31..=37)
}

fn is_terminal_tile(tile: i32) -> bool {
    matches!(tile, 1 | 9 | 11 | 19 | 21 | 29)
}

fn is_valid_sequence(sequence: &[i32]) -> bool {
    if sequence.len() != 3 {
        return false;
    }
    let [a, b, c] = [sequence[0], sequence[1], sequence[2]];
    same_suit(a, b) && same_suit(a, c) && a + 1 == b && b + 1 == c
}

fn is_valid_tile(tile: i32) -> bool {
    SHENYANG_MAHJONG_TILE_KINDS.contains(&tile)
}

fn is_triplet_meld(meld: &WsShenyangMahjongMeld) -> bool {
    matches!(
        meld.kind,
        ShenyangMahjongMeldKind::PENG | ShenyangMahjongMeldKind::GANG
    ) && meld.tiles.len() >= 3
        && meld.tiles.iter().all(|tile| *tile == meld.tiles[0])
}

pub fn is_win(tiles: &[i32]) -> bool {
    is_standard_win(tiles) || is_seven_pairs_win(tiles)
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

fn is_pair_single_wait(tiles: &[i32], win_tile: i32) -> bool {
    if tiles.iter().filter(|tile| **tile == win_tile).count() != 2 {
        return false;
    }
    let Some(mut counts) = counts_after_removing(tiles, &[win_tile, win_tile]) else {
        return false;
    };
    can_form_sets(&mut counts)
}

fn is_closed_middle_wait(tiles: &[i32], win_tile: i32) -> bool {
    is_suited_tile(win_tile)
        && same_suit(win_tile - 1, win_tile)
        && same_suit(win_tile + 1, win_tile)
        && counts_after_removing(tiles, &[win_tile - 1, win_tile, win_tile + 1])
            .is_some_and(|counts| can_form_sets_with_one_pair(&counts))
}

fn is_edge_wait(tiles: &[i32], win_tile: i32) -> bool {
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
            is_complete_win(&test, melds.len())
        })
        .collect::<Vec<_>>();
    waits.len() == 1 && waits[0] == win_tile
}

fn is_unique_complete_wait_with_rule(
    tiles: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_tile: i32,
    win_rule: i32,
) -> bool {
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
            is_complete_win_with_melds(&test, melds, win_rule)
        })
        .collect::<Vec<_>>();
    waits.len() == 1 && waits[0] == win_tile
}

pub fn satisfies_shenyang_basic_win(tiles: &[i32], melds: &[WsShenyangMahjongMeld]) -> bool {
    if is_seven_pairs_win(tiles) {
        return true;
    }
    let all_tiles = all_tiles_with_melds(tiles, melds);
    let is_pure_one_suit = is_pure_one_suit_tiles(&all_tiles);
    let has_triplet_or_dragon_pair = melds.iter().any(is_triplet_meld)
        || has_triplet_in_standard_decomposition(tiles)
        || has_dragon_pair_as_standard_pair(tiles);
    has_open_meld(melds)
        && (is_pure_one_suit
            || (has_three_suits(&all_tiles)
                && has_terminal_or_honor(&all_tiles)
                && has_triplet_or_dragon_pair))
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

pub fn win_rule_from_configs(configs: &HashMap<String, i32>) -> i32 {
    match configs.get("win_rule").copied().unwrap_or(WIN_RULE_RELAXED) {
        WIN_RULE_SHENYANG_BASIC => WIN_RULE_SHENYANG_BASIC,
        _ => WIN_RULE_RELAXED,
    }
}

#[cfg(test)]
mod tests {
    use share_type_public::games::shenyang_mahjong::{
        ShenyangMahjongMeldKind, WsShenyangMahjongMeld,
    };

    use super::{
        WIN_RULE_RELAXED, WIN_RULE_SHENYANG_BASIC, can_chi, can_concealed_gang, can_gang, can_peng,
        has_triplet_in_standard_decomposition, is_complete_win, is_complete_win_with_melds,
        is_piao_hu_win, is_pure_one_suit_win, is_seven_pairs_win, is_single_wait_shape,
        is_single_wait_shape_with_rule, is_standard_win, is_unique_complete_wait, is_win,
        satisfies_shenyang_basic_win,
    };

    #[test]
    fn chi_requires_real_sequence() {
        let hand = vec![1, 2, 4, 5, 6];
        assert!(can_chi(&hand, 3, &[1, 2]));
        assert!(can_chi(&hand, 3, &[2, 4]));
        assert!(!can_chi(&hand, 3, &[4, 6]));
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
    fn concealed_gang_requires_four_copies() {
        let hand = vec![31, 31, 31, 31, 32, 33];
        assert!(can_concealed_gang(&hand, 31));
        assert!(!can_concealed_gang(&hand, 32));
    }

    #[test]
    fn gang_requires_three_copies_for_discard_claim() {
        let hand = vec![31, 31, 31, 32, 33];
        assert!(can_gang(&hand, 31));
        assert!(!can_gang(&hand, 32));
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
    fn peng_requires_two_copies() {
        let hand = vec![31, 31, 32, 33];
        assert!(can_peng(&hand, 31));
        assert!(!can_peng(&hand, 32));
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
    fn pure_one_suit_rejects_honor_tiles() {
        let tiles = vec![1, 2, 3, 4, 5, 6, 7, 7, 7, 31, 31];
        let melds = vec![meld(ShenyangMahjongMeldKind::CHI, vec![2, 3, 4], Some(1))];

        assert!(!is_pure_one_suit_win(&tiles, &melds));
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
    fn shenyang_basic_accepts_dragon_pair_without_triplet() {
        let tiles = vec![11, 12, 13, 21, 22, 23, 4, 5, 6, 35, 35];
        let melds = vec![meld(ShenyangMahjongMeldKind::CHI, vec![1, 2, 3], Some(1))];

        assert!(satisfies_shenyang_basic_win(&tiles, &melds));
    }

    #[test]
    fn shenyang_basic_accepts_open_triplet_with_three_suits_and_terminal() {
        let tiles = vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35];
        let melds = vec![meld(ShenyangMahjongMeldKind::CHI, vec![1, 2, 3], Some(1))];

        assert!(satisfies_shenyang_basic_win(&tiles, &melds));
    }

    #[test]
    fn shenyang_basic_accepts_pure_one_suit_without_three_suits() {
        let tiles = vec![1, 2, 3, 4, 5, 6, 7, 7, 7, 9, 9];
        let melds = vec![meld(ShenyangMahjongMeldKind::CHI, vec![2, 3, 4], Some(1))];

        assert!(is_pure_one_suit_win(&tiles, &melds));
        assert!(satisfies_shenyang_basic_win(&tiles, &melds));
    }

    #[test]
    fn shenyang_basic_accepts_open_pure_one_suit_without_terminal_or_triplet() {
        let tiles = vec![3, 4, 5, 4, 5, 6, 5, 6, 7, 8, 8];
        let melds = vec![meld(ShenyangMahjongMeldKind::CHI, vec![2, 3, 4], Some(1))];

        assert!(is_pure_one_suit_win(&tiles, &melds));
        assert!(!has_triplet_in_standard_decomposition(&tiles));
        assert!(satisfies_shenyang_basic_win(&tiles, &melds));
    }

    #[test]
    fn shenyang_basic_rejects_non_dragon_pair_without_triplet() {
        let tiles = vec![11, 12, 13, 21, 22, 23, 4, 5, 6, 8, 8];
        let melds = vec![meld(ShenyangMahjongMeldKind::CHI, vec![1, 2, 3], Some(1))];

        assert!(!satisfies_shenyang_basic_win(&tiles, &melds));
    }

    #[test]
    fn shenyang_basic_rejects_open_all_simples_standard_without_terminal() {
        let tiles = vec![2, 3, 4, 12, 13, 14, 22, 23, 24, 6, 6];
        let melds = vec![meld(ShenyangMahjongMeldKind::PENG, vec![5, 5, 5], Some(1))];

        assert!(is_complete_win_with_melds(&tiles, &melds, WIN_RULE_RELAXED));
        assert!(!satisfies_shenyang_basic_win(&tiles, &melds));
    }

    #[test]
    fn shenyang_basic_rejects_sequence_reuse_as_fake_triplet() {
        let tiles = vec![1, 2, 2, 3, 3, 3, 4, 4, 5, 26, 26];
        let melds = vec![meld(
            ShenyangMahjongMeldKind::CHI,
            vec![11, 12, 13],
            Some(1),
        )];

        assert!(is_complete_win_with_melds(&tiles, &melds, WIN_RULE_RELAXED));
        assert!(!satisfies_shenyang_basic_win(&tiles, &melds));
    }

    #[test]
    fn shenyang_basic_accepts_concealed_triplet_decomposition() {
        let tiles = vec![1, 1, 1, 2, 3, 4, 21, 22, 23, 8, 8];
        let melds = vec![meld(
            ShenyangMahjongMeldKind::CHI,
            vec![11, 12, 13],
            Some(1),
        )];

        assert!(satisfies_shenyang_basic_win(&tiles, &melds));
    }

    #[test]
    fn shenyang_basic_rejects_open_win_without_three_suits() {
        let tiles = vec![11, 12, 13, 11, 12, 13, 31, 31, 31, 35, 35];
        let melds = vec![meld(ShenyangMahjongMeldKind::CHI, vec![1, 2, 3], Some(1))];

        assert!(!satisfies_shenyang_basic_win(&tiles, &melds));
    }

    #[test]
    fn shenyang_basic_rejects_pure_one_suit_without_open_meld() {
        let tiles = vec![1, 2, 3, 2, 3, 4, 4, 5, 6, 7, 7, 7, 9, 9];

        assert!(is_pure_one_suit_win(&tiles, &[]));
        assert!(!satisfies_shenyang_basic_win(&tiles, &[]));
    }

    #[test]
    fn shenyang_basic_rejects_concealed_gang_as_open_meld() {
        let tiles = vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35];
        let melds = vec![meld(ShenyangMahjongMeldKind::GANG, vec![1, 1, 1, 1], None)];

        assert!(is_complete_win_with_melds(&tiles, &melds, WIN_RULE_RELAXED));
        assert!(!satisfies_shenyang_basic_win(&tiles, &melds));
    }

    #[test]
    fn shenyang_basic_rejects_standard_win_without_open_meld() {
        let tiles = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35];

        assert!(!satisfies_shenyang_basic_win(&tiles, &[]));
    }

    #[test]
    fn shenyang_basic_seven_pairs_ignores_basic_requirements() {
        let tiles = vec![2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8];

        assert!(is_seven_pairs_win(&tiles));
        assert!(satisfies_shenyang_basic_win(&tiles, &[]));
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
    fn single_wait_shape_accepts_unique_terminal_wait() {
        let tiles = vec![1, 1, 1, 13, 14, 15, 16, 17, 17, 17, 17, 18, 18, 19];

        assert!(is_standard_win(&tiles));
        assert!(is_unique_complete_wait(&tiles, &[], 1));
        assert!(is_single_wait_shape(&tiles, &[], 1));
    }

    #[test]
    fn single_wait_shape_accepts_pair_wait() {
        let tiles = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 35, 35];

        assert!(is_single_wait_shape(&tiles, &[], 35));
    }

    #[test]
    fn single_wait_shape_with_rule_rejects_closed_standard_basic_win() {
        let tiles = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35];

        assert!(is_single_wait_shape_with_rule(
            &tiles,
            &[],
            35,
            WIN_RULE_RELAXED
        ));
        assert!(!is_single_wait_shape_with_rule(
            &tiles,
            &[],
            35,
            WIN_RULE_SHENYANG_BASIC
        ));
    }

    #[test]
    fn single_wait_shape_accepts_pair_wait_with_honor_triplet() {
        let tiles = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35];

        assert!(is_single_wait_shape(&tiles, &[], 35));
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
    fn single_wait_shape_rejects_closed_middle_shape_with_multiple_waits() {
        let tiles = vec![6, 7, 7, 8, 8, 9, 11, 12, 13, 15, 15, 15, 22, 22];

        assert!(is_standard_win(&tiles));
        assert!(!is_single_wait_shape(&tiles, &[], 8));
    }

    #[test]
    fn standard_hand_is_recognized() {
        let tiles = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35];
        assert!(is_standard_win(&tiles));
        assert!(is_win(&tiles));
    }
}
