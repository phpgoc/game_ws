use std::sync::Arc;
use std::time::Duration;

use share_type_public::{
    LandlordPhase, WsCode,
    games::landlord::{
        WsDealEvent, WsDealFaceDownCardsEvent,
        WsShowHiddenCardsEvent, WsLandlordGameOverEvent,
    },
};
use tokio::sync::Mutex;
use ws_common::{Delivery, OutboundPayload, RoomService, SessionSenders};

use crate::game_state::LandlordLoopState;

// ─── Helpers ──────────────────────────────────────────────────────────

fn player_name(state: &LandlordLoopState, position: usize) -> String {
    state.base.player_name(position)
}

/// Get the per-turn timeout for the current position:
/// away players use away_time, others use play_time.
fn turn_timeout(state: &LandlordLoopState, configs: &std::collections::HashMap<String, i32>) -> u32 {
    let away_time = configs.get("away_time").copied().unwrap_or(5) as u32;
    let play_time = configs.get("play_time").copied().unwrap_or(30) as u32;
    if state.base.is_away(state.current_position) {
        away_time
    } else {
        play_time
    }
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

/// Build a Dispatch that sends an event to all room members.
fn dispatch_all(
    room_key: &str,
    code: i32,
    data: serde_json::Value,
    room_service: &RoomService,
) -> Vec<Delivery> {
    room_service
        .get_room_members(room_key)
        .iter()
        .map(|(sid, _, _)| Delivery {
            recipient: *sid,
            payload: OutboundPayload::Event(share_type_public::CommonEvent {
                code,
                data: data.clone(),
            }),
        })
        .collect()
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
        .get_room_members(room_key)
        .iter()
        .filter_map(|(sid, _, pos)| {
            if *pos == position {
                Some(Delivery {
                    recipient: *sid,
                    payload: OutboundPayload::Event(share_type_public::CommonEvent {
                        code,
                        data: data.clone(),
                    }),
                })
            } else {
                None
            }
        })
        .collect()
}

// ─── Phase Handlers ───────────────────────────────────────────────────

/// Start → deal cards, advance to CallLandlord, set timer.
async fn handle_start_phase(
    room_key: &str,
    state: &Arc<std::sync::Mutex<LandlordLoopState>>,
    sorted_positions: &[usize],
    room_service: &Arc<Mutex<RoomService>>,
    senders: &SessionSenders,
    configs: &std::collections::HashMap<String, i32>,
) {
    // 1. Deal cards & advance phase inside lock
    let deal_data: Vec<(usize, Vec<i32>, String)>;
    let hidden: Vec<i32>;

    {
        let mut s = state.lock().unwrap();
        s.generate_card();
        s.next_phase(); // Start → CallLandlord
        s.base.action_received = false;
        s.base.turn_countdown = turn_timeout(&s, configs);

        hidden = s.hidden_cards.clone();
        deal_data = sorted_positions
            .iter()
            .filter_map(|&pos| {
                let hand = s.hands.get(&pos)?.clone();
                Some((pos, hand, player_name(&s, pos)))
            })
            .collect();
    }

    // 2. Send events (outside lock)
    let rs = room_service.lock().await;
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
    dispatch.extend(dispatch_all(
        room_key,
        WsCode::DEAL_FACE_DOWN_CARDS as i32,
        serde_json::to_value(WsDealFaceDownCardsEvent { cards: hidden })
            .unwrap_or_default(),
        &rs,
    ));
    drop(rs);
    send_dispatch(dispatch, senders).await;
}

/// Advance through the CallLandlord phase: move to next player, check completion.
///
/// When a full round completes (we're back at call_position), determine landlord:
/// - If nobody called (score == 0) → redeal
/// - Otherwise, the highest bidder (last in call_history with max score) becomes landlord
async fn handle_call_landlord_phase(
    state: &Arc<std::sync::Mutex<LandlordLoopState>>,
    sorted_positions: &[usize],
    configs: &std::collections::HashMap<String, i32>,
) {
    let mut s = state.lock().unwrap();

    // Move to next position
    let current_idx = sorted_positions
        .iter()
        .position(|&p| p == s.current_position)
        .unwrap_or(0);
    s.current_position = sorted_positions[(current_idx + 1) % sorted_positions.len()];

    // If we've looped back to call_position, one full round is done
    if s.current_position == s.call_position {
        if s.score == 0 {
            // Everyone passed → redeal
            s.redeal();
            return;
        }

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
            hand.extend(hidden);
            hand.sort_unstable();
        }
        // Reset for landlord's first play
        s.base.action_received = false;
        s.base.turn_countdown = turn_timeout(&s, configs);
        return;
    }

    // Reset for next caller in the same round
    s.base.action_received = false;
    s.base.turn_countdown = turn_timeout(&s, configs);
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
) -> bool {
    let mut game_over = false;
    let mut winner_pos = None;

    {
        let mut s = state.lock().unwrap();
        let pos = s.current_position;
        let played = std::mem::take(&mut s.current_play);

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
            let current_idx = sorted_positions
                .iter()
                .position(|&p| p == pos)
                .unwrap_or(0);
            let next_pos = sorted_positions[(current_idx + 1) % sorted_positions.len()];
            s.current_position = next_pos;

            // If we've come full circle back to the last_play_position,
            // that means everyone else passed → last_play_position leads a new round
            if s.last_play_position == next_pos {
                s.last_play.clear();
            }

            // Reset for next turn
            s.base.action_received = false;
            s.base.turn_countdown = turn_timeout(&s, configs);
        }
    }

    if game_over {
        let rs = room_service.lock().await;
        let mut dispatch = Vec::new();

        // Broadcast hidden cards reveal
        let hidden = state.lock().unwrap().hidden_cards.clone();
        dispatch.extend(dispatch_all(
            room_key,
            WsCode::SHOW_HIDDEN_CARDS as i32,
            serde_json::to_value(WsShowHiddenCardsEvent { cards: hidden })
                .unwrap_or_default(),
            &rs,
        ));

        // Broadcast game over
        let landlord = state.lock().unwrap().landlord_position;
        let is_landlord_win = landlord.map(|lp| winner_pos == Some(lp)).unwrap_or(false);
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
        send_dispatch(dispatch, senders).await;
        return true;
    }

    false
}

/// Handle timeout: mark the current player as away and simulate their action.
fn handle_timeout(state: &mut LandlordLoopState, _phase: LandlordPhase) {
    let pos = state.current_position;
    state.base.mark_away(pos);

    match state.phase {
        LandlordPhase::CallLandlord => {
            // Timed out = no call (score 0). Record it.
            state.call_history.push((pos, 0));
            state.base.action_received = true;
        }
        LandlordPhase::Play => {
            // Timed out = pass (empty play)
            state.current_play = Vec::new();
            state.base.action_received = true;
        }
        _ => {}
    }
}

// ─── Game Loop ────────────────────────────────────────────────────────

pub(crate) fn start_game_loop(
    room_key: String,
    state: Arc<std::sync::Mutex<LandlordLoopState>>,
    room_service: Arc<Mutex<RoomService>>,
    senders: SessionSenders,
) {
    tokio::spawn(async move {
        let sorted_positions: Vec<usize> = {
            let s = state.lock().unwrap();
            let mut p: Vec<usize> = s.base.players.keys().copied().collect();
            p.sort();
            p
        };

        // Read configs once; they won't change mid-game.
        let configs: std::collections::HashMap<String, i32> = {
            room_service
                .lock()
                .await
                .get_room_configs(&room_key)
                .unwrap_or_default()
        };

        loop {
            // Tick once per second
            tokio::time::sleep(Duration::from_secs(1)).await;

            // ─── Phase-independent checks ─────────────────────────────
            {
                let s = state.lock().unwrap();
                if s.base.paused {
                    continue;
                }
            }

            // ─── Tick: countdown or timeout ───────────────────────────
            let phase = {
                let s = state.lock().unwrap();
                s.phase
            };

            if matches!(phase, LandlordPhase::CallLandlord | LandlordPhase::Play) {
                let mut s = state.lock().unwrap();
                if s.base.action_received {
                    // Action received — let phase handler process it
                } else if s.base.turn_countdown > 0 {
                    s.base.turn_countdown -= 1;
                    continue; // Wait for action or timeout
                } else {
                    // Timeout
                    handle_timeout(&mut s, phase);
                }
            }

            // ─── Phase dispatch ────────────────────────────────────────
            let current_phase = { state.lock().unwrap().phase };

            match current_phase {
                LandlordPhase::Start => {
                    handle_start_phase(
                        &room_key,
                        &state,
                        &sorted_positions,
                        &room_service,
                        &senders,
                        &configs,
                    )
                    .await;
                }
                LandlordPhase::CallLandlord => {
                    handle_call_landlord_phase(&state, &sorted_positions, &configs).await;
                }
                LandlordPhase::Play => {
                    let ended = handle_play_phase(
                        &state,
                        &sorted_positions,
                        &configs,
                        &room_key,
                        &room_service,
                        &senders,
                    )
                    .await;
                    if ended {
                        break;
                    }
                }
                LandlordPhase::Settlement => {
                    break;
                }
            }
        }

        // Cleanup
        room_service.lock().await.clear_room_game_state(&room_key);
    });
}
