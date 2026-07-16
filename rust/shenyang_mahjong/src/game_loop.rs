use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use share_type_public::WsCode;
use tokio::sync::Mutex;
use ws_common::{RoomService, SessionSenders};

use crate::ai::{maybe_play_ai_turn, maybe_resolve_ai_claims};
use crate::game::{
    LoopStateRegistry, current_play_time, perform_discard, push_phase_change,
    push_private_deal_events, push_room_event, redeal_after_settlement_with_configs,
    resolve_claim_window, settle_draw, settlement_time,
};
use crate::game_state::{ClaimResponse, ShenyangMahjongLoopState};
use share_type_public::games::shenyang_mahjong::ShenyangMahjongPhase;

fn auto_discard_tile(state: &ShenyangMahjongLoopState, position: usize) -> Option<i32> {
    if let Some(tile) = state.last_drawn_tile
        && state
            .hands
            .get(&position)
            .map(|hand| hand.contains(&tile))
            .unwrap_or(false)
    {
        return Some(tile);
    }
    state
        .hands
        .get(&position)
        .and_then(|hand| hand.last().copied())
}

async fn deliver(dispatch: ws_common::Dispatch, senders: &SessionSenders) {
    let mut frames = Vec::with_capacity(dispatch.messages.len());
    for message in dispatch.messages {
        if let Ok(frame) = ws_common::to_text_message(&message.payload) {
            frames.push((message.recipient, frame));
        }
    }
    let senders = senders.lock().await;
    for (session_id, frame) in frames {
        if let Some(tx) = senders.get(&session_id) {
            let _ = tx.send(frame);
        }
    }
}

fn settlement_should_stop(state: &Arc<std::sync::Mutex<ShenyangMahjongLoopState>>) -> bool {
    let state = state.lock().unwrap();
    state.players_snapshot().len() != 4 || state.stop_requested()
}

fn room_uses_common_state(
    room: &RoomService,
    room_key: &str,
    common: &Arc<std::sync::Mutex<ws_common::CommonGameState>>,
) -> bool {
    room.room_common_state(room_key)
        .is_some_and(|current| Arc::ptr_eq(&current, common))
}

fn loop_stop_requested(state: &Arc<std::sync::Mutex<ShenyangMahjongLoopState>>) -> bool {
    state.lock().unwrap().stop_requested()
}

async fn sleep_or_stop(
    state: &Arc<std::sync::Mutex<ShenyangMahjongLoopState>>,
    duration: Duration,
) -> bool {
    let mut remaining = duration.as_millis();
    while remaining > 0 {
        if loop_stop_requested(state) {
            return true;
        }
        let step = remaining.min(100) as u64;
        tokio::time::sleep(Duration::from_millis(step)).await;
        remaining -= u128::from(step);
    }
    loop_stop_requested(state)
}

fn should_resolve_timed_out_claims(state: &ShenyangMahjongLoopState) -> bool {
    state.turn_countdown() == 0 && state.claim_window.is_some()
}

fn perform_auto_discard_or_settle(
    room_service: &RoomService,
    room_key: &str,
    state: &mut ShenyangMahjongLoopState,
    configs: &HashMap<String, i32>,
    dispatch: &mut ws_common::Dispatch,
    position: usize,
    tile: Option<i32>,
) -> bool {
    let discarded = tile.is_some_and(|tile| {
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
    if !discarded {
        settle_draw(room_service, room_key, state, configs, dispatch);
    }
    discarded
}

pub(crate) fn start_game_loop(
    room_key: String,
    state: Arc<std::sync::Mutex<ShenyangMahjongLoopState>>,
    room_service: Arc<Mutex<RoomService>>,
    senders: SessionSenders,
    loop_states: LoopStateRegistry,
) {
    tokio::spawn(async move {
        let common = { Arc::clone(&state.lock().unwrap().base) };
        let configs: HashMap<String, i32> = room_service
            .lock()
            .await
            .room_configs(&room_key)
            .unwrap_or_default();

        loop {
            if loop_stop_requested(&state) {
                break;
            }
            if state.lock().unwrap().is_paused() {
                if sleep_or_stop(&state, Duration::from_secs(1)).await {
                    break;
                }
                continue;
            }
            let phase = { state.lock().unwrap().phase };
            match phase {
                ShenyangMahjongPhase::Start => {
                    if state.lock().unwrap().stop_requested() {
                        break;
                    }
                    {
                        let mut guard = state.lock().unwrap();
                        guard.deal_new_round();
                        guard.set_turn_countdown(current_play_time(&configs));
                    }
                    let mut dispatch = ws_common::Dispatch::default();
                    {
                        let room = room_service.lock().await;
                        if !room_uses_common_state(&room, &room_key, &common) {
                            break;
                        }
                        let guard = state.lock().unwrap();
                        push_phase_change(
                            &room,
                            &room_key,
                            &mut dispatch,
                            guard.phase,
                            guard.current_position,
                            guard.turn_countdown(),
                        );
                        push_private_deal_events(&room, &room_key, &guard, &mut dispatch);
                        push_room_event(
                            &room,
                            &room_key,
                            &mut dispatch,
                            WsCode::CHANGE_DEAL as i32,
                            serde_json::json!({
                                "position": guard.current_position as i32,
                                "turn_countdown": guard.turn_countdown() as i32,
                            }),
                        );
                    }
                    deliver(dispatch, &senders).await;
                }
                ShenyangMahjongPhase::Play => {
                    if sleep_or_stop(&state, Duration::from_secs(1)).await {
                        break;
                    }
                    if state.lock().unwrap().is_paused() {
                        continue;
                    }

                    let mut ai_dispatch = ws_common::Dispatch::default();
                    let ai_acted = {
                        let room = room_service.lock().await;
                        if !room_uses_common_state(&room, &room_key, &common) {
                            break;
                        }
                        let mut guard = state.lock().unwrap();
                        if maybe_resolve_ai_claims(
                            &room,
                            &room_key,
                            &mut guard,
                            &configs,
                            &mut ai_dispatch,
                        ) {
                            true
                        } else {
                            maybe_play_ai_turn(
                                &room,
                                &room_key,
                                &mut guard,
                                &configs,
                                &mut ai_dispatch,
                            )
                        }
                    };
                    if ai_acted {
                        deliver(ai_dispatch, &senders).await;
                        continue;
                    }

                    let mut should_resolve_claims = false;
                    let mut should_auto_discard = None;
                    {
                        let guard = state.lock().unwrap();
                        if guard.stop_requested() {
                            break;
                        }
                        if guard.turn_countdown() == 0 {
                            if guard.claim_window.is_some() {
                                should_resolve_claims = should_resolve_timed_out_claims(&guard);
                            } else {
                                should_auto_discard = Some((
                                    guard.current_position,
                                    auto_discard_tile(&guard, guard.current_position),
                                ));
                            }
                        }
                    }

                    if should_resolve_claims {
                        let mut dispatch = ws_common::Dispatch::default();
                        {
                            let room = room_service.lock().await;
                            if !room_uses_common_state(&room, &room_key, &common) {
                                break;
                            }
                            let mut guard = state.lock().unwrap();
                            if let Some(claim_window) = guard.claim_window.as_mut() {
                                for position in claim_window.eligible_positions.clone() {
                                    claim_window
                                        .responses
                                        .entry(position)
                                        .or_insert(ClaimResponse::Pass);
                                }
                            }
                            resolve_claim_window(
                                &room,
                                &room_key,
                                &mut guard,
                                &configs,
                                &mut dispatch,
                            );
                        }
                        deliver(dispatch, &senders).await;
                        continue;
                    }

                    if let Some((position, tile)) = should_auto_discard {
                        let mut dispatch = ws_common::Dispatch::default();
                        {
                            let room = room_service.lock().await;
                            if !room_uses_common_state(&room, &room_key, &common) {
                                break;
                            }
                            let mut guard = state.lock().unwrap();
                            if guard.current_position != position || guard.claim_window.is_some() {
                                continue;
                            }
                            let _ = perform_auto_discard_or_settle(
                                &room,
                                &room_key,
                                &mut guard,
                                &configs,
                                &mut dispatch,
                                position,
                                tile,
                            );
                        }
                        deliver(dispatch, &senders).await;
                        continue;
                    }

                    {
                        let guard = state.lock().unwrap();
                        if guard.stop_requested() {
                            break;
                        }
                        let countdown = guard.turn_countdown();
                        if countdown > 0 {
                            guard.set_turn_countdown(countdown - 1);
                        }
                    }
                }
                ShenyangMahjongPhase::Settlement => {
                    if settlement_should_stop(&state) {
                        break;
                    }
                    if sleep_or_stop(&state, Duration::from_secs(settlement_time(&configs))).await
                        || settlement_should_stop(&state)
                    {
                        break;
                    }
                    {
                        let mut guard = state.lock().unwrap();
                        redeal_after_settlement_with_configs(&mut guard, &configs);
                    }
                    let mut dispatch = ws_common::Dispatch::default();
                    {
                        let room = room_service.lock().await;
                        if !room_uses_common_state(&room, &room_key, &common) {
                            break;
                        }
                        let guard = state.lock().unwrap();
                        push_phase_change(
                            &room,
                            &room_key,
                            &mut dispatch,
                            guard.phase,
                            guard.current_position,
                            guard.turn_countdown(),
                        );
                    }
                    deliver(dispatch, &senders).await;
                }
            }
        }

        let should_cleanup = {
            let mut states = loop_states.lock().unwrap();
            if states
                .get(&room_key)
                .is_some_and(|current| Arc::ptr_eq(current, &state))
            {
                states.remove(&room_key);
                true
            } else {
                false
            }
        };
        if should_cleanup {
            room_service
                .lock()
                .await
                .clear_room_game_state_if_same(&room_key, &common);
        }
    });
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use ws_common::CommonGameState;

    use super::*;

    #[test]
    fn auto_discard_falls_back_to_the_last_tile_in_hand() {
        let base = Arc::new(Mutex::new(CommonGameState::default()));
        let mut state = ShenyangMahjongLoopState::new(base);
        state.hands.insert(0, vec![1, 2, 9]);
        state.last_drawn_tile = Some(8);

        assert_eq!(auto_discard_tile(&state, 0), Some(9));
    }

    #[test]
    fn auto_discard_prefers_the_last_drawn_tile() {
        let base = Arc::new(Mutex::new(CommonGameState::default()));
        let mut state = ShenyangMahjongLoopState::new(base);
        state.hands.insert(0, vec![1, 2, 9]);
        state.last_drawn_tile = Some(2);

        assert_eq!(auto_discard_tile(&state, 0), Some(2));
    }

    #[test]
    fn timed_out_empty_claim_window_is_resolved() {
        let base = Arc::new(Mutex::new(CommonGameState::default()));
        let mut state = ShenyangMahjongLoopState::new(base);
        state.claim_window = Some(crate::game_state::ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: crate::game_state::ClaimWindowKind::Discard,
            eligible_positions: Vec::new(),
            responses: HashMap::new(),
        });
        state.set_turn_countdown(0);

        assert!(should_resolve_timed_out_claims(&state));
    }

    #[test]
    fn failed_auto_discard_settles_round() {
        let base = Arc::new(Mutex::new(CommonGameState::default()));
        let mut state = ShenyangMahjongLoopState::new(base);
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        state
            .hands
            .insert(0, vec![4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31, 32, 33, 99]);
        state.discards.insert(0, Vec::new());
        state.wall = vec![36];
        state.last_drawn_tile = Some(4);
        let original_hand = state.hands.get(&0).cloned().unwrap();
        let mut dispatch = ws_common::Dispatch::default();

        assert!(!perform_auto_discard_or_settle(
            &RoomService::default(),
            "room",
            &mut state,
            &HashMap::new(),
            &mut dispatch,
            0,
            Some(4),
        ));

        assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
        assert!(
            state
                .settlement
                .as_ref()
                .is_some_and(|settlement| settlement.winner_positions.is_empty())
        );
        assert_eq!(state.hands.get(&0), Some(&original_hand));
        assert!(state.discards.get(&0).unwrap().is_empty());
    }

    #[test]
    fn successful_auto_discard_keeps_round_playing() {
        let base = Arc::new(Mutex::new(CommonGameState::default()));
        let mut state = ShenyangMahjongLoopState::new(base);
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        state
            .hands
            .insert(0, vec![4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31, 32, 33, 34]);
        state.discards.insert(0, Vec::new());
        state.wall = vec![36];
        state.last_drawn_tile = Some(4);
        let mut dispatch = ws_common::Dispatch::default();

        assert!(perform_auto_discard_or_settle(
            &RoomService::default(),
            "room",
            &mut state,
            &HashMap::new(),
            &mut dispatch,
            0,
            Some(4),
        ));

        assert_eq!(state.phase, ShenyangMahjongPhase::Play);
        assert!(state.settlement.is_none());
        assert_eq!(state.discards.get(&0), Some(&vec![4]));
        assert!(state.hands.get(&0).unwrap().contains(&36));
    }

    #[test]
    fn missing_auto_discard_tile_settles_round() {
        let base = Arc::new(Mutex::new(CommonGameState::default()));
        let mut state = ShenyangMahjongLoopState::new(base);
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        state.hands.insert(0, Vec::new());
        state.discards.insert(0, Vec::new());
        let mut dispatch = ws_common::Dispatch::default();

        assert!(!perform_auto_discard_or_settle(
            &RoomService::default(),
            "room",
            &mut state,
            &HashMap::new(),
            &mut dispatch,
            0,
            None,
        ));

        assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
        assert!(
            state
                .settlement
                .as_ref()
                .is_some_and(|settlement| settlement.winner_positions.is_empty())
        );
    }
}
