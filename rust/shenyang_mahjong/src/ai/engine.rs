use std::collections::HashMap;

use ws_common::{Dispatch, RoomService};

use crate::game::{
    can_self_draw_hu_with_configs, can_self_gang, perform_discard, perform_self_draw_hu,
    perform_self_gang, resolve_claim_window,
};
use crate::game_state::{ClaimResponse, ClaimWindowKind, ShenyangMahjongLoopState};
use crate::rules::{is_complete_win_with_melds_and_open_rule, win_rule_from_configs};

use super::decision::{
    AiClaimChoice, choose_claim_from_view, choose_discard_from_view, choose_self_gang_from_view,
    should_pass_self_draw_hu_from_view,
};
use super::observation::{AiClaimView, AiPublicTable, build_public_table_with_configs};

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
    let win_rule = win_rule_from_configs(configs);
    if can_self_draw_hu_with_configs(state, position, configs) {
        let table = build_public_table_with_configs(state, configs);
        if let Some(win_tile) = state.last_drawn_tile
            && !state.pending_gang_draw
            && state.wall_count() > 0
            && should_pass_self_draw_hu_from_view(&hand, &table, position, win_rule, win_tile)
            && perform_discard(
                room_service,
                room_key,
                state,
                configs,
                dispatch,
                position,
                win_tile,
            )
        {
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
    if let Some(tile) = choose_discard_from_view(&hand, &table, position, win_rule) {
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
    let win_rule = win_rule_from_configs(configs);

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
        let choice =
            if is_rob_gang && claim_hu_is_complete(&hand, claim, &table, position, win_rule) {
                AiClaimChoice::Hu
            } else {
                choose_claim_from_view(&hand, claim, &table, position, win_rule)
                    .unwrap_or(AiClaimChoice::Pass)
            };
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
        return true;
    }
    false
}

fn self_hand(state: &ShenyangMahjongLoopState, position: usize) -> Option<Vec<i32>> {
    state.hands.get(&position).cloned()
}

fn claim_hu_is_complete(
    hand: &[i32],
    claim: &AiClaimView,
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> bool {
    let mut win_hand = hand.to_vec();
    win_hand.push(claim.tile);
    win_hand.sort_unstable();
    let melds = table
        .seats
        .get(&position)
        .map(|seat| seat.melds.as_slice())
        .unwrap_or(&[]);
    is_complete_win_with_melds_and_open_rule(&win_hand, melds, win_rule, table.chi_opens_door)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use share_type_public::games::shenyang_mahjong::{
        ShenyangMahjongMeldKind, ShenyangMahjongPhase, WsShenyangMahjongMeld,
        WsShenyangMahjongScoreChange,
    };
    use ws_common::CommonGameState;

    use super::*;
    use crate::game::build_settlement_event_with_configs;
    use crate::game_state::ClaimWindowState;
    use crate::rules::{
        WIN_RULE_RELAXED, WIN_RULE_SHENYANG_BASIC, is_complete_win_with_melds_and_open_rule,
        win_rule_from_configs,
    };

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
            &relaxed_configs(),
            &mut dispatch,
        ));

        let claim_window = state.claim_window.as_ref().expect("claim window");
        assert!(matches!(
            claim_window.responses.get(&0),
            Some(ClaimResponse::Pass)
        ));
        assert!(!claim_window.responses.contains_key(&1));
        assert_eq!(state.phase, ShenyangMahjongPhase::Play);
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
        let win_rule = win_rule_from_configs(configs);
        let chi_opens_door = configs.get("chi_opens_door").copied().unwrap_or(1) == 1;

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
                is_complete_win_with_melds_and_open_rule(&hand, melds, win_rule, chi_opens_door),
                "seed {seed} winner {winner} should have a legal Shenyang Mahjong hand: hand={hand:?}, melds={melds:?}, settlement={settlement:?}"
            );
        }
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
    fn away_position_self_draws_closed_basic_pure_one_suit() {
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
    fn away_position_takes_rob_gang_hu_in_capped_room() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_away(0);
        state.current_position = 1;
        state.dealer_position = 1;
        state
            .hands
            .insert(0, vec![14, 15, 17, 18, 19, 21, 22, 23, 35, 35]);
        state.melds.insert(0, vec![test_peng_meld(1)]);
        state.hands.insert(1, vec![16]);
        state.melds.insert(1, vec![test_peng_meld(16)]);
        state.claim_window = Some(ClaimWindowState {
            tile: 16,
            from_position: 1,
            kind: ClaimWindowKind::RobGang,
            eligible_positions: vec![0],
            responses: HashMap::new(),
        });
        let configs = HashMap::from([
            ("win_rule".to_owned(), WIN_RULE_SHENYANG_BASIC),
            ("max_fan".to_owned(), 2),
        ]);
        let table = build_public_table_with_configs(&state, &configs);
        let claim = table.claim_window.as_ref().expect("claim window");
        let mut dispatch = Dispatch::default();

        assert!(claim_hu_is_complete(
            state.hands.get(&0).unwrap(),
            claim,
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC,
        ));
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
        assert!(settlement.is_reverse_win);
        let event = build_settlement_event_with_configs(&state, &configs)
            .expect("settlement event should be buildable");
        assert!(event.winner_details[0].is_reverse_win);
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
        state.wall = vec![37; 24];
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
    fn away_position_passes_low_fan_self_draw_for_live_capped_wait() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_away(0);
        state.dealer_position = 1;
        state.wall = vec![37; 48];
        state.discards.insert(1, vec![16]);
        state
            .hands
            .insert(0, vec![13, 14, 15, 15, 16, 16, 16, 17, 28, 28, 28]);
        state.melds.insert(0, vec![test_peng_meld(1)]);
        state.last_drawn_tile = Some(16);
        let configs = HashMap::from([("max_fan".to_owned(), 2)]);
        let mut dispatch = Dispatch::default();

        assert!(maybe_play_ai_turn(
            &RoomService::default(),
            "room",
            &mut state,
            &configs,
            &mut dispatch,
        ));

        assert_eq!(state.phase, ShenyangMahjongPhase::Play);
        assert!(state.settlement.is_none());
        assert_eq!(state.discards.get(&0).unwrap(), &vec![16]);
        assert_eq!(state.discards.get(&1).unwrap(), &vec![16]);
        assert_eq!(state.last_drawn_tile, Some(37));
        assert_eq!(
            state.hands.get(&0).unwrap(),
            &vec![13, 14, 15, 15, 16, 16, 17, 28, 28, 28]
        );
    }

    #[test]
    fn away_position_takes_gang_draw_self_draw_in_capped_room() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_away(0);
        state.dealer_position = 1;
        state.wall = vec![37; 48];
        state.discards.insert(1, vec![16]);
        state
            .hands
            .insert(0, vec![13, 14, 15, 15, 16, 16, 16, 17, 28, 28, 28]);
        state.melds.insert(0, vec![test_peng_meld(1)]);
        state.last_drawn_tile = Some(16);
        state.pending_gang_draw = true;
        let configs = HashMap::from([("max_fan".to_owned(), 2)]);
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
        assert!(settlement.is_gang_draw);
        let event = build_settlement_event_with_configs(&state, &configs)
            .expect("settlement event should be buildable");
        assert!(event.winner_details[0].is_gang_draw);
    }

    #[test]
    fn away_position_takes_haidilao_self_draw_in_capped_room() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_away(0);
        state.dealer_position = 1;
        state.wall = Vec::new();
        state.discards.insert(1, vec![16]);
        state
            .hands
            .insert(0, vec![13, 14, 15, 15, 16, 16, 16, 17, 28, 28, 28]);
        state.melds.insert(0, vec![test_peng_meld(1)]);
        state.last_drawn_tile = Some(16);
        let configs = HashMap::from([("max_fan".to_owned(), 2)]);
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
        assert!(!settlement.is_gang_draw);
        assert!(settlement.is_haidilao);
        let event = build_settlement_event_with_configs(&state, &configs)
            .expect("settlement event should be buildable");
        assert!(!event.winner_details[0].is_gang_draw);
        assert!(event.winner_details[0].is_haidilao);
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
    fn away_position_uses_ai_self_gang_before_discard() {
        let mut state = playable_state();
        state.base.lock().unwrap().mark_away(0);
        state
            .hands
            .insert(0, vec![4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35, 35, 35, 35]);
        state.last_drawn_tile = Some(35);
        state.wall = vec![37; 24];
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
    fn four_ai_positions_settle_relaxed_seeded_round() {
        let configs = relaxed_configs();
        let state = run_seeded_ai_round_with_configs(2026070406, 260, &configs);
        let settlement = state.settlement.as_ref().expect("AI relaxed settlement");
        let total_discards = state.discards.values().map(Vec::len).sum::<usize>();

        assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
        assert!(
            settlement.is_self_draw
                || settlement.from_position.is_some()
                || settlement.winner_positions.is_empty(),
            "relaxed seeded AI round should settle as a self draw, discard win, or legal draw"
        );
        assert!(total_discards > 0);
        assert_seeded_settlement_winners_are_legal(&state, &configs, 2026070406);
        assert_seeded_settlement_event_is_consistent(&state, &configs, 2026070406);
    }

    fn one_fan_capped_configs() -> HashMap<String, i32> {
        HashMap::from([("max_fan".to_owned(), 1)])
    }

    fn test_peng_meld(tile: i32) -> WsShenyangMahjongMeld {
        WsShenyangMahjongMeld {
            kind: ShenyangMahjongMeldKind::PENG,
            tiles: vec![tile, tile, tile],
            from_position: Some(1),
        }
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

    fn relaxed_configs() -> HashMap<String, i32> {
        HashMap::from([("win_rule".to_owned(), WIN_RULE_RELAXED)])
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
}
