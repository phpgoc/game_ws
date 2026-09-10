use super::claim_updates::{plays_for, rob_gang_room};
use super::*;
use crate::game_loop::resolve_timeout;

fn snapshots(state: &ShenyangMahjongLoopState) -> Vec<Value> {
    (0..4)
        .map(|position| {
            serde_json::to_value(build_table_snapshot_event_with_configs(
                state,
                position,
                &default_configs(),
            ))
            .unwrap()
        })
        .collect()
}

fn respond(
    room: &mut RoomService,
    handler: &mut ShenyangMahjongGameHandler,
    position: usize,
    action: ShenyangMahjongAction,
) -> Dispatch {
    handler.handle_game_request(
        room,
        position as u64 + 1,
        play_request(action, Vec::new(), Some(6), Some(0)),
    )
}

fn timeout(room: &RoomService, state: &mut ShenyangMahjongLoopState) -> (bool, Dispatch) {
    let mut dispatch = Dispatch::default();
    let resolved = resolve_timeout(
        room,
        "mahjong-request-room",
        state,
        &default_configs(),
        &mut dispatch,
    );
    (resolved, dispatch)
}

#[test]
fn final_response_before_timeout_preserves_settlement_or_replacement_draw() {
    for action in [ShenyangMahjongAction::HU, ShenyangMahjongAction::PASS] {
        let (mut room, mut handler, shared) = rob_gang_room();
        respond(&mut room, &mut handler, 1, action);
        // The loop may have observed zero before it waited for the room lock.
        shared.lock().unwrap().set_turn_countdown(0);
        let accepted = respond(&mut room, &mut handler, 2, action);
        assert_eq!(
            response_code(&accepted, 3, Routes::PLAY),
            Some(WsResponseCode::OK as i32)
        );
        let mut state = shared.lock().unwrap();
        assert!(state.claim_window.is_none());
        if action == ShenyangMahjongAction::HU {
            assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
            assert_eq!(
                state.settlement.as_ref().unwrap().winner_positions,
                vec![1, 2]
            );
        } else {
            assert_eq!(
                state.turn_countdown(),
                current_play_time(&default_configs())
            );
            assert_eq!(state.wall_count(), 1);
            assert_eq!(plays_for(&accepted, 1).len(), 2);
        }
        let before = snapshots(&state);
        let (resolved, stale_timeout) = timeout(&room, &mut state);
        assert!(
            !resolved,
            "a previous claim deadline cannot act on the next state"
        );
        assert!(stale_timeout.messages.is_empty());
        assert_eq!(snapshots(&state), before);
        assert!((0..4).all(|position| !state.is_away(position)));
    }
}

#[test]
fn timeout_before_final_response_rejects_late_claim_without_repeating_resolution() {
    for first in [ShenyangMahjongAction::HU, ShenyangMahjongAction::PASS] {
        for late in [ShenyangMahjongAction::HU, ShenyangMahjongAction::PASS] {
            let (mut room, mut handler, shared) = rob_gang_room();
            respond(&mut room, &mut handler, 1, first);
            let before_late = {
                let mut state = shared.lock().unwrap();
                state.set_turn_countdown(0);
                let (resolved, dispatch) = timeout(&room, &mut state);
                assert!(resolved);
                assert!(state.claim_window.is_none());
                let actions: Vec<_> = plays_for(&dispatch, 1)
                    .into_iter()
                    .map(|play| play.action)
                    .collect();
                if first == ShenyangMahjongAction::HU {
                    assert_eq!(actions, vec![ShenyangMahjongAction::HU]);
                    assert_eq!(state.settlement.as_ref().unwrap().winner_positions, vec![1]);
                    assert_eq!(state.wall_count(), 2);
                } else {
                    assert_eq!(
                        actions,
                        vec![ShenyangMahjongAction::GANG, ShenyangMahjongAction::DRAW]
                    );
                    assert_eq!(state.wall_count(), 1);
                }
                assert!((0..4).all(|position| !state.is_away(position)));
                snapshots(&state)
            };
            let rejected = respond(&mut room, &mut handler, 2, late);
            assert_eq!(
                response_code(&rejected, 3, Routes::PLAY),
                Some(WsResponseCode::NO_PERMISSION as i32)
            );
            assert!(!has_room_event(&rejected, WsCode::PLAY));
            assert!(!has_room_event(&rejected, WsCode::GAME_OVER));
            let mut state = shared.lock().unwrap();
            assert_eq!(snapshots(&state), before_late);
            let (resolved, repeated) = timeout(&room, &mut state);
            assert!(!resolved);
            assert!(repeated.messages.is_empty());
            assert_eq!(snapshots(&state), before_late);
        }
    }
}

#[test]
fn partial_response_at_zero_keeps_accepted_hu_when_other_claimant_times_out() {
    let (mut room, mut handler, shared) = rob_gang_room();
    shared.lock().unwrap().set_turn_countdown(0);
    let accepted = respond(&mut room, &mut handler, 1, ShenyangMahjongAction::HU);
    assert_eq!(
        response_code(&accepted, 2, Routes::PLAY),
        Some(WsResponseCode::OK as i32)
    );
    assert!(!has_room_event(&accepted, WsCode::GAME_OVER));
    let mut state = shared.lock().unwrap();
    assert_eq!(state.turn_countdown(), 0);
    let (resolved, dispatch) = timeout(&room, &mut state);
    assert!(resolved);
    assert_eq!(plays_for(&dispatch, 1).len(), 1);
    assert_eq!(state.settlement.as_ref().unwrap().winner_positions, vec![1]);
    assert_eq!(state.wall_count(), 2);
    assert!((0..4).all(|position| !state.is_away(position)));
}

#[test]
fn timeout_rechecks_live_countdown_pause_stop_and_round_phase() {
    for boundary in ["new-window", "paused", "stopped", "new-round"] {
        let (room, _, shared) = rob_gang_room();
        let mut state = shared.lock().unwrap();
        state.set_turn_countdown(0);
        match boundary {
            "new-window" => state.set_turn_countdown(5),
            "paused" => state.base.lock().unwrap().pause(),
            "stopped" => state.request_stop(),
            "new-round" => redeal_after_settlement_with_configs(&mut state, &default_configs()),
            _ => unreachable!(),
        }
        let before = snapshots(&state);
        let (resolved, dispatch) = timeout(&room, &mut state);
        assert!(!resolved, "{boundary} must invalidate a pending timeout");
        assert!(dispatch.messages.is_empty());
        assert_eq!(snapshots(&state), before);
        assert!((0..4).all(|position| !state.is_away(position)));
    }
}

#[test]
fn turn_timeout_reads_current_member_capability_without_reentering_the_game_lock() {
    for member in [false, true] {
        let (mut room, mut handler, shared) = rob_gang_room();
        respond(&mut room, &mut handler, 1, ShenyangMahjongAction::PASS);
        respond(&mut room, &mut handler, 2, ShenyangMahjongAction::PASS);
        let mut state = shared.lock().unwrap();
        state.base.lock().unwrap().set_member_position(0, member);
        state.set_turn_countdown(0);
        let (resolved, dispatch) = timeout(&room, &mut state);
        assert!(resolved);
        assert!(state.is_away(0));
        assert_eq!(state.is_ai_controlled_position(0), member);
        let away = dispatch
            .messages
            .iter()
            .find_map(|message| match &message.payload {
                OutboundPayload::Event(event) if event.code == WsCode::AWAY as i32 => {
                    Some(&event.data)
                }
                _ => None,
            })
            .expect("turn timeout broadcasts presence once");
        assert_eq!(away["position"], 0);
        assert_eq!(away["is_ai_takeover"], member);
        assert!(has_room_event(&dispatch, WsCode::PLAY));
    }
}
