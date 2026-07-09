use share_type_public::games::shenyang_mahjong::SHENYANG_MAHJONG_TILE_KINDS;

pub(super) fn is_dragon(tile: i32) -> bool {
    matches!(tile, 35..=37)
}

pub(super) fn is_honor(tile: i32) -> bool {
    matches!(tile, 31..=37)
}

pub(super) fn is_suited(tile: i32) -> bool {
    matches!(tile, 1..=9 | 11..=19 | 21..=29)
}

pub(super) fn is_valid_tile(tile: i32) -> bool {
    SHENYANG_MAHJONG_TILE_KINDS.contains(&tile)
}

pub(super) fn is_wind(tile: i32) -> bool {
    matches!(tile, 31..=34)
}

pub(super) fn tile_is_terminal(tile: i32) -> bool {
    is_suited(tile) && matches!(tile_rank(tile), 1 | 9)
}

pub(super) fn tile_rank(tile: i32) -> i32 {
    tile % 10
}

pub(super) fn tile_suit(tile: i32) -> i32 {
    tile / 10
}

pub(super) fn unique_tiles(hand: &[i32]) -> Vec<i32> {
    let mut tiles = hand
        .iter()
        .copied()
        .filter(|tile| is_valid_tile(*tile))
        .collect::<Vec<_>>();
    tiles.sort_unstable();
    tiles.dedup();
    tiles
}
