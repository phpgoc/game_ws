use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use share_type_public::WsCode;
use tokio::sync::Mutex;
use ws_common::{RoomService, SessionSenders};

use crate::ai::{maybe_play_ai_turn, maybe_resolve_ai_claims};
use crate::game::{
    LoopStateRegistry, current_play_time, perform_discard, push_phase_change,
    push_private_deal_events, push_room_event, resolve_claim_window, settlement_time, start_time,
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

pub(crate) fn start_game_loop(
    room_key: String,
    state: Arc<std::sync::Mutex<ShenyangMahjongLoopState>>,
    room_service: Arc<Mutex<RoomService>>,
    senders: SessionSenders,
    loop_states: LoopStateRegistry,
) {
    tokio::spawn(async move {
        let configs: HashMap<String, i32> = room_service
            .lock()
            .await
            .get_room_configs(&room_key)
            .unwrap_or_default();

        loop {
            if state.lock().unwrap().is_paused() {
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
            let phase = { state.lock().unwrap().phase };
            match phase {
                ShenyangMahjongPhase::Start => {
                    tokio::time::sleep(Duration::from_secs(start_time(&configs))).await;
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
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    if state.lock().unwrap().is_paused() {
                        continue;
                    }

                    let mut ai_dispatch = ws_common::Dispatch::default();
                    let ai_acted = {
                        let room = room_service.lock().await;
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
                            if let Some(claim_window) = &guard.claim_window {
                                should_resolve_claims = !claim_window.eligible_positions.is_empty();
                            } else {
                                should_auto_discard =
                                    auto_discard_tile(&guard, guard.current_position)
                                        .map(|tile| (guard.current_position, tile));
                            }
                        }
                    }

                    if should_resolve_claims {
                        let mut dispatch = ws_common::Dispatch::default();
                        {
                            let room = room_service.lock().await;
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
                            let mut guard = state.lock().unwrap();
                            if guard.current_position != position || guard.claim_window.is_some() {
                                continue;
                            }
                            if !perform_discard(
                                &room,
                                &room_key,
                                &mut guard,
                                &configs,
                                &mut dispatch,
                                position,
                                tile,
                            ) {
                                continue;
                            }
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
                    tokio::time::sleep(Duration::from_secs(settlement_time(&configs))).await;
                    if settlement_should_stop(&state) {
                        break;
                    }
                    {
                        let mut guard = state.lock().unwrap();
                        guard.redeal();
                    }
                    let mut dispatch = ws_common::Dispatch::default();
                    {
                        let room = room_service.lock().await;
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

        loop_states.lock().unwrap().remove(&room_key);
    });
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use ws_common::game_state::CommonGameState;

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
}
