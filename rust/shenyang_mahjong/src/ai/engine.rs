use std::collections::HashMap;

use ws_common::{Dispatch, RoomService};

use crate::game::{
    can_self_draw_hu_with_configs, can_self_gang, perform_discard, perform_self_draw_hu,
    perform_self_gang, resolve_claim_window,
};
use crate::game_state::{ClaimResponse, ClaimWindowKind, ShenyangMahjongLoopState};
use crate::rules::win_rule_from_configs;

use super::decision::{
    AiClaimChoice, choose_claim_from_view, choose_discard_from_view, choose_self_gang_from_view,
};
use super::observation::build_public_table_with_configs;

fn choose_self_gang_tile(
    state: &ShenyangMahjongLoopState,
    configs: &HashMap<String, i32>,
    position: usize,
    hand: &[i32],
) -> Option<i32> {
    let mut tiles = hand.to_vec();
    tiles.sort_unstable();
    tiles.dedup();
    let candidate_tiles = tiles
        .into_iter()
        .filter(|tile| can_self_gang(state, position, *tile))
        .collect::<Vec<_>>();
    let table = build_public_table_with_configs(state, configs);
    choose_self_gang_from_view(
        hand,
        &candidate_tiles,
        &table,
        position,
        win_rule_from_configs(configs),
    )
}

pub fn maybe_play_ai_turn(
    room_service: &RoomService,
    room_key: &str,
    state: &mut ShenyangMahjongLoopState,
    configs: &HashMap<String, i32>,
    dispatch: &mut Dispatch,
) -> bool {
    if state.phase != share_type_public::games::shenyang_mahjong::ShenyangMahjongPhase::Play
        || state.claim_window.is_some()
    {
        return false;
    }
    let position = state.current_position;
    if !state.is_ai_controlled_position(position) {
        return false;
    }

    let Some(hand) = self_hand(state, position) else {
        return false;
    };
    if hand.is_empty() {
        return false;
    }
    if can_self_draw_hu_with_configs(state, position, configs) {
        perform_self_draw_hu(room_service, room_key, state, configs, dispatch, position);
        return true;
    }

    if let Some(tile) = choose_self_gang_tile(state, configs, position, &hand) {
        return perform_self_gang(
            room_service,
            room_key,
            state,
            configs,
            dispatch,
            position,
            tile,
        );
    }

    let table = build_public_table_with_configs(state, configs);
    if let Some(tile) =
        choose_discard_from_view(&hand, &table, position, win_rule_from_configs(configs))
    {
        return perform_discard(
            room_service,
            room_key,
            state,
            configs,
            dispatch,
            position,
            tile,
        );
    }
    false
}

pub fn maybe_resolve_ai_claims(
    room_service: &RoomService,
    room_key: &str,
    state: &mut ShenyangMahjongLoopState,
    configs: &HashMap<String, i32>,
    dispatch: &mut Dispatch,
) -> bool {
    let Some(claim_window) = state.claim_window.clone() else {
        return false;
    };
    let is_rob_gang = matches!(claim_window.kind, ClaimWindowKind::RobGang);
    let table = build_public_table_with_configs(state, configs);
    let Some(claim) = table.claim_window.as_ref() else {
        return false;
    };

    let mut changed = false;
    for position in claim_window.eligible_positions {
        if claim_window.responses.contains_key(&position)
            || !state.is_ai_controlled_position(position)
        {
            continue;
        }
        let Some(hand) = self_hand(state, position) else {
            continue;
        };
        let choice = choose_claim_from_view(
            &hand,
            claim,
            &table,
            position,
            win_rule_from_configs(configs),
        )
        .unwrap_or(AiClaimChoice::Pass);
        let response = match choice {
            AiClaimChoice::Hu => ClaimResponse::Hu,
            AiClaimChoice::Gang if !is_rob_gang => ClaimResponse::Gang,
            AiClaimChoice::Peng if !is_rob_gang => ClaimResponse::Peng,
            AiClaimChoice::Chi { consume_tiles } if !is_rob_gang => {
                ClaimResponse::Chi { consume_tiles }
            }
            AiClaimChoice::Pass => ClaimResponse::Pass,
            _ => ClaimResponse::Pass,
        };
        if let Some(current) = state.claim_window.as_mut() {
            current.responses.insert(position, response);
            changed = true;
        }
    }

    if changed {
        let all_received = state
            .claim_window
            .as_ref()
            .map(|window| {
                window
                    .eligible_positions
                    .iter()
                    .all(|item| window.responses.contains_key(item))
            })
            .unwrap_or(false);
        if all_received {
            resolve_claim_window(room_service, room_key, state, configs, dispatch);
            return true;
        }
    }
    false
}

fn self_hand(state: &ShenyangMahjongLoopState, position: usize) -> Option<Vec<i32>> {
    state.hands.get(&position).cloned()
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use share_type_public::games::shenyang_mahjong::{
        ShenyangMahjongMeldKind, ShenyangMahjongPhase, WsShenyangMahjongMeld,
    };
    use ws_common::game_state::CommonGameState;

    use super::*;
    use crate::game_state::ClaimWindowState;
    use crate::rules::WIN_RULE_RELAXED;

    fn relaxed_configs() -> HashMap<String, i32> {
        HashMap::from([("win_rule".to_owned(), WIN_RULE_RELAXED)])
    }

    #[test]
    fn away_position_does_not_self_draw_without_drawn_tile() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_away(0);
        state
            .hands
            .insert(0, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35]);
        let mut dispatch = Dispatch::default();

        assert!(!maybe_play_ai_turn(
            &RoomService::default(),
            "room",
            &mut state,
            &HashMap::new(),
            &mut dispatch,
        ));

        assert_eq!(state.phase, ShenyangMahjongPhase::Play);
        assert!(state.discards.get(&0).unwrap().is_empty());
        assert!(state.settlement.is_none());
    }

    #[test]
    fn away_position_uses_ai_claim_response() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_away(0);
        state.current_position = 1;
        state
            .hands
            .insert(0, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35]);
        state.discards.insert(1, vec![35]);
        state.claim_window = Some(ClaimWindowState {
            tile: 35,
            from_position: 1,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![0],
            responses: HashMap::new(),
        });
        let mut dispatch = Dispatch::default();

        assert!(maybe_resolve_ai_claims(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
        ));

        assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
        assert_eq!(
            state
                .settlement
                .as_ref()
                .map(|settlement| settlement.winner_positions.clone()),
            Some(vec![0]),
        );
    }

    #[test]
    fn away_position_uses_ai_discard() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_away(0);
        let mut dispatch = Dispatch::default();

        assert!(maybe_play_ai_turn(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
        ));

        assert_eq!(state.hands.get(&0).unwrap().len(), 13);
        assert_eq!(state.discards.get(&0).unwrap().len(), 1);
    }

    #[test]
    fn away_position_uses_ai_gang_claim_response() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_away(0);
        state.current_position = 1;
        state
            .hands
            .insert(0, vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 35, 35, 35]);
        state.discards.insert(1, vec![35]);
        state.wall = vec![37];
        state.claim_window = Some(ClaimWindowState {
            tile: 35,
            from_position: 1,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![0],
            responses: HashMap::new(),
        });
        let mut dispatch = Dispatch::default();

        assert!(maybe_resolve_ai_claims(
            &RoomService::default(),
            "room",
            &mut state,
            &HashMap::new(),
            &mut dispatch,
        ));

        assert!(state.claim_window.is_none());
        assert_eq!(state.current_position, 0);
        assert_eq!(state.hands.get(&0).unwrap().len(), 11);
        assert!(state.hands.get(&0).unwrap().contains(&37));
        assert_eq!(
            state.melds.get(&0).unwrap().first().unwrap().tiles,
            vec![35, 35, 35, 35]
        );
    }

    #[test]
    fn away_position_uses_ai_self_draw_for_seven_pairs() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_away(0);
        state
            .hands
            .insert(0, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35]);
        state.last_drawn_tile = Some(35);
        let mut dispatch = Dispatch::default();

        assert!(maybe_play_ai_turn(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
        ));

        assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
        assert_eq!(
            state
                .settlement
                .as_ref()
                .map(|settlement| settlement.winner_positions.clone()),
            Some(vec![0]),
        );
        assert!(state.discards.get(&0).unwrap().is_empty());
    }

    #[test]
    fn away_position_uses_ai_self_draw_for_open_basic_pure_one_suit() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_away(0);
        state.hands.insert(0, vec![3, 4, 5, 4, 5, 6, 5, 6, 7, 8, 8]);
        state.melds.insert(
            0,
            vec![WsShenyangMahjongMeld {
                kind: ShenyangMahjongMeldKind::CHI,
                tiles: vec![2, 3, 4],
                from_position: Some(1),
            }],
        );
        state.last_drawn_tile = Some(8);
        let mut dispatch = Dispatch::default();

        assert!(maybe_play_ai_turn(
            &RoomService::default(),
            "room",
            &mut state,
            &HashMap::new(),
            &mut dispatch,
        ));

        assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
        assert_eq!(
            state
                .settlement
                .as_ref()
                .map(|settlement| settlement.winner_positions.clone()),
            Some(vec![0]),
        );
        assert!(state.discards.get(&0).unwrap().is_empty());
    }

    #[test]
    fn away_position_does_not_self_draw_closed_basic_pure_one_suit() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_away(0);
        state
            .hands
            .insert(0, vec![1, 2, 3, 2, 3, 4, 4, 5, 6, 7, 7, 7, 9, 9]);
        state.last_drawn_tile = Some(9);
        let mut dispatch = Dispatch::default();

        assert!(maybe_play_ai_turn(
            &RoomService::default(),
            "room",
            &mut state,
            &HashMap::new(),
            &mut dispatch,
        ));

        assert_eq!(state.phase, ShenyangMahjongPhase::Play);
        assert!(state.settlement.is_none());
        assert_eq!(state.hands.get(&0).unwrap().len(), 13);
        assert_eq!(state.discards.get(&0).unwrap().len(), 1);
    }

    #[test]
    fn away_position_uses_ai_self_gang_before_discard() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_away(0);
        state
            .hands
            .insert(0, vec![4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35, 35, 35, 35]);
        state.last_drawn_tile = Some(35);
        state.wall = vec![37];
        let mut dispatch = Dispatch::default();

        assert!(maybe_play_ai_turn(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
        ));

        assert_eq!(state.current_position, 0);
        assert_eq!(state.last_drawn_tile, Some(37));
        assert!(state.hands.get(&0).unwrap().contains(&37));
        assert_eq!(
            state.melds.get(&0).unwrap().first().unwrap().tiles,
            vec![35, 35, 35, 35]
        );
        assert!(state.discards.get(&0).unwrap().is_empty());
    }

    #[test]
    fn disconnected_position_uses_ai_discard() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_disconnected(0);
        let mut dispatch = Dispatch::default();

        assert!(maybe_play_ai_turn(
            &RoomService::default(),
            "room",
            &mut state,
            &HashMap::new(),
            &mut dispatch,
        ));

        assert_eq!(state.hands.get(&0).unwrap().len(), 13);
        assert_eq!(state.discards.get(&0).unwrap().len(), 1);
    }

    #[test]
    fn four_ai_positions_can_finish_seeded_round_with_win() {
        let mut state = seeded_ai_round_state(2026070402);
        let room_service = RoomService::default();
        let configs = HashMap::new();
        let mut dispatch = Dispatch::default();

        for step in 0..220 {
            if state.phase == ShenyangMahjongPhase::Settlement {
                break;
            }

            let acted =
                maybe_resolve_ai_claims(&room_service, "room", &mut state, &configs, &mut dispatch)
                    || maybe_play_ai_turn(
                        &room_service,
                        "room",
                        &mut state,
                        &configs,
                        &mut dispatch,
                    );

            assert!(
                acted,
                "AI round stalled at step {step}, phase={:?}, current_position={}, wall={}, claim_window={:?}",
                state.phase,
                state.current_position,
                state.wall_count(),
                state.claim_window
            );
        }

        let settlement = state.settlement.as_ref().expect("AI round settlement");
        let total_discards = state.discards.values().map(Vec::len).sum::<usize>();

        assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
        assert!(
            !settlement.winner_positions.is_empty(),
            "seeded AI round should end with a winning hand"
        );
        assert!(settlement.win_tile.is_some());
        assert!(total_discards > 0);
    }

    fn playable_state() -> ShenyangMahjongLoopState {
        let base = Arc::new(Mutex::new(CommonGameState::default()));
        {
            let mut common = base.lock().unwrap();
            for position in 0..4 {
                common.add_player(position, position as u64 + 1, &format!("P{}", position));
            }
        }
        let mut state = ShenyangMahjongLoopState::new(base);
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        state.dealer_position = 0;
        state
            .hands
            .insert(0, vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state
                .melds
                .insert(position, Vec::<WsShenyangMahjongMeld>::new());
        }
        state.wall = vec![37, 36, 35, 34, 33, 32];
        state
    }

    fn seeded_ai_round_state(seed: u64) -> ShenyangMahjongLoopState {
        let base = Arc::new(Mutex::new(CommonGameState::default()));
        {
            let mut common = base.lock().unwrap();
            for position in 0..4 {
                common.add_player(position, position as u64 + 1, &format!("AI {position}"));
                common.mark_ai_position(position);
            }
        }
        let mut state = ShenyangMahjongLoopState::new(base);
        state.set_wall_seed_base_for_test(Some(seed));
        state.deal_new_round();
        state
    }
}
