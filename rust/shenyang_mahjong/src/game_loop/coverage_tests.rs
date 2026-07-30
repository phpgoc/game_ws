use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use serde_json::json;
use share_type_public::{CommonEvent, GameId, Routes, WsCode, WsJoinRequest};
use tokio::sync::Mutex as AsyncMutex;
use ws_common::{
    ClientRequest, CommonGameState, Delivery, Dispatch, OutboundPayload, RoomService,
    SessionSenders,
};

use crate::game_state::{
    ClaimResponse, ClaimWindowKind, ClaimWindowState, ShenyangMahjongLoopState,
};

use super::{
    apply_timeout_control, auto_discard_tile, deliver, loop_stop_requested, room_uses_common_state,
    settlement_should_stop, should_resolve_timed_out_claims, sleep_or_stop, timed_out_positions,
};

fn join_request(name: &str) -> ClientRequest {
    ClientRequest {
        route: Routes::JOIN as i32,
        data: serde_json::to_value(WsJoinRequest {
            name: name.to_string(),
            password: "room".to_string(),
            game_id: GameId::SHENYANG_MAHJONG,
            session_id: String::new(),
            avatar_url: String::new(),
        })
        .unwrap(),
    }
}

fn state_with_players(player_count: usize) -> ShenyangMahjongLoopState {
    let common = Arc::new(Mutex::new(CommonGameState::default()));
    for position in 0..player_count {
        common
            .lock()
            .unwrap()
            .add_player(position, (position + 1) as u64, &format!("p{position}"));
    }
    ShenyangMahjongLoopState::new(common)
}

#[test]
fn timeout_positions_only_include_unanswered_claimants_or_current_turn() {
    let mut state = state_with_players(4);
    state.current_position = 2;
    state.set_turn_countdown(1);
    assert!(timed_out_positions(&state).is_empty());
    assert!(!should_resolve_timed_out_claims(&state));

    state.set_turn_countdown(0);
    assert_eq!(timed_out_positions(&state), vec![2]);
    assert!(!should_resolve_timed_out_claims(&state));

    state.claim_window = Some(ClaimWindowState {
        tile: 3,
        from_position: 0,
        kind: ClaimWindowKind::Discard,
        eligible_positions: vec![1, 2, 3],
        responses: HashMap::from([(1, ClaimResponse::Pass)]),
    });
    assert_eq!(timed_out_positions(&state), vec![2, 3]);
    assert!(should_resolve_timed_out_claims(&state));
}

#[test]
fn timeout_control_skips_nonwaiting_and_already_ai_controlled_positions() {
    let mut state = state_with_players(2);
    state.current_position = 0;
    state.set_turn_countdown(1);
    assert!(apply_timeout_control(&mut state, &[(0, true)]).is_empty());

    state.set_turn_countdown(0);
    assert!(apply_timeout_control(&mut state, &[(1, true)]).is_empty());
    state.base.lock().unwrap().mark_ai_takeover_position(0);
    assert!(apply_timeout_control(&mut state, &[(0, true)]).is_empty());
    assert!(!state.is_away(0));
}

#[test]
fn room_state_checks_require_the_current_room_instance() {
    let mut room = RoomService::default();
    room.handle_common_request(
        1,
        &join_request("owner"),
        GameId::SHENYANG_MAHJONG,
        crate::game_setting::build_shenyang_mahjong_settings,
    );
    let common = room.room_common_state("room").expect("room state");
    assert!(room_uses_common_state(&room, "room", &common));
    assert!(!room_uses_common_state(
        &room,
        "room",
        &Arc::new(Mutex::new(CommonGameState::default())),
    ));
    assert!(!room_uses_common_state(&room, "missing", &common));
}

#[test]
fn settlement_stops_for_incomplete_or_stopped_rooms() {
    let incomplete = Arc::new(Mutex::new(state_with_players(3)));
    assert!(settlement_should_stop(&incomplete));

    let active = Arc::new(Mutex::new(state_with_players(4)));
    assert!(!settlement_should_stop(&active));
    active.lock().unwrap().request_stop();
    assert!(settlement_should_stop(&active));
    assert!(loop_stop_requested(&active));
}

#[tokio::test]
async fn delivery_and_stop_helpers_handle_recipients_and_zero_waits() {
    let (sender, mut receiver, _disconnected) = ws_common::session_sender_channel(1);
    let senders: SessionSenders = Arc::new(AsyncMutex::new(HashMap::from([(1, sender)])));
    let dispatch = Dispatch {
        messages: vec![
            Delivery {
                recipient: 1,
                payload: OutboundPayload::Event(CommonEvent {
                    code: WsCode::START as i32,
                    data: json!({}),
                }),
            },
            Delivery {
                recipient: 2,
                payload: OutboundPayload::Event(CommonEvent {
                    code: WsCode::START as i32,
                    data: json!({}),
                }),
            },
        ],
    };
    deliver(dispatch, &senders).await;
    assert!(receiver.recv().await.is_some());

    let state = Arc::new(Mutex::new(state_with_players(4)));
    assert!(!sleep_or_stop(&state, Duration::ZERO).await);
    state.lock().unwrap().request_stop();
    assert!(sleep_or_stop(&state, Duration::from_millis(1)).await);
}

#[test]
fn automatic_discard_returns_none_for_empty_hands() {
    let state = state_with_players(1);
    assert_eq!(auto_discard_tile(&state, 0), None);
}
