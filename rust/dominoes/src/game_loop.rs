use std::sync::{Arc, Mutex};
use std::time::Duration;

use share_type_public::{DominoesActionSource, DominoesPhase};
use tokio::sync::Mutex as AsyncMutex;
use ws_common::{Dispatch, RoomService, SessionSenders};

use crate::action;
use crate::core::{DISCONNECTED_TURN_SECONDS, DominoesRoundState};
use crate::game::{
    StateHandle, StateRegistry, append_action_outcome, append_round_start_bundle,
    append_snapshot_and_turn, round_start_bundle,
};

const LOOP_POLL: Duration = Duration::from_millis(100);
const COUNTDOWN_TICK: Duration = Duration::from_secs(1);
const AI_TURN_SECONDS: u32 = 1;

fn room_matches_common(
    room_service: &RoomService,
    room_key: &str,
    expected_common: &Arc<Mutex<ws_common::CommonGameState>>,
) -> bool {
    room_service
        .room_common_state(room_key)
        .is_some_and(|current| Arc::ptr_eq(&current, expected_common))
}

fn state_should_stop(state: &DominoesRoundState) -> bool {
    let common = state.base.lock().expect("dominoes common state lock");
    common.stop_requested || common.players.len() != state.positions.len()
}

fn automatic_source_and_cap(state: &DominoesRoundState) -> (Option<DominoesActionSource>, u32) {
    let common = state.base.lock().expect("dominoes common state lock");
    let position = state.current_position;
    if common.is_ai_position(position) {
        return (Some(DominoesActionSource::NativeAi), AI_TURN_SECONDS);
    }
    if common.is_ai_takeover_position(position) {
        return (Some(DominoesActionSource::AiTakeover), AI_TURN_SECONDS);
    }
    if common.is_away(position) || common.is_disconnected(position) {
        return (None, DISCONNECTED_TURN_SECONDS);
    }
    (None, state.remaining_seconds)
}

fn is_paused(state: &DominoesRoundState) -> bool {
    state
        .base
        .lock()
        .expect("dominoes common state lock")
        .paused
}

async fn send_dispatch(dispatch: Dispatch, senders: &SessionSenders) {
    let mut encoded = Vec::with_capacity(dispatch.messages.len());
    for delivery in dispatch.messages {
        if let Ok(frame) = ws_common::to_text_message(&delivery.payload) {
            encoded.push((delivery.recipient, frame));
        }
    }
    let senders = senders.lock().await;
    for (recipient, frame) in encoded {
        if let Some(sender) = senders.get(&recipient) {
            let _ = sender.send(frame);
        }
    }
}

async fn emit_snapshot(
    room_key: &str,
    expected_common: &Arc<Mutex<ws_common::CommonGameState>>,
    snapshot: share_type_public::WsDominoesTableSnapshotEvent,
    room_service: &Arc<AsyncMutex<RoomService>>,
    senders: &SessionSenders,
) -> bool {
    let room_service = room_service.lock().await;
    if !room_matches_common(&room_service, room_key, expected_common) {
        return false;
    }
    let mut dispatch = Dispatch::default();
    append_snapshot_and_turn(&room_service, room_key, &snapshot, &mut dispatch);
    drop(room_service);
    send_dispatch(dispatch, senders).await;
    true
}

async fn emit_outcome(
    room_key: &str,
    expected_common: &Arc<Mutex<ws_common::CommonGameState>>,
    outcome: action::ActionOutcome,
    room_service: &Arc<AsyncMutex<RoomService>>,
    senders: &SessionSenders,
) -> bool {
    let room_service = room_service.lock().await;
    if !room_matches_common(&room_service, room_key, expected_common) {
        return false;
    }
    let mut dispatch = Dispatch::default();
    append_action_outcome(&room_service, room_key, outcome, &mut dispatch);
    drop(room_service);
    send_dispatch(dispatch, senders).await;
    true
}

async fn emit_round_start(
    room_key: &str,
    expected_common: &Arc<Mutex<ws_common::CommonGameState>>,
    bundle: crate::game::RoundStartBundle,
    room_service: &Arc<AsyncMutex<RoomService>>,
    senders: &SessionSenders,
) -> bool {
    let room_service = room_service.lock().await;
    if !room_matches_common(&room_service, room_key, expected_common) {
        return false;
    }
    let mut dispatch = Dispatch::default();
    append_round_start_bundle(&room_service, room_key, bundle, &mut dispatch);
    drop(room_service);
    send_dispatch(dispatch, senders).await;
    true
}

fn remove_registry_state(room_key: &str, state: &StateHandle, states: &StateRegistry) {
    let mut states = states.lock().expect("dominoes registry lock");
    if states
        .get(room_key)
        .is_some_and(|current| Arc::ptr_eq(current, state))
    {
        states.remove(room_key);
    }
}

pub(crate) fn start_game_loop(
    room_key: String,
    state: StateHandle,
    room_service: Arc<AsyncMutex<RoomService>>,
    senders: SessionSenders,
    states: StateRegistry,
) {
    tokio::spawn(async move {
        let expected_common = Arc::clone(&state.lock().expect("dominoes state lock").base);
        loop {
            let room_is_current = {
                let room_service = room_service.lock().await;
                room_matches_common(&room_service, &room_key, &expected_common)
            };
            if !room_is_current || state_should_stop(&state.lock().expect("dominoes state lock")) {
                break;
            }

            let (phase, revision, remaining, paused, cap_snapshot, source) = {
                let mut state = state.lock().expect("dominoes state lock");
                let paused = is_paused(&state);
                let (source, cap) = automatic_source_and_cap(&state);
                let cap_snapshot = (state.phase == DominoesPhase::Play
                    && state.cap_remaining_seconds(cap))
                .then(|| state.table_snapshot());
                (
                    state.phase,
                    state.turn_revision,
                    state.remaining_seconds,
                    paused,
                    cap_snapshot,
                    source,
                )
            };

            if let Some(snapshot) = cap_snapshot
                && !emit_snapshot(
                    &room_key,
                    &expected_common,
                    snapshot,
                    &room_service,
                    &senders,
                )
                .await
            {
                break;
            }
            if paused {
                tokio::time::sleep(LOOP_POLL).await;
                continue;
            }

            match phase {
                DominoesPhase::Play if remaining == 0 => {
                    let outcome = {
                        let mut state = state.lock().expect("dominoes state lock");
                        if state.phase != DominoesPhase::Play
                            || state.turn_revision != revision
                            || state.remaining_seconds != 0
                            || is_paused(&state)
                        {
                            None
                        } else {
                            let source = source.unwrap_or(DominoesActionSource::Timeout);
                            let result = action::automatic_turn(
                                &mut state,
                                source,
                                |state, position, legal_plays| {
                                    if source == DominoesActionSource::Timeout {
                                        legal_plays[0]
                                    } else {
                                        crate::ai::choose_play(state, position, legal_plays)
                                    }
                                },
                            );
                            match result {
                                Ok(outcome) => Some(outcome),
                                Err(_) => {
                                    state.remaining_seconds = 1;
                                    None
                                }
                            }
                        }
                    };
                    if let Some(outcome) = outcome
                        && !emit_outcome(
                            &room_key,
                            &expected_common,
                            outcome,
                            &room_service,
                            &senders,
                        )
                        .await
                    {
                        break;
                    }
                }
                DominoesPhase::Play => {
                    tokio::time::sleep(COUNTDOWN_TICK).await;
                    let snapshot = {
                        let mut state = state.lock().expect("dominoes state lock");
                        if is_paused(&state)
                            || !state.tick_remaining_seconds(revision)
                            || state_should_stop(&state)
                        {
                            None
                        } else {
                            Some(state.table_snapshot())
                        }
                    };
                    if let Some(snapshot) = snapshot
                        && !emit_snapshot(
                            &room_key,
                            &expected_common,
                            snapshot,
                            &room_service,
                            &senders,
                        )
                        .await
                    {
                        break;
                    }
                }
                DominoesPhase::RoundOver if remaining == 0 => {
                    let bundle = {
                        let mut state = state.lock().expect("dominoes state lock");
                        if state.phase != DominoesPhase::RoundOver
                            || state.turn_revision != revision
                            || is_paused(&state)
                        {
                            None
                        } else if state.start_next_round().is_ok() {
                            Some(round_start_bundle(&state))
                        } else {
                            None
                        }
                    };
                    if let Some(bundle) = bundle
                        && !emit_round_start(
                            &room_key,
                            &expected_common,
                            bundle,
                            &room_service,
                            &senders,
                        )
                        .await
                    {
                        break;
                    }
                }
                DominoesPhase::RoundOver => {
                    tokio::time::sleep(COUNTDOWN_TICK).await;
                    let snapshot = {
                        let mut state = state.lock().expect("dominoes state lock");
                        if is_paused(&state)
                            || !state.tick_round_transition(revision)
                            || state_should_stop(&state)
                        {
                            None
                        } else {
                            Some(state.table_snapshot())
                        }
                    };
                    if let Some(snapshot) = snapshot
                        && !emit_snapshot(
                            &room_key,
                            &expected_common,
                            snapshot,
                            &room_service,
                            &senders,
                        )
                        .await
                    {
                        break;
                    }
                }
                DominoesPhase::GameOver => tokio::time::sleep(LOOP_POLL).await,
            }
        }
        remove_registry_state(&room_key, &state, &states);
    });
}

#[cfg(test)]
#[path = "game_loop/tests.rs"]
mod tests;
