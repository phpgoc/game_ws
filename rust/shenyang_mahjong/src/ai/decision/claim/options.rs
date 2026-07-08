use super::*;

pub(in crate::ai::decision) fn chi_options(hand: &[i32], tile: i32) -> Vec<Vec<i32>> {
    let mut options = Vec::new();
    for consume_tiles in [
        [tile - 2, tile - 1],
        [tile - 1, tile + 1],
        [tile + 1, tile + 2],
    ] {
        if !can_chi(hand, tile, &consume_tiles) {
            continue;
        }
        options.push(consume_tiles.to_vec());
    }
    options
}
