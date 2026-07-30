use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use share_type_public::{GameId, LandlordPhase, Routes, WsJoinRequest};
use tokio::sync::Mutex as AsyncMutex;
use ws_common::{ClientRequest, RoomService, SessionSenders};

use crate::game_state::{LandlordGameState, LandlordLoopState};

use super::{
    AutoActionReason, activate_ai_bomb_signal, choose_timeout_play, clear_stale_ai_bomb_signal,
    fixed_wait_seconds, handle_automatic_action, handle_call_landlord_phase, handle_play_phase,
    handle_settlement_phase, handle_start_phase, sleep_or_stop, start_game_loop,
};

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
            && state
                .call_history
                .iter()
                .any(|entry| *entry == (timed_out_position, 0))
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

#[tokio::test]
async fn call_landlord_phase_redeals_advances_and_assigns_the_highest_bidder() {
    let (room_service, state, _loop_states) = loop_room();
    let senders: SessionSenders = Arc::new(AsyncMutex::new(HashMap::new()));
    let common = Arc::clone(&state.lock().unwrap().base);
    let positions = [0, 1, 2];

    {
        let mut state = state.lock().unwrap();
        state.phase = LandlordPhase::CallLandlord;
        state.call_position = 0;
        state.current_position = 2;
        state.score = 0;
        state.call_history = vec![(0, 0), (1, 0), (2, 0)];
    }
    handle_call_landlord_phase(
        "room",
        &state,
        &positions,
        &HashMap::new(),
        &room_service,
        &senders,
        &common,
    )
    .await;
    {
        let state = state.lock().unwrap();
        assert_eq!(state.phase, LandlordPhase::Start);
        assert_eq!(state.call_position, 1);
        assert_eq!(state.current_position, 1);
    }

    {
        let mut state = state.lock().unwrap();
        state.phase = LandlordPhase::CallLandlord;
        state.call_position = 0;
        state.current_position = 0;
        state.score = 1;
        state.call_history = vec![(0, 1)];
    }
    handle_call_landlord_phase(
        "room",
        &state,
        &positions,
        &HashMap::new(),
        &room_service,
        &senders,
        &common,
    )
    .await;
    {
        let state = state.lock().unwrap();
        assert_eq!(state.phase, LandlordPhase::CallLandlord);
        assert_eq!(state.current_position, 1);
        assert!(!state.action_received());
    }

    {
        let mut state = state.lock().unwrap();
        state.phase = LandlordPhase::CallLandlord;
        state.call_position = 0;
        state.current_position = 2;
        state.score = 2;
        state.call_history = vec![(0, 1), (1, 2), (2, 0)];
        state.hidden_cards = vec![52, 53, 54];
        state.hands = HashMap::from([(0, vec![1]), (1, vec![2]), (2, vec![3])]);
    }
    handle_call_landlord_phase(
        "room",
        &state,
        &positions,
        &HashMap::new(),
        &room_service,
        &senders,
        &common,
    )
    .await;
    let state = state.lock().unwrap();
    assert_eq!(state.phase, LandlordPhase::Play);
    assert_eq!(state.landlord_position, Some(1));
    assert_eq!(state.current_position, 1);
    assert_eq!(state.hands[&1], vec![2, 52, 53, 54]);
}

#[tokio::test]
async fn play_phase_records_passes_cards_and_a_new_round_leader() {
    let (room_service, state, _loop_states) = loop_room();
    let senders: SessionSenders = Arc::new(AsyncMutex::new(HashMap::new()));
    let common = Arc::clone(&state.lock().unwrap().base);
    let positions = [0, 1, 2];

    {
        let mut state = state.lock().unwrap();
        state.phase = LandlordPhase::Play;
        state.current_position = 0;
        state.last_play_position = 2;
        state.last_play = vec![2];
        state.current_play.clear();
        state.hands = HashMap::from([(0, vec![3, 4]), (1, vec![5]), (2, vec![2])]);
    }
    assert!(
        !handle_play_phase(
            &state,
            &positions,
            &HashMap::new(),
            "room",
            &room_service,
            &senders,
            &common,
        )
        .await
    );
    {
        let state = state.lock().unwrap();
        assert_eq!(state.current_position, 1);
        assert_eq!(state.play_history.len(), 1);
        assert_eq!(state.play_history[0].benchmark, vec![2]);
    }

    {
        let mut state = state.lock().unwrap();
        state.current_position = 1;
        state.current_play = vec![5];
        state.last_play.clear();
        state.last_play_position = 1;
        state.hands.insert(1, vec![5, 6]);
    }
    assert!(
        !handle_play_phase(
            &state,
            &positions,
            &HashMap::new(),
            "room",
            &room_service,
            &senders,
            &common,
        )
        .await
    );
    {
        let state = state.lock().unwrap();
        assert_eq!(state.current_position, 2);
        assert_eq!(state.hands[&1], vec![6]);
        assert_eq!(state.last_play, vec![5]);
    }

    {
        let mut state = state.lock().unwrap();
        state.current_position = 1;
        state.current_play.clear();
        state.last_play_position = 2;
        state.last_play = vec![5];
    }
    assert!(
        !handle_play_phase(
            &state,
            &positions,
            &HashMap::new(),
            "room",
            &room_service,
            &senders,
            &common,
        )
        .await
    );
    let state = state.lock().unwrap();
    assert_eq!(state.current_position, 2);
    assert!(state.last_play.is_empty());
}

#[tokio::test]
async fn settlement_phase_redeals_without_waiting_when_configured_to_zero() {
    let (room_service, state, _loop_states) = loop_room();
    let senders: SessionSenders = Arc::new(AsyncMutex::new(HashMap::new()));
    let common = Arc::clone(&state.lock().unwrap().base);
    let configs = HashMap::from([("settlement_time".to_owned(), 0)]);
    state.lock().unwrap().phase = LandlordPhase::Settlement;

    assert!(
        !handle_settlement_phase("room", &state, &configs, &room_service, &senders, &common,).await
    );
    let state = state.lock().unwrap();
    assert_eq!(state.phase, LandlordPhase::Start);
    assert_eq!(state.call_position, 1);
    assert_eq!(state.current_position, 1);
}

#[tokio::test]
async fn start_phase_guards_stopped_nonstart_and_replaced_rooms() {
    let (room_service, state, _loop_states) = loop_room();
    let senders: SessionSenders = Arc::new(AsyncMutex::new(HashMap::new()));
    let common = Arc::clone(&state.lock().unwrap().base);
    let positions = [0, 1, 2];

    assert_eq!(fixed_wait_seconds(&HashMap::new(), "missing", 5), 5);
    assert_eq!(
        fixed_wait_seconds(&HashMap::from([("wait".to_owned(), -1)]), "wait", 5,),
        0
    );

    state.lock().unwrap().phase = LandlordPhase::CallLandlord;
    assert!(
        !handle_start_phase(
            "room",
            &state,
            &positions,
            &room_service,
            &senders,
            &HashMap::new(),
            &common,
        )
        .await
    );

    state.lock().unwrap().phase = LandlordPhase::Start;
    room_service
        .lock()
        .await
        .reset_room_common_state_for_new_game("room")
        .expect("replace room common state");
    assert!(
        handle_start_phase(
            "room",
            &state,
            &positions,
            &room_service,
            &senders,
            &HashMap::new(),
            &common,
        )
        .await
    );
}

#[tokio::test]
async fn game_loop_applies_an_ai_play_before_advancing_the_turn() {
    let (room_service, state, loop_states) = loop_room();
    let senders: SessionSenders = Arc::new(AsyncMutex::new(HashMap::new()));
    {
        let mut state = state.lock().unwrap();
        state.phase = LandlordPhase::Play;
        state.current_position = 0;
        state.last_play_position = 0;
        state.landlord_position = Some(0);
        state.hands = HashMap::from([(0, vec![2, 3]), (1, vec![4]), (2, vec![5])]);
        state.base.lock().unwrap().mark_ai_position(0);
    }

    start_game_loop(
        "room".to_owned(),
        Arc::clone(&state),
        Arc::clone(&room_service),
        senders,
        Arc::clone(&loop_states),
    );

    wait_until("automatic AI play", || {
        let state = state.lock().unwrap();
        state.current_position == 1
            && state.hands[&0] == vec![3]
            && state
                .play_history
                .iter()
                .any(|record| record.position == 0 && record.cards == [2])
    })
    .await;

    state.lock().unwrap().request_stop();
    wait_until("AI loop cleanup", || {
        !loop_states.lock().unwrap().contains_key("room")
    })
    .await;
}

#[tokio::test]
async fn game_loop_announces_disconnected_players_as_away() {
    let (room_service, state, loop_states) = loop_room();
    let senders: SessionSenders = Arc::new(AsyncMutex::new(HashMap::new()));
    state.lock().unwrap().phase = LandlordPhase::CallLandlord;
    room_service.lock().await.disconnect(1);

    start_game_loop(
        "room".to_owned(),
        Arc::clone(&state),
        Arc::clone(&room_service),
        senders,
        Arc::clone(&loop_states),
    );

    wait_until("disconnected player marked away", || {
        state.lock().unwrap().is_away(0)
    })
    .await;
    assert!(state.lock().unwrap().is_disconnected(0));

    state.lock().unwrap().request_stop();
    wait_until("disconnected loop cleanup", || {
        !loop_states.lock().unwrap().contains_key("room")
    })
    .await;
}

#[tokio::test]
async fn game_loop_decrements_a_waiting_human_turn() {
    let (room_service, state, loop_states) = loop_room();
    let senders: SessionSenders = Arc::new(AsyncMutex::new(HashMap::new()));

    start_game_loop(
        "room".to_owned(),
        Arc::clone(&state),
        Arc::clone(&room_service),
        senders,
        Arc::clone(&loop_states),
    );

    wait_until("human landlord call phase", || {
        let state = state.lock().unwrap();
        state.phase == LandlordPhase::CallLandlord && state.turn_countdown() == 30
    })
    .await;
    wait_until("human turn countdown", || {
        state.lock().unwrap().turn_countdown() == 29
    })
    .await;

    state.lock().unwrap().request_stop();
    wait_until("human turn loop cleanup", || {
        !loop_states.lock().unwrap().contains_key("room")
    })
    .await;
}

#[tokio::test]
async fn paused_game_loop_only_stops_after_its_pause_wait_is_interrupted() {
    let (room_service, state, loop_states) = loop_room();
    let senders: SessionSenders = Arc::new(AsyncMutex::new(HashMap::new()));
    state.lock().unwrap().base.lock().unwrap().pause();

    start_game_loop(
        "room".to_owned(),
        Arc::clone(&state),
        Arc::clone(&room_service),
        senders,
        Arc::clone(&loop_states),
    );

    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(loop_states.lock().unwrap().contains_key("room"));
    state.lock().unwrap().request_stop();
    wait_until("paused loop cleanup", || {
        !loop_states.lock().unwrap().contains_key("room")
    })
    .await;
}

#[test]
fn automatic_action_helpers_cover_empty_hands_bomb_signal_reuse_and_timeout_passes() {
    let (_room_service, state, _loop_states) = loop_room();
    let mut state = state.lock().unwrap();
    state.phase = LandlordPhase::Play;
    state.current_position = 0;
    state.last_play.clear();
    state.hands = HashMap::from([(0, vec![7]), (1, vec![8])]);
    assert_eq!(choose_timeout_play(&state, 0), vec![7]);

    state.last_play_position = 1;
    state.last_play = vec![8];
    assert!(choose_timeout_play(&state, 0).is_empty());
    assert!(choose_timeout_play(&state, 2).is_empty());

    activate_ai_bomb_signal(&mut state, 0);
    activate_ai_bomb_signal(&mut state, 0);
    assert!(state.ai_bomb_signal_used);
    state.ai_bomb_signal_benchmark = None;
    clear_stale_ai_bomb_signal(&mut state, 0);
    assert_eq!(state.ai_bomb_signal_position, None);

    state.phase = LandlordPhase::Start;
    let (away_position, event) = handle_automatic_action(&mut state, AutoActionReason::Timeout);
    assert_eq!(away_position, Some(0));
    assert!(event.is_none());
}
