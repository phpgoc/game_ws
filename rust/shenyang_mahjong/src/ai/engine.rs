use std::collections::HashMap;

use ws_common::{Dispatch, RoomService};

use super::decision::{
    AiClaimChoice, choose_claim_from_view, choose_discard_from_view,
    choose_forced_discard_from_view, choose_self_gang_from_view, choose_xi_gang_from_view,
    claim_known_tile_counts_are_possible, is_complete_win_for_table,
    should_pass_self_draw_hu_from_view,
};
use super::observation::{AiClaimView, AiPublicTable, build_public_table_with_configs};
use crate::game::{
    can_declare_xi_gang, can_self_draw_hu_with_configs, can_self_gang, claim_window_matches_source,
    perform_discard, perform_self_draw_hu, perform_self_gang, perform_xi_gang,
    resolve_claim_window,
};
use crate::game_state::{ClaimResponse, ClaimWindowKind, ShenyangMahjongLoopState};

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
    choose_self_gang_from_view(hand, &candidate_tiles, &table, position)
}

fn claim_hu_is_complete(
    hand: &[i32],
    claim: &AiClaimView,
    table: &AiPublicTable,
    position: usize,
) -> bool {
    let mut win_hand = hand.to_vec();
    win_hand.push(claim.tile);
    win_hand.sort_unstable();
    let melds = table
        .seats
        .get(&position)
        .map(|seat| seat.melds.as_slice())
        .unwrap_or(&[]);
    claim_known_tile_counts_are_possible(hand, melds, claim, table)
        && is_complete_win_for_table(&win_hand, melds, table)
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
    let mut passed_self_draw_tile = None;
    if can_self_draw_hu_with_configs(state, position, configs) {
        let table = build_public_table_with_configs(state, configs);
        if let Some(win_tile) = state.last_drawn_tile
            && should_pass_self_draw_hu_from_view(&hand, &table, position, win_tile)
        {
            passed_self_draw_tile = Some(win_tile);
        } else {
            perform_self_draw_hu(room_service, room_key, state, configs, dispatch, position);
            return true;
        }
    }

    if state.is_ting(position) {
        return state.last_drawn_tile.is_some_and(|tile| {
            perform_discard(
                room_service,
                room_key,
                state,
                configs,
                dispatch,
                position,
                tile,
            )
        });
    }

    let xi_gang_options = state.xi_gang_options_for_position(position);
    if !xi_gang_options.is_empty() {
        let table = build_public_table_with_configs(state, configs);
        if let Some(tiles) = choose_xi_gang_from_view(&hand, &xi_gang_options, &table, position)
            && can_declare_xi_gang(state, position, &tiles)
        {
            return perform_xi_gang(
                room_service,
                room_key,
                state,
                configs,
                dispatch,
                position,
                &tiles,
            );
        }
    }
    if let Some(win_tile) = passed_self_draw_tile {
        if perform_discard(
            room_service,
            room_key,
            state,
            configs,
            dispatch,
            position,
            win_tile,
        ) {
            return true;
        }
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
    let discard = choose_discard_from_view(&hand, &table, position)
        .or_else(|| choose_forced_discard_from_view(&hand, &table, position));
    if let Some(tile) = discard {
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
    if state.phase != share_type_public::games::shenyang_mahjong::ShenyangMahjongPhase::Play {
        return false;
    }
    let Some(claim_window) = state.claim_window.clone() else {
        return false;
    };
    let is_rob_gang = matches!(claim_window.kind, ClaimWindowKind::RobGang);
    let Some(claim) = build_public_table_with_configs(state, configs).claim_window else {
        return false;
    };
    let claim_matches_source = claim_window_matches_source(state, &claim_window);

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
        let table = build_public_table_with_configs(state, configs);
        let choice = if !claim_matches_source {
            AiClaimChoice::Pass
        } else if state.is_ting(position) {
            match choose_claim_from_view(&hand, &claim, &table, position) {
                Some(AiClaimChoice::Hu)
                    if claim_hu_is_complete(&hand, &claim, &table, position) =>
                {
                    AiClaimChoice::Hu
                }
                _ => AiClaimChoice::Pass,
            }
        } else {
            choose_claim_from_view(&hand, &claim, &table, position).unwrap_or(AiClaimChoice::Pass)
        };
        let response = match choice {
            AiClaimChoice::Hu => ClaimResponse::Hu,
            AiClaimChoice::Gang if !is_rob_gang => ClaimResponse::Gang,
            AiClaimChoice::Peng if !is_rob_gang => ClaimResponse::Peng,
            AiClaimChoice::Chi { consume_tiles } if !is_rob_gang => {
                ClaimResponse::Chi { consume_tiles }
            }
            AiClaimChoice::Pass => ClaimResponse::AiPass,
            _ => ClaimResponse::AiPass,
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
        return true;
    }
    false
}

fn self_hand(state: &ShenyangMahjongLoopState, position: usize) -> Option<Vec<i32>> {
    state.hands.get(&position).cloned()
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use share_type_public::WsCode;
    use share_type_public::games::shenyang_mahjong::{
        SHENYANG_MAHJONG_TILE_KINDS, ShenyangMahjongMeldKind, ShenyangMahjongPhase,
        ShenyangMahjongWinPattern, WsShenyangMahjongMeld, WsShenyangMahjongScoreChange,
    };
    use ws_common::{CommonGameState, OutboundPayload};

    use super::*;
    use crate::game::build_settlement_event_with_configs;
    use crate::game_state::ClaimWindowState;
    use crate::rules::{
        ShenyangMahjongWinContext, is_complete_win_with_melds,
        is_complete_win_with_melds_with_context,
    };

    #[test]
    fn ai_claim_passes_invalid_rob_gang_source() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_ai_position(0);
        state.current_position = 1;
        state
            .hands
            .insert(0, vec![14, 15, 17, 18, 19, 21, 22, 23, 35, 35]);
        state.melds.insert(0, vec![test_peng_meld(1)]);
        state
            .hands
            .insert(1, vec![2, 5, 8, 11, 14, 16, 17, 21, 31, 32, 33]);
        state.melds.insert(1, vec![test_peng_meld_from(16, 2)]);
        state.last_drawn_tile = Some(17);
        state.claim_window = Some(ClaimWindowState {
            tile: 16,
            from_position: 1,
            kind: ClaimWindowKind::RobGang,
            eligible_positions: vec![0, 2],
            responses: HashMap::new(),
        });
        let configs = HashMap::new();
        let mut dispatch = Dispatch::default();

        assert!(maybe_resolve_ai_claims(
            &RoomService::default(),
            "room",
            &mut state,
            &configs,
            &mut dispatch,
        ));

        let claim_window = state
            .claim_window
            .as_ref()
            .expect("claim window stays open");
        assert!(matches!(
            claim_window.responses.get(&0),
            Some(ClaimResponse::AiPass)
        ));
        assert!(!claim_window.responses.contains_key(&2));
        assert!(dispatch.messages.is_empty());
    }

    #[test]
    fn ai_claim_passes_mismatched_discard_source() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_ai_position(0);
        state.current_position = 1;
        state
            .hands
            .insert(0, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35]);
        state.discards.insert(1, vec![36]);
        state.claim_window = Some(ClaimWindowState {
            tile: 35,
            from_position: 1,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![0, 2],
            responses: HashMap::new(),
        });
        let mut dispatch = Dispatch::default();

        assert!(maybe_resolve_ai_claims(
            &RoomService::default(),
            "room",
            &mut state,
            &default_configs(),
            &mut dispatch,
        ));

        let claim_window = state
            .claim_window
            .as_ref()
            .expect("claim window stays open");
        assert!(matches!(
            claim_window.responses.get(&0),
            Some(ClaimResponse::AiPass)
        ));
        assert!(!claim_window.responses.contains_key(&2));
        assert!(dispatch.messages.is_empty());
    }

    #[test]
    fn ai_claim_response_rejects_outside_play_phase() {
        let mut state = playable_state();
        state.phase = ShenyangMahjongPhase::Settlement;
        state.base.lock().unwrap().mark_ai_position(0);
        state.current_position = 2;
        state.discards.insert(2, vec![35]);
        state.claim_window = Some(ClaimWindowState {
            tile: 35,
            from_position: 2,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![0, 1],
            responses: HashMap::new(),
        });
        let mut dispatch = Dispatch::default();

        assert!(!maybe_resolve_ai_claims(
            &RoomService::default(),
            "room",
            &mut state,
            &default_configs(),
            &mut dispatch,
        ));

        assert!(
            state
                .claim_window
                .as_ref()
                .is_some_and(|window| window.responses.is_empty())
        );
        assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
        assert!(dispatch.messages.is_empty());
    }

    #[test]
    fn ai_claim_response_reports_progress_while_waiting_for_human() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_ai_position(0);
        state.current_position = 2;
        state
            .hands
            .insert(0, vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
        state
            .hands
            .insert(1, vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
        state.discards.insert(2, vec![35]);
        state.claim_window = Some(ClaimWindowState {
            tile: 35,
            from_position: 2,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![0, 1],
            responses: HashMap::new(),
        });
        let mut dispatch = Dispatch::default();

        assert!(maybe_resolve_ai_claims(
            &RoomService::default(),
            "room",
            &mut state,
            &default_configs(),
            &mut dispatch,
        ));

        let claim_window = state.claim_window.as_ref().expect("claim window");
        assert!(matches!(
            claim_window.responses.get(&0),
            Some(ClaimResponse::AiPass)
        ));
        assert!(!claim_window.responses.contains_key(&1));
        assert_eq!(state.phase, ShenyangMahjongPhase::Play);
    }

    #[test]
    fn ai_claim_hu_joins_human_winner_without_cutting_off_multi_hu() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_ai_position(2);
        state.hands.insert(0, Vec::new());
        state
            .hands
            .insert(1, vec![1, 2, 11, 12, 13, 21, 22, 23, 35, 35]);
        state
            .hands
            .insert(2, vec![1, 2, 14, 15, 16, 24, 25, 26, 35, 35]);
        state.hands.insert(3, Vec::new());
        state.melds.insert(1, vec![test_peng_meld_from(31, 3)]);
        state.melds.insert(2, vec![test_peng_meld_from(32, 3)]);
        state.discards.insert(0, vec![3]);
        state.wall = vec![34];
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1, 2],
            responses: HashMap::from([(1, ClaimResponse::Hu)]),
        });
        let mut dispatch = Dispatch::default();

        assert!(maybe_resolve_ai_claims(
            &RoomService::default(),
            "room",
            &mut state,
            &default_configs(),
            &mut dispatch,
        ));

        let settlement = state.settlement.as_ref().expect("settlement");
        assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
        assert_eq!(settlement.winner_positions, vec![1, 2]);
        assert_eq!(settlement.from_position, Some(0));
        assert_eq!(settlement.win_tile, Some(3));
        assert_eq!(state.discards.get(&0), Some(&Vec::<i32>::new()));
    }

    #[test]
    fn ai_claim_hu_does_not_chase_cap_after_human_hu() {
        let mut state = capped_multi_hu_claim_state();
        state.base.lock().unwrap().mark_ai_position(0);
        state
            .claim_window
            .as_mut()
            .unwrap()
            .responses
            .insert(2, ClaimResponse::Hu);
        let configs = HashMap::from([("max_fan".to_owned(), 4)]);
        let mut dispatch = Dispatch::default();

        assert!(maybe_resolve_ai_claims(
            &RoomService::default(),
            "room",
            &mut state,
            &configs,
            &mut dispatch,
        ));

        let settlement = state.settlement.as_ref().expect("settlement");
        let mut winners = settlement.winner_positions.clone();
        winners.sort_unstable();
        assert_eq!(winners, vec![0, 2]);
        assert_eq!(settlement.from_position, Some(1));
        assert_eq!(settlement.win_tile, Some(16));
    }

    #[test]
    fn earlier_ai_claim_pass_joins_later_human_hu() {
        let mut state = capped_multi_hu_claim_state();
        state.base.lock().unwrap().mark_ai_position(0);
        let configs = HashMap::from([("max_fan".to_owned(), 4)]);
        let mut dispatch = Dispatch::default();

        assert!(maybe_resolve_ai_claims(
            &RoomService::default(),
            "room",
            &mut state,
            &configs,
            &mut dispatch,
        ));
        assert!(matches!(
            state
                .claim_window
                .as_ref()
                .and_then(|window| window.responses.get(&0)),
            Some(ClaimResponse::AiPass)
        ));

        state
            .claim_window
            .as_mut()
            .unwrap()
            .responses
            .insert(2, ClaimResponse::Hu);
        resolve_claim_window(
            &RoomService::default(),
            "room",
            &mut state,
            &configs,
            &mut dispatch,
        );

        let settlement = state.settlement.as_ref().expect("settlement");
        let mut winners = settlement.winner_positions.clone();
        winners.sort_unstable();
        assert_eq!(winners, vec![0, 2]);
        assert_eq!(settlement.from_position, Some(1));
        assert_eq!(settlement.win_tile, Some(16));
    }

    #[test]
    fn earlier_human_claim_pass_does_not_join_later_hu() {
        let mut state = capped_multi_hu_claim_state();
        state.base.lock().unwrap().mark_ai_position(0);
        state.claim_window.as_mut().unwrap().responses =
            HashMap::from([(0, ClaimResponse::Pass), (2, ClaimResponse::Hu)]);
        let mut dispatch = Dispatch::default();

        resolve_claim_window(
            &RoomService::default(),
            "room",
            &mut state,
            &HashMap::from([("max_fan".to_owned(), 4)]),
            &mut dispatch,
        );

        let settlement = state.settlement.as_ref().expect("settlement");
        assert_eq!(settlement.winner_positions, vec![2]);
    }

    #[test]
    fn earlier_ai_claim_pass_stays_pass_when_everyone_passes() {
        let mut state = capped_multi_hu_claim_state();
        state.base.lock().unwrap().mark_ai_position(0);
        let configs = HashMap::from([("max_fan".to_owned(), 4)]);
        let mut dispatch = Dispatch::default();

        assert!(maybe_resolve_ai_claims(
            &RoomService::default(),
            "room",
            &mut state,
            &configs,
            &mut dispatch,
        ));
        state
            .claim_window
            .as_mut()
            .unwrap()
            .responses
            .insert(2, ClaimResponse::Pass);
        resolve_claim_window(
            &RoomService::default(),
            "room",
            &mut state,
            &configs,
            &mut dispatch,
        );

        assert!(state.settlement.is_none());
        assert!(state.claim_window.is_none());
        assert_eq!(state.phase, ShenyangMahjongPhase::Play);
        assert_eq!(state.current_position, 2);
    }

    #[test]
    fn earlier_ai_claim_pass_does_not_join_invalid_hu_response() {
        let mut state = capped_multi_hu_claim_state();
        state.hands.insert(2, Vec::new());
        state.claim_window.as_mut().unwrap().responses =
            HashMap::from([(0, ClaimResponse::AiPass), (2, ClaimResponse::Hu)]);
        let mut dispatch = Dispatch::default();

        resolve_claim_window(
            &RoomService::default(),
            "room",
            &mut state,
            &HashMap::from([("max_fan".to_owned(), 4)]),
            &mut dispatch,
        );

        assert!(state.settlement.is_none());
        assert!(state.claim_window.is_none());
        assert_eq!(state.phase, ShenyangMahjongPhase::Play);
    }

    #[test]
    fn earlier_ai_claim_pass_joins_later_rob_gang_hu() {
        let mut state = playable_state();
        state.current_position = 1;
        state.dealer_position = 3;
        state
            .hands
            .insert(0, vec![2, 3, 11, 12, 13, 21, 22, 23, 35, 35]);
        state
            .hands
            .insert(1, vec![4, 5, 6, 7, 8, 11, 14, 16, 17, 31, 32]);
        state
            .hands
            .insert(2, vec![2, 3, 14, 15, 16, 24, 25, 26, 36, 36]);
        state.hands.insert(3, Vec::new());
        state.melds.insert(0, vec![test_peng_meld(9)]);
        state.melds.insert(1, vec![test_peng_meld_from(4, 2)]);
        state.melds.insert(2, vec![test_peng_meld_from(32, 3)]);
        state.wall = vec![37];
        state.last_drawn_tile = Some(4);
        state.claim_window = Some(ClaimWindowState {
            tile: 4,
            from_position: 1,
            kind: ClaimWindowKind::RobGang,
            eligible_positions: vec![0, 2],
            responses: HashMap::from([(0, ClaimResponse::AiPass), (2, ClaimResponse::Hu)]),
        });
        let mut dispatch = Dispatch::default();

        resolve_claim_window(
            &RoomService::default(),
            "room",
            &mut state,
            &HashMap::new(),
            &mut dispatch,
        );

        let settlement = state.settlement.as_ref().expect("settlement");
        let mut winners = settlement.winner_positions.clone();
        winners.sort_unstable();
        assert_eq!(winners, vec![0, 2]);
        assert!(settlement.is_reverse_win);
        assert_eq!(settlement.from_position, Some(1));
        assert_eq!(settlement.win_tile, Some(4));
        assert!(!state.hands.get(&1).unwrap().contains(&4));
    }

    #[test]
    fn later_ai_claim_hu_does_not_chase_cap_after_ai_hu() {
        let mut state = capped_multi_hu_claim_state();
        {
            let mut base = state.base.lock().unwrap();
            base.mark_ai_position(0);
            base.mark_ai_position(2);
        }
        state.declare_ting(2);
        state.claim_window.as_mut().unwrap().eligible_positions = vec![2, 0];
        let configs = HashMap::from([("max_fan".to_owned(), 4)]);
        let mut dispatch = Dispatch::default();

        assert!(maybe_resolve_ai_claims(
            &RoomService::default(),
            "room",
            &mut state,
            &configs,
            &mut dispatch,
        ));

        let settlement = state.settlement.as_ref().expect("settlement");
        let mut winners = settlement.winner_positions.clone();
        winners.sort_unstable();
        assert_eq!(winners, vec![0, 2]);
        assert_eq!(settlement.from_position, Some(1));
        assert_eq!(settlement.win_tile, Some(16));
    }

    #[test]
    fn ai_turn_does_not_declare_ting_when_bonus_is_enabled() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_ai_position(0);
        state
            .hands
            .insert(0, vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 12, 21, 21, 31, 32]);
        state.last_drawn_tile = Some(32);
        let configs = HashMap::from([("ting_fan".to_owned(), 1)]);
        let mut dispatch = Dispatch::default();

        assert!(crate::game::ting_discard_tiles_for_position(&state, 0, &configs).is_empty());
        assert!(maybe_play_ai_turn(
            &RoomService::default(),
            "room",
            &mut state,
            &configs,
            &mut dispatch,
        ));

        assert!(!state.is_ting(0));
        assert_eq!(state.discards.get(&0).map(Vec::len), Some(1));
    }

    #[test]
    fn ai_turn_actively_declares_available_xi_gang() {
        let mut state = playable_state();
        state.current_position = 1;
        state.base.lock().unwrap().mark_ai_position(1);
        state
            .hands
            .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 35, 36, 37]);
        state.melds.insert(1, Vec::new());
        state.discards.insert(1, Vec::new());
        state.last_drawn_tile = Some(37);
        state.xi_gang_options.insert(1, vec![vec![35, 36, 37]]);
        let mut dispatch = Dispatch::default();

        assert!(maybe_play_ai_turn(
            &RoomService::default(),
            "room",
            &mut state,
            &HashMap::new(),
            &mut dispatch,
        ));

        assert_eq!(state.melds.get(&1).unwrap().len(), 1);
        assert_eq!(
            state.melds.get(&1).unwrap()[0].kind,
            ShenyangMahjongMeldKind::XI_GANG
        );
        assert!(state.discards.get(&1).unwrap().is_empty());
    }

    #[test]
    fn ai_turn_last_draw_self_draws_after_dragon_xi_gang_as_haidilao() {
        let mut state = playable_state();
        state.current_position = 1;
        state.base.lock().unwrap().mark_ai_position(1);
        state
            .hands
            .insert(1, vec![1, 1, 1, 11, 11, 11, 21, 21, 21, 35, 35, 35, 36, 37]);
        state.melds.insert(1, Vec::new());
        state.discards.insert(1, Vec::new());
        state.last_drawn_tile = Some(37);
        state.wall.clear();
        state.xi_gang_options.insert(1, vec![vec![35, 36, 37]]);
        let configs = HashMap::new();
        let mut dispatch = Dispatch::default();

        assert!(maybe_play_ai_turn(
            &RoomService::default(),
            "room",
            &mut state,
            &configs,
            &mut dispatch,
        ));
        assert_eq!(state.phase, ShenyangMahjongPhase::Play);
        assert_eq!(state.melds.get(&1).unwrap().len(), 1);
        assert_eq!(
            state.melds.get(&1).unwrap()[0].kind,
            ShenyangMahjongMeldKind::XI_GANG
        );
        assert_eq!(state.last_drawn_tile, Some(37));

        assert!(maybe_play_ai_turn(
            &RoomService::default(),
            "room",
            &mut state,
            &configs,
            &mut dispatch,
        ));

        let settlement = state.settlement.as_ref().expect("settlement");
        assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
        assert_eq!(settlement.winner_positions, vec![1]);
        assert!(settlement.is_self_draw);
        assert!(settlement.is_haidilao);
        assert!(!settlement.is_gang_draw);
        assert!(state.discards.get(&1).unwrap().is_empty());
        let event = build_settlement_event_with_configs(&state, &configs)
            .expect("post-Xi-Gang settlement event");
        assert_eq!(
            event.winner_details[0].pattern,
            ShenyangMahjongWinPattern::PiaoHu
        );
        assert!(event.is_haidilao);
        assert!(event.winner_details[0].is_haidilao);
        assert!(!event.winner_details[0].is_gang_draw);
    }

    #[test]
    fn ai_turn_declares_both_xi_gangs_in_order() {
        let mut state = playable_state();
        state.current_position = 1;
        state.base.lock().unwrap().mark_ai_position(1);
        state
            .hands
            .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 31, 32, 33, 34, 35, 36, 37]);
        state.melds.insert(1, Vec::new());
        state.discards.insert(1, Vec::new());
        state.last_drawn_tile = Some(37);
        state.wall = vec![4];
        state
            .xi_gang_options
            .insert(1, vec![vec![31, 32, 33, 34], vec![35, 36, 37]]);
        let mut dispatch = Dispatch::default();

        assert!(maybe_play_ai_turn(
            &RoomService::default(),
            "room",
            &mut state,
            &HashMap::new(),
            &mut dispatch,
        ));
        assert_eq!(state.melds.get(&1).unwrap().len(), 1);
        assert_eq!(state.wall_count(), 0);
        assert_eq!(
            state.xi_gang_options_for_position(1),
            vec![vec![35, 36, 37]]
        );

        assert!(maybe_play_ai_turn(
            &RoomService::default(),
            "room",
            &mut state,
            &HashMap::new(),
            &mut dispatch,
        ));
        assert_eq!(state.melds.get(&1).unwrap().len(), 2);
        assert!(state.xi_gang_options_for_position(1).is_empty());
        assert!(state.discards.get(&1).unwrap().is_empty());

        assert!(maybe_play_ai_turn(
            &RoomService::default(),
            "room",
            &mut state,
            &HashMap::new(),
            &mut dispatch,
        ));
        assert!(state.hands.get(&1).unwrap().contains(&21));
        assert_eq!(state.discards.get(&1).unwrap().len(), 1);
        assert_ne!(state.discards.get(&1).unwrap()[0], 21);
    }

    #[test]
    fn ai_turn_preserves_multiple_dragon_pairs_after_wind_xi_gang() {
        let mut state = playable_state();
        state.current_position = 1;
        state.base.lock().unwrap().mark_ai_position(1);
        state
            .hands
            .insert(1, vec![1, 2, 3, 11, 21, 31, 32, 33, 34, 35, 35, 36, 36, 37]);
        state.melds.insert(1, Vec::new());
        state.discards.insert(1, Vec::new());
        state.last_drawn_tile = Some(37);
        state.wall = vec![22];
        state
            .xi_gang_options
            .insert(1, vec![vec![31, 32, 33, 34], vec![35, 36, 37]]);
        let mut dispatch = Dispatch::default();

        assert!(maybe_play_ai_turn(
            &RoomService::default(),
            "room",
            &mut state,
            &HashMap::new(),
            &mut dispatch,
        ));
        assert_eq!(state.melds.get(&1).unwrap().len(), 1);
        assert_eq!(
            state.xi_gang_options_for_position(1),
            vec![vec![35, 36, 37]]
        );
        assert!(state.discards.get(&1).unwrap().is_empty());

        assert!(maybe_play_ai_turn(
            &RoomService::default(),
            "room",
            &mut state,
            &HashMap::new(),
            &mut dispatch,
        ));
        assert_eq!(state.melds.get(&1).unwrap().len(), 1);
        assert!(state.xi_gang_options_for_position(1).is_empty());
        assert_eq!(state.discards.get(&1).unwrap().len(), 1);
    }

    #[test]
    fn ai_turn_hu_before_wind_xi_gang_when_seven_pairs_complete() {
        let mut state = playable_state();
        state.current_position = 1;
        state.base.lock().unwrap().mark_ai_position(1);
        state.hands.insert(
            1,
            vec![1, 1, 11, 11, 21, 21, 31, 31, 32, 32, 33, 33, 34, 34],
        );
        state.melds.insert(1, Vec::new());
        state.discards.insert(1, Vec::new());
        state.last_drawn_tile = Some(34);
        state.wall = vec![22];
        state.xi_gang_options.insert(1, vec![vec![31, 32, 33, 34]]);
        let mut dispatch = Dispatch::default();

        assert!(maybe_play_ai_turn(
            &RoomService::default(),
            "room",
            &mut state,
            &HashMap::new(),
            &mut dispatch,
        ));

        assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
        assert_eq!(state.settlement.as_ref().unwrap().winner_positions, vec![1]);
        assert!(state.melds.get(&1).unwrap().is_empty());
        assert_eq!(state.wall, vec![22]);
    }

    fn assert_seeded_settlement_event_is_consistent(
        state: &ShenyangMahjongLoopState,
        configs: &HashMap<String, i32>,
        seed: u64,
    ) {
        let settlement = state.settlement.as_ref().expect("AI round settlement");
        let event = build_settlement_event_with_configs(state, configs)
            .expect("AI round settlement event should be buildable");
        let winner_positions = settlement
            .winner_positions
            .iter()
            .map(|position| *position as i32)
            .collect::<Vec<_>>();

        assert_eq!(
            event.winner_positions, winner_positions,
            "seed {seed} settlement event should report the same winners as state"
        );
        assert_eq!(
            event.from_position,
            settlement.from_position.map(|position| position as i32),
            "seed {seed} settlement event should report the same payer as state"
        );
        assert_eq!(
            event.win_tile, settlement.win_tile,
            "seed {seed} settlement event should report the same win tile as state"
        );
        assert_eq!(
            event.is_self_draw, settlement.is_self_draw,
            "seed {seed} settlement event should report the same self-draw flag as state"
        );
        assert_eq!(
            event.players.len(),
            event.score_changes.len(),
            "seed {seed} settlement event should score every player snapshot"
        );
        assert_eq!(
            event
                .score_changes
                .iter()
                .map(|change| change.score)
                .sum::<i32>(),
            0,
            "seed {seed} settlement scores should be zero-sum: {:?}",
            event.score_changes
        );

        if settlement.winner_positions.is_empty() {
            assert!(
                event.winner_details.is_empty(),
                "seed {seed} draw settlement should not include winner details"
            );
            assert!(
                event.score_changes.iter().all(|change| change.score == 0),
                "seed {seed} draw settlement should score everyone as zero: {:?}",
                event.score_changes
            );
            return;
        }

        assert_eq!(
            event.winner_details.len(),
            settlement.winner_positions.len(),
            "seed {seed} should have one winner detail per winner"
        );
        for winner in &settlement.winner_positions {
            let winner_score = settlement_score_for_position(&event.score_changes, *winner);
            assert!(
                winner_score > 0,
                "seed {seed} winner {winner} should gain score: {:?}",
                event.score_changes
            );
            let detail = event
                .winner_details
                .iter()
                .find(|detail| detail.position == *winner as i32)
                .expect("winner detail should exist");
            assert_eq!(
                detail.score, winner_score,
                "seed {seed} winner detail score should match score_changes"
            );
            assert_eq!(detail.is_self_draw, settlement.is_self_draw);
            assert_eq!(detail.is_reverse_win, settlement.is_reverse_win);
            assert_eq!(detail.is_gang_draw, settlement.is_gang_draw);
            assert_eq!(detail.is_haidilao, settlement.is_haidilao);
        }

        if settlement.is_self_draw {
            assert!(
                settlement.from_position.is_none(),
                "seed {seed} self-draw settlement should not have a discard payer"
            );
            for change in &event.score_changes {
                let position = change.position as usize;
                if settlement.winner_positions.contains(&position) {
                    continue;
                }
                assert!(
                    change.score < 0,
                    "seed {seed} self-draw loser {position} should pay: {:?}",
                    event.score_changes
                );
            }
        } else {
            let from_position = settlement
                .from_position
                .expect("discard win settlement should have a payer");
            assert!(
                !settlement.winner_positions.contains(&from_position),
                "seed {seed} discard payer should not also be a winner"
            );
            assert!(
                settlement_score_for_position(&event.score_changes, from_position) < 0,
                "seed {seed} discard payer should lose score: {:?}",
                event.score_changes
            );
            for change in &event.score_changes {
                let position = change.position as usize;
                if settlement.winner_positions.contains(&position) || position == from_position {
                    continue;
                }
                assert_eq!(
                    change.score, 0,
                    "seed {seed} non-payer loser {position} should not pay on discard win"
                );
            }
        }
    }

    fn assert_seeded_settlement_winners_are_legal(
        state: &ShenyangMahjongLoopState,
        configs: &HashMap<String, i32>,
        seed: u64,
    ) {
        let settlement = state.settlement.as_ref().expect("AI round settlement");
        let context = ShenyangMahjongWinContext::from_configs(configs);
        for winner in &settlement.winner_positions {
            let mut hand = state.hands.get(winner).cloned().unwrap_or_default();
            if !settlement.is_self_draw
                && let Some(tile) = settlement.win_tile
            {
                hand.push(tile);
                hand.sort_unstable();
            }
            let melds = state.melds.get(winner).map(Vec::as_slice).unwrap_or(&[]);

            assert!(
                is_complete_win_with_melds_with_context(&hand, melds, context),
                "seed {seed} winner {winner} should have a legal Shenyang Mahjong hand: hand={hand:?}, melds={melds:?}, settlement={settlement:?}"
            );
        }
    }

    #[test]
    fn away_position_discards_complete_shape_after_claim() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_away(0);
        state.base.lock().unwrap().mark_ai_takeover_position(0);
        state
            .hands
            .insert(0, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 35, 35]);
        state.melds.insert(0, vec![test_peng_meld(31)]);
        state.last_drawn_tile = None;
        let mut dispatch = Dispatch::default();

        assert!(is_complete_win_with_melds(
            state.hands.get(&0).unwrap(),
            state.melds.get(&0).unwrap(),
        ));
        assert!(!can_self_draw_hu_with_configs(
            &state,
            0,
            &default_configs()
        ));
        assert!(maybe_play_ai_turn(
            &RoomService::default(),
            "room",
            &mut state,
            &default_configs(),
            &mut dispatch,
        ));

        assert_eq!(state.hands.get(&0).unwrap().len(), 10);
        assert_eq!(state.discards.get(&0).unwrap().len(), 1);
    }

    #[test]
    fn away_position_does_not_self_draw_with_unowned_drawn_tile() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_away(0);
        state.base.lock().unwrap().mark_ai_takeover_position(0);
        state
            .hands
            .insert(0, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35]);
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
        assert_eq!(state.discards.get(&0).unwrap().len(), 1);
        assert!(state.settlement.is_none());
    }

    #[test]
    fn away_position_does_not_self_draw_without_drawn_tile() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_away(0);
        state.base.lock().unwrap().mark_ai_takeover_position(0);
        state
            .hands
            .insert(0, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35]);
        let mut dispatch = Dispatch::default();

        assert!(maybe_play_ai_turn(
            &RoomService::default(),
            "room",
            &mut state,
            &HashMap::new(),
            &mut dispatch,
        ));

        assert_eq!(state.phase, ShenyangMahjongPhase::Play);
        assert_eq!(state.discards.get(&0).unwrap().len(), 1);
        assert!(state.settlement.is_none());
    }

    #[test]
    fn away_position_takes_self_draw_when_payment_modifiers_reach_cap() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_away(0);
        state.base.lock().unwrap().mark_ai_takeover_position(0);
        state.dealer_position = 1;
        state.wall = vec![37; 20];
        state.discards.insert(1, vec![16]);
        state
            .hands
            .insert(0, vec![13, 14, 15, 15, 16, 16, 16, 17, 28, 28, 28]);
        state.melds.insert(0, vec![test_peng_meld(1)]);
        state.last_drawn_tile = Some(16);
        let configs = HashMap::from([("max_fan".to_owned(), 4)]);
        let mut dispatch = Dispatch::default();

        assert!(maybe_play_ai_turn(
            &RoomService::default(),
            "room",
            &mut state,
            &configs,
            &mut dispatch,
        ));

        let settlement = state.settlement.as_ref().expect("settlement");
        assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
        assert_eq!(settlement.winner_positions, vec![0]);
        assert!(settlement.is_self_draw);
        assert!(state.discards.get(&0).unwrap().is_empty());
        assert_eq!(state.discards.get(&1).unwrap(), &vec![16]);
        let event = build_settlement_event_with_configs(&state, &configs)
            .expect("capped self-draw settlement event");
        assert_eq!(settlement_score_for_position(&event.score_changes, 0), 12);
        for position in 1..4 {
            assert_eq!(
                settlement_score_for_position(&event.score_changes, position),
                -4
            );
        }
    }

    #[test]
    fn away_position_self_draws_closed_basic_pure_one_suit() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_away(0);
        state.base.lock().unwrap().mark_ai_takeover_position(0);
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

        assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
        assert_eq!(
            state
                .settlement
                .as_ref()
                .map(|settlement| settlement.winner_positions.clone()),
            Some(vec![0])
        );
    }

    #[test]
    fn away_position_self_draws_configured_closed_sequence_after_xi_gang() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_away(0);
        state.base.lock().unwrap().mark_ai_takeover_position(0);
        state
            .hands
            .insert(0, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 35, 35]);
        state.melds.insert(
            0,
            vec![WsShenyangMahjongMeld {
                kind: ShenyangMahjongMeldKind::XI_GANG,
                tiles: vec![31, 32, 33, 34],
                from_position: None,
            }],
        );
        state.last_drawn_tile = Some(35);
        let default_configs = HashMap::new();
        let configs = HashMap::from([("allow_first_chi".to_owned(), 0)]);
        let mut dispatch = Dispatch::default();

        assert!(!can_self_draw_hu_with_configs(&state, 0, &default_configs));
        assert!(can_self_draw_hu_with_configs(&state, 0, &configs));
        assert!(maybe_play_ai_turn(
            &RoomService::default(),
            "room",
            &mut state,
            &configs,
            &mut dispatch,
        ));

        let settlement = state.settlement.as_ref().expect("settlement");
        assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
        assert_eq!(settlement.winner_positions, vec![0]);
        assert!(!settlement.is_gang_draw);
        assert!(state.discards.get(&0).unwrap().is_empty());
        assert_eq!(
            state.melds.get(&0).unwrap()[0].kind,
            ShenyangMahjongMeldKind::XI_GANG
        );
        let event = build_settlement_event_with_configs(&state, &configs)
            .expect("configured closed sequence settlement event");
        assert_eq!(
            event.winner_details[0].pattern,
            ShenyangMahjongWinPattern::Standard
        );
        assert!(event.winner_details[0].score > 0);
    }

    #[test]
    fn away_position_takes_gang_draw_self_draw_in_capped_room() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_away(0);
        state.base.lock().unwrap().mark_ai_takeover_position(0);
        state.dealer_position = 1;
        state.wall = vec![37; 48];
        state.discards.insert(1, vec![16]);
        state
            .hands
            .insert(0, vec![13, 14, 15, 15, 16, 16, 16, 17, 28, 28, 28]);
        state.melds.insert(
            0,
            vec![WsShenyangMahjongMeld {
                kind: ShenyangMahjongMeldKind::GANG,
                tiles: vec![1, 1, 1, 1],
                from_position: Some(1),
            }],
        );
        state.last_drawn_tile = Some(16);
        state.pending_gang_draw = true;
        let configs = HashMap::from([("max_fan".to_owned(), 4)]);
        let table = build_public_table_with_configs(&state, &configs);
        let mut dispatch = Dispatch::default();

        assert_eq!(table.current_self_draw_bonus_fan, 1);
        assert!(!should_pass_self_draw_hu_from_view(
            state.hands.get(&0).unwrap(),
            &table,
            0,
            16,
        ));
        assert!(maybe_play_ai_turn(
            &RoomService::default(),
            "room",
            &mut state,
            &configs,
            &mut dispatch,
        ));

        let settlement = state.settlement.as_ref().expect("settlement");
        assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
        assert_eq!(settlement.winner_positions, vec![0]);
        assert!(settlement.is_gang_draw);
        let event = build_settlement_event_with_configs(&state, &configs)
            .expect("settlement event should be buildable");
        assert!(event.winner_details[0].is_gang_draw);
    }

    #[test]
    fn away_position_takes_haidilao_self_draw_in_capped_room() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_away(0);
        state.base.lock().unwrap().mark_ai_takeover_position(0);
        state.dealer_position = 1;
        state.wall = Vec::new();
        state.discards.insert(1, vec![16]);
        state
            .hands
            .insert(0, vec![13, 14, 15, 15, 16, 16, 16, 17, 28, 28, 28]);
        state.melds.insert(0, vec![test_peng_meld(1)]);
        state.last_drawn_tile = Some(16);
        let configs = HashMap::from([("max_fan".to_owned(), 4)]);
        let table = build_public_table_with_configs(&state, &configs);
        let mut dispatch = Dispatch::default();

        assert_eq!(table.current_self_draw_bonus_fan, 1);
        assert!(!should_pass_self_draw_hu_from_view(
            state.hands.get(&0).unwrap(),
            &table,
            0,
            16,
        ));
        assert!(maybe_play_ai_turn(
            &RoomService::default(),
            "room",
            &mut state,
            &configs,
            &mut dispatch,
        ));

        let settlement = state.settlement.as_ref().expect("settlement");
        assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
        assert_eq!(settlement.winner_positions, vec![0]);
        assert!(!settlement.is_gang_draw);
        assert!(settlement.is_haidilao);
        let event = build_settlement_event_with_configs(&state, &configs)
            .expect("settlement event should be buildable");
        assert!(!event.winner_details[0].is_gang_draw);
        assert!(event.winner_details[0].is_haidilao);
    }

    #[test]
    fn away_position_takes_late_low_fan_self_draw_when_capped_wait_is_unlikely() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_away(0);
        state.base.lock().unwrap().mark_ai_takeover_position(0);
        state.dealer_position = 1;
        state.wall = vec![37; 4];
        state.discards.insert(1, vec![16]);
        state
            .hands
            .insert(0, vec![13, 14, 15, 15, 16, 16, 16, 17, 28, 28, 28]);
        state
            .hands
            .insert(1, vec![2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5]);
        state
            .hands
            .insert(2, vec![5, 5, 5, 6, 6, 6, 6, 7, 7, 7, 7, 8, 8]);
        state
            .hands
            .insert(3, vec![8, 8, 9, 9, 9, 9, 11, 11, 11, 11, 12, 12, 12]);
        state.melds.insert(0, vec![test_peng_meld(1)]);
        state.last_drawn_tile = Some(16);
        let configs = HashMap::from([("max_fan".to_owned(), 4)]);
        let mut dispatch = Dispatch::default();

        assert!(maybe_play_ai_turn(
            &RoomService::default(),
            "room",
            &mut state,
            &configs,
            &mut dispatch,
        ));

        let settlement = state.settlement.as_ref().expect("settlement");
        assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
        assert_eq!(settlement.winner_positions, vec![0]);
        assert_eq!(settlement.win_tile, Some(16));
        assert!(settlement.is_self_draw);
        assert!(state.discards.get(&0).unwrap().is_empty());
    }

    #[test]
    fn away_position_takes_low_fan_self_draw_without_full_wall_cycle() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_away(0);
        state.base.lock().unwrap().mark_ai_takeover_position(0);
        state.dealer_position = 1;
        state.wall = vec![37; 3];
        state.discards.insert(1, vec![16]);
        state
            .hands
            .insert(0, vec![13, 14, 15, 15, 16, 16, 16, 17, 28, 28, 28]);
        state.melds.insert(0, vec![test_peng_meld(1)]);
        state.last_drawn_tile = Some(16);
        let configs = HashMap::from([("max_fan".to_owned(), 4)]);
        let mut dispatch = Dispatch::default();

        assert!(maybe_play_ai_turn(
            &RoomService::default(),
            "room",
            &mut state,
            &configs,
            &mut dispatch,
        ));

        let settlement = state.settlement.as_ref().expect("settlement");
        assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
        assert_eq!(settlement.winner_positions, vec![0]);
        assert_eq!(settlement.win_tile, Some(16));
        assert!(settlement.is_self_draw);
        assert!(state.discards.get(&0).unwrap().is_empty());
    }

    #[test]
    fn away_position_takes_rob_gang_hu_in_capped_room() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_away(0);
        state.base.lock().unwrap().mark_ai_takeover_position(0);
        state.current_position = 1;
        state.dealer_position = 3;
        state
            .hands
            .insert(0, vec![2, 3, 11, 12, 13, 21, 22, 23, 35, 35]);
        state.melds.insert(0, vec![test_peng_meld(9)]);
        state
            .hands
            .insert(1, vec![4, 5, 6, 7, 8, 11, 14, 16, 17, 31, 32]);
        state.melds.insert(1, vec![test_peng_meld_from(4, 2)]);
        state.last_drawn_tile = Some(4);
        state.claim_window = Some(ClaimWindowState {
            tile: 4,
            from_position: 1,
            kind: ClaimWindowKind::RobGang,
            eligible_positions: vec![0],
            responses: HashMap::new(),
        });
        let configs = HashMap::from([("max_fan".to_owned(), 4)]);
        let table = build_public_table_with_configs(&state, &configs);
        let claim = table.claim_window.as_ref().expect("claim window");
        let mut dispatch = Dispatch::default();

        assert!(table.claim_is_rob_gang);
        assert!(claim_hu_is_complete(
            state.hands.get(&0).unwrap(),
            claim,
            &table,
            0,
        ));
        assert_eq!(
            choose_claim_from_view(state.hands.get(&0).unwrap(), claim, &table, 0),
            Some(AiClaimChoice::Hu)
        );
        assert!(maybe_resolve_ai_claims(
            &RoomService::default(),
            "room",
            &mut state,
            &configs,
            &mut dispatch,
        ));

        let settlement = state.settlement.as_ref().expect("settlement");
        assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
        assert_eq!(settlement.winner_positions, vec![0]);
        assert_eq!(settlement.win_tile, Some(4));
        assert!(settlement.is_reverse_win);
        let event = build_settlement_event_with_configs(&state, &configs)
            .expect("settlement event should be buildable");
        assert!(event.winner_details[0].is_reverse_win);
    }

    #[test]
    fn away_ting_position_takes_capped_discard_hu() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_away(0);
        state.base.lock().unwrap().mark_ai_takeover_position(0);
        state.current_position = 1;
        state.dealer_position = 3;
        state
            .hands
            .insert(0, vec![13, 14, 15, 15, 16, 16, 17, 28, 28, 28]);
        state.melds.insert(0, vec![test_peng_meld(1)]);
        state.discards.insert(0, vec![16]);
        state.melds.insert(1, vec![test_peng_meld_from(9, 2)]);
        state.discards.insert(1, vec![16]);
        state.wall = vec![
            2, 3, 4, 5, 6, 7, 8, 11, 12, 18, 19, 21, 23, 24, 25, 26, 27, 29, 31, 32,
        ];
        state.claim_window = Some(ClaimWindowState {
            tile: 16,
            from_position: 1,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![0],
            responses: HashMap::new(),
        });
        state.declare_ting(0);
        let configs = HashMap::from([("max_fan".to_owned(), 4), ("ting_fan".to_owned(), 1)]);
        let table = build_public_table_with_configs(&state, &configs);
        let claim = table.claim_window.as_ref().expect("claim window");
        let hand = state.hands.get(&0).unwrap();
        let mut dispatch = Dispatch::default();

        assert_eq!(
            choose_claim_from_view(hand, claim, &table, 0),
            Some(AiClaimChoice::Hu)
        );
        assert!(maybe_resolve_ai_claims(
            &RoomService::default(),
            "room",
            &mut state,
            &configs,
            &mut dispatch,
        ));

        let settlement = state.settlement.as_ref().expect("settlement");
        assert_eq!(settlement.winner_positions, vec![0]);
        let event = build_settlement_event_with_configs(&state, &configs)
            .expect("settlement event should be buildable");
        assert_eq!(settlement_score_for_position(&event.score_changes, 0), 4);
    }

    #[test]
    fn away_ting_position_passes_one_fan_short_discard_hu_for_live_capped_wait() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_away(0);
        state.base.lock().unwrap().mark_ai_takeover_position(0);
        state.current_position = 1;
        state.dealer_position = 3;
        state
            .hands
            .insert(0, vec![13, 14, 15, 15, 16, 16, 17, 28, 28, 28]);
        state.melds.insert(0, vec![test_peng_meld(1)]);
        state.discards.insert(0, vec![16]);
        state.melds.insert(1, vec![test_peng_meld_from(9, 2)]);
        state.discards.insert(1, vec![16]);
        state.wall = vec![
            2, 3, 4, 5, 6, 7, 8, 11, 12, 18, 19, 21, 23, 24, 25, 26, 27, 29, 31, 32,
        ];
        state.claim_window = Some(ClaimWindowState {
            tile: 16,
            from_position: 1,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![0],
            responses: HashMap::new(),
        });
        state.declare_ting(0);
        let configs = HashMap::from([("max_fan".to_owned(), 4), ("ting_fan".to_owned(), 0)]);
        let table = build_public_table_with_configs(&state, &configs);
        let claim = table.claim_window.as_ref().expect("claim window");
        let hand = state.hands.get(&0).unwrap();
        let mut dispatch = Dispatch::default();

        assert_eq!(
            choose_claim_from_view(hand, claim, &table, 0),
            Some(AiClaimChoice::Pass)
        );
        assert!(maybe_resolve_ai_claims(
            &RoomService::default(),
            "room",
            &mut state,
            &configs,
            &mut dispatch,
        ));

        assert!(state.settlement.is_none());
        assert!(state.is_ting(0));
        assert!(dispatch.messages.iter().all(|message| {
            !matches!(
                &message.payload,
                OutboundPayload::Event(event) if event.code == WsCode::GAME_OVER as i32
            )
        }));
    }

    #[test]
    fn away_ting_position_takes_self_draw_before_capped_wait_chase() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_away(0);
        state.base.lock().unwrap().mark_ai_takeover_position(0);
        state.dealer_position = 3;
        state
            .hands
            .insert(0, vec![13, 14, 15, 15, 16, 16, 16, 17, 28, 28, 28]);
        state.melds.insert(0, vec![test_peng_meld(1)]);
        state.melds.insert(3, vec![test_peng_meld_from(9, 1)]);
        state.discards.insert(1, vec![16]);
        state.wall = vec![
            2, 3, 4, 5, 6, 7, 8, 11, 12, 18, 19, 21, 23, 24, 25, 26, 27, 29, 31, 32,
        ];
        state.last_drawn_tile = Some(16);
        state.declare_ting(0);
        let configs = HashMap::from([("max_fan".to_owned(), 8), ("ting_fan".to_owned(), 1)]);
        let table = build_public_table_with_configs(&state, &configs);
        let hand = state.hands.get(&0).unwrap().clone();
        let mut dispatch = Dispatch::default();

        assert!(!should_pass_self_draw_hu_from_view(&hand, &table, 0, 16,));
        assert!(maybe_play_ai_turn(
            &RoomService::default(),
            "room",
            &mut state,
            &configs,
            &mut dispatch,
        ));

        let settlement = state.settlement.as_ref().expect("settlement");
        assert_eq!(settlement.winner_positions, vec![0]);
        assert!(settlement.is_self_draw);
        assert!(state.discards.get(&0).unwrap().is_empty());
        let event = build_settlement_event_with_configs(&state, &configs)
            .expect("ting self-draw settlement event");
        assert_eq!(settlement_score_for_position(&event.score_changes, 0), 24);
        for position in 1..4 {
            assert_eq!(
                settlement_score_for_position(&event.score_changes, position),
                -8
            );
        }
    }

    #[test]
    fn away_ting_position_discards_one_fan_short_self_draw_for_live_capped_wait() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_away(0);
        state.base.lock().unwrap().mark_ai_takeover_position(0);
        state.dealer_position = 3;
        state
            .hands
            .insert(0, vec![13, 14, 15, 15, 16, 16, 16, 17, 28, 28, 28]);
        state.melds.insert(0, vec![test_peng_meld(1)]);
        state.melds.insert(3, vec![test_peng_meld_from(9, 1)]);
        state.discards.insert(1, vec![16]);
        state.wall = vec![
            2, 3, 4, 5, 6, 7, 8, 11, 12, 18, 19, 21, 23, 24, 25, 26, 27, 29, 31, 32,
        ];
        state.last_drawn_tile = Some(16);
        state.declare_ting(0);
        let configs = HashMap::from([("max_fan".to_owned(), 8), ("ting_fan".to_owned(), 0)]);
        let table = build_public_table_with_configs(&state, &configs);
        let hand = state.hands.get(&0).unwrap().clone();
        let mut dispatch = Dispatch::default();

        assert!(should_pass_self_draw_hu_from_view(&hand, &table, 0, 16));
        assert!(maybe_play_ai_turn(
            &RoomService::default(),
            "room",
            &mut state,
            &configs,
            &mut dispatch,
        ));

        assert!(state.settlement.is_none());
        assert!(state.is_ting(0));
        assert_eq!(state.discards.get(&0).unwrap().last(), Some(&16));
        assert!(dispatch.messages.iter().all(|message| {
            !matches!(
                &message.payload,
                OutboundPayload::Event(event) if event.code == WsCode::GAME_OVER as i32
            )
        }));
    }

    #[test]
    fn away_position_uses_ai_claim_response() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_away(0);
        state.base.lock().unwrap().mark_ai_takeover_position(0);
        state.current_position = 1;
        state
            .hands
            .insert(0, vec![2, 3, 4, 11, 12, 13, 31, 31, 31, 35]);
        state.melds.insert(0, vec![test_peng_meld(21)]);
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
            &default_configs(),
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
        state.base.lock().unwrap().mark_ai_takeover_position(0);
        let mut dispatch = Dispatch::default();

        assert!(maybe_play_ai_turn(
            &RoomService::default(),
            "room",
            &mut state,
            &default_configs(),
            &mut dispatch,
        ));

        assert_eq!(state.hands.get(&0).unwrap().len(), 13);
        assert_eq!(state.discards.get(&0).unwrap().len(), 1);
    }

    #[test]
    fn away_position_uses_ai_gang_claim_response() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_away(0);
        state.base.lock().unwrap().mark_ai_takeover_position(0);
        state.current_position = 1;
        state
            .hands
            .insert(0, vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 35, 35, 35]);
        state.discards.insert(1, vec![35]);
        state.wall = SHENYANG_MAHJONG_TILE_KINDS
            .into_iter()
            .filter(|tile| !matches!(tile, 35 | 37))
            .take(23)
            .chain(std::iter::once(37))
            .collect();
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
    fn away_position_uses_ai_self_draw_for_open_basic_pure_one_suit() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_away(0);
        state.base.lock().unwrap().mark_ai_takeover_position(0);
        state.hands.insert(0, vec![3, 4, 5, 4, 5, 6, 5, 6, 7, 8, 8]);
        state.melds.insert(
            0,
            vec![WsShenyangMahjongMeld {
                kind: ShenyangMahjongMeldKind::CHI,
                tiles: vec![2, 3, 4],
                from_position: Some(3),
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
    fn away_position_uses_ai_self_draw_for_seven_pairs() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_away(0);
        state.base.lock().unwrap().mark_ai_takeover_position(0);
        state
            .hands
            .insert(0, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35]);
        state.last_drawn_tile = Some(35);
        let mut dispatch = Dispatch::default();

        assert!(maybe_play_ai_turn(
            &RoomService::default(),
            "room",
            &mut state,
            &default_configs(),
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
    fn away_position_uses_ai_self_gang_before_discard() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_away(0);
        state.base.lock().unwrap().mark_ai_takeover_position(0);
        state
            .hands
            .insert(0, vec![4, 5, 6, 11, 12, 13, 31, 35, 35, 35, 35]);
        state.melds.insert(0, vec![test_peng_meld(21)]);
        state.last_drawn_tile = Some(35);
        state.wall = vec![37; 24];
        let mut dispatch = Dispatch::default();

        assert!(maybe_play_ai_turn(
            &RoomService::default(),
            "room",
            &mut state,
            &default_configs(),
            &mut dispatch,
        ));

        assert_eq!(state.current_position, 0);
        assert_eq!(state.last_drawn_tile, Some(37));
        assert!(state.hands.get(&0).unwrap().contains(&37));
        assert!(
            state
                .melds
                .get(&0)
                .unwrap()
                .iter()
                .any(|meld| meld.tiles == vec![35, 35, 35, 35])
        );
        assert!(state.discards.get(&0).unwrap().is_empty());
    }

    #[test]
    fn disconnected_position_waits_for_timeout_fallback_without_takeover() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_disconnected(0);
        let mut dispatch = Dispatch::default();

        assert!(!maybe_play_ai_turn(
            &RoomService::default(),
            "room",
            &mut state,
            &HashMap::new(),
            &mut dispatch,
        ));

        assert_eq!(state.hands.get(&0).unwrap().len(), 14);
        assert!(state.discards.get(&0).unwrap().is_empty());
    }

    #[test]
    fn four_ai_positions_can_finish_seeded_round_with_win() {
        let state = run_seeded_ai_round(2026070402, 220);
        let settlement = state.settlement.as_ref().expect("AI round settlement");
        let total_discards = state.discards.values().map(Vec::len).sum::<usize>();

        assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
        assert!(
            !settlement.winner_positions.is_empty(),
            "seeded AI round should end with a winning hand"
        );
        assert!(settlement.win_tile.is_some());
        assert!(total_discards > 0);
        assert_seeded_settlement_winners_are_legal(&state, &HashMap::new(), 2026070402);
        assert_seeded_settlement_event_is_consistent(&state, &HashMap::new(), 2026070402);
    }

    #[test]
    fn four_ai_positions_settle_multiple_seeded_rounds() {
        for seed in [2026070403, 2026070404, 2026070405] {
            let state = run_seeded_ai_round(seed, 260);
            let settlement = state.settlement.as_ref().expect("AI round settlement");
            let total_discards = state.discards.values().map(Vec::len).sum::<usize>();

            assert_eq!(
                state.phase,
                ShenyangMahjongPhase::Settlement,
                "seed {seed} should settle"
            );
            assert!(
                settlement.is_self_draw
                    || settlement.from_position.is_some()
                    || settlement.winner_positions.is_empty(),
                "seed {seed} settlement should be a self draw, discard win, or legal draw"
            );
            assert!(
                total_discards > 0,
                "seed {seed} should play at least one discard"
            );
            assert_seeded_settlement_winners_are_legal(&state, &HashMap::new(), seed);
            assert_seeded_settlement_event_is_consistent(&state, &HashMap::new(), seed);
        }
    }

    #[test]
    fn four_ai_positions_settle_one_fan_capped_seeded_round() {
        let state = run_seeded_ai_round_with_configs(2026070402, 220, &one_fan_capped_configs());
        let settlement = state.settlement.as_ref().expect("AI capped settlement");
        let total_discards = state.discards.values().map(Vec::len).sum::<usize>();

        assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
        assert!(
            settlement.is_self_draw
                || settlement.from_position.is_some()
                || settlement.winner_positions.is_empty(),
            "capped seeded AI round should settle as a self draw, discard win, or legal draw"
        );
        assert!(total_discards > 0);
        assert_seeded_settlement_winners_are_legal(&state, &one_fan_capped_configs(), 2026070402);
        assert_seeded_settlement_event_is_consistent(&state, &one_fan_capped_configs(), 2026070402);
    }

    #[test]
    fn four_ai_positions_settle_standard_seeded_round() {
        let configs = default_configs();
        let state = run_seeded_ai_round_with_configs(2026070406, 260, &configs);
        let settlement = state.settlement.as_ref().expect("AI standard settlement");
        let total_discards = state.discards.values().map(Vec::len).sum::<usize>();

        assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
        assert!(
            settlement.is_self_draw
                || settlement.from_position.is_some()
                || settlement.winner_positions.is_empty(),
            "standard seeded AI round should settle as a self draw, discard win, or legal draw"
        );
        assert!(total_discards > 0);
        assert_seeded_settlement_winners_are_legal(&state, &configs, 2026070406);
        assert_seeded_settlement_event_is_consistent(&state, &configs, 2026070406);
    }

    #[test]
    fn four_ai_positions_settle_first_chi_disabled_seeded_round() {
        let configs = HashMap::from([("allow_first_chi".to_owned(), 0)]);
        let state = run_seeded_ai_round_with_configs(2026070407, 260, &configs);
        let settlement = state
            .settlement
            .as_ref()
            .expect("AI first-Chi-disabled settlement");
        let total_discards = state.discards.values().map(Vec::len).sum::<usize>();

        assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
        assert!(
            settlement.is_self_draw
                || settlement.from_position.is_some()
                || settlement.winner_positions.is_empty(),
            "first-Chi-disabled seeded AI round should settle as a self draw, discard win, or legal draw"
        );
        assert!(total_discards > 0);
        assert_seeded_settlement_winners_are_legal(&state, &configs, 2026070407);
        assert_seeded_settlement_event_is_consistent(&state, &configs, 2026070407);
    }

    fn one_fan_capped_configs() -> HashMap<String, i32> {
        HashMap::from([("max_fan".to_owned(), 2)])
    }

    fn capped_multi_hu_claim_state() -> ShenyangMahjongLoopState {
        let mut state = playable_state();
        state.current_position = 1;
        state.dealer_position = 3;
        state
            .hands
            .insert(0, vec![13, 14, 15, 15, 16, 16, 17, 28, 28, 28]);
        state.hands.insert(1, Vec::new());
        state
            .hands
            .insert(2, vec![2, 3, 4, 11, 12, 13, 14, 15, 35, 35]);
        state.hands.insert(3, Vec::new());
        state.melds.insert(0, vec![test_peng_meld_from(1, 3)]);
        state.melds.insert(1, vec![test_peng_meld_from(9, 3)]);
        state.melds.insert(2, vec![test_peng_meld_from(21, 3)]);
        state.discards.insert(0, vec![16]);
        state.discards.insert(1, vec![16]);
        state.claim_window = Some(ClaimWindowState {
            tile: 16,
            from_position: 1,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![0, 2],
            responses: HashMap::new(),
        });
        state
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

    fn default_configs() -> HashMap<String, i32> {
        HashMap::new()
    }

    #[test]
    fn rob_gang_hu_passes_from_impossible_known_tile_state() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_away(0);
        state.base.lock().unwrap().mark_ai_takeover_position(0);
        state.current_position = 1;
        state
            .hands
            .insert(0, vec![14, 15, 17, 18, 19, 21, 22, 23, 35, 35]);
        state.melds.insert(0, vec![test_peng_meld(1)]);
        state
            .hands
            .insert(1, vec![2, 5, 8, 11, 14, 16, 17, 21, 31, 32, 33]);
        state.melds.insert(1, vec![test_peng_meld_from(16, 2)]);
        state.discards.insert(2, vec![14, 14, 14, 14]);
        state.last_drawn_tile = Some(16);
        state.claim_window = Some(ClaimWindowState {
            tile: 16,
            from_position: 1,
            kind: ClaimWindowKind::RobGang,
            eligible_positions: vec![0, 2],
            responses: HashMap::new(),
        });
        let configs = HashMap::new();
        let table = build_public_table_with_configs(&state, &configs);
        let claim = table.claim_window.as_ref().expect("claim window");
        let mut dispatch = Dispatch::default();

        assert!(!claim_hu_is_complete(
            state.hands.get(&0).unwrap(),
            claim,
            &table,
            0,
        ));
        assert!(maybe_resolve_ai_claims(
            &RoomService::default(),
            "room",
            &mut state,
            &configs,
            &mut dispatch,
        ));

        let claim_window = state
            .claim_window
            .as_ref()
            .expect("claim window stays open");
        assert!(matches!(
            claim_window.responses.get(&0),
            Some(ClaimResponse::AiPass)
        ));
        assert!(!claim_window.responses.contains_key(&2));
        assert!(dispatch.messages.is_empty());
    }

    #[test]
    fn rob_gang_hu_passes_when_unowned_claim_tile_is_fifth_known_copy() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_away(0);
        state.base.lock().unwrap().mark_ai_takeover_position(0);
        state.current_position = 1;
        state
            .hands
            .insert(0, vec![14, 15, 17, 18, 19, 21, 22, 23, 35, 35]);
        state.melds.insert(0, vec![test_peng_meld(1)]);
        state
            .hands
            .insert(1, vec![2, 5, 8, 11, 14, 16, 17, 21, 31, 32, 33]);
        state.melds.insert(1, vec![test_peng_meld_from(16, 2)]);
        state.discards.insert(2, vec![16]);
        state.last_drawn_tile = Some(16);
        state.claim_window = Some(ClaimWindowState {
            tile: 16,
            from_position: 1,
            kind: ClaimWindowKind::RobGang,
            eligible_positions: vec![0, 2],
            responses: HashMap::new(),
        });
        let configs = HashMap::new();
        let table = build_public_table_with_configs(&state, &configs);
        let claim = table.claim_window.as_ref().expect("claim window");
        let mut dispatch = Dispatch::default();

        assert!(!claim_hu_is_complete(
            state.hands.get(&0).unwrap(),
            claim,
            &table,
            0,
        ));
        assert!(maybe_resolve_ai_claims(
            &RoomService::default(),
            "room",
            &mut state,
            &configs,
            &mut dispatch,
        ));

        let claim_window = state
            .claim_window
            .as_ref()
            .expect("claim window stays open");
        assert!(matches!(
            claim_window.responses.get(&0),
            Some(ClaimResponse::AiPass)
        ));
        assert!(!claim_window.responses.contains_key(&2));
        assert!(dispatch.messages.is_empty());
    }

    fn run_seeded_ai_round(seed: u64, max_steps: usize) -> ShenyangMahjongLoopState {
        run_seeded_ai_round_with_configs(seed, max_steps, &HashMap::new())
    }

    fn run_seeded_ai_round_with_configs(
        seed: u64,
        max_steps: usize,
        configs: &HashMap<String, i32>,
    ) -> ShenyangMahjongLoopState {
        let mut state = seeded_ai_round_state(seed);
        let room_service = RoomService::default();
        let mut dispatch = Dispatch::default();

        for step in 0..max_steps {
            if state.phase == ShenyangMahjongPhase::Settlement {
                break;
            }

            let acted =
                maybe_resolve_ai_claims(&room_service, "room", &mut state, configs, &mut dispatch)
                    || maybe_play_ai_turn(
                        &room_service,
                        "room",
                        &mut state,
                        configs,
                        &mut dispatch,
                    );

            assert!(
                acted,
                "AI round stalled for seed {seed} at step {step}, phase={:?}, current_position={}, wall={}, claim_window={:?}",
                state.phase,
                state.current_position,
                state.wall_count(),
                state.claim_window
            );
        }

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

    fn settlement_score_for_position(
        score_changes: &[WsShenyangMahjongScoreChange],
        position: usize,
    ) -> i32 {
        score_changes
            .iter()
            .find(|change| change.position == position as i32)
            .map(|change| change.score)
            .unwrap_or(0)
    }

    fn test_peng_meld(tile: i32) -> WsShenyangMahjongMeld {
        test_peng_meld_from(tile, 1)
    }

    fn test_peng_meld_from(tile: i32, from_position: i32) -> WsShenyangMahjongMeld {
        WsShenyangMahjongMeld {
            kind: ShenyangMahjongMeldKind::PENG,
            tiles: vec![tile, tile, tile],
            from_position: Some(from_position),
        }
    }
}
