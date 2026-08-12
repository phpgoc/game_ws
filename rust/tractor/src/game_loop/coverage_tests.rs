use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::Duration,
};

use share_type_public::{
    CommonEvent, GameId, Routes, TractorPhase, TractorRank, TractorWsCode, WsCode, WsJoinRequest,
    games::tractor::WsTractorBottomCardsEvent,
};
use tokio::sync::Mutex as AsyncMutex;
use ws_common::{ClientRequest, CommonGameState, OutboundPayload, RoomService, SessionSenders};

use super::{
    StateRegistry, TractorGameState, TractorStateHandle, build_auto_bury_dispatch,
    build_auto_dispatch, build_deal_dispatch, current_bury_time, current_play_time,
    position_has_active_membership, remove_registered_state_if_same, room_uses_common_state,
    settlement_event, settlement_time, sleep_or_stop, start_game_loop, stop_requested,
    timed_out_human_position,
};
use crate::game_setting::{
    KEY_AI_ACTION_TIME, KEY_AWAY_TIME, KEY_PLAY_TIME, KEY_SETTLEMENT_TIME, build_tractor_settings,
};
use crate::game_state::TractorRules;

const ROOM_KEY: &str = "loop-coverage";

fn join_request(name: &str) -> ClientRequest {
    ClientRequest {
        route: Routes::JOIN as i32,
        data: serde_json::to_value(WsJoinRequest {
            name: name.to_owned(),
            password: ROOM_KEY.to_owned(),
            game_id: GameId::TRACTOR,
            session_id: String::new(),
            avatar_url: String::new(),
        })
        .expect("serialize join request"),
    }
}

fn room_with_players() -> (RoomService, Arc<Mutex<CommonGameState>>) {
    let mut room = RoomService::default();
    for session_id in 1..=4 {
        room.handle_common_request(
            session_id,
            &join_request(&format!("u{session_id}")),
            GameId::TRACTOR,
            build_tractor_settings,
        )
        .expect("join handled by common room service");
    }
    let common = room.room_common_state(ROOM_KEY).expect("room common state");
    (room, common)
}

fn rules(bottom_card_count: usize) -> TractorRules {
    TractorRules {
        attacking_win_score: 80,
        score_per_level: 40,
        shutout_bonus_levels: 1,
        bottom_card_count,
        deck_count: 2,
        final_target_rank: TractorRank::A,
        target_rank: TractorRank::TWO,
        trump_suit: None,
    }
}

fn state_handle(common: Arc<Mutex<CommonGameState>>, phase: TractorPhase) -> TractorStateHandle {
    let mut state = TractorGameState::from_common(common);
    state.phase = phase;
    state.rules = rules(2);
    state.dealer_position = 0;
    state.current_position = 0;
    Arc::new(Mutex::new(state))
}

fn event_payloads(dispatch: &ws_common::Dispatch, code: i32) -> Vec<&serde_json::Value> {
    dispatch
        .messages
        .iter()
        .filter_map(|message| match &message.payload {
            OutboundPayload::Event(CommonEvent {
                code: event_code,
                data,
            }) if *event_code == code => Some(data),
            _ => None,
        })
        .collect()
}

async fn wait_until(description: &str, mut ready: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while !ready() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {description}"));
}

#[test]
fn auto_bury_counts_down_then_marks_an_away_human_and_broadcasts_changes() {
    let (room, common) = room_with_players();
    let state = state_handle(common, TractorPhase::Bury);
    {
        let mut guard = state.lock().unwrap();
        guard.hands.insert(0, vec![1, 2, 3]);
        guard.base.lock().unwrap().turn_countdown = 1;
    }

    let first = build_auto_bury_dispatch(ROOM_KEY, &room, &state, &HashMap::new(), None);
    assert!(first.messages.is_empty());
    assert_eq!(state.lock().unwrap().base.lock().unwrap().turn_countdown, 0);

    let dispatch = build_auto_bury_dispatch(ROOM_KEY, &room, &state, &HashMap::new(), None);
    let guard = state.lock().unwrap();
    assert_eq!(guard.phase, TractorPhase::Play);
    assert_eq!(guard.bottom_cards.len(), 2);
    assert_eq!(guard.hands[&0].len(), 1);
    assert!(guard.base.lock().unwrap().is_away(0));
    drop(guard);

    assert_eq!(event_payloads(&dispatch, WsCode::AWAY as i32).len(), 4);
    assert_eq!(
        event_payloads(
            &dispatch,
            share_type_public::TractorWsCode::BOTTOM_BURIED as i32
        )
        .len(),
        4
    );
    assert_eq!(
        event_payloads(&dispatch, WsCode::TABLE_SNAPSHOT as i32).len(),
        4
    );
}

#[test]
fn auto_bury_member_timeout_enables_ai_takeover_and_keeps_the_event_flag() {
    let (room, common) = room_with_players();
    let state = state_handle(common, TractorPhase::Bury);
    state.lock().unwrap().hands.insert(0, vec![1, 2, 3]);

    let dispatch = build_auto_bury_dispatch(ROOM_KEY, &room, &state, &HashMap::new(), Some(0));
    let base = Arc::clone(&state.lock().unwrap().base);
    let base = base.lock().unwrap();
    assert!(base.is_away(0));
    assert!(base.is_ai_takeover_position(0));
    drop(base);

    let away_events = event_payloads(&dispatch, WsCode::AWAY as i32);
    assert_eq!(away_events.len(), 4);
    assert!(
        away_events
            .iter()
            .all(|event| event["is_ai_takeover"] == true)
    );
}

#[test]
fn existing_member_takeover_waits_for_the_human_bottom_window() {
    let (room, common) = room_with_players();
    let state = state_handle(common, TractorPhase::Bury);
    {
        let mut guard = state.lock().unwrap();
        guard.hands.insert(0, vec![1, 2, 3]);
        let mut base = guard.base.lock().unwrap();
        base.mark_ai_takeover_position(0);
        base.turn_countdown = 2;
    }
    let configs = HashMap::from([(KEY_AI_ACTION_TIME.to_owned(), 20)]);

    assert_eq!(
        super::action_loop_delay(&configs, &state.lock().unwrap()),
        Duration::from_secs(1)
    );
    let first = build_auto_bury_dispatch(ROOM_KEY, &room, &state, &configs, None);
    assert!(first.messages.is_empty());
    assert_eq!(state.lock().unwrap().base.lock().unwrap().turn_countdown, 1);
    let second = build_auto_bury_dispatch(ROOM_KEY, &room, &state, &configs, None);
    assert!(second.messages.is_empty());
    assert_eq!(state.lock().unwrap().base.lock().unwrap().turn_countdown, 0);
    let finished = build_auto_bury_dispatch(ROOM_KEY, &room, &state, &configs, None);
    assert_eq!(state.lock().unwrap().phase, TractorPhase::Play);
    assert_eq!(
        event_payloads(&finished, TractorWsCode::BOTTOM_BURIED as i32).len(),
        4
    );
}

#[test]
fn auto_dispatch_broadcasts_settlement_when_its_play_finishes_every_hand() {
    let (room, common) = room_with_players();
    let state = state_handle(common, TractorPhase::Play);
    {
        let mut state = state.lock().unwrap();
        state.current_position = 0;
        state.hands = HashMap::from([(0, vec![1]), (1, vec![]), (2, vec![]), (3, vec![])]);
        state.base.lock().unwrap().mark_ai_position(0);
    }

    let dispatch = build_auto_dispatch(ROOM_KEY, &room, &state, &HashMap::new(), None);

    assert_eq!(state.lock().unwrap().phase, TractorPhase::Settlement);
    assert_eq!(state.lock().unwrap().base.lock().unwrap().turn_countdown, 0);
    assert_eq!(event_payloads(&dispatch, WsCode::PLAY as i32).len(), 4);
    assert_eq!(event_payloads(&dispatch, WsCode::GAME_OVER as i32).len(), 4);
    assert!(
        event_payloads(&dispatch, WsCode::TABLE_SNAPSHOT as i32)
            .iter()
            .all(|snapshot| snapshot["turn_countdown"] == 0)
    );
}

#[test]
fn deal_dispatch_sends_private_deal_and_bottom_cards_then_exposes_the_snapshot() {
    let (room, common) = room_with_players();
    let state = state_handle(common, TractorPhase::Deal);
    {
        let mut guard = state.lock().unwrap();
        guard.deal_queue = VecDeque::from([(0, 1)]);
        guard.total_deal_count = 1;
        guard.bottom_cards = vec![53, 54];
        guard.hands.insert(0, Vec::new());
    }
    let configs = HashMap::from([(KEY_PLAY_TIME.to_owned(), 7)]);

    let dispatch = build_deal_dispatch(ROOM_KEY, &room, &state, &configs);
    let guard = state.lock().unwrap();
    assert_eq!(guard.phase, TractorPhase::Bury);
    assert_eq!(guard.dealt_count, 1);
    assert_eq!(guard.base.lock().unwrap().turn_countdown, 21);
    drop(guard);

    assert_eq!(event_payloads(&dispatch, WsCode::DEAL as i32).len(), 1);
    let bottom = event_payloads(
        &dispatch,
        share_type_public::TractorWsCode::BOTTOM_CARDS as i32,
    );
    assert_eq!(bottom.len(), 1);
    let bottom: WsTractorBottomCardsEvent =
        serde_json::from_value((*bottom[0]).clone()).expect("bottom cards event");
    assert_eq!(bottom.position, 0);
    assert_eq!(bottom.cards, vec![53, 54]);
    assert_eq!(
        event_payloads(&dispatch, WsCode::TABLE_SNAPSHOT as i32).len(),
        4
    );
}

#[test]
fn disconnected_human_dealer_still_receives_the_full_bottom_window() {
    let (room, common) = room_with_players();
    let state = state_handle(common, TractorPhase::Deal);
    {
        let mut guard = state.lock().unwrap();
        guard.deal_queue = VecDeque::from([(0, 1)]);
        guard.total_deal_count = 1;
        guard.bottom_cards = vec![53, 54];
        guard.hands.insert(0, Vec::new());
        guard.base.lock().unwrap().mark_disconnected(0);
    }
    let configs = HashMap::from([
        (KEY_AWAY_TIME.to_owned(), 4),
        (KEY_PLAY_TIME.to_owned(), 30),
    ]);

    let dispatch = build_deal_dispatch(ROOM_KEY, &room, &state, &configs);
    let guard = state.lock().unwrap();
    assert_eq!(guard.phase, TractorPhase::Bury);
    assert_eq!(guard.base.lock().unwrap().turn_countdown, 90);
    drop(guard);

    let snapshots = event_payloads(&dispatch, WsCode::TABLE_SNAPSHOT as i32);
    assert_eq!(snapshots.len(), 4);
    assert!(
        snapshots
            .iter()
            .all(|snapshot| snapshot["turn_countdown"] == 90)
    );
}

#[test]
fn later_round_final_deal_waits_for_selection_in_the_bottom_window() {
    let (room, common) = room_with_players();
    let state = state_handle(common, TractorPhase::Deal);
    {
        let mut state = state.lock().unwrap();
        state.deal_queue = VecDeque::from([(0, 1)]);
        state.total_deal_count = 1;
        state.bottom_cards = vec![53, 54];
        state.hands.insert(0, Vec::new());
        state.round_index = 1;
        state.declaration = None;
    }

    let dispatch = build_deal_dispatch(ROOM_KEY, &room, &state, &HashMap::new());

    assert!(event_payloads(&dispatch, TractorWsCode::TRUMP_DECLARED as i32).is_empty());
    let guard = state.lock().unwrap();
    assert_eq!(guard.phase, TractorPhase::Bury);
    assert!(guard.rules.trump_suit.is_none());
    drop(guard);
    state.lock().unwrap().base.lock().unwrap().turn_countdown = 0;

    let dispatch = build_auto_bury_dispatch(ROOM_KEY, &room, &state, &HashMap::new(), None);
    assert_eq!(
        event_payloads(&dispatch, TractorWsCode::TRUMP_DECLARED as i32).len(),
        4
    );
    let guard = state.lock().unwrap();
    assert_eq!(guard.phase, TractorPhase::Play);
    assert!(guard.rules.trump_suit.is_some());
}

#[tokio::test]
async fn loop_helpers_handle_membership_timeout_cleanup_and_stop_requests() {
    let (room, _) = room_with_players();
    assert!(
        !position_has_active_membership(&Arc::new(tokio::sync::Mutex::new(room)), ROOM_KEY, 0)
            .await
    );

    let (mut room, common) = room_with_players();
    assert!(room.set_session_active_membership(1, true));
    let room = Arc::new(tokio::sync::Mutex::new(room));
    assert!(position_has_active_membership(&room, ROOM_KEY, 0).await);

    let state = state_handle(common, TractorPhase::Play);
    {
        let guard = state.lock().unwrap();
        guard.base.lock().unwrap().turn_countdown = 0;
    }
    assert_eq!(
        timed_out_human_position(&state.lock().unwrap(), TractorPhase::Play),
        Some(0)
    );
    {
        let guard = state.lock().unwrap();
        guard.base.lock().unwrap().mark_ai_takeover_position(0);
    }
    assert_eq!(
        timed_out_human_position(&state.lock().unwrap(), TractorPhase::Play),
        None
    );
    assert_eq!(
        timed_out_human_position(&state.lock().unwrap(), TractorPhase::Start),
        None
    );

    let configs = HashMap::from([
        (KEY_AWAY_TIME.to_owned(), 4),
        (KEY_PLAY_TIME.to_owned(), 30),
        (KEY_SETTLEMENT_TIME.to_owned(), 9),
    ]);
    assert_eq!(current_play_time(&configs, &state.lock().unwrap()), 4);
    assert_eq!(current_bury_time(&configs, &state.lock().unwrap()), 90);
    assert_eq!(settlement_time(&configs), 9);
    assert_eq!(settlement_time(&HashMap::new()), 5);

    assert!(!stop_requested(&state));
    assert!(!sleep_or_stop(&state, Duration::ZERO).await);
    state.lock().unwrap().base.lock().unwrap().request_stop();
    assert!(stop_requested(&state));
    assert!(sleep_or_stop(&state, Duration::ZERO).await);
}

#[test]
fn state_registry_and_room_identity_helpers_only_act_on_the_current_state() {
    let (room, common) = room_with_players();
    let current = state_handle(Arc::clone(&common), TractorPhase::Settlement);
    let stale = state_handle(
        Arc::new(Mutex::new(CommonGameState::new())),
        TractorPhase::Start,
    );
    let states: StateRegistry = Arc::new(Mutex::new(HashMap::from([(
        ROOM_KEY.to_owned(),
        Arc::clone(&current),
    )])));

    assert!(room_uses_common_state(&room, ROOM_KEY, &common));
    assert!(!room_uses_common_state(
        &room,
        ROOM_KEY,
        &Arc::new(Mutex::new(CommonGameState::new()))
    ));
    remove_registered_state_if_same(&states, ROOM_KEY, &stale);
    assert!(states.lock().unwrap().contains_key(ROOM_KEY));
    remove_registered_state_if_same(&states, ROOM_KEY, &current);
    assert!(!states.lock().unwrap().contains_key(ROOM_KEY));

    let settlement = settlement_event(&current.lock().unwrap());
    assert_eq!(settlement.target_rank, TractorRank::TWO);
    assert_eq!(settlement.winner_positions, vec![0, 2]);
}

#[tokio::test]
async fn game_loop_auto_buries_then_cleans_up_the_registered_state() {
    let (room, common) = room_with_players();
    let room = Arc::new(AsyncMutex::new(room));
    let state = state_handle(common, TractorPhase::Bury);
    state.lock().unwrap().hands.insert(0, vec![1, 2, 3]);
    let states: StateRegistry = Arc::new(Mutex::new(HashMap::from([(
        ROOM_KEY.to_owned(),
        Arc::clone(&state),
    )])));
    let senders: SessionSenders = Arc::new(AsyncMutex::new(HashMap::new()));

    start_game_loop(
        ROOM_KEY.to_owned(),
        Arc::clone(&state),
        Arc::clone(&room),
        senders,
        Arc::clone(&states),
    );

    wait_until("automatic tractor bury", || {
        state.lock().unwrap().phase == TractorPhase::Play
    })
    .await;
    {
        let state = state.lock().unwrap();
        assert_eq!(state.bottom_cards.len(), 2);
        assert_eq!(state.hands[&0].len(), 1);
    }

    state.lock().unwrap().base.lock().unwrap().request_stop();
    wait_until("tractor loop registry cleanup", || {
        !states.lock().unwrap().contains_key(ROOM_KEY)
    })
    .await;
}

#[tokio::test]
async fn game_loop_deals_a_card_before_waiting_for_the_next_deal_step() {
    let (room, common) = room_with_players();
    let room = Arc::new(AsyncMutex::new(room));
    let state = state_handle(common, TractorPhase::Deal);
    {
        let mut state = state.lock().unwrap();
        state.deal_queue = VecDeque::from([(0, 1)]);
        state.total_deal_count = 1;
        state.bottom_cards = vec![53, 54];
        state.hands.insert(0, Vec::new());
    }
    let states: StateRegistry = Arc::new(Mutex::new(HashMap::from([(
        ROOM_KEY.to_owned(),
        Arc::clone(&state),
    )])));
    let senders: SessionSenders = Arc::new(AsyncMutex::new(HashMap::new()));

    start_game_loop(
        ROOM_KEY.to_owned(),
        Arc::clone(&state),
        room,
        senders,
        Arc::clone(&states),
    );

    wait_until("single tractor deal", || {
        let state = state.lock().unwrap();
        state.phase == TractorPhase::Bury && state.dealt_count == 1
    })
    .await;
    state.lock().unwrap().base.lock().unwrap().request_stop();
    wait_until("deal loop cleanup", || {
        !states.lock().unwrap().contains_key(ROOM_KEY)
    })
    .await;
}

#[tokio::test]
async fn game_loop_applies_ai_play_and_enters_settlement() {
    let (room, common) = room_with_players();
    let room = Arc::new(AsyncMutex::new(room));
    let state = state_handle(common, TractorPhase::Play);
    {
        let mut state = state.lock().unwrap();
        state.current_position = 0;
        state.hands = HashMap::from([(0, vec![1]), (1, vec![]), (2, vec![]), (3, vec![])]);
        state.base.lock().unwrap().mark_ai_position(0);
    }
    let states: StateRegistry = Arc::new(Mutex::new(HashMap::from([(
        ROOM_KEY.to_owned(),
        Arc::clone(&state),
    )])));
    let senders: SessionSenders = Arc::new(AsyncMutex::new(HashMap::new()));

    start_game_loop(
        ROOM_KEY.to_owned(),
        Arc::clone(&state),
        room,
        senders,
        Arc::clone(&states),
    );

    wait_until("AI tractor play settlement", || {
        state.lock().unwrap().phase == TractorPhase::Settlement
    })
    .await;
    state.lock().unwrap().base.lock().unwrap().request_stop();
    wait_until("AI play loop cleanup", || {
        !states.lock().unwrap().contains_key(ROOM_KEY)
    })
    .await;
}

#[tokio::test]
async fn game_loop_removes_a_stale_state_before_entering_the_loop() {
    let (mut room, common) = room_with_players();
    let state = state_handle(Arc::clone(&common), TractorPhase::Deal);
    let states: StateRegistry = Arc::new(Mutex::new(HashMap::from([(
        ROOM_KEY.to_owned(),
        Arc::clone(&state),
    )])));
    room.reset_room_common_state_for_new_game(ROOM_KEY)
        .expect("replace room common state");
    let room = Arc::new(AsyncMutex::new(room));
    let senders: SessionSenders = Arc::new(AsyncMutex::new(HashMap::new()));

    start_game_loop(
        ROOM_KEY.to_owned(),
        state,
        room,
        senders,
        Arc::clone(&states),
    );

    wait_until("stale tractor state cleanup", || {
        !states.lock().unwrap().contains_key(ROOM_KEY)
    })
    .await;
}

#[tokio::test]
async fn paused_and_settling_game_loops_honor_stop_requests() {
    for phase in [TractorPhase::Start, TractorPhase::Settlement] {
        let (room, common) = room_with_players();
        let room = Arc::new(AsyncMutex::new(room));
        let state = state_handle(common, phase);
        if phase == TractorPhase::Start {
            state.lock().unwrap().base.lock().unwrap().pause();
        }
        let states: StateRegistry = Arc::new(Mutex::new(HashMap::from([(
            ROOM_KEY.to_owned(),
            Arc::clone(&state),
        )])));
        let senders: SessionSenders = Arc::new(AsyncMutex::new(HashMap::new()));

        start_game_loop(
            ROOM_KEY.to_owned(),
            Arc::clone(&state),
            room,
            senders,
            Arc::clone(&states),
        );

        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(states.lock().unwrap().contains_key(ROOM_KEY));
        state.lock().unwrap().base.lock().unwrap().request_stop();
        wait_until("paused or settling tractor loop cleanup", || {
            !states.lock().unwrap().contains_key(ROOM_KEY)
        })
        .await;
    }
}
