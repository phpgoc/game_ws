use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use share_type_public::{GameId, LandlordPhase, Routes, WsJoinRequest};
use tokio::sync::Mutex as AsyncMutex;
use ws_common::{ClientRequest, RoomService, SessionSenders};

use crate::game_state::{LandlordGameState, LandlordLoopState};

use super::{sleep_or_stop, start_game_loop};

fn join_request(name: &str) -> ClientRequest {
    ClientRequest {
        route: Routes::JOIN as i32,
        data: serde_json::to_value(WsJoinRequest {
            name: name.to_owned(),
            password: "room".to_owned(),
            game_id: GameId::LANDLORD,
            session_id: String::new(),
            avatar_url: String::new(),
        })
        .expect("landlord join request serializes"),
    }
}

fn loop_room() -> (
    Arc<AsyncMutex<RoomService>>,
    Arc<Mutex<LandlordLoopState>>,
    Arc<Mutex<HashMap<String, Arc<Mutex<LandlordLoopState>>>>>,
) {
    let mut room = RoomService::default();
    for session_id in 1..=3 {
        room.connect(session_id);
        room.handle_common_request(
            session_id,
            &join_request(&format!("p{session_id}")),
            GameId::LANDLORD,
            crate::game_setting::build_landlord_settings,
        )
        .expect("join landlord room for loop coverage");
    }
    let common = room.room_common_state("room").expect("room common state");
    let state = Arc::new(Mutex::new(LandlordLoopState::new(common)));
    room.set_room_game_state(
        "room",
        Box::new(LandlordGameState::from_loop_state(Arc::clone(&state))),
    );
    let loop_states = Arc::new(Mutex::new(HashMap::from([(
        "room".to_owned(),
        Arc::clone(&state),
    )])));
    (Arc::new(AsyncMutex::new(room)), state, loop_states)
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

#[tokio::test]
async fn game_loop_deals_the_first_hand_and_cleans_up_after_stop() {
    let (room_service, state, loop_states) = loop_room();
    let senders: SessionSenders = Arc::new(AsyncMutex::new(HashMap::new()));

    start_game_loop(
        "room".to_owned(),
        Arc::clone(&state),
        Arc::clone(&room_service),
        senders,
        Arc::clone(&loop_states),
    );

    wait_until("the first landlord deal", || {
        let state = state.lock().unwrap();
        state.phase == LandlordPhase::CallLandlord
            && state.hands.len() == 3
            && state.hands.values().all(|hand| hand.len() == 17)
            && state.hidden_cards.len() == 3
    })
    .await;

    state.lock().unwrap().request_stop();
    wait_until("landlord loop state cleanup", || {
        !loop_states.lock().unwrap().contains_key("room")
    })
    .await;

    assert!(
        room_service
            .lock()
            .await
            .room_common_state("room")
            .is_some()
    );
}

#[tokio::test]
async fn game_loop_timeout_marks_a_member_away_and_enables_ai_takeover() {
    let (room_service, state, loop_states) = loop_room();
    let senders: SessionSenders = Arc::new(AsyncMutex::new(HashMap::new()));

    start_game_loop(
        "room".to_owned(),
        Arc::clone(&state),
        Arc::clone(&room_service),
        senders,
        Arc::clone(&loop_states),
    );

    wait_until("initial landlord call phase", || {
        state.lock().unwrap().phase == LandlordPhase::CallLandlord
    })
    .await;
    let timed_out_position = {
        let state = state.lock().unwrap();
        let position = state.current_position;
        state
            .base
            .lock()
            .unwrap()
            .set_member_position(position, true);
        state.set_turn_countdown(0);
        position
    };
    wait_until("member timeout takeover", || {
        let state = state.lock().unwrap();
        state.is_away(timed_out_position)
            && state.is_ai_takeover_position(timed_out_position)
            && state.call_history == vec![(timed_out_position, 0)]
    })
    .await;

    state.lock().unwrap().request_stop();
    wait_until("timeout loop cleanup", || {
        !loop_states.lock().unwrap().contains_key("room")
    })
    .await;
}

#[tokio::test]
async fn game_loop_exits_without_touching_a_replaced_room_common_state() {
    let (room_service, state, loop_states) = loop_room();
    let senders: SessionSenders = Arc::new(AsyncMutex::new(HashMap::new()));
    let original_common = Arc::clone(&state.lock().unwrap().base);

    start_game_loop(
        "room".to_owned(),
        Arc::clone(&state),
        Arc::clone(&room_service),
        senders,
        Arc::clone(&loop_states),
    );

    let replacement_common = room_service
        .lock()
        .await
        .reset_room_common_state_for_new_game("room")
        .expect("reset live room common state");
    assert!(!Arc::ptr_eq(&original_common, &replacement_common));

    wait_until("stale landlord loop cleanup", || {
        !loop_states.lock().unwrap().contains_key("room")
    })
    .await;
    let current_common = room_service
        .lock()
        .await
        .room_common_state("room")
        .expect("replacement room common state remains");
    assert!(Arc::ptr_eq(&current_common, &replacement_common));
}

#[tokio::test]
async fn sleep_helper_reports_stops_before_waiting() {
    let (_room_service, state, _loop_states) = loop_room();
    assert!(!sleep_or_stop(&state, Duration::ZERO).await);
    state.lock().unwrap().request_stop();
    assert!(sleep_or_stop(&state, Duration::from_millis(1)).await);
}
