use std::{sync::Arc, time::Duration};

use share_type_public::{
    CommonEvent, UpgradePhase, UpgradeWsCode, WsCode, WsPositionEvent, WsUpgradeBottomBuriedEvent,
    WsUpgradeBottomCardsEvent, WsUpgradeDealEvent, WsUpgradeHandEvent, WsUpgradePlayEvent,
};
use tokio::sync::Mutex;
use ws_common::{
    Delivery, Dispatch, GameState, OutboundPayload, RoomService, SessionId, SessionSenders,
    to_text_message,
};

use crate::{
    game::StateRegistry,
    game_setting::{KEY_AWAY_TIME, KEY_DEAL_TIME, KEY_FIRST_DEAL_TIME, KEY_PLAY_TIME},
    state::{PLAYER_COUNT, UpgradeStateHandle},
};

const SETTLEMENT_DELAY: Duration = Duration::from_secs(3);

fn stop_requested(state: &UpgradeStateHandle) -> bool {
    state.lock().unwrap().stop_requested()
}

async fn sleep_or_stop(state: &UpgradeStateHandle, duration: Duration) -> bool {
    let mut remaining = duration.as_millis();
    while remaining > 0 {
        if stop_requested(state) {
            return true;
        }
        let step = remaining.min(100) as u64;
        tokio::time::sleep(Duration::from_millis(step)).await;
        remaining -= u128::from(step);
    }
    stop_requested(state)
}

pub fn start_upgrade_game_loop(
    room_key: String,
    state: UpgradeStateHandle,
    room_service: Arc<Mutex<RoomService>>,
    senders: SessionSenders,
    states: StateRegistry,
) {
    tokio::spawn(async move {
        let common = { Arc::clone(&state.lock().unwrap().base) };
        let configs = {
            let room = room_service.lock().await;
            if !room_uses_common_state(&room, &room_key, &common) {
                drop(room);
                remove_registered_state_if_same(&states, &room_key, &state);
                return;
            }
            room.room_configs(&room_key).unwrap_or_default()
        };
        let play_time = configs.get(KEY_PLAY_TIME).copied().unwrap_or(30).max(1) as u32;
        let away_time = configs.get(KEY_AWAY_TIME).copied().unwrap_or(5).max(1) as u32;
        loop {
            let (stop_requested, paused, phase) = {
                let guard = state.lock().unwrap();
                let base = guard.base.lock().unwrap();
                (base.stop_requested(), base.paused, guard.phase)
            };
            if stop_requested {
                break;
            }
            if paused {
                if sleep_or_stop(&state, Duration::from_secs(1)).await {
                    break;
                }
                continue;
            }
            let room_is_current = {
                let room = room_service.lock().await;
                room_uses_common_state(&room, &room_key, &common)
            };
            if !room_is_current {
                break;
            }
            match phase {
                UpgradePhase::Deal => {
                    let delay = {
                        let guard = state.lock().unwrap();
                        deal_step_delay(&configs, guard.round_index, guard.total_deal_count)
                    };
                    let dispatch = {
                        let room = room_service.lock().await;
                        build_deal_dispatch(&room_key, &state, &room, &configs)
                    };
                    deliver(dispatch, &senders).await;
                    if sleep_or_stop(&state, delay).await {
                        break;
                    }
                    continue;
                }
                UpgradePhase::Bury => {
                    let dispatch =
                        timeout_bury_dispatch(&room_key, &state, &room_service, play_time).await;
                    deliver(dispatch, &senders).await;
                }
                UpgradePhase::Play => {
                    let dispatch = timeout_play_dispatch(
                        &room_key,
                        &state,
                        &room_service,
                        play_time,
                        away_time,
                    )
                    .await;
                    deliver(dispatch, &senders).await;
                }
                UpgradePhase::Settlement => {
                    if sleep_or_stop(&state, SETTLEMENT_DELAY).await {
                        break;
                    }
                    let mut dispatch = Dispatch::default();
                    let advanced = {
                        let mut guard = state.lock().unwrap();
                        if guard.stop_requested() {
                            break;
                        }
                        if guard.phase != UpgradePhase::Settlement {
                            continue;
                        }
                        guard.advance_after_settlement().unwrap_or(false)
                    };
                    if advanced {
                        let snapshot = state.lock().unwrap().snapshot();
                        let room = room_service.lock().await;
                        room.broadcast(
                            &room_key,
                            WsCode::START as i32,
                            serde_json::json!({}),
                            &mut dispatch,
                        );
                        room.broadcast(
                            &room_key,
                            WsCode::TABLE_SNAPSHOT as i32,
                            snapshot,
                            &mut dispatch,
                        );
                    } else {
                        break;
                    }
                    deliver(dispatch, &senders).await;
                }
                UpgradePhase::Start => break,
            }
            if sleep_or_stop(&state, Duration::from_secs(1)).await {
                break;
            }
        }

        room_service
            .lock()
            .await
            .clear_room_game_state_if_same(&room_key, &common);
        remove_registered_state_if_same(&states, &room_key, &state);
    });
}

fn room_uses_common_state(
    room: &RoomService,
    room_key: &str,
    common: &Arc<std::sync::Mutex<ws_common::CommonGameState>>,
) -> bool {
    room.room_common_state(room_key)
        .is_some_and(|current| Arc::ptr_eq(&current, common))
}

fn remove_registered_state_if_same(
    states: &StateRegistry,
    room_key: &str,
    state: &UpgradeStateHandle,
) {
    let mut states = states.lock().unwrap();
    if states
        .get(room_key)
        .is_some_and(|current| Arc::ptr_eq(current, state))
    {
        states.remove(room_key);
    }
}

fn build_deal_dispatch(
    room_key: &str,
    state: &UpgradeStateHandle,
    room: &RoomService,
    configs: &std::collections::HashMap<String, i32>,
) -> Dispatch {
    let mut dispatch = Dispatch::default();
    let Some((position, deal, declaration, finished, dealer, hands, bottom, snapshot)) = (|| {
        let mut guard = state.lock().unwrap();
        let (position, card, finished, declaration) = guard.deal_next_card()?;
        if finished {
            guard.set_turn_countdown(bury_window_time(configs));
        }
        let deal = WsUpgradeDealEvent {
            position: position as i32,
            cards: vec![card],
            deck_count: i32::from(guard.rules.deck_count.get()),
            hand_count: guard.private_hand(position).len() as i32,
            bottom_card_count: guard.rules.bottom_card_count as i32,
            target_rank: guard.target_rank_protocol(),
            dealt_count: guard.dealt_count as i32,
            total_deal_count: guard.total_deal_count as i32,
        };
        Some((
            position,
            deal,
            declaration,
            finished,
            guard.dealer_position,
            (0..PLAYER_COUNT)
                .map(|position| guard.private_hand(position))
                .collect::<Vec<_>>(),
            guard.exposed_bottom(),
            guard.snapshot(),
        ))
    })() else {
        return dispatch;
    };

    let members = room.room_members(room_key);
    for (session_id, _, member_position, _) in &members {
        if *member_position == position {
            push_private(
                &mut dispatch,
                *session_id,
                WsCode::DEAL as i32,
                deal.clone(),
            );
        }
    }
    if let Some(declaration) = declaration {
        room.broadcast(
            room_key,
            UpgradeWsCode::TRUMP_DECLARED as i32,
            declaration,
            &mut dispatch,
        );
    }
    if finished {
        for (session_id, _, member_position, _) in members {
            push_private(
                &mut dispatch,
                session_id,
                UpgradeWsCode::HAND_UPDATED as i32,
                WsUpgradeHandEvent {
                    position: member_position as i32,
                    cards: hands.get(member_position).cloned().unwrap_or_default(),
                },
            );
            if member_position == dealer {
                push_private(
                    &mut dispatch,
                    session_id,
                    UpgradeWsCode::BOTTOM_CARDS as i32,
                    WsUpgradeBottomCardsEvent {
                        position: dealer as i32,
                        cards: bottom.clone(),
                        required_count: bottom.len() as i32,
                    },
                );
            }
        }
    }
    room.broadcast(
        room_key,
        WsCode::TABLE_SNAPSHOT as i32,
        snapshot,
        &mut dispatch,
    );
    dispatch
}

pub(crate) fn bury_window_time(configs: &std::collections::HashMap<String, i32>) -> u32 {
    configs
        .get(KEY_PLAY_TIME)
        .copied()
        .unwrap_or(30)
        .max(1)
        .saturating_mul(3) as u32
}

fn deal_step_delay(
    configs: &std::collections::HashMap<String, i32>,
    round_index: i32,
    total_deal_count: usize,
) -> Duration {
    let regular_total = configs.get(KEY_DEAL_TIME).copied().unwrap_or(3_000).max(1) as u64;
    let total = if round_index == 0 {
        let first = configs
            .get(KEY_FIRST_DEAL_TIME)
            .copied()
            .unwrap_or(15_000)
            .max(1) as u64;
        first.max(regular_total.saturating_mul(3))
    } else {
        regular_total
    };
    Duration::from_millis((total / total_deal_count.max(1) as u64).max(1))
}

fn timed_out_human_position(
    state: &crate::state::UpgradeGameState,
    phase: UpgradePhase,
) -> Option<usize> {
    let position = match phase {
        UpgradePhase::Bury => state.dealer_position,
        UpgradePhase::Play => state.current_position,
        _ => return None,
    };
    (!is_ai_controlled_position(state, position) && state.base.lock().unwrap().turn_countdown == 0)
        .then_some(position)
}

async fn timed_out_member_position(
    room_key: &str,
    state: &UpgradeStateHandle,
    room_service: &Arc<Mutex<RoomService>>,
    phase: UpgradePhase,
) -> Option<usize> {
    let position = {
        let guard = state.lock().unwrap();
        timed_out_human_position(&guard, phase)
    }?;
    room_service
        .lock()
        .await
        .room_position_has_active_membership(room_key, position)
        .then_some(position)
}

fn mark_timed_out_human(
    state: &crate::state::UpgradeGameState,
    position: usize,
    member_timeout_position: Option<usize>,
) -> Option<WsPositionEvent> {
    let mut base = state.base.lock().unwrap();
    let away_changed = base.mark_away(position);
    let takeover_changed =
        member_timeout_position == Some(position) && base.mark_ai_takeover_position(position);
    (away_changed || takeover_changed).then(|| WsPositionEvent {
        position: position as i32,
        is_ai_takeover: base.is_ai_takeover_position(position),
    })
}

async fn timeout_bury_dispatch(
    room_key: &str,
    state: &UpgradeStateHandle,
    room_service: &Arc<Mutex<RoomService>>,
    play_time: u32,
) -> Dispatch {
    let mut dispatch = Dispatch::default();
    let member_timeout_position =
        timed_out_member_position(room_key, state, room_service, UpgradePhase::Bury).await;
    let mut away_event = None;
    let result = {
        let mut guard = state.lock().unwrap();
        let position = guard.dealer_position;
        let controlled = is_ai_controlled_position(&guard, position);
        if !controlled && guard.base.lock().unwrap().turn_countdown > 0 {
            let countdown = guard.base.lock().unwrap().turn_countdown.saturating_sub(1);
            guard.set_turn_countdown(countdown);
            None
        } else {
            if !controlled {
                away_event = mark_timed_out_human(&guard, position, member_timeout_position);
            }
            if guard.timeout_bury().unwrap_or(false) {
                guard.set_turn_countdown(play_time);
                Some((
                    guard.dealer_position,
                    guard.player_name(guard.dealer_position),
                    guard.rules.bottom_card_count,
                    guard.private_hand(guard.dealer_position),
                    guard.snapshot(),
                ))
            } else {
                None
            }
        }
    };
    let room = room_service.lock().await;
    if let Some(event) = away_event {
        room.broadcast(room_key, WsCode::AWAY as i32, event, &mut dispatch);
    }
    let Some((position, name, bottom_card_count, hand, snapshot)) = result else {
        return dispatch;
    };
    room.broadcast(
        room_key,
        UpgradeWsCode::BOTTOM_BURIED as i32,
        WsUpgradeBottomBuriedEvent {
            position: position as i32,
            name,
            bottom_card_count: bottom_card_count as i32,
        },
        &mut dispatch,
    );
    send_private(
        &room,
        room_key,
        position,
        UpgradeWsCode::HAND_UPDATED as i32,
        WsUpgradeHandEvent {
            position: position as i32,
            cards: hand,
        },
        &mut dispatch,
    );
    room.broadcast(
        room_key,
        WsCode::TABLE_SNAPSHOT as i32,
        snapshot,
        &mut dispatch,
    );
    dispatch
}

async fn timeout_play_dispatch(
    room_key: &str,
    state: &UpgradeStateHandle,
    room_service: &Arc<Mutex<RoomService>>,
    play_time: u32,
    away_time: u32,
) -> Dispatch {
    let mut dispatch = Dispatch::default();
    let member_timeout_position =
        timed_out_member_position(room_key, state, room_service, UpgradePhase::Play).await;
    let mut away_event = None;
    let result = {
        let mut guard = state.lock().unwrap();
        let position = guard.current_position;
        let controlled = is_ai_controlled_position(&guard, position);
        if !controlled && guard.base.lock().unwrap().turn_countdown > 0 {
            // A disconnect after the turn started must converge to the short
            // away window. Bury is handled separately and remains complete.
            let inactive = {
                let base = guard.base.lock().unwrap();
                base.is_away(position) || base.is_disconnected(position)
            };
            let limit = if inactive {
                away_time.max(1)
            } else {
                play_time.max(1)
            };
            let countdown = guard
                .base
                .lock()
                .unwrap()
                .turn_countdown
                .min(limit)
                .saturating_sub(1);
            guard.set_turn_countdown(countdown);
            None
        } else {
            if !controlled {
                away_event = mark_timed_out_human(&guard, position, member_timeout_position);
            }
            match guard.timeout_play() {
                Ok(Some(resolution)) => {
                    if !resolution.finished {
                        guard.set_turn_countdown(play_time);
                    }
                    Some((
                        position,
                        guard.player_name(position),
                        resolution,
                        guard.current_position,
                        guard.trick_index,
                        guard.private_hand(position).len(),
                        guard.snapshot(),
                    ))
                }
                _ => None,
            }
        }
    };
    let room = room_service.lock().await;
    if let Some(event) = away_event {
        room.broadcast(room_key, WsCode::AWAY as i32, event, &mut dispatch);
    }
    let Some((position, name, resolution, next_position, trick_index, remaining, snapshot)) =
        result
    else {
        return dispatch;
    };
    room.broadcast(
        room_key,
        WsCode::PLAY as i32,
        WsUpgradePlayEvent {
            position: position as i32,
            name,
            cards: resolution.played_cards,
            trick_index,
            next_position: next_position as i32,
            remaining_hand_count: remaining as i32,
            failed_throw: resolution.failed_throw,
        },
        &mut dispatch,
    );
    room.broadcast(
        room_key,
        WsCode::TABLE_SNAPSHOT as i32,
        snapshot,
        &mut dispatch,
    );
    if resolution.finished {
        let settlement = state.lock().unwrap().settlement_event();
        room.broadcast(
            room_key,
            WsCode::GAME_OVER as i32,
            settlement,
            &mut dispatch,
        );
    }
    dispatch
}

fn is_ai_controlled_position(state: &crate::state::UpgradeGameState, position: usize) -> bool {
    let base = state.base.lock().unwrap();
    base.is_ai_position(position) || base.is_ai_takeover_position(position)
}

fn send_private<T: serde::Serialize>(
    room: &RoomService,
    room_key: &str,
    position: usize,
    code: i32,
    payload: T,
    dispatch: &mut Dispatch,
) {
    let Some((session_id, _, _, _)) = room
        .room_members(room_key)
        .into_iter()
        .find(|(_, _, member_position, _)| *member_position == position)
    else {
        return;
    };
    dispatch.messages.push(Delivery {
        recipient: session_id,
        payload: OutboundPayload::Event(CommonEvent {
            code,
            data: serde_json::to_value(payload).unwrap_or_default(),
        }),
    });
}

fn push_private<T: serde::Serialize>(
    dispatch: &mut Dispatch,
    recipient: SessionId,
    code: i32,
    payload: T,
) {
    dispatch.messages.push(Delivery {
        recipient,
        payload: OutboundPayload::Event(CommonEvent {
            code,
            data: serde_json::to_value(payload).unwrap_or_default(),
        }),
    });
}

async fn deliver(dispatch: Dispatch, senders: &SessionSenders) {
    let senders = senders.lock().await;
    for delivery in dispatch.messages {
        let Some(sender) = senders.get(&delivery.recipient) else {
            continue;
        };
        if let Ok(frame) = to_text_message(&delivery.payload) {
            let _ = sender.send(frame);
        }
    }
}

#[cfg(test)]
#[path = "game_loop/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "game_loop/coverage_tests.rs"]
mod coverage_tests;
