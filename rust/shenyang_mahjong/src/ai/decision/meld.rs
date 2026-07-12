use share_type_public::games::shenyang_mahjong::{ShenyangMahjongMeldKind, WsShenyangMahjongMeld};

use super::tile::{is_suited, is_valid_tile, tile_suit};

pub(super) fn claim_gang_meld(tile: i32, from_position: usize) -> WsShenyangMahjongMeld {
    WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::GANG,
        tiles: vec![tile, tile, tile, tile],
        from_position: Some(from_position as i32),
    }
}

pub(super) fn claim_peng_meld(tile: i32, from_position: usize) -> WsShenyangMahjongMeld {
    WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![tile, tile, tile],
        from_position: Some(from_position as i32),
    }
}

pub(super) fn has_concealed_gang_meld(melds: &[WsShenyangMahjongMeld]) -> bool {
    melds
        .iter()
        .filter(|meld| meld.kind == ShenyangMahjongMeldKind::GANG && meld.from_position.is_none())
        .any(is_triplet_like_meld)
}

pub(super) fn has_open_meld(melds: &[WsShenyangMahjongMeld]) -> bool {
    melds.iter().any(is_open_meld)
}

pub(super) fn has_peng_meld(melds: &[WsShenyangMahjongMeld], tile: i32) -> bool {
    melds.iter().any(|meld| {
        meld.kind == ShenyangMahjongMeldKind::PENG && meld_primary_tile(meld) == Some(tile)
    })
}

pub(super) fn is_open_meld(meld: &WsShenyangMahjongMeld) -> bool {
    meld.from_position.is_some() && is_valid_meld(meld)
}

pub(super) fn is_sequence_meld(meld: &WsShenyangMahjongMeld) -> bool {
    if meld.kind != ShenyangMahjongMeldKind::CHI || meld.tiles.len() != 3 {
        return false;
    }
    let mut tiles = meld.tiles.clone();
    tiles.sort_unstable();
    let [a, b, c] = [tiles[0], tiles[1], tiles[2]];
    is_suited(a)
        && tile_suit(a) == tile_suit(b)
        && tile_suit(a) == tile_suit(c)
        && a + 1 == b
        && b + 1 == c
}

pub(super) fn is_triplet_like_meld(meld: &WsShenyangMahjongMeld) -> bool {
    meld_primary_tile(meld).is_some()
}

pub(super) fn is_valid_meld(meld: &WsShenyangMahjongMeld) -> bool {
    is_triplet_like_meld(meld) || is_sequence_meld(meld)
}

pub(super) fn valid_meld_count(melds: &[WsShenyangMahjongMeld]) -> usize {
    melds.iter().filter(|meld| is_valid_meld(meld)).count()
}

pub(super) fn meld_primary_tile(meld: &WsShenyangMahjongMeld) -> Option<i32> {
    let expected_len = match meld.kind {
        ShenyangMahjongMeldKind::PENG => 3,
        ShenyangMahjongMeldKind::GANG => 4,
        ShenyangMahjongMeldKind::CHI => return None,
    };
    if meld.tiles.len() != expected_len {
        return None;
    }
    let first = *meld.tiles.first()?;
    meld.tiles
        .iter()
        .all(|tile| *tile == first)
        .then_some(first)
        .filter(|tile| is_valid_tile(*tile))
}

pub(super) fn promoted_added_gang_melds(
    melds: &[WsShenyangMahjongMeld],
    tile: i32,
) -> Vec<WsShenyangMahjongMeld> {
    let mut next_melds = melds.to_vec();
    if let Some(meld) = next_melds.iter_mut().find(|meld| {
        meld.kind == ShenyangMahjongMeldKind::PENG && meld_primary_tile(meld) == Some(tile)
    }) {
        meld.kind = ShenyangMahjongMeldKind::GANG;
        meld.tiles = vec![tile, tile, tile, tile];
    }
    next_melds
}

pub(super) fn valid_meld_tiles(melds: &[WsShenyangMahjongMeld]) -> impl Iterator<Item = i32> + '_ {
    melds
        .iter()
        .filter(|meld| is_valid_meld(meld))
        .flat_map(|meld| meld.tiles.iter().copied())
}
