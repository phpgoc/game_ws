use std::{collections::HashMap, sync::Arc, time::Duration};

use serde_json::Value;
use share_type_public::{
    CommonEvent, TractorPhase, TractorWsCode, WsCode, WsPositionEvent,
    games::tractor::{
        WsTractorBottomBuriedEvent, WsTractorBottomCardsEvent, WsTractorDealEvent,
        WsTractorHandEvent, WsTractorPlayEvent, WsTractorSettlementEvent,
        WsTractorTableSnapshotEvent,
    },
};
use tokio::sync::Mutex;
use ws_common::{
    Delivery, Dispatch, OutboundPayload, RoomService, SessionId, SessionSenders, dlog,
};

use crate::{
    game_setting::{
        KEY_AI_ACTION_TIME, KEY_AWAY_TIME, KEY_DEAL_TIME, KEY_FIRST_DEAL_TIME, KEY_PLAY_TIME,
        KEY_SETTLEMENT_TIME,
    },
    game_state::{TractorGameState, TractorStateHandle},
};

type StateRegistry = Arc<std::sync::Mutex<HashMap<String, TractorStateHandle>>>;

fn action_loop_delay(configs: &HashMap<String, i32>, state: &TractorGameState) -> Duration {
    if state.phase == TractorPhase::Settlement {
        return Duration::ZERO;
    }
    let controlled = match state.phase {
        TractorPhase::Bury => state.is_ai_controlled_position(state.dealer_position),
        TractorPhase::Play => state.is_ai_controlled_position(state.current_position),
        _ => false,
    };
    if controlled {
        return Duration::from_millis(
            configs
                .get(KEY_AI_ACTION_TIME)
                .copied()
                .unwrap_or(1_000)
                .max(1) as u64,
        );
    }
    Duration::from_secs(1)
}

fn build_auto_bury_dispatch(
    room_key: &str,
    room_service: &RoomService,
    state: &TractorStateHandle,
    configs: &HashMap<String, i32>,
) -> Dispatch {
    let mut dispatch = Dispatch::default();
    let mut away_position = None;
    let result = {
        let mut state = state.lock().unwrap();
        if state.phase != TractorPhase::Bury {
            return dispatch;
        }
        let position = state.dealer_position;
        let controlled = state.is_ai_controlled_position(position);
        if !controlled && state.base.lock().unwrap().turn_countdown > 0 {
            let next = state.base.lock().unwrap().turn_countdown.saturating_sub(1);
            state.set_turn_countdown(next);
            return dispatch;
        }
        if !controlled && state.base.lock().unwrap().mark_away(position) {
            away_position = Some(position);
        }
        let cards = if controlled {
            state.choose_auto_bury()
        } else {
            state.choose_timeout_bury()
        };
        let Some(cards) = cards else {
            return dispatch;
        };
        if state.bury_bottom(position, cards).is_err() {
            return dispatch;
        }
        let countdown = current_play_time(configs, &state);
        state.set_turn_countdown(countdown);
        Some((
            position,
            state.player_name(position),
            state.rules.bottom_card_count,
            state.hands.get(&position).cloned().unwrap_or_default(),
            state.snapshot(),
        ))
    };

    if let Some(position) = away_position {
        room_service.broadcast(
            room_key,
            WsCode::AWAY as i32,
            WsPositionEvent {
                position: position as i32,
                is_ai_takeover: false,
            },
            &mut dispatch,
        );
    }
    let Some((position, name, bottom_card_count, hand, snapshot)) = result else {
        return dispatch;
    };
    room_service.broadcast(
        room_key,
        TractorWsCode::BOTTOM_BURIED as i32,
        WsTractorBottomBuriedEvent {
            position: position as i32,
            name,
            bottom_card_count: bottom_card_count as i32,
        },
        &mut dispatch,
    );
    for (session_id, _, member_position, _) in room_service.room_members(room_key) {
        if member_position == position {
            push_direct_event(
                &mut dispatch,
                session_id,
                TractorWsCode::HAND_UPDATED as i32,
                WsTractorHandEvent {
                    position: position as i32,
                    cards: hand.clone(),
                },
            );
        }
    }
    push_table_snapshot(room_key, room_service, snapshot, &mut dispatch);
    dispatch
}

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
        let cards = if controlled {
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
        room_service.broadcast(
            room_key,
            WsCode::AWAY as i32,
            WsPositionEvent {
                position: position as i32,
                is_ai_takeover: false,
            },
            &mut dispatch,
        );
    }
    let Some((play_event, snapshot, settlement)) = auto_result else {
        return dispatch;
    };
    room_service.broadcast(room_key, WsCode::PLAY as i32, play_event, &mut dispatch);
    push_table_snapshot(room_key, room_service, snapshot, &mut dispatch);
    if let Some(settlement) = settlement {
        let trump_suit = state.lock().unwrap().rules.trump_suit;
        crate::official::settle_round(
            room_service,
            room_key,
            &settlement.winner_positions,
            settlement.score,
            settlement.target_rank,
            trump_suit,
        );
        room_service.broadcast(
            room_key,
            WsCode::GAME_OVER as i32,
            settlement,
            &mut dispatch,
        );
    }
    dispatch
}

fn build_deal_dispatch(
    room_key: &str,
    room_service: &RoomService,
    state: &TractorStateHandle,
    configs: &HashMap<String, i32>,
) -> Dispatch {
    let mut dispatch = Dispatch::default();
    let (position, deal_event, declaration, finished, dealer_position, bottom_cards, snapshot) = {
        let mut state = state.lock().unwrap();
        let Some((position, card, finished, declaration)) = state.deal_next_card() else {
            return dispatch;
        };
        if finished {
            let countdown = current_play_time(configs, &state);
            state.set_turn_countdown(countdown);
        }
        let deal_event = WsTractorDealEvent {
            position: position as i32,
            cards: vec![card],
            deck_count: state.rules.deck_count as i32,
            hand_count: state.remaining_hand_count(position),
            bottom_card_count: state.rules.bottom_card_count as i32,
            target_rank: state.rules.target_rank,
            dealt_count: state.dealt_count as i32,
            total_deal_count: state.total_deal_count as i32,
        };
        (
            position,
            deal_event,
            declaration,
            finished,
            state.dealer_position,
            state.dealer_bottom_cards().unwrap_or_default(),
            state.snapshot(),
        )
    };

    for (session_id, _, member_position, _) in room_service.room_members(room_key) {
        if member_position == position {
            push_direct_event(
                &mut dispatch,
                session_id,
                WsCode::DEAL as i32,
                deal_event.clone(),
            );
        }
        if finished && member_position == dealer_position {
            push_direct_event(
                &mut dispatch,
                session_id,
                TractorWsCode::BOTTOM_CARDS as i32,
                WsTractorBottomCardsEvent {
                    position: dealer_position as i32,
                    cards: bottom_cards.clone(),
                    required_count: bottom_cards.len() as i32,
                },
            );
        }
    }
    if let Some(declaration) = declaration {
        room_service.broadcast(
            room_key,
            TractorWsCode::TRUMP_DECLARED as i32,
            declaration,
            &mut dispatch,
        );
    }
    push_table_snapshot(room_key, room_service, snapshot, &mut dispatch);
    dispatch
}

fn current_play_time(configs: &HashMap<String, i32>, state: &TractorGameState) -> u32 {
    let inactive = {
        let base = state.base.lock().unwrap();
        base.is_away(state.current_position) || base.is_disconnected(state.current_position)
    };
    let key = if state.is_ai_controlled_position(state.current_position) || inactive {
        KEY_AWAY_TIME
    } else {
        KEY_PLAY_TIME
    };
    configs.get(key).copied().unwrap_or(30).max(1) as u32
}

fn deal_step_delay(configs: &HashMap<String, i32>, state: &TractorGameState) -> Duration {
    let key = if state.round_index == 0 {
        KEY_FIRST_DEAL_TIME
    } else {
        KEY_DEAL_TIME
    };
    let default = if state.round_index == 0 {
        15_000
    } else {
        3_000
    };
    let total_millis = configs.get(key).copied().unwrap_or(default).max(1) as u64;
    Duration::from_millis((total_millis / state.total_deal_count.max(1) as u64).max(1))
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

fn push_table_snapshot(
    room_key: &str,
    room_service: &RoomService,
    snapshot: WsTractorTableSnapshotEvent,
    dispatch: &mut Dispatch,
) {
    room_service.broadcast(room_key, WsCode::TABLE_SNAPSHOT as i32, snapshot, dispatch);
}

fn remove_registered_state_if_same(
    states: &StateRegistry,
    room_key: &str,
    state: &TractorStateHandle,
) {
    let mut states = states.lock().unwrap();
    if states
        .get(room_key)
        .is_some_and(|current| Arc::ptr_eq(current, state))
    {
        states.remove(room_key);
    }
}

fn room_uses_common_state(
    room: &RoomService,
    room_key: &str,
    common: &Arc<std::sync::Mutex<ws_common::CommonGameState>>,
) -> bool {
    room.room_common_state(room_key)
        .is_some_and(|current| Arc::ptr_eq(&current, common))
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

async fn sleep_or_stop(state: &TractorStateHandle, duration: Duration) -> bool {
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

pub(crate) fn start_game_loop(
    room_key: String,
    state: TractorStateHandle,
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
                if sleep_or_stop(&state, Duration::from_secs(1)).await {
                    break;
                }
                continue;
            }

            let phase = { state.lock().unwrap().phase };
            match phase {
                TractorPhase::Deal => {
                    let delay = {
                        let guard = state.lock().unwrap();
                        deal_step_delay(&configs, &guard)
                    };
                    let dispatch = {
                        let room = room_service.lock().await;
                        if !room_uses_common_state(&room, &room_key, &common) {
                            break;
                        }
                        build_deal_dispatch(&room_key, &room, &state, &configs)
                    };
                    if !dispatch.messages.is_empty() {
                        deliver(dispatch, &senders).await;
                    }
                    if sleep_or_stop(&state, delay).await {
                        break;
                    }
                }
                TractorPhase::Bury => {
                    let dispatch = {
                        let room = room_service.lock().await;
                        if !room_uses_common_state(&room, &room_key, &common) {
                            break;
                        }
                        build_auto_bury_dispatch(&room_key, &room, &state, &configs)
                    };
                    if !dispatch.messages.is_empty() {
                        deliver(dispatch, &senders).await;
                    }
                    let delay = {
                        let guard = state.lock().unwrap();
                        action_loop_delay(&configs, &guard)
                    };
                    if sleep_or_stop(&state, delay).await {
                        break;
                    }
                }
                TractorPhase::Play => {
                    let dispatch = {
                        let room = room_service.lock().await;
                        if !room_uses_common_state(&room, &room_key, &common) {
                            break;
                        }
                        build_auto_dispatch(&room_key, &room, &state, &configs)
                    };
                    if !dispatch.messages.is_empty() {
                        deliver(dispatch, &senders).await;
                    }
                    let delay = {
                        let guard = state.lock().unwrap();
                        action_loop_delay(&configs, &guard)
                    };
                    if sleep_or_stop(&state, delay).await {
                        break;
                    }
                }
                TractorPhase::Settlement => {
                    if sleep_or_stop(&state, Duration::from_secs(settlement_time(&configs))).await {
                        break;
                    }
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
                                guard.set_turn_countdown(0);
                                (true, guard.snapshot())
                            }
                            _ => (false, guard.snapshot()),
                        }
                    };
                    if advanced {
                        let mut dispatch = Dispatch::default();
                        {
                            let room = room_service.lock().await;
                            if !room_uses_common_state(&room, &room_key, &common) {
                                break;
                            }
                            room.broadcast(
                                &room_key,
                                WsCode::START as i32,
                                serde_json::json!({}),
                                &mut dispatch,
                            );
                            push_table_snapshot(&room_key, &room, snapshot, &mut dispatch);
                        }
                        deliver(dispatch, &senders).await;
                    }
                }
                _ => {
                    if sleep_or_stop(&state, Duration::from_secs(1)).await {
                        break;
                    }
                }
            }
        }

        room_service
            .lock()
            .await
            .clear_room_game_state_if_same(&room_key, &common);
        remove_registered_state_if_same(&states, &room_key, &state);
    });
}

fn stop_requested(state: &TractorStateHandle) -> bool {
    let common = { Arc::clone(&state.lock().unwrap().base) };
    common.lock().unwrap().stop_requested()
}

#[cfg(test)]
mod tests {
    use super::*;
    use share_type_public::{TractorPhase, TractorRank};
    use std::sync::{Arc, Mutex as StdMutex};
    use ws_common::CommonGameState;

    use crate::game_state::{TractorGameState, TractorRules};

    #[test]
    fn ai_action_delay_is_milliseconds_without_accelerating_human_turns() {
        let state = test_state_with_ai_leader();
        let configs = HashMap::from([(KEY_AI_ACTION_TIME.to_owned(), 75)]);
        let mut state = state.lock().unwrap();

        assert_eq!(
            action_loop_delay(&configs, &state),
            Duration::from_millis(75)
        );
        state.current_position = 1;
        assert_eq!(action_loop_delay(&configs, &state), Duration::from_secs(1));
        state.phase = TractorPhase::Settlement;
        assert_eq!(action_loop_delay(&configs, &state), Duration::ZERO);
    }

    #[test]
    fn auto_dispatch_uses_smart_ai_for_ai_position_lead() {
        let state = test_state_with_ai_leader();
        let room = RoomService::default();
        let configs = HashMap::new();

        let _ = build_auto_dispatch("room", &room, &state, &configs);

        let guard = state.lock().unwrap();
        assert_eq!(guard.current_trick.len(), 1);
        assert_eq!(guard.current_trick[0].position, 0);
        // Leads a low plain singleton (rank 2) to build a void, keeping the joker
        // and the high trumps in reserve.
        assert_eq!(guard.current_trick[0].cards, vec![1]);
        assert!(!guard.hands.get(&0).unwrap().contains(&1));
        assert!(guard.hands.get(&0).unwrap().contains(&53));
    }

    #[test]
    fn member_takeover_uses_smart_ai_and_fast_delay() {
        let state = test_state_with_ai_leader();
        {
            let guard = state.lock().unwrap();
            let mut base = guard.base.lock().unwrap();
            base.ai_positions.remove(&0);
            base.mark_away(0);
            base.mark_ai_takeover_position(0);
        }
        let configs = HashMap::from([(KEY_AI_ACTION_TIME.to_owned(), 75)]);
        assert_eq!(
            action_loop_delay(&configs, &state.lock().unwrap()),
            Duration::from_millis(75)
        );

        let _ = build_auto_dispatch("room", &RoomService::default(), &state, &configs);

        let guard = state.lock().unwrap();
        assert_eq!(guard.current_trick[0].cards, vec![1]);
        assert!(guard.hands.get(&0).unwrap().contains(&53));
    }

    #[test]
    fn nonmember_away_uses_human_delay() {
        let state = test_state_with_ai_leader();
        {
            let guard = state.lock().unwrap();
            let mut base = guard.base.lock().unwrap();
            base.ai_positions.remove(&0);
            base.mark_away(0);
        }
        let configs = HashMap::from([
            (KEY_AI_ACTION_TIME.to_owned(), 75),
            (KEY_AWAY_TIME.to_owned(), 4),
            (KEY_PLAY_TIME.to_owned(), 30),
        ]);

        assert_eq!(
            action_loop_delay(&configs, &state.lock().unwrap()),
            Duration::from_secs(1)
        );
        assert_eq!(current_play_time(&configs, &state.lock().unwrap()), 4);
    }

    #[test]
    fn first_round_deal_uses_the_slower_configured_duration() {
        let state = test_state_with_ai_leader();
        let configs = HashMap::from([
            (KEY_FIRST_DEAL_TIME.to_owned(), 12_000),
            (KEY_DEAL_TIME.to_owned(), 2_000),
        ]);
        {
            let mut state = state.lock().unwrap();
            state.total_deal_count = 100;
            state.round_index = 0;
            assert_eq!(
                deal_step_delay(&configs, &state),
                Duration::from_millis(120)
            );
            state.round_index = 1;
            assert_eq!(deal_step_delay(&configs, &state), Duration::from_millis(20));
        }
    }

    #[test]
    fn old_loop_cleanup_does_not_remove_recreated_room_state() {
        let old = Arc::new(StdMutex::new(TractorGameState::from_common(Arc::new(
            StdMutex::new(CommonGameState::default()),
        ))));
        let current = Arc::new(StdMutex::new(TractorGameState::from_common(Arc::new(
            StdMutex::new(CommonGameState::default()),
        ))));
        let states = Arc::new(StdMutex::new(HashMap::from([(
            "same-name".to_string(),
            Arc::clone(&current),
        )])));

        remove_registered_state_if_same(&states, "same-name", &old);

        assert!(
            states
                .lock()
                .unwrap()
                .get("same-name")
                .is_some_and(|state| Arc::ptr_eq(state, &current))
        );
    }

    fn test_state_with_ai_leader() -> TractorStateHandle {
        let mut common = CommonGameState::new();
        for position in 0..4 {
            common.add_player(position, position as u64 + 1, &format!("u{position}"));
        }
        common.mark_ai_position(0);

        let mut state = TractorGameState::from_common(Arc::new(StdMutex::new(common)));
        state.phase = TractorPhase::Play;
        state.rules = TractorRules {
            blood_enabled: true,
            blood_score_per_unit: 40,
            blood_start_score: 80,
            bottom_card_count: 8,
            deck_count: 2,
            final_target_rank: TractorRank::A,
            removed_rank_count: 0,
            target_rank: TractorRank::A,
            trump_suit: None,
        };
        state.current_position = 0;
        state.hands.insert(0, vec![13, 26, 39, 53, 1, 14]);
        state.hands.insert(1, vec![2]);
        state.hands.insert(2, vec![3]);
        state.hands.insert(3, vec![4]);
        Arc::new(StdMutex::new(state))
    }
}
