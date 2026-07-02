use share_type_public::games::shenyang_mahjong::SHENYANG_MAHJONG_TILE_KINDS;

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

fn is_suited_tile(tile: i32) -> bool {
    matches!(tile, 1..=9 | 11..=19 | 21..=29)
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
    use super::{can_chi, can_gang, can_peng, is_standard_win};

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
    fn peng_requires_two_copies() {
        let hand = vec![31, 31, 32, 33];
        assert!(can_peng(&hand, 31));
        assert!(!can_peng(&hand, 32));
    }

    #[test]
    fn standard_hand_is_recognized() {
        let tiles = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35];
        assert!(is_standard_win(&tiles));
    }
}
