use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

use share_type_public::{
    CommonEvent, GameId, Routes, UpgradePhase, UpgradeWsCode, WsCode, WsJoinRequest,
    WsUpgradeBottomCardsEvent,
};
use tokio::sync::Mutex as AsyncMutex;
use upgrade_common::Suit;
use ws_common::{
    ClientRequest, CommonGameState, OutboundPayload, RoomService, SessionSenders,
    session_sender_channel,
};

use super::{
    build_deal_dispatch, deliver, push_private, timeout_bury_dispatch, timeout_play_dispatch,
};
use crate::{
    game_setting::{KEY_PLAY_TIME, build_upgrade_settings},
    state::{UpgradeGameState, UpgradeStateHandle},
};

const ROOM_KEY: &str = "upgrade-loop-coverage";

fn join_request(name: &str) -> ClientRequest {
    ClientRequest {
        route: Routes::JOIN as i32,
        data: serde_json::to_value(WsJoinRequest {
            name: name.to_owned(),
            password: ROOM_KEY.to_owned(),
            game_id: GameId::UPGRADE,
            session_id: String::new(),
            avatar_url: String::new(),
        })
        .expect("serialize upgrade loop join request"),
    }
}

fn room_with_players() -> (RoomService, Arc<Mutex<CommonGameState>>) {
    let mut room = RoomService::default();
    for session_id in 1..=4 {
        room.handle_common_request(
            session_id,
            &join_request(&format!("u{session_id}")),
            GameId::UPGRADE,
            build_upgrade_settings,
        )
        .expect("join handled by common room service");
    }
    let common = room.room_common_state(ROOM_KEY).expect("room common state");
    (room, common)
}

fn state_handle(common: Arc<Mutex<CommonGameState>>, phase: UpgradePhase) -> UpgradeStateHandle {
    let mut state = UpgradeGameState::from_common(common);
    state.phase = phase;
    state.rules.trump_suit = Some(Suit::Heart);
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

#[test]
fn deal_dispatch_sends_private_cards_bottom_and_snapshot() {
    let (room, common) = room_with_players();
    let state = state_handle(common, UpgradePhase::Deal);
    {
        let mut state = state.lock().unwrap();
        state.rules.trump_suit = None;
        state.rules.bottom_card_count = 2;
        state.deal_queue = VecDeque::from([(0, 2)]);
        state.total_deal_count = 1;
        state.bottom_cards = vec![53, 54];
        state.hands.insert(0, Vec::new());
    }
    let configs = HashMap::from([(KEY_PLAY_TIME.to_owned(), 7)]);

    let dispatch = build_deal_dispatch(ROOM_KEY, &state, &room, &configs);

    let state = state.lock().unwrap();
    assert_eq!(state.phase, UpgradePhase::Bury);
    assert_eq!(state.dealt_count, 1);
    assert_eq!(state.base.lock().unwrap().turn_countdown, 21);
    drop(state);
    assert_eq!(event_payloads(&dispatch, WsCode::DEAL as i32).len(), 1);
    assert_eq!(
        event_payloads(&dispatch, UpgradeWsCode::TRUMP_DECLARED as i32).len(),
        4
    );
    assert_eq!(
        event_payloads(&dispatch, UpgradeWsCode::HAND_UPDATED as i32).len(),
        4
    );
    let bottom = event_payloads(&dispatch, UpgradeWsCode::BOTTOM_CARDS as i32);
    assert_eq!(bottom.len(), 1);
    let bottom: WsUpgradeBottomCardsEvent =
        serde_json::from_value((*bottom[0]).clone()).expect("upgrade bottom event");
    assert_eq!(bottom.position, 0);
    assert_eq!(bottom.cards, vec![53, 54]);
    assert_eq!(
        event_payloads(&dispatch, WsCode::TABLE_SNAPSHOT as i32).len(),
        4
    );
}

#[tokio::test]
async fn later_round_timeout_selects_trump_buries_and_broadcasts() {
    let (room, common) = room_with_players();
    let room = Arc::new(AsyncMutex::new(room));
    let state = state_handle(common, UpgradePhase::Deal);
    {
        let mut state = state.lock().unwrap();
        state.rules.trump_suit = None;
        state.rules.bottom_card_count = 2;
        state.round_index = 1;
        state.deal_queue = VecDeque::from([(0, 1)]);
        state.total_deal_count = 1;
        state.bottom_cards = vec![53, 54];
        state.hands.insert(0, Vec::new());
    }
    let dispatch = {
        let room = room.lock().await;
        build_deal_dispatch(ROOM_KEY, &state, &room, &HashMap::new())
    };
    assert!(event_payloads(&dispatch, UpgradeWsCode::TRUMP_DECLARED as i32).is_empty());
    {
        let state = state.lock().unwrap();
        assert_eq!(state.phase, UpgradePhase::Bury);
        assert!(state.rules.trump_suit.is_none());
        state.base.lock().unwrap().turn_countdown = 0;
    }

    let dispatch = timeout_bury_dispatch(ROOM_KEY, &state, &room, 30).await;

    let state = state.lock().unwrap();
    assert_eq!(state.phase, UpgradePhase::Play);
    assert_eq!(state.rules.trump_suit, Some(Suit::Spade));
    assert_eq!(state.bottom_cards.len(), 2);
    assert_eq!(state.hands[&0].len(), 1);
    drop(state);
    assert_eq!(
        event_payloads(&dispatch, UpgradeWsCode::BOTTOM_BURIED as i32).len(),
        4
    );
    assert_eq!(
        event_payloads(&dispatch, UpgradeWsCode::HAND_UPDATED as i32).len(),
        1
    );
    assert_eq!(
        event_payloads(&dispatch, WsCode::TABLE_SNAPSHOT as i32).len(),
        4
    );
}

#[tokio::test]
async fn ai_controlled_bury_ignores_the_human_countdown() {
    let (room, common) = room_with_players();
    let room = Arc::new(AsyncMutex::new(room));
    let state = state_handle(common, UpgradePhase::Bury);
    {
        let mut state = state.lock().unwrap();
        state.rules.bottom_card_count = 2;
        state.round_index = 1;
        state.rules.trump_suit = None;
        state.bottom_cards = vec![53, 54];
        state.hands.insert(0, vec![1, 2, 3]);
        let mut base = state.base.lock().unwrap();
        base.mark_ai_position(0);
        base.turn_countdown = 90;
    }

    let dispatch = timeout_bury_dispatch(ROOM_KEY, &state, &room, 30).await;

    let state = state.lock().unwrap();
    assert_eq!(state.phase, UpgradePhase::Play);
    assert_eq!(state.base.lock().unwrap().turn_countdown, 30);
    drop(state);
    assert_eq!(
        event_payloads(&dispatch, UpgradeWsCode::BOTTOM_BURIED as i32).len(),
        4
    );
}

#[tokio::test]
async fn timeout_play_finishes_the_last_trick_and_broadcasts_settlement() {
    let (room, common) = room_with_players();
    let room = Arc::new(AsyncMutex::new(room));
    let state = state_handle(common, UpgradePhase::Play);
    {
        let mut state = state.lock().unwrap();
        state.bottom_cards.clear();
        state.hands = HashMap::from([(0, vec![1]), (1, vec![14]), (2, vec![27]), (3, vec![40])]);
    }

    for turn in 0..4 {
        state.lock().unwrap().base.lock().unwrap().turn_countdown = 0;
        let dispatch = timeout_play_dispatch(ROOM_KEY, &state, &room, 30).await;
        assert_eq!(event_payloads(&dispatch, WsCode::PLAY as i32).len(), 4);
        assert_eq!(
            event_payloads(&dispatch, WsCode::GAME_OVER as i32).len(),
            if turn == 3 { 4 } else { 0 }
        );
    }
    assert_eq!(state.lock().unwrap().phase, UpgradePhase::Settlement);
}

#[tokio::test]
async fn ai_takeover_play_ignores_the_human_countdown() {
    let (room, common) = room_with_players();
    let room = Arc::new(AsyncMutex::new(room));
    let state = state_handle(common, UpgradePhase::Play);
    {
        let mut state = state.lock().unwrap();
        state.bottom_cards.clear();
        state.hands = HashMap::from([(0, vec![1]), (1, vec![14]), (2, vec![27]), (3, vec![40])]);
        let mut base = state.base.lock().unwrap();
        base.mark_ai_takeover_position(0);
        base.turn_countdown = 30;
    }

    let dispatch = timeout_play_dispatch(ROOM_KEY, &state, &room, 30).await;

    let state = state.lock().unwrap();
    assert_eq!(state.current_trick.len(), 1);
    assert_eq!(state.current_position, 1);
    drop(state);
    assert_eq!(event_payloads(&dispatch, WsCode::PLAY as i32).len(), 4);
}

#[tokio::test]
async fn member_human_play_timeout_marks_away_and_broadcasts_ai_takeover() {
    let (room, common) = room_with_players();
    common.lock().unwrap().set_member_position(0, true);
    let room = Arc::new(AsyncMutex::new(room));
    let state = state_handle(common, UpgradePhase::Play);
    {
        let mut state = state.lock().unwrap();
        state.bottom_cards.clear();
        state.hands = HashMap::from([(0, vec![1]), (1, vec![14]), (2, vec![27]), (3, vec![40])]);
        state.base.lock().unwrap().turn_countdown = 0;
    }

    let dispatch = timeout_play_dispatch(ROOM_KEY, &state, &room, 30).await;

    let state = state.lock().unwrap();
    let base = state.base.lock().unwrap();
    assert!(base.is_away(0));
    assert!(base.is_ai_takeover_position(0));
    drop(base);
    drop(state);
    let away = event_payloads(&dispatch, WsCode::AWAY as i32);
    assert_eq!(away.len(), 4);
    assert_eq!(away[0]["position"], 0);
    assert_eq!(away[0]["is_ai_takeover"], true);
}

#[tokio::test]
async fn member_human_bury_timeout_marks_away_and_broadcasts_ai_takeover() {
    let (room, common) = room_with_players();
    common.lock().unwrap().set_member_position(0, true);
    let room = Arc::new(AsyncMutex::new(room));
    let state = state_handle(common, UpgradePhase::Bury);
    {
        let mut state = state.lock().unwrap();
        state.rules.bottom_card_count = 2;
        state.round_index = 1;
        state.rules.trump_suit = None;
        state.bottom_cards = vec![53, 54];
        state.hands.insert(0, vec![1, 2, 3]);
        state.base.lock().unwrap().turn_countdown = 0;
    }

    let dispatch = timeout_bury_dispatch(ROOM_KEY, &state, &room, 30).await;

    let state = state.lock().unwrap();
    let base = state.base.lock().unwrap();
    assert!(base.is_away(0));
    assert!(base.is_ai_takeover_position(0));
    drop(base);
    drop(state);
    let away = event_payloads(&dispatch, WsCode::AWAY as i32);
    assert_eq!(away.len(), 4);
    assert_eq!(away[0]["position"], 0);
    assert_eq!(away[0]["is_ai_takeover"], true);
}

#[tokio::test]
async fn delivery_disconnects_a_slow_client_when_its_queue_is_full() {
    let (sender, mut receiver, mut disconnected) = session_sender_channel(1);
    let senders: SessionSenders = Arc::new(AsyncMutex::new(HashMap::from([(1, sender)])));
    let mut dispatch = ws_common::Dispatch::default();
    for value in [1, 2] {
        push_private(&mut dispatch, 1, WsCode::MESSAGE as i32, value);
    }

    deliver(dispatch, &senders).await;

    disconnected
        .changed()
        .await
        .expect("slow-client disconnect notification");
    assert!(*disconnected.borrow());
    assert!(receiver.recv().await.is_some());
}

#[test]
fn private_delivery_ignores_a_missing_room_position() {
    let mut dispatch = ws_common::Dispatch::default();
    super::send_private(
        &RoomService::default(),
        "missing-room",
        0,
        UpgradeWsCode::HAND_UPDATED as i32,
        serde_json::json!({}),
        &mut dispatch,
    );
    assert!(dispatch.messages.is_empty());
}
