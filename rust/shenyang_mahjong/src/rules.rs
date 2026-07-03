use std::collections::HashMap;

use share_type_public::games::shenyang_mahjong::SHENYANG_MAHJONG_TILE_KINDS;
use share_type_public::games::shenyang_mahjong::WsShenyangMahjongMeld;

pub const WIN_RULE_RELAXED: i32 = 0;
pub const WIN_RULE_SHENYANG_BASIC: i32 = 1;

pub fn win_rule_from_configs(configs: &HashMap<String, i32>) -> i32 {
    match configs.get("win_rule").copied().unwrap_or(WIN_RULE_RELAXED) {
        WIN_RULE_SHENYANG_BASIC => WIN_RULE_SHENYANG_BASIC,
        _ => WIN_RULE_RELAXED,
    }
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

pub fn can_concealed_gang(hand: &[i32], target_tile: i32) -> bool {
    hand.iter().filter(|&&tile| tile == target_tile).count() >= 4
}

pub fn can_peng(hand: &[i32], target_tile: i32) -> bool {
    hand.iter().filter(|&&tile| tile == target_tile).count() >= 2
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

pub fn is_win(tiles: &[i32]) -> bool {
    is_standard_win(tiles) || is_seven_pairs_win(tiles)
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

pub fn satisfies_shenyang_basic_win(tiles: &[i32], melds: &[WsShenyangMahjongMeld]) -> bool {
    if melds.is_empty() {
        return is_seven_pairs_win(tiles) && has_three_suits(tiles) && has_terminal_or_honor(tiles);
    }
    let all_tiles = all_tiles_with_melds(tiles, melds);
    has_three_suits(&all_tiles)
        && has_open_meld(melds)
        && has_terminal_or_honor(&all_tiles)
        && (has_triplet_or_quad(&all_tiles) || has_dragon_pair_as_standard_pair(tiles))
}

fn is_suited_tile(tile: i32) -> bool {
    matches!(tile, 1..=9 | 11..=19 | 21..=29)
}

fn all_tiles_with_melds(tiles: &[i32], melds: &[WsShenyangMahjongMeld]) -> Vec<i32> {
    let mut all_tiles = tiles.to_vec();
    for meld in melds {
        all_tiles.extend(meld.tiles.iter().copied());
    }
    all_tiles
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

fn has_triplet_or_quad(tiles: &[i32]) -> bool {
    let counts = tile_counts(tiles);
    SHENYANG_MAHJONG_TILE_KINDS
        .into_iter()
        .any(|tile| counts[tile as usize] >= 3)
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

fn tile_counts(tiles: &[i32]) -> [u8; 38] {
    let mut counts = [0u8; 38];
    for &tile in tiles {
        if is_valid_tile(tile) {
            counts[tile as usize] += 1;
        }
    }
    counts
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

pub fn sort_tiles(hand: &mut [i32]) {
    hand.sort_unstable();
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

#[cfg(test)]
mod tests {
    use share_type_public::games::shenyang_mahjong::{
        ShenyangMahjongMeldKind, WsShenyangMahjongMeld,
    };

    use super::{
        can_chi, can_concealed_gang, can_gang, can_peng, is_complete_win, is_seven_pairs_win,
        is_standard_win, is_win, satisfies_shenyang_basic_win,
    };

    #[test]
    fn chi_requires_real_sequence() {
        let hand = vec![1, 2, 4, 5, 6];
        assert!(can_chi(&hand, 3, &[1, 2]));
        assert!(can_chi(&hand, 3, &[2, 4]));
        assert!(!can_chi(&hand, 3, &[4, 6]));
    }

    #[test]
    fn gang_requires_three_copies_for_discard_claim() {
        let hand = vec![31, 31, 31, 32, 33];
        assert!(can_gang(&hand, 31));
        assert!(!can_gang(&hand, 32));
    }

    #[test]
    fn concealed_gang_requires_four_copies() {
        let hand = vec![31, 31, 31, 31, 32, 33];
        assert!(can_concealed_gang(&hand, 31));
        assert!(!can_concealed_gang(&hand, 32));
    }

    #[test]
    fn peng_requires_two_copies() {
        let hand = vec![31, 31, 32, 33];
        assert!(can_peng(&hand, 31));
        assert!(!can_peng(&hand, 32));
    }

    #[test]
    fn standard_hand_is_recognized() {
        let tiles = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35];
        assert!(is_standard_win(&tiles));
        assert!(is_win(&tiles));
    }

    #[test]
    fn seven_pairs_hand_is_recognized() {
        let tiles = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 31, 31];
        assert!(is_seven_pairs_win(&tiles));
        assert!(is_win(&tiles));
    }

    #[test]
    fn seven_pairs_allows_four_of_a_kind_as_two_pairs() {
        let tiles = vec![1, 1, 1, 1, 11, 11, 12, 12, 21, 21, 22, 22, 31, 31];
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
    fn shenyang_basic_accepts_open_triplet_with_three_suits_and_terminal() {
        let tiles = vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35];
        let melds = vec![meld(ShenyangMahjongMeldKind::CHI, vec![1, 2, 3], Some(1))];

        assert!(satisfies_shenyang_basic_win(&tiles, &melds));
    }

    #[test]
    fn shenyang_basic_rejects_standard_win_without_open_meld() {
        let tiles = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35];

        assert!(!satisfies_shenyang_basic_win(&tiles, &[]));
    }

    #[test]
    fn shenyang_basic_rejects_open_win_without_three_suits() {
        let tiles = vec![11, 12, 13, 11, 12, 13, 31, 31, 31, 35, 35];
        let melds = vec![meld(ShenyangMahjongMeldKind::CHI, vec![1, 2, 3], Some(1))];

        assert!(!satisfies_shenyang_basic_win(&tiles, &melds));
    }

    #[test]
    fn shenyang_basic_accepts_dragon_pair_without_triplet() {
        let tiles = vec![11, 12, 13, 21, 22, 23, 4, 5, 6, 35, 35];
        let melds = vec![meld(ShenyangMahjongMeldKind::CHI, vec![1, 2, 3], Some(1))];

        assert!(satisfies_shenyang_basic_win(&tiles, &melds));
    }

    #[test]
    fn shenyang_basic_rejects_non_dragon_pair_without_triplet() {
        let tiles = vec![11, 12, 13, 21, 22, 23, 4, 5, 6, 8, 8];
        let melds = vec![meld(ShenyangMahjongMeldKind::CHI, vec![1, 2, 3], Some(1))];

        assert!(!satisfies_shenyang_basic_win(&tiles, &melds));
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
}
