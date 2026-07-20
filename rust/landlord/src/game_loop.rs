use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use share_type_public::{
    LandlordPhase, WsCode, WsPositionEvent,
    games::landlord::{
        WsCallLandlordEvent, WsDealEvent, WsDealOpenCardsEvent, WsLandlordGameOverEvent,
        WsPlayEvent, WsShowHiddenCardsEvent,
    },
};
use tokio::sync::Mutex;
use ws_common::{Delivery, OutboundPayload, RoomService, SessionSenders, dlog};

use crate::ai::{choose_bid, choose_play};
use crate::game_state::{LandlordLoopState, LandlordPlayRecord};

const AI_ACTION_DELAY: Duration = Duration::from_millis(300);

enum AutoBroadcastEvent {
    Call(WsCallLandlordEvent),
    Play(WsPlayEvent),
}

type CallLandlordTransition = (Option<(String, Vec<i32>)>, Option<usize>, bool);

fn room_matches_common(
    room_service: &RoomService,
    room_key: &str,
    expected_common: &Arc<std::sync::Mutex<ws_common::CommonGameState>>,
) -> bool {
    room_service
        .room_common_state(room_key)
        .is_some_and(|current| Arc::ptr_eq(&current, expected_common))
}

/// Build a Dispatch that sends an event to all room members.
fn dispatch_all(
    room_key: &str,
    code: i32,
    data: serde_json::Value,
    room_service: &RoomService,
) -> Vec<Delivery> {
    room_service
        .connected_session_ids(room_key)
        .into_iter()
        .map(|sid| Delivery {
            recipient: sid,
            payload: OutboundPayload::Event(share_type_public::CommonEvent {
                code,
                data: data.clone(),
            }),
        })
        .collect()
}

fn dispatch_phase(
    room_key: &str,
    state: &LandlordLoopState,
    room_service: &RoomService,
) -> Vec<Delivery> {
    dispatch_all(
        room_key,
        WsCode::CHANGE_PHASE as i32,
        serde_json::json!({
            "phase": state.phase as i32,
            "position": state.current_position as i32,
        }),
        room_service,
    )
}

/// Build a Dispatch that sends an event to a specific position in the room.
fn dispatch_to_position(
    room_key: &str,
    position: usize,
    code: i32,
    data: serde_json::Value,
    room_service: &RoomService,
) -> Vec<Delivery> {
    room_service
        .connected_session_ids_for_position(room_key, position)
        .into_iter()
        .map(|sid| Delivery {
            recipient: sid,
            payload: OutboundPayload::Event(share_type_public::CommonEvent {
                code,
                data: data.clone(),
            }),
        })
        .collect()
}

fn fixed_wait_seconds(
    configs: &std::collections::HashMap<String, i32>,
    key: &str,
    default: u64,
) -> u64 {
    configs.get(key).copied().unwrap_or(default as i32).max(0) as u64
}

/// Advance through the CallLandlord phase: move to next player, check completion.
///
/// When a full round completes (we're back at call_position), determine landlord:
/// - If nobody called (score == 0) → redeal
/// - Otherwise, the highest bidder (last in call_history with max score) becomes landlord
async fn handle_call_landlord_phase(
    room_key: &str,
    state: &Arc<std::sync::Mutex<LandlordLoopState>>,
    sorted_positions: &[usize],
    configs: &std::collections::HashMap<String, i32>,
    room_service: &Arc<Mutex<RoomService>>,
    senders: &SessionSenders,
    expected_common: &Arc<std::sync::Mutex<ws_common::CommonGameState>>,
) {
    if loop_should_stop(state) {
        return;
    }
    let (open_hidden_event, next_turn_position, phase_changed): CallLandlordTransition = {
        let mut s = state.lock().unwrap();
        let current_idx = sorted_positions
            .iter()
            .position(|&p| p == s.current_position)
            .unwrap_or(0);
        let next_pos = sorted_positions[(current_idx + 1) % sorted_positions.len()];
        let should_finalize = if s.score == 3 {
            true
        } else {
            s.current_position = next_pos;
            s.current_position == s.call_position
        };

        if should_finalize {
            if s.score == 0 {
                // Everyone passed → redeal
                s.redeal();
                (None, Some(s.current_position), true)
            } else {
                // Find the player with the highest bid.
                // call_history is in call order; the highest score wins.
                // If tie (can't happen in 3-player since each round is one direction),
                // the last highest bidder wins.
                let max_score = s.call_history.iter().map(|(_, sc)| *sc).max().unwrap_or(0);
                let landlord_pos = s
                    .call_history
                    .iter()
                    .rev()
                    .find(|(_, sc)| *sc == max_score)
                    .map(|(pos, _)| *pos)
                    .unwrap_or(s.call_position);

                s.landlord_position = Some(landlord_pos);
                s.next_phase(); // CallLandlord → Play
                // Give hidden cards to landlord
                let hidden = s.hidden_cards.clone();
                if let Some(hand) = s.hands.get_mut(&landlord_pos) {
                    hand.extend(hidden.clone());
                    hand.sort_unstable();
                }
                // Reset for landlord's first play
                s.set_action_received(false);
                s.set_turn_countdown(turn_timeout(&s, configs));
                (
                    Some((player_name(&s, landlord_pos), hidden)),
                    Some(s.current_position),
                    true,
                )
            }
        } else {
            s.current_position = next_pos;
            // Reset for next caller in the same round
            s.set_action_received(false);
            s.set_turn_countdown(turn_timeout(&s, configs));
            (None, Some(s.current_position), false)
        }
    };

    if open_hidden_event.is_none() && next_turn_position.is_none() && !phase_changed {
        return;
    }

    let rs = room_service.lock().await;
    if !room_matches_common(&rs, room_key, expected_common) {
        return;
    }
    let mut dispatch = Vec::new();
    if phase_changed {
        let s = state.lock().unwrap();
        dispatch.extend(dispatch_phase(room_key, &s, &rs));
    }
    if let Some((name, cards)) = open_hidden_event {
        dispatch.extend(dispatch_all(
            room_key,
            WsCode::DEAL_OPEN_CARDS as i32,
            serde_json::to_value(WsDealOpenCardsEvent { name, cards }).unwrap_or_default(),
            &rs,
        ));
    }
    if let Some(position) = next_turn_position {
        let countdown = state.lock().unwrap().turn_countdown();
        dispatch.extend(dispatch_all(
            room_key,
            WsCode::CHANGE_DEAL as i32,
            serde_json::json!({
                "position": position as i32,
                "turn_countdown": countdown as i32,
            }),
            &rs,
        ));
    }
    drop(rs);
    send_dispatch(dispatch, senders).await;
}

/// Process a play tick: apply the current play (pass or cards), advance turns.
///
/// Logic:
/// - If current player played cards → update last_play, remove from hand, check win
/// - If current player passed (empty) → just advance
/// - When we loop back to last_play_position and the round leader didn't just pass,
///   the round leader gets to lead again (last_play cleared).
///
/// Returns true if the game is over (settlement).
async fn handle_play_phase(
    state: &Arc<std::sync::Mutex<LandlordLoopState>>,
    sorted_positions: &[usize],
    configs: &std::collections::HashMap<String, i32>,
    room_key: &str,
    room_service: &Arc<Mutex<RoomService>>,
    senders: &SessionSenders,
    expected_common: &Arc<std::sync::Mutex<ws_common::CommonGameState>>,
) -> bool {
    if loop_should_stop(state) {
        return true;
    }
    let mut game_over = false;
    let mut winner_pos = None;
    let mut next_turn_position: Option<usize> = None;

    {
        let mut s = state.lock().unwrap();
        let pos = s.current_position;
        let played = std::mem::take(&mut s.current_play);
        let benchmark = if s.last_play.is_empty() || s.last_play_position == pos {
            Vec::new()
        } else {
            s.last_play.clone()
        };
        s.play_history.push(LandlordPlayRecord {
            position: pos,
            cards: played.clone(),
            benchmark,
        });

        if played.is_empty() {
            // Player passed — nothing changes except who's next.
        } else {
            // Player played cards — this becomes the new benchmark
            s.last_play_position = pos;
            s.last_play = played;

            // Remove played cards from hand
            let play_back = s.last_play.clone();
            if let Some(hand) = s.hands.get_mut(&pos) {
                for card in &play_back {
                    if let Some(idx) = hand.iter().position(|c| c == card) {
                        hand.remove(idx);
                    }
                }
                // Check win condition
                if hand.is_empty() {
                    winner_pos = Some(pos);
                    s.next_phase(); // Play → Settlement
                    game_over = true;
                }
            }
        }

        if !game_over {
            // Advance to next position
            let current_idx = sorted_positions.iter().position(|&p| p == pos).unwrap_or(0);
            let next_pos = sorted_positions[(current_idx + 1) % sorted_positions.len()];
            s.current_position = next_pos;

            // If we've come full circle back to the last_play_position,
            // that means everyone else passed → last_play_position leads a new round
            if s.last_play_position == next_pos {
                s.last_play.clear();
            }

            // Reset for next turn
            s.set_action_received(false);
            s.set_turn_countdown(turn_timeout(&s, configs));
            next_turn_position = Some(next_pos);
        }
    }

    if let Some(position) = next_turn_position {
        let rs = room_service.lock().await;
        if !room_matches_common(&rs, room_key, expected_common) {
            return true;
        }
        let countdown = state.lock().unwrap().turn_countdown();
        let dispatch = dispatch_all(
            room_key,
            WsCode::CHANGE_DEAL as i32,
            serde_json::json!({
                "position": position as i32,
                "turn_countdown": countdown as i32,
            }),
            &rs,
        );
        drop(rs);
        send_dispatch(dispatch, senders).await;
    }

    if game_over {
        let rs = room_service.lock().await;
        if !room_matches_common(&rs, room_key, expected_common) {
            return true;
        }
        let mut dispatch = Vec::new();
        {
            let s = state.lock().unwrap();
            dispatch.extend(dispatch_phase(room_key, &s, &rs));
        }
        let (hidden_owner_name, hidden_cards, remaining_hands) = {
            let s = state.lock().unwrap();
            let hidden_cards = s.hidden_cards.clone();
            let players = s.players_snapshot();
            let hidden_owner_name = s
                .landlord_position
                .and_then(|pos| players.get(&pos).map(|(_, name)| name.clone()))
                .unwrap_or_default();
            let mut remaining_hands: Vec<(usize, Vec<i32>)> = s
                .hands
                .iter()
                .map(|(pos, cards)| (*pos, cards.clone()))
                .collect();
            remaining_hands.sort_by_key(|(pos, _)| *pos);
            let payloads = remaining_hands
                .into_iter()
                .map(|(pos, cards)| WsDealOpenCardsEvent {
                    name: players
                        .get(&pos)
                        .map(|(_, name)| name.clone())
                        .unwrap_or_default(),
                    cards,
                })
                .collect::<Vec<_>>();
            (hidden_owner_name, hidden_cards, payloads)
        };

        // Broadcast hidden cards reveal
        dispatch.extend(dispatch_all(
            room_key,
            WsCode::SHOW_HIDDEN_CARDS as i32,
            serde_json::to_value(WsShowHiddenCardsEvent {
                name: hidden_owner_name,
                cards: hidden_cards,
            })
            .unwrap_or_default(),
            &rs,
        ));
        // 显示每个人剩余的手牌
        for item in remaining_hands {
            dispatch.extend(dispatch_all(
                room_key,
                WsCode::DEAL_OPEN_CARDS as i32,
                serde_json::to_value(item).unwrap_or_default(),
                &rs,
            ));
        }

        // Broadcast game over
        let (is_landlord_win, landlord_position, score) = {
            let mut s = state.lock().unwrap();
            let is_win = s
                .landlord_position
                .map(|lp| winner_pos == Some(lp))
                .unwrap_or(false);
            let landlord_position = s.landlord_position;
            let score = s.score;
            s.apply_settlement_scores(is_win);
            (is_win, landlord_position, score)
        };
        dispatch.extend(dispatch_all(
            room_key,
            WsCode::GAME_OVER as i32,
            serde_json::to_value(WsLandlordGameOverEvent {
                is_landlord: is_landlord_win,
            })
            .unwrap_or_default(),
            &rs,
        ));
        drop(rs);
        crate::official::settle_round(
            room_service,
            room_key,
            expected_common,
            landlord_position,
            is_landlord_win,
            score,
        )
        .await;
        send_dispatch(dispatch, senders).await;
        return true;
    }

    false
}

async fn handle_settlement_phase(
    room_key: &str,
    state: &Arc<std::sync::Mutex<LandlordLoopState>>,
    configs: &std::collections::HashMap<String, i32>,
    room_service: &Arc<Mutex<RoomService>>,
    senders: &SessionSenders,
    expected_common: &Arc<std::sync::Mutex<ws_common::CommonGameState>>,
) -> bool {
    // A disconnected seat stays in the roster and is auto-played as away.
    // Common requests stop only when a player really quits or the final
    // connected human leaves, so a partial disconnect must not end the loop.
    if settlement_should_stop(state) {
        return true;
    }
    if sleep_or_stop(
        state,
        Duration::from_secs(fixed_wait_seconds(configs, "settlement_time", 5)),
    )
    .await
    {
        return true;
    }
    let (phase, position) = {
        let mut s = state.lock().unwrap();
        if s.phase != LandlordPhase::Settlement {
            return false;
        }
        if s.stop_requested() || s.players_snapshot().len() != 3 {
            return true;
        }
        s.redeal();
        (s.phase as i32, s.current_position as i32)
    };
    let rs = room_service.lock().await;
    if !room_matches_common(&rs, room_key, expected_common) {
        return true;
    }
    let dispatch = dispatch_all(
        room_key,
        WsCode::CHANGE_PHASE as i32,
        serde_json::json!({
            "phase": phase,
            "position": position,
        }),
        &rs,
    );
    drop(rs);
    send_dispatch(dispatch, senders).await;
    false
}

// ─── Phase Handlers ───────────────────────────────────────────────────

/// Start → deal cards, then immediately advance to CallLandlord.
async fn handle_start_phase(
    room_key: &str,
    state: &Arc<std::sync::Mutex<LandlordLoopState>>,
    sorted_positions: &[usize],
    room_service: &Arc<Mutex<RoomService>>,
    senders: &SessionSenders,
    configs: &std::collections::HashMap<String, i32>,
    expected_common: &Arc<std::sync::Mutex<ws_common::CommonGameState>>,
) -> bool {
    // 1. Deal cards inside lock
    let deal_data: Vec<(usize, Vec<i32>, String)>;

    {
        let mut s = state.lock().unwrap();
        if s.stop_requested() || s.players_snapshot().len() != sorted_positions.len() {
            return true;
        }
        if s.phase != LandlordPhase::Start {
            return false;
        }
        s.generate_card();
        deal_data = sorted_positions
            .iter()
            .filter_map(|&pos| {
                let hand = s.hands.get(&pos)?.clone();
                Some((pos, hand, player_name(&s, pos)))
            })
            .collect();
    };

    // 2. Send events (outside lock)
    let rs = room_service.lock().await;
    if !room_matches_common(&rs, room_key, expected_common) {
        return true;
    }
    let mut dispatch = Vec::new();
    for (pos, hand, name) in &deal_data {
        dispatch.extend(dispatch_to_position(
            room_key,
            *pos,
            WsCode::DEAL as i32,
            serde_json::to_value(WsDealEvent {
                name: name.clone(),
                cards: hand.clone(),
            })
            .unwrap_or_default(),
            &rs,
        ));
    }
    drop(rs);
    send_dispatch(dispatch, senders).await;

    let (first_call_position, phase_payload) = {
        let mut s = state.lock().unwrap();
        if s.stop_requested() || s.players_snapshot().len() != sorted_positions.len() {
            return true;
        }
        if s.phase != LandlordPhase::Start {
            return false;
        }
        s.next_phase(); // Start → CallLandlord
        s.set_action_received(false);
        s.set_turn_countdown(turn_timeout(&s, configs));
        (
            s.current_position,
            (s.phase as i32, s.current_position as i32),
        )
    };
    let rs = room_service.lock().await;
    if !room_matches_common(&rs, room_key, expected_common) {
        return true;
    }
    let mut dispatch = dispatch_all(
        room_key,
        WsCode::CHANGE_PHASE as i32,
        serde_json::json!({
            "phase": phase_payload.0,
            "position": phase_payload.1,
        }),
        &rs,
    );
    dispatch.extend(dispatch_all(
        room_key,
        WsCode::CHANGE_DEAL as i32,
        serde_json::json!({
            "position": first_call_position as i32,
            "turn_countdown": state.lock().unwrap().turn_countdown() as i32,
        }),
        &rs,
    ));
    drop(rs);
    send_dispatch(dispatch, senders).await;
    false
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AutoActionReason {
    Ai,
    MemberTimeout,
    Timeout,
}

/// Execute an AI action or take over a timed-out human action. Only real
/// timeouts mark a seat away; an AI seat remains an active virtual player.
fn handle_automatic_action(
    state: &mut LandlordLoopState,
    reason: AutoActionReason,
) -> (Option<usize>, Option<AutoBroadcastEvent>) {
    let pos = state.current_position;
    let newly_away = if matches!(
        reason,
        AutoActionReason::MemberTimeout | AutoActionReason::Timeout
    ) && state.mark_away(pos)
    {
        Some(pos)
    } else {
        None
    };
    let mut auto_event = None;
    // 广播away

    match state.phase {
        LandlordPhase::CallLandlord => {
            let score = if reason != AutoActionReason::Timeout {
                choose_bid(state, pos)
            } else {
                0
            };
            let name = state.player_name(pos);
            if score > 0 {
                state.score = score as u32;
            }
            state.call_history.push((pos, score));
            println!(
                "[landlord][auto-call] pos={} name={} reason={:?} score={} history_len={}",
                pos,
                name,
                reason,
                score,
                state.call_history.len()
            );
            auto_event = Some(AutoBroadcastEvent::Call(WsCallLandlordEvent {
                name,
                score,
            }));
            next_call(state);
        }
        LandlordPhase::Play => {
            let auto_cards = if reason != AutoActionReason::Timeout {
                choose_play(state, pos)
            } else {
                choose_timeout_play(state, pos)
            };
            let name = state.player_name(pos);
            println!(
                "[landlord][auto-play] pos={} name={} reason={:?} cards={:?}",
                pos, name, reason, auto_cards
            );
            auto_event = Some(AutoBroadcastEvent::Play(WsPlayEvent {
                name,
                cards: auto_cards.clone(),
            }));
            state.current_play = auto_cards;
            next_play(state);
        }
        _ => {}
    }
    (newly_away, auto_event)
}

fn choose_timeout_play(state: &LandlordLoopState, position: usize) -> Vec<i32> {
    if !state.last_play.is_empty() && state.last_play_position != position {
        return Vec::new();
    }
    state
        .hands
        .get(&position)
        .and_then(|hand| hand.first().copied())
        .map(|card| vec![card])
        .unwrap_or_default()
}

fn loop_should_stop(state: &Arc<std::sync::Mutex<LandlordLoopState>>) -> bool {
    let s = state.lock().unwrap();
    s.stop_requested() || s.players_snapshot().len() != 3
}

fn next_call(state: &mut LandlordLoopState) {
    state.set_action_received(true);
}

fn next_play(state: &mut LandlordLoopState) {
    state.set_action_received(true);
}

// ─── Helpers ──────────────────────────────────────────────────────────

fn player_name(state: &LandlordLoopState, position: usize) -> String {
    state.player_name(position)
}

/// Send a dispatch to all recipients via session senders.
async fn send_dispatch(dispatch: Vec<Delivery>, senders: &SessionSenders) {
    let mut encoded = Vec::with_capacity(dispatch.len());
    for delivery in dispatch {
        if let Ok(frame) = ws_common::to_text_message(&delivery.payload) {
            encoded.push((delivery.recipient, frame));
        }
    }
    let senders = senders.lock().await;
    for (recipient, frame) in encoded {
        if let Some(tx) = senders.get(&recipient) {
            let _ = tx.send(frame);
        }
    }
}

async fn position_has_active_membership(
    room_service: &Arc<Mutex<RoomService>>,
    room_key: &str,
    position: usize,
) -> bool {
    let official_session_id = room_service
        .lock()
        .await
        .room_position_official_session_id(room_key, position);
    match official_session_id {
        Some(session_id) => crate::official::has_active_membership(session_id).await,
        None => false,
    }
}

fn settlement_should_stop(state: &Arc<std::sync::Mutex<LandlordLoopState>>) -> bool {
    let s = state.lock().unwrap();
    s.stop_requested() || s.players_snapshot().len() != 3
}

/// Promote disconnected seats to away immediately instead of waiting for the
/// normal human timeout. The current turn keeps any shorter remaining timeout,
/// but is capped to `away_time` as soon as the loop observes the disconnect.
fn synchronize_disconnected_players(
    state: &mut LandlordLoopState,
    configs: &std::collections::HashMap<String, i32>,
) -> Vec<usize> {
    if !matches!(
        state.phase,
        LandlordPhase::CallLandlord | LandlordPhase::Play
    ) {
        return Vec::new();
    }

    let disconnected_positions = state
        .players_snapshot()
        .keys()
        .copied()
        .filter(|position| state.is_disconnected(*position))
        .collect::<Vec<_>>();
    let newly_away = disconnected_positions
        .iter()
        .copied()
        .filter(|position| state.mark_away(*position))
        .collect::<Vec<_>>();

    if !state.action_received() && state.is_disconnected(state.current_position) {
        let away_time = configs.get("away_time").copied().unwrap_or(5).max(0) as u32;
        if state.turn_countdown() > away_time {
            state.set_turn_countdown(away_time);
        }
    }

    newly_away
}

async fn sleep_or_stop(
    state: &Arc<std::sync::Mutex<LandlordLoopState>>,
    duration: Duration,
) -> bool {
    let mut remaining = duration.as_millis();
    while remaining > 0 {
        if loop_should_stop(state) {
            return true;
        }
        let step = remaining.min(100) as u64;
        tokio::time::sleep(Duration::from_millis(step)).await;
        remaining -= u128::from(step);
    }
    loop_should_stop(state)
}

// ─── Game Loop ────────────────────────────────────────────────────────

pub(crate) fn start_game_loop(
    room_key: String,
    state: Arc<std::sync::Mutex<LandlordLoopState>>,
    room_service: Arc<Mutex<RoomService>>,
    senders: SessionSenders,
    loop_states: Arc<std::sync::Mutex<HashMap<String, Arc<std::sync::Mutex<LandlordLoopState>>>>>,
) {
    tokio::spawn(async move {
        let expected_common = { Arc::clone(&state.lock().unwrap().base) };
        let sorted_positions: Vec<usize> = {
            let s = state.lock().unwrap();
            let mut p: Vec<usize> = s.players_snapshot().keys().copied().collect();
            p.sort();
            p
        };

        // Read configs once; they won't change mid-game.
        let configs: std::collections::HashMap<String, i32> = {
            let room_service = room_service.lock().await;
            if room_matches_common(&room_service, &room_key, &expected_common) {
                room_service.room_configs(&room_key).unwrap_or_default()
            } else {
                HashMap::new()
            }
        };

        loop {
            if state.lock().unwrap().stop_requested() {
                break;
            }
            let room_is_current = {
                let room_service = room_service.lock().await;
                room_matches_common(&room_service, &room_key, &expected_common)
            };
            if !room_is_current {
                break;
            }

            let disconnected_away_positions = {
                let mut state = state.lock().unwrap();
                synchronize_disconnected_players(&mut state, &configs)
            };
            if !disconnected_away_positions.is_empty() {
                let room_service = room_service.lock().await;
                if !room_matches_common(&room_service, &room_key, &expected_common) {
                    break;
                }
                let mut dispatch = Vec::new();
                for position in disconnected_away_positions {
                    dispatch.extend(dispatch_all(
                        &room_key,
                        WsCode::AWAY as i32,
                        serde_json::to_value(WsPositionEvent {
                            position: position as i32,
                            is_ai_takeover: room_service
                                .room_position_is_ai_takeover(&room_key, position),
                        })
                        .unwrap_or_default(),
                        &room_service,
                    ));
                }
                drop(room_service);
                send_dispatch(dispatch, &senders).await;
            }

            let paused = { state.lock().unwrap().is_paused() };
            if paused {
                if sleep_or_stop(&state, Duration::from_secs(1)).await {
                    break;
                }
                continue;
            }

            let current_phase = { state.lock().unwrap().phase };
            let player_num = state.lock().unwrap().base.lock().unwrap().players.len();
            dlog!(
                ws_common::tracing::Level::INFO,
                "[landlord][game-loop] room={} phase={:?} play number ={} action_received={}",
                room_key,
                current_phase,
                player_num,
                state.lock().unwrap().action_received()
            );
            if player_num != 3 {
                break;
            }

            match current_phase {
                LandlordPhase::Start => {
                    let should_stop = handle_start_phase(
                        &room_key,
                        &state,
                        &sorted_positions,
                        &room_service,
                        &senders,
                        &configs,
                        &expected_common,
                    )
                    .await;
                    if should_stop {
                        break;
                    }
                }
                LandlordPhase::CallLandlord | LandlordPhase::Play => {
                    let mut away_position: Option<usize> = None;
                    let mut auto_event: Option<AutoBroadcastEvent> = None;
                    let pending_turn = {
                        let s = state.lock().unwrap();
                        (!s.action_received()).then_some((
                            s.phase,
                            s.current_position,
                            s.is_ai_controlled_position(s.current_position),
                        ))
                    };
                    if let Some((waiting_phase, waiting_position, waiting_for_ai)) = pending_turn {
                        let wait_duration = if waiting_for_ai {
                            AI_ACTION_DELAY
                        } else {
                            Duration::from_secs(1)
                        };
                        if sleep_or_stop(&state, wait_duration).await {
                            break;
                        }
                        if state.lock().unwrap().is_paused() {
                            continue;
                        }
                        let member_timeout_authorized = if waiting_for_ai {
                            false
                        } else {
                            let timed_out = {
                                let s = state.lock().unwrap();
                                s.phase == waiting_phase
                                    && s.current_position == waiting_position
                                    && !s.action_received()
                                    && s.turn_countdown() == 0
                                    && !s.is_ai_controlled_position(waiting_position)
                            };
                            timed_out
                                && position_has_active_membership(
                                    &room_service,
                                    &room_key,
                                    waiting_position,
                                )
                                .await
                        };
                        let mut s = state.lock().unwrap();
                        if s.phase != waiting_phase || s.current_position != waiting_position {
                            continue;
                        }
                        if s.action_received() {
                            // Action received while waiting this tick.
                        } else if waiting_for_ai && s.is_ai_controlled_position(waiting_position) {
                            (away_position, auto_event) =
                                handle_automatic_action(&mut s, AutoActionReason::Ai);
                        } else if s.turn_countdown() > 0 {
                            let next_countdown = s.turn_countdown() - 1;
                            s.set_turn_countdown(next_countdown);
                            continue;
                        } else {
                            if member_timeout_authorized {
                                s.base
                                    .lock()
                                    .unwrap()
                                    .mark_ai_takeover_position(waiting_position);
                            }
                            (away_position, auto_event) = handle_automatic_action(
                                &mut s,
                                if member_timeout_authorized {
                                    AutoActionReason::MemberTimeout
                                } else {
                                    AutoActionReason::Timeout
                                },
                            );
                        }
                    }
                    if away_position.is_some() || auto_event.is_some() {
                        let rs = room_service.lock().await;
                        if !room_matches_common(&rs, &room_key, &expected_common) {
                            break;
                        }
                        let mut dispatch = Vec::new();
                        if let Some(position) = away_position {
                            dispatch.extend(dispatch_all(
                                &room_key,
                                WsCode::AWAY as i32,
                                serde_json::to_value(WsPositionEvent {
                                    position: position as i32,
                                    is_ai_takeover: rs
                                        .room_position_is_ai_takeover(&room_key, position),
                                })
                                .unwrap_or_default(),
                                &rs,
                            ));
                        }
                        if let Some(event) = auto_event {
                            match event {
                                AutoBroadcastEvent::Call(payload) => {
                                    dispatch.extend(dispatch_all(
                                        &room_key,
                                        WsCode::CALL_LANDLORD as i32,
                                        serde_json::to_value(payload).unwrap_or_default(),
                                        &rs,
                                    ));
                                }
                                AutoBroadcastEvent::Play(payload) => {
                                    dispatch.extend(dispatch_all(
                                        &room_key,
                                        WsCode::PLAY as i32,
                                        serde_json::to_value(payload).unwrap_or_default(),
                                        &rs,
                                    ));
                                }
                            }
                        }
                        drop(rs);
                        send_dispatch(dispatch, &senders).await;
                    }

                    if loop_should_stop(&state) {
                        break;
                    }

                    let phase_after_tick = { state.lock().unwrap().phase };
                    match phase_after_tick {
                        LandlordPhase::CallLandlord => {
                            handle_call_landlord_phase(
                                &room_key,
                                &state,
                                &sorted_positions,
                                &configs,
                                &room_service,
                                &senders,
                                &expected_common,
                            )
                            .await;
                        }
                        LandlordPhase::Play => {
                            let _ended = handle_play_phase(
                                &state,
                                &sorted_positions,
                                &configs,
                                &room_key,
                                &room_service,
                                &senders,
                                &expected_common,
                            )
                            .await;
                        }
                        _ => {}
                    }
                }
                LandlordPhase::Settlement => {
                    if handle_settlement_phase(
                        &room_key,
                        &state,
                        &configs,
                        &room_service,
                        &senders,
                        &expected_common,
                    )
                    .await
                    {
                        break;
                    }
                }
            }
        }

        // Cleanup
        room_service
            .lock()
            .await
            .clear_room_game_state_if_same(&room_key, &expected_common);
        let mut states = loop_states.lock().unwrap();
        if states
            .get(&room_key)
            .is_some_and(|current| Arc::ptr_eq(current, &state))
        {
            states.remove(&room_key);
        }
    });
}

/// Get the per-turn timeout for the current position:
/// AI players act after a short millisecond delay, away players use away_time,
/// and active human players use play_time.
fn turn_timeout(
    state: &LandlordLoopState,
    configs: &std::collections::HashMap<String, i32>,
) -> u32 {
    let away_time = configs.get("away_time").copied().unwrap_or(5).max(0) as u32;
    let play_time = configs.get("play_time").copied().unwrap_or(30).max(0) as u32;
    if state.is_ai_controlled_position(state.current_position) {
        1
    } else if state.is_away(state.current_position) || state.is_disconnected(state.current_position)
    {
        away_time
    } else {
        play_time
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use ws_common::CommonGameState;

    use super::*;

    fn state_with_ai(ai_position: usize) -> LandlordLoopState {
        let mut common = CommonGameState::new();
        for position in 0..3 {
            common.add_player(position, position as u64 + 1, &format!("P{position}"));
        }
        common.mark_ai_position(ai_position);
        let mut state = LandlordLoopState::new(Arc::new(Mutex::new(common)));
        state.hands = HashMap::from([
            (0, vec![1, 14, 27, 2, 15]),
            (1, vec![3, 16, 4, 17]),
            (2, vec![5, 18, 6, 19]),
        ]);
        state
    }

    #[test]
    fn ai_play_acts_without_becoming_away() {
        let mut state = state_with_ai(1);
        state.phase = LandlordPhase::Play;
        state.landlord_position = Some(0);
        state.current_position = 1;
        state.last_play_position = 0;
        state.last_play = vec![2];

        let (away_position, event) = handle_automatic_action(&mut state, AutoActionReason::Ai);

        assert_eq!(away_position, None);
        assert!(!state.is_away(1));
        assert!(state.action_received());
        assert!(!state.current_play.is_empty());
        assert!(matches!(event, Some(AutoBroadcastEvent::Play(_))));
    }

    #[test]
    fn human_timeout_marks_away_and_passes_bid() {
        let mut state = state_with_ai(2);
        state.phase = LandlordPhase::CallLandlord;
        state.current_position = 0;

        let (away_position, event) = handle_automatic_action(&mut state, AutoActionReason::Timeout);

        assert_eq!(away_position, Some(0));
        assert!(state.is_away(0));
        assert_eq!(state.call_history, vec![(0, 0)]);
        assert!(matches!(event, Some(AutoBroadcastEvent::Call(_))));
    }

    #[test]
    fn member_timeout_marks_away_and_uses_ai_for_the_same_action() {
        let mut state = state_with_ai(2);
        state.phase = LandlordPhase::Play;
        state.landlord_position = Some(0);
        state.current_position = 0;
        state.last_play_position = 0;
        state.last_play.clear();
        let expected = choose_play(&state, 0);
        assert!(
            expected.len() > 1,
            "test hand must distinguish AI from fallback"
        );
        state.base.lock().unwrap().mark_ai_takeover_position(0);

        let (away_position, event) =
            handle_automatic_action(&mut state, AutoActionReason::MemberTimeout);

        assert_eq!(away_position, Some(0));
        assert!(state.is_away(0));
        assert_eq!(state.current_play, expected);
        assert!(matches!(event, Some(AutoBroadcastEvent::Play(_))));
    }

    #[test]
    fn ai_timeout_is_short_without_changing_human_timeouts() {
        let mut state = state_with_ai(1);
        let configs = HashMap::from([("away_time".to_owned(), 4), ("play_time".to_owned(), 35)]);

        state.current_position = 1;
        assert_eq!(turn_timeout(&state, &configs), 1);

        state.current_position = 0;
        assert_eq!(turn_timeout(&state, &configs), 35);
        state.mark_away(0);
        assert_eq!(turn_timeout(&state, &configs), 4);
    }

    #[test]
    fn member_takeover_uses_ai_delay_and_bid_strategy() {
        let mut state = state_with_ai(2);
        let configs = HashMap::from([("away_time".to_owned(), 4), ("play_time".to_owned(), 35)]);
        state.phase = LandlordPhase::CallLandlord;
        state.current_position = 0;
        state.mark_away(0);
        state.base.lock().unwrap().mark_ai_takeover_position(0);
        let expected_bid = choose_bid(&state, 0);

        assert_eq!(turn_timeout(&state, &configs), 1);
        let (away_position, event) = handle_automatic_action(&mut state, AutoActionReason::Ai);

        assert_eq!(away_position, None);
        assert!(matches!(
            event,
            Some(AutoBroadcastEvent::Call(WsCallLandlordEvent { score, .. }))
                if score == expected_bid
        ));
    }

    #[test]
    fn nonmember_away_keeps_timeout_fallback() {
        let mut state = state_with_ai(2);
        let configs = HashMap::from([("away_time".to_owned(), 4), ("play_time".to_owned(), 35)]);
        state.phase = LandlordPhase::Play;
        state.current_position = 0;
        state.last_play_position = 1;
        state.last_play = vec![3];
        state.mark_away(0);

        assert_eq!(turn_timeout(&state, &configs), 4);
        let (_, event) = handle_automatic_action(&mut state, AutoActionReason::Timeout);
        assert!(matches!(
            event,
            Some(AutoBroadcastEvent::Play(WsPlayEvent { cards, .. })) if cards.is_empty()
        ));
    }

    #[test]
    fn disconnected_human_is_immediately_away_and_uses_away_timeout() {
        let mut state = state_with_ai(2);
        let configs = HashMap::from([("away_time".to_owned(), 4), ("play_time".to_owned(), 35)]);
        state.phase = LandlordPhase::CallLandlord;
        state.current_position = 0;
        state.set_turn_countdown(35);
        state.base.lock().unwrap().mark_disconnected(0);

        let newly_away = synchronize_disconnected_players(&mut state, &configs);

        assert_eq!(newly_away, vec![0]);
        assert!(state.is_away(0));
        assert_eq!(state.turn_countdown(), 4);
        assert_eq!(turn_timeout(&state, &configs), 4);
        assert!(synchronize_disconnected_players(&mut state, &configs).is_empty());
    }

    #[test]
    fn settlement_continues_with_a_disconnected_seat_until_common_requests_stop() {
        let mut state = state_with_ai(2);
        state.phase = LandlordPhase::Settlement;
        state.base.lock().unwrap().mark_disconnected(0);
        let state = Arc::new(Mutex::new(state));

        assert!(!settlement_should_stop(&state));

        state.lock().unwrap().request_stop();
        assert!(settlement_should_stop(&state));
    }
}
