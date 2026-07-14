use std::collections::{HashMap, HashSet};

use share_type_public::games::shenyang_mahjong::WsShenyangMahjongMeld;

use crate::game::public_discards_for_position;
use crate::game_state::{ShenyangMahjongLoopState, meld_source_is_valid_for_positions};
use crate::rules::is_valid_meld;

#[derive(Debug, Clone)]
pub struct AiClaimView {
    pub tile: i32,
    pub from_position: usize,
    pub eligible_positions: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct AiPublicTable {
    pub current_position: usize,
    pub dealer_position: usize,
    pub wall_count: usize,
    pub max_fan: Option<i32>,
    pub allow_chi: bool,
    pub chi_opens_door: bool,
    pub claim_window: Option<AiClaimView>,
    pub seats: HashMap<usize, AiSeatView>,
}

#[derive(Debug, Clone)]
pub struct AiSeatView {
    pub position: usize,
    pub hand_count: usize,
    pub discards: Vec<i32>,
    pub melds: Vec<WsShenyangMahjongMeld>,
}

pub fn build_public_table_with_configs(
    state: &ShenyangMahjongLoopState,
    configs: &HashMap<String, i32>,
) -> AiPublicTable {
    let players = state.players_snapshot();
    let player_positions = players.keys().copied().collect::<HashSet<_>>();
    let mut seats = HashMap::new();
    for (position, _) in players {
        seats.insert(
            position,
            AiSeatView {
                position,
                hand_count: state
                    .hands
                    .get(&position)
                    .map(|hand| hand.len())
                    .unwrap_or(0),
                discards: public_discards_for_position(state, position),
                melds: state
                    .melds
                    .get(&position)
                    .into_iter()
                    .flatten()
                    .filter(|meld| {
                        meld_source_is_valid_for_positions(meld, position, &player_positions)
                    })
                    .filter(|meld| is_valid_meld(meld))
                    .cloned()
                    .collect(),
            },
        );
    }

    let claim_window = state.claim_window.as_ref().map(|window| AiClaimView {
        tile: window.tile,
        from_position: window.from_position,
        eligible_positions: window.eligible_positions.clone(),
    });

    AiPublicTable {
        current_position: state.current_position,
        dealer_position: state.dealer_position,
        wall_count: state.wall_count(),
        max_fan: configs.get("max_fan").copied().filter(|fan| *fan > 0),
        allow_chi: configs.get("allow_chi").copied().unwrap_or(1) == 1,
        chi_opens_door: configs.get("chi_opens_door").copied().unwrap_or(1) == 1,
        claim_window,
        seats,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use share_type_public::games::shenyang_mahjong::{
        ShenyangMahjongMeldKind, WsShenyangMahjongMeld,
    };
    use ws_common::CommonGameState;

    use super::*;

    #[test]
    fn public_table_filters_melds_with_invalid_source_positions() {
        let base = Arc::new(Mutex::new(CommonGameState::default()));
        {
            let mut common = base.lock().unwrap();
            for position in 0..4 {
                common.add_player(position, position as u64 + 1, &format!("P{position}"));
            }
        }
        let mut state = ShenyangMahjongLoopState::new(base);
        state.melds.insert(
            1,
            vec![
                test_meld(ShenyangMahjongMeldKind::PENG, 3, Some(0)),
                test_meld(ShenyangMahjongMeldKind::GANG, 4, None),
                test_meld(ShenyangMahjongMeldKind::PENG, 5, Some(1)),
                test_meld(ShenyangMahjongMeldKind::PENG, 6, None),
                test_meld(ShenyangMahjongMeldKind::PENG, 7, Some(-1)),
                test_meld(ShenyangMahjongMeldKind::PENG, 8, Some(9)),
                test_meld(ShenyangMahjongMeldKind::CHI, 11, Some(0)),
                test_meld(ShenyangMahjongMeldKind::CHI, 21, Some(2)),
            ],
        );

        let table = build_public_table_with_configs(&state, &HashMap::new());
        let melds = &table.seats.get(&1).expect("seat 1").melds;

        assert_eq!(melds.len(), 3);
        assert_eq!(melds[0].from_position, Some(0));
        assert_eq!(melds[1].from_position, None);
        assert_eq!(melds[2].from_position, Some(0));
    }

    #[test]
    fn public_table_filters_malformed_meld_shapes() {
        let base = Arc::new(Mutex::new(CommonGameState::default()));
        {
            let mut common = base.lock().unwrap();
            for position in 0..4 {
                common.add_player(position, position as u64 + 1, &format!("P{position}"));
            }
        }
        let mut state = ShenyangMahjongLoopState::new(base);
        state.melds.insert(
            1,
            vec![
                WsShenyangMahjongMeld {
                    kind: ShenyangMahjongMeldKind::PENG,
                    tiles: vec![3, 3],
                    from_position: Some(0),
                },
                WsShenyangMahjongMeld {
                    kind: ShenyangMahjongMeldKind::CHI,
                    tiles: vec![11, 11, 12],
                    from_position: Some(0),
                },
                test_meld(ShenyangMahjongMeldKind::PENG, 4, Some(2)),
            ],
        );

        let table = build_public_table_with_configs(&state, &HashMap::new());
        let melds = &table.seats.get(&1).expect("seat 1").melds;

        assert_eq!(melds.len(), 1);
        assert_eq!(melds[0].tiles, vec![4, 4, 4]);
    }

    #[test]
    fn public_table_filters_invalid_discards() {
        let base = Arc::new(Mutex::new(CommonGameState::default()));
        {
            let mut common = base.lock().unwrap();
            for position in 0..4 {
                common.add_player(position, position as u64 + 1, &format!("P{position}"));
            }
        }
        let mut state = ShenyangMahjongLoopState::new(base);
        state.discards.insert(1, vec![3, 99, 35, -1]);

        let table = build_public_table_with_configs(&state, &HashMap::new());

        assert_eq!(table.seats.get(&1).expect("seat 1").discards, vec![3, 35]);
    }

    fn test_meld(
        kind: ShenyangMahjongMeldKind,
        tile: i32,
        from_position: Option<i32>,
    ) -> WsShenyangMahjongMeld {
        let tiles = match kind {
            ShenyangMahjongMeldKind::GANG => vec![tile; 4],
            ShenyangMahjongMeldKind::PENG => vec![tile; 3],
            ShenyangMahjongMeldKind::CHI => vec![tile, tile + 1, tile + 2],
        };
        WsShenyangMahjongMeld {
            kind,
            tiles,
            from_position,
        }
    }
}
