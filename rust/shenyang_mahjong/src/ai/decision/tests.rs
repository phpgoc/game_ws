mod claim;
mod defense;
mod discard;
mod misc;
mod piao;
mod pure_one_suit;
mod score;
mod self_gang;
mod seven_pairs;
mod shenyang_rule;
mod xi_gang;

use std::collections::HashMap;

use super::*;
use crate::ai::observation::{AiClaimView, AiSeatView};
use crate::rules::{WIN_RULE_RELAXED, WIN_RULE_SHENYANG_BASIC};

fn dead_basic_heng_discards(hand: &[i32]) -> Vec<i32> {
    let mut counts = HashMap::<i32, usize>::new();
    for tile in hand.iter().copied() {
        *counts.entry(tile).or_default() += 1;
    }

    let mut discards = SHENYANG_MAHJONG_TILE_KINDS
        .into_iter()
        .flat_map(|tile| {
            let count = counts.get(&tile).copied().unwrap_or(0);
            let visible = if is_dragon(tile) && count < 2 {
                3
            } else if !is_dragon(tile) && count < 3 {
                2
            } else {
                0
            };
            std::iter::repeat_n(tile, visible)
        })
        .collect::<Vec<_>>();
    sort_tiles(&mut discards);
    discards
}

fn dead_terminal_or_honor_discards() -> Vec<i32> {
    SHENYANG_MAHJONG_TILE_KINDS
        .into_iter()
        .filter(|tile| is_honor(*tile) || tile_is_terminal(*tile))
        .flat_map(|tile| std::iter::repeat_n(tile, 4))
        .collect()
}

fn table_with_discards(position: usize, discards: Vec<i32>) -> AiPublicTable {
    let mut seats = HashMap::new();
    seats.insert(
        0,
        AiSeatView {
            position: 0,
            hand_count: 14,
            discards: Vec::new(),
            melds: Vec::new(),
        },
    );
    seats.insert(
        position,
        AiSeatView {
            position,
            hand_count: 10,
            discards,
            melds: Vec::new(),
        },
    );
    AiPublicTable {
        current_position: 0,
        dealer_position: 1,
        wall_count: 60,
        max_fan: None,
        allow_first_chi: true,
        claim_window: None,
        seats,
    }
}

fn test_chi_meld(start_tile: i32) -> WsShenyangMahjongMeld {
    WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::CHI,
        tiles: vec![start_tile, start_tile + 1, start_tile + 2],
        from_position: Some(1),
    }
}

fn test_concealed_gang_meld(tile: i32) -> WsShenyangMahjongMeld {
    WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::GANG,
        tiles: vec![tile, tile, tile, tile],
        from_position: None,
    }
}

fn test_gang_meld(tile: i32) -> WsShenyangMahjongMeld {
    WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::GANG,
        tiles: vec![tile, tile, tile, tile],
        from_position: Some(1),
    }
}

fn test_peng_meld(tile: i32) -> WsShenyangMahjongMeld {
    WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![tile, tile, tile],
        from_position: Some(1),
    }
}
