use std::{collections::HashMap, sync::Arc, time::Duration};

use serde_json::Value;
use share_type_public::{
    CommonEvent, TractorPhase, WsCode, WsPositionEvent,
    games::tractor::{
        WsTractorDealEvent, WsTractorPlayEvent, WsTractorSettlementEvent,
        WsTractorTableSnapshotEvent,
    },
};
use tokio::sync::Mutex;
use ws_common::{
    Delivery, Dispatch, OutboundPayload, RoomService, SessionId, SessionSenders, dlog,
};

use crate::{
    game_setting::{KEY_AWAY_TIME, KEY_PLAY_TIME, KEY_SETTLEMENT_TIME},
    game_state::{TractorGameState, TractorStateHandle},
};

type StateRegistry = Arc<std::sync::Mutex<HashMap<String, TractorStateHandle>>>;

fn build_auto_dispatch(
    room_key: &str,
    room_service: &RoomService,
    state: &TractorStateHandle,
    configs: &HashMap<String, i32>,
) -> Dispatch {
    let mut dispatch = Dispatch::default();
    let mut away_position = None;
    let auto_result = {
        let mut s = state.lock().unwrap();
        if s.phase != TractorPhase::Play {
            return dispatch;
        }
        let position = s.current_position;
        let controlled = s.is_ai_controlled_position(position);
        if !controlled && s.base.lock().unwrap().turn_countdown > 0 {
            let next = s.base.lock().unwrap().turn_countdown.saturating_sub(1);
            s.set_turn_countdown(next);
            return dispatch;
        }
        if !controlled && s.base.lock().unwrap().mark_away(position) {
            away_position = Some(position);
        }
        let is_ai = { s.base.lock().unwrap().is_ai_position(position) };
        let cards = if is_ai {
            crate::ai::decide(&s, position)
        } else {
            s.choose_auto_play(position)
        };
        let Some(cards) = cards else {
            dlog!(
                ws_common::tracing::Level::WARN,
                "[tractor][ai] no legal auto play room={} position={}",
                room_key,
                position
            );
            return dispatch;
        };
        let name = s.player_name(position);
        let Ok(played) = s.play_cards(position, name.clone(), cards) else {
            return dispatch;
        };
        let countdown = current_play_time(configs, &s);
        s.set_turn_countdown(countdown);
        let play_event = WsTractorPlayEvent {
            position: played.position,
            name,
            cards: played.cards,
            trick_index: s.trick_index,
            next_position: s.current_position as i32,
            remaining_hand_count: s.remaining_hand_count(position),
        };
        let snapshot = s.snapshot();
        let settlement = s.is_finished().then(|| settlement_event(&s));
        Some((play_event, snapshot, settlement))
    };

    if let Some(position) = away_position {
        room_service.send_all(
            room_key,
            WsCode::AWAY as i32,
            WsPositionEvent {
                position: position as i32,
            },
            &mut dispatch,
        );
    }
    let Some((play_event, snapshot, settlement)) = auto_result else {
        return dispatch;
    };
    room_service.send_all(room_key, WsCode::PLAY as i32, play_event, &mut dispatch);
    push_table_snapshot(room_key, room_service, snapshot, &mut dispatch);
    if let Some(settlement) = settlement {
        crate::official::settle_round(
            room_service,
            room_key,
            &settlement.winner_positions,
            settlement.score,
            settlement.target_rank,
        );
        room_service.send_all(
            room_key,
            WsCode::GAME_OVER as i32,
            settlement,
            &mut dispatch,
        );
    }
    dispatch
}

fn current_play_time(configs: &HashMap<String, i32>, state: &TractorGameState) -> u32 {
    let key = if state.is_ai_controlled_position(state.current_position) {
        KEY_AWAY_TIME
    } else {
        KEY_PLAY_TIME
    };
    configs.get(key).copied().unwrap_or(30).max(1) as u32
}

async fn deliver(dispatch: Dispatch, senders: &SessionSenders) {
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

fn push_direct_event<T: serde::Serialize>(
    dispatch: &mut Dispatch,
    recipient: SessionId,
    code: i32,
    payload: T,
) {
    dispatch.messages.push(Delivery {
        recipient,
        payload: OutboundPayload::Event(CommonEvent {
            code,
            data: serde_json::to_value(payload).unwrap_or(Value::Null),
        }),
    });
}

fn push_private_deals(
    room_key: &str,
    room_service: &RoomService,
    state: &TractorGameState,
    dispatch: &mut Dispatch,
) {
    for (session_id, _, position, _) in room_service.get_room_members(room_key) {
        push_direct_event(
            dispatch,
            session_id,
            WsCode::DEAL as i32,
            WsTractorDealEvent {
                position: position as i32,
                cards: state.hands.get(&position).cloned().unwrap_or_default(),
                deck_count: state.rules.deck_count as i32,
                hand_count: state.hand_count() as i32,
                bottom_card_count: state.bottom_cards.len() as i32,
                target_rank: state.rules.target_rank,
            },
        );
    }
}

fn push_table_snapshot(
    room_key: &str,
    room_service: &RoomService,
    snapshot: WsTractorTableSnapshotEvent,
    dispatch: &mut Dispatch,
) {
    room_service.send_all(room_key, WsCode::TABLE_SNAPSHOT as i32, snapshot, dispatch);
}

fn settlement_event(state: &TractorGameState) -> WsTractorSettlementEvent {
    let score = state.settlement_score();
    WsTractorSettlementEvent {
        winner_positions: state.winner_positions(),
        score,
        blood_units: state.rules.blood_units(score),
        target_rank: state.rules.target_rank,
        match_finished: state.match_finished(),
        next_target_rank: state.next_target_rank(),
    }
}

fn settlement_time(configs: &HashMap<String, i32>) -> u64 {
    configs
        .get(KEY_SETTLEMENT_TIME)
        .copied()
        .unwrap_or(5)
        .max(1) as u64
}

pub(crate) fn start_game_loop(
    room_key: String,
    state: TractorStateHandle,
    room_service: Arc<Mutex<RoomService>>,
    senders: SessionSenders,
    states: StateRegistry,
) {
    tokio::spawn(async move {
        let configs = room_service
            .lock()
            .await
            .get_room_configs(&room_key)
            .unwrap_or_default();

        loop {
            let (stop_requested, paused) = {
                let guard = state.lock().unwrap();
                let base = guard.base.lock().unwrap();
                (base.stop_requested(), base.paused)
            };
            if stop_requested {
                break;
            }
            if paused {
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }

            let phase = { state.lock().unwrap().phase };
            match phase {
                TractorPhase::Play => {
                    let dispatch = {
                        let room = room_service.lock().await;
                        build_auto_dispatch(&room_key, &room, &state, &configs)
                    };
                    if !dispatch.messages.is_empty() {
                        deliver(dispatch, &senders).await;
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                TractorPhase::Settlement => {
                    tokio::time::sleep(Duration::from_secs(settlement_time(&configs))).await;
                    let (advanced, snapshot) = {
                        let mut guard = state.lock().unwrap();
                        if guard.phase != TractorPhase::Settlement {
                            continue;
                        }
                        if guard.match_finished() {
                            break;
                        }
                        match guard.advance_after_settlement() {
                            Ok(true) => {
                                let countdown = current_play_time(&configs, &guard);
                                guard.set_turn_countdown(countdown);
                                (true, guard.snapshot())
                            }
                            _ => (false, guard.snapshot()),
                        }
                    };
                    if advanced {
                        let mut dispatch = Dispatch::default();
                        {
                            let room = room_service.lock().await;
                            let guard = state.lock().unwrap();
                            room.send_all(
                                &room_key,
                                WsCode::START as i32,
                                serde_json::json!({}),
                                &mut dispatch,
                            );
                            push_private_deals(&room_key, &room, &guard, &mut dispatch);
                            push_table_snapshot(&room_key, &room, snapshot, &mut dispatch);
                        }
                        deliver(dispatch, &senders).await;
                    }
                }
                _ => {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }

        let common = { Arc::clone(&state.lock().unwrap().base) };
        room_service
            .lock()
            .await
            .clear_room_game_state_if_same(&room_key, &common);
        states.lock().unwrap().remove(&room_key);
    });
}
