use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use serde_json::Value;
use share_type_public::{
    GameId, GameParam, GameParamRange, Routes, WsCode, WsRequest, WsResponseCode,
};

use super::{Dispatch, OutboundPayload, RequestResponse, RoomService};
use crate::game_setting::GameSettings;
use crate::game_state::{CommonGameState, GameState};

struct NoAcceptState {
    common: Arc<Mutex<CommonGameState>>,
}

fn common_request(
    service: &mut RoomService,
    session_id: u64,
    game_id: GameId,
    route: Routes,
    data: serde_json::Value,
) -> Dispatch {
    service
        .handle_common_request(
            session_id,
            &WsRequest {
                route: route as i32,
                data,
            },
            game_id,
            settings,
        )
        .expect("common route")
}

fn join_room(
    service: &mut RoomService,
    session_id: u64,
    name: &str,
    room_key: &str,
    game_id: GameId,
) -> Dispatch {
    common_request(
        service,
        session_id,
        game_id,
        Routes::JOIN,
        serde_json::json!({
            "name": name,
            "password": room_key,
            "game_id": game_id as i32
        }),
    )
}

fn has_response(dispatch: &Dispatch, route: Routes, code: WsResponseCode) -> bool {
    dispatch
        .messages
        .iter()
        .any(|message| match &message.payload {
            OutboundPayload::Response(RequestResponse::WithoutData(response)) => {
                response.route == route as i32 && response.code as i32 == code as i32
            }
            OutboundPayload::Response(RequestResponse::WithData(response)) => {
                response.route == route as i32 && response.code as i32 == code as i32
            }
            OutboundPayload::Event(_) => false,
        })
}

#[test]
fn ai_name_prefix_matches_game_family() {
    let service = RoomService::default();

    assert_eq!(
        service.next_ai_name("texas-room", GameId::TEXAS_HOLD_EM, 1),
        "Bot 1"
    );
    assert_eq!(
        service.next_ai_name("open-room", GameId::OPEN_HOLD_EM, 2),
        "Bot 2"
    );
    assert_eq!(
        service.next_ai_name("mahjong-room", GameId::SHENYANG_MAHJONG, 1),
        "AI 1"
    );
}

#[test]
fn every_non_p2p_game_id_can_add_ai_through_common_room_service() {
    for game_id in [
        GameId::LANDLORD,
        GameId::SHENYANG_MAHJONG,
        GameId::TEXAS_HOLD_EM,
        GameId::TRACTOR,
        GameId::OPEN_HOLD_EM,
        GameId::SHORT_DECK_HOLD_EM,
        GameId::OMAHA_HOLD_EM,
    ] {
        let mut service = RoomService::default();
        let room_key = format!("ai-room-{}", game_id as i32);
        let joined = join_room(&mut service, 1, "owner", &room_key, game_id);
        assert!(has_response(&joined, Routes::JOIN, WsResponseCode::JOINED));

        let added = common_request(
            &mut service,
            1,
            game_id,
            Routes::ADD_AI,
            serde_json::json!({ "count": 1 }),
        );

        assert!(has_response(&added, Routes::ADD_AI, WsResponseCode::OK));
        let common = service
            .room_common_state(&room_key)
            .expect("room common state");
        let common = common.lock().unwrap();
        assert_eq!(common.players.len(), 2);
        assert_eq!(common.ai_positions.len(), 1);
    }
}

#[test]
fn ai_counts_toward_capacity_and_removal_frees_the_seat_for_a_human() {
    let mut service = RoomService::default();
    let _ = join_room(&mut service, 1, "owner", "capacity-room", GameId::LANDLORD);
    let added = common_request(
        &mut service,
        1,
        GameId::LANDLORD,
        Routes::ADD_AI,
        serde_json::json!({ "count": 2 }),
    );
    assert!(has_response(&added, Routes::ADD_AI, WsResponseCode::OK));

    let full_join = join_room(&mut service, 2, "human", "capacity-room", GameId::LANDLORD);
    assert!(has_response(
        &full_join,
        Routes::JOIN,
        WsResponseCode::NO_PERMISSION
    ));
    assert_eq!(service.session_position(2), None);

    let removed = common_request(
        &mut service,
        1,
        GameId::LANDLORD,
        Routes::REMOVE_AI,
        serde_json::json!({ "position": 1 }),
    );
    assert!(has_response(
        &removed,
        Routes::REMOVE_AI,
        WsResponseCode::OK
    ));
    assert!(removed.messages.iter().any(|message| {
        matches!(
            &message.payload,
            OutboundPayload::Event(event)
                if event.code == WsCode::QUIT as i32
                    && event.data.get("name").and_then(Value::as_str) == Some("AI 1")
        )
    }));

    let joined = join_room(&mut service, 2, "human", "capacity-room", GameId::LANDLORD);
    assert!(has_response(&joined, Routes::JOIN, WsResponseCode::JOINED));
    assert_eq!(service.session_position(2), Some(1));
}

#[test]
fn only_owner_can_manage_ai_and_only_before_start() {
    let mut service = RoomService::default();
    let _ = join_room(&mut service, 1, "owner", "remove-room", GameId::LANDLORD);
    let _ = join_room(&mut service, 2, "member", "remove-room", GameId::LANDLORD);
    let member_add = common_request(
        &mut service,
        2,
        GameId::LANDLORD,
        Routes::ADD_AI,
        serde_json::json!({ "count": 1 }),
    );
    assert!(has_response(
        &member_add,
        Routes::ADD_AI,
        WsResponseCode::NO_PERMISSION
    ));
    let _ = common_request(
        &mut service,
        1,
        GameId::LANDLORD,
        Routes::ADD_AI,
        serde_json::json!({ "count": 1 }),
    );

    let member_remove = common_request(
        &mut service,
        2,
        GameId::LANDLORD,
        Routes::REMOVE_AI,
        serde_json::json!({ "position": 2 }),
    );
    assert!(has_response(
        &member_remove,
        Routes::REMOVE_AI,
        WsResponseCode::NO_PERMISSION
    ));

    let human_remove = common_request(
        &mut service,
        1,
        GameId::LANDLORD,
        Routes::REMOVE_AI,
        serde_json::json!({ "position": 1 }),
    );
    assert!(has_response(
        &human_remove,
        Routes::REMOVE_AI,
        WsResponseCode::NO_PERMISSION
    ));

    let room_key = service.room_key_of(1).expect("room key");
    let common = service
        .room_common_state(&room_key)
        .expect("room common state");
    service.set_room_game_state(
        &room_key,
        Box::new(NoAcceptState {
            common: Arc::clone(&common),
        }),
    );
    let started_remove = common_request(
        &mut service,
        1,
        GameId::LANDLORD,
        Routes::REMOVE_AI,
        serde_json::json!({ "position": 2 }),
    );
    assert!(has_response(
        &started_remove,
        Routes::REMOVE_AI,
        WsResponseCode::NO_PERMISSION
    ));
    assert!(common.lock().unwrap().is_ai_position(2));

    let mut started_service = RoomService::default();
    let _ = join_room(
        &mut started_service,
        10,
        "owner",
        "started-add-room",
        GameId::LANDLORD,
    );
    let started_room_key = started_service.room_key_of(10).expect("room key");
    let started_common = started_service
        .room_common_state(&started_room_key)
        .expect("common state");
    started_service.set_room_game_state(
        &started_room_key,
        Box::new(NoAcceptState {
            common: Arc::clone(&started_common),
        }),
    );
    let started_add = common_request(
        &mut started_service,
        10,
        GameId::LANDLORD,
        Routes::ADD_AI,
        serde_json::json!({ "count": 1 }),
    );
    assert!(has_response(
        &started_add,
        Routes::ADD_AI,
        WsResponseCode::NO_PERMISSION
    ));
    assert_eq!(started_common.lock().unwrap().players.len(), 1);
}

#[test]
fn clear_game_state_if_same_restores_room_acceptance() {
    let mut service = RoomService::default();
    service.connect(1);
    service.connect(2);
    service.connect(3);

    for (session_id, name) in [(1_u64, "u1"), (2, "u2")] {
        let _ = service.handle_common_request(
            session_id,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({
                    "name": name,
                    "password": "p1",
                    "game_id": GameId::LANDLORD as i32
                }),
            },
            GameId::LANDLORD,
            settings,
        );
    }

    let room_key = service.room_key_of(1).expect("room key");
    let common = service.room_common_state(&room_key).expect("common state");
    service.set_room_game_state(
        &room_key,
        Box::new(NoAcceptState {
            common: Arc::clone(&common),
        }),
    );

    service.clear_room_game_state_if_same(&room_key, &common);
    let join_after_clear = service
        .handle_common_request(
            3,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({
                    "name": "u3",
                    "password": "p1",
                    "game_id": GameId::LANDLORD as i32
                }),
            },
            GameId::LANDLORD,
            settings,
        )
        .expect("join common");
    let joined = join_after_clear
        .messages
        .iter()
        .any(|item| match &item.payload {
            OutboundPayload::Response(RequestResponse::WithData(resp)) => {
                resp.code as i32 == WsResponseCode::JOINED as i32
            }
            _ => false,
        });

    assert!(joined);
    assert_eq!(service.session_position(3), Some(2));
}

#[test]
fn clearing_game_state_preserves_room_members() {
    let mut service = RoomService::default();
    service.connect(1);
    service.connect(2);

    let _ = service.handle_common_request(
        1,
        &WsRequest {
            route: Routes::JOIN as i32,
            data: serde_json::json!({"name":"u1","password":"p1","game_id":GameId::LANDLORD as i32}),
        },
        GameId::LANDLORD,
        settings,
    );
    let _ = service.handle_common_request(
        2,
        &WsRequest {
            route: Routes::JOIN as i32,
            data: serde_json::json!({"name":"u2","password":"p1","game_id":GameId::LANDLORD as i32}),
        },
        GameId::LANDLORD,
        settings,
    );
    let _ = service.disconnect(2);

    service.clear_room_game_state("p1");

    let players = service.room_members("p1");
    assert_eq!(players.len(), 2);
    assert!(
        players
            .iter()
            .any(|(_, name, position, _)| { *position == 0 && name == "u1" })
    );
    assert!(
        players
            .iter()
            .any(|(_, name, position, _)| { *position == 1 && name == "u2" })
    );

    let rejoin = service
        .handle_common_request(
            2,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({"name":"u2","password":"p1","game_id":GameId::LANDLORD as i32}),
            },
            GameId::LANDLORD,
            settings,
        )
        .expect("join common");
    let joined = rejoin.messages.iter().any(|item| match &item.payload {
        OutboundPayload::Response(RequestResponse::WithData(resp)) => {
            item.recipient == 2 && resp.code as i32 == WsResponseCode::JOINED as i32
        }
        _ => false,
    });
    assert!(joined);
    assert_eq!(service.session_position(2), Some(1));
}

#[test]
fn disband_allows_join_recreate_room() {
    let mut service = RoomService::default();
    service.connect(1);
    service.connect(2);
    service.connect(3);

    let _ = service.handle_common_request(
        1,
        &WsRequest {
            route: Routes::JOIN as i32,
            data: serde_json::json!({"name":"u1","password":"p1","game_id":GameId::LANDLORD as i32}),
        },
        GameId::LANDLORD,
        settings,
    );
    let _ = service.handle_common_request(
        2,
        &WsRequest {
            route: Routes::JOIN as i32,
            data: serde_json::json!({"name":"u2","password":"p1","game_id":GameId::LANDLORD as i32}),
        },
        GameId::LANDLORD,
        settings,
    );
    let _ = service.handle_common_request(
        1,
        &WsRequest {
            route: Routes::DISBAND as i32,
            data: serde_json::json!({}),
        },
        GameId::LANDLORD,
        settings,
    );

    let join_after_disband = service
        .handle_common_request(
            3,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({"name":"u3","password":"p1","game_id":GameId::LANDLORD as i32}),
            },
            GameId::LANDLORD,
            settings,
        )
        .expect("join common");
    let joined_ok = join_after_disband
        .messages
        .iter()
        .any(|item| match &item.payload {
            OutboundPayload::Response(RequestResponse::WithData(resp)) => {
                resp.code as i32 == WsResponseCode::JOINED as i32
            }
            _ => false,
        });
    assert!(joined_ok);
}

#[test]
fn disconnected_name_can_rejoin_same_position() {
    let mut service = RoomService::default();
    service.connect(1);
    service.connect(2);
    service.connect(3);

    let _ = service.handle_common_request(
        1,
        &WsRequest {
            route: Routes::JOIN as i32,
            data: serde_json::json!({"name":"u1","password":"p1","game_id":GameId::LANDLORD as i32}),
        },
        GameId::LANDLORD,
        settings,
    );
    let _ = service.handle_common_request(
        2,
        &WsRequest {
            route: Routes::JOIN as i32,
            data: serde_json::json!({"name":"u2","password":"p1","game_id":GameId::LANDLORD as i32}),
        },
        GameId::LANDLORD,
        settings,
    );

    let common = service.room_common_state("p1").expect("common state");
    common.lock().unwrap().mark_away(0);
    let disconnect = service.disconnect(1);
    let inactive_event = disconnect.messages.iter().any(|item| match &item.payload {
        OutboundPayload::Event(event) if event.code == WsCode::JOIN as i32 => {
            item.recipient == 2
                && event.data.get("name").and_then(|v| v.as_str()) == Some("u1")
                && event.data.get("position").and_then(|v| v.as_i64()) == Some(0)
                && event.data.get("is_active").and_then(|v| v.as_bool()) == Some(false)
        }
        _ => false,
    });
    assert!(inactive_event);

    let rejoin = service
        .handle_common_request(
            3,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({"name":"u1","password":"p1","game_id":GameId::LANDLORD as i32}),
            },
            GameId::LANDLORD,
            settings,
        )
        .expect("join common");

    assert_eq!(service.session_position(3), Some(0));
    {
        let common = common.lock().unwrap();
        assert!(!common.is_disconnected(0));
        assert!(!common.is_away(0));
    }
    assert!(
        service
            .room_members("p1")
            .iter()
            .any(|(session_id, _, position, _)| *position == 0 && *session_id == 3)
    );
    let active_event = rejoin.messages.iter().any(|item| match &item.payload {
        OutboundPayload::Event(event) if event.code == WsCode::JOIN as i32 => {
            item.recipient == 2
                && event.data.get("name").and_then(|v| v.as_str()) == Some("u1")
                && event.data.get("position").and_then(|v| v.as_i64()) == Some(0)
                && event.data.get("is_active").and_then(|v| v.as_bool()) == Some(true)
        }
        _ => false,
    });
    assert!(active_event);
    let joined = rejoin.messages.iter().any(|item| match &item.payload {
        OutboundPayload::Response(RequestResponse::WithData(resp)) => {
            item.recipient == 3 && resp.code as i32 == WsResponseCode::JOINED as i32
        }
        _ => false,
    });
    assert!(joined);
}

#[test]
fn different_name_replaces_disconnected_position_when_new_seats_are_locked() {
    let mut service = RoomService::default();
    let _ = join_room(&mut service, 1, "owner", "locked-room", GameId::LANDLORD);
    let _ = common_request(
        &mut service,
        2,
        GameId::LANDLORD,
        Routes::JOIN,
        serde_json::json!({
            "name": "old-player",
            "password": "locked-room",
            "game_id": GameId::LANDLORD as i32,
            "avatar_url": "old-avatar"
        }),
    );
    let common = service
        .room_common_state("locked-room")
        .expect("common state");
    common.lock().unwrap().mark_away(1);

    let _ = service.disconnect(2);
    service.set_room_game_state(
        "locked-room",
        Box::new(NoAcceptState {
            common: Arc::clone(&common),
        }),
    );

    let replacement = join_room(
        &mut service,
        3,
        "replacement",
        "locked-room",
        GameId::LANDLORD,
    );

    assert!(has_response(
        &replacement,
        Routes::JOIN,
        WsResponseCode::JOINED
    ));
    assert_eq!(service.session_position(3), Some(1));
    let mut members = service.room_members("locked-room");
    members.sort_by_key(|(_, _, position, _)| *position);
    assert_eq!(
        members,
        vec![
            (1, "owner".to_string(), 0, String::new()),
            (3, "replacement".to_string(), 1, String::new()),
        ]
    );
    {
        let common = common.lock().unwrap();
        assert!(!common.is_disconnected(1));
        assert!(!common.is_away(1));
        assert_eq!(common.player_avatar(1), "");
    }

    let old_player = join_room(
        &mut service,
        4,
        "old-player",
        "locked-room",
        GameId::LANDLORD,
    );
    assert!(has_response(
        &old_player,
        Routes::JOIN,
        WsResponseCode::NO_PERMISSION
    ));
}

#[test]
fn disconnect_removes_room_only_after_last_connected_human_leaves() {
    let mut service = RoomService::default();
    let _ = join_room(&mut service, 1, "u1", "disconnect-room", GameId::LANDLORD);
    let _ = join_room(&mut service, 2, "u2", "disconnect-room", GameId::LANDLORD);
    let common = service
        .room_common_state("disconnect-room")
        .expect("room common state");
    common.lock().unwrap().turn_countdown = 37;

    let first_disconnect = service.disconnect(1);

    assert!(service.room_exists("disconnect-room"));
    assert_eq!(service.connected_session_ids("disconnect-room"), vec![2]);
    {
        let common = common.lock().unwrap();
        assert!(common.is_disconnected(0));
        assert!(!common.stop_requested());
        assert_eq!(common.turn_countdown, 37);
    }
    assert!(first_disconnect.messages.iter().any(|message| {
        message.recipient == 2
            && matches!(
                &message.payload,
                OutboundPayload::Event(event)
                    if event.code == WsCode::JOIN as i32
                        && event.data.get("is_active").and_then(Value::as_bool)
                            == Some(false)
            )
    }));

    let last_disconnect = service.disconnect(2);

    assert!(last_disconnect.messages.is_empty());
    assert!(!service.room_exists("disconnect-room"));
    assert_eq!(service.room_count(), 0);
    let common = common.lock().unwrap();
    assert!(common.is_disconnected(1));
    assert!(common.stop_requested());
    assert_eq!(common.turn_countdown, 0);
    // A normal disconnect retains seats in the old state so a game loop
    // can treat it as away/AI takeover until the room is terminated.
    assert_eq!(common.players.len(), 2);
}

#[test]
fn authorized_disconnect_marks_the_retained_seat_for_ai_takeover() {
    let mut service = RoomService::default();
    let _ = join_room(&mut service, 1, "u1", "takeover-room", GameId::LANDLORD);
    let _ = join_room(&mut service, 2, "u2", "takeover-room", GameId::LANDLORD);
    let common = service
        .room_common_state("takeover-room")
        .expect("room common state");

    let disconnect = service.disconnect_with_ai_takeover(1, true);

    let common = common.lock().unwrap();
    assert!(common.is_disconnected(0));
    assert!(common.is_ai_takeover_position(0));
    assert!(disconnect.messages.iter().any(|message| {
        message.recipient == 2
            && matches!(
                &message.payload,
                OutboundPayload::Event(event)
                    if event.code == WsCode::JOIN as i32
                        && event.data.get("is_ai_takeover").and_then(Value::as_bool)
                            == Some(true)
            )
    }));
}

#[test]
fn new_game_reset_preserves_takeover_until_the_human_rejoins() {
    let mut service = RoomService::default();
    let _ = join_room(
        &mut service,
        1,
        "u1",
        "reset-takeover-room",
        GameId::LANDLORD,
    );
    let _ = join_room(
        &mut service,
        2,
        "u2",
        "reset-takeover-room",
        GameId::LANDLORD,
    );

    let disconnect = service.disconnect_with_ai_takeover(1, true);
    assert!(disconnect.messages.iter().any(|message| {
        matches!(
            &message.payload,
            OutboundPayload::Event(event)
                if event.data.get("is_ai_takeover").and_then(Value::as_bool) == Some(true)
        )
    }));
    let common = service
        .room_common_state("reset-takeover-room")
        .expect("common state");
    common.lock().unwrap().mark_away(0);

    let next_common = service
        .reset_room_common_state_for_new_game("reset-takeover-room")
        .expect("reset common state");
    {
        let common = next_common.lock().unwrap();
        assert!(common.is_away(0));
        assert!(common.is_disconnected(0));
        assert!(common.is_ai_takeover_position(0));
        assert!(!common.is_ai_position(0));
    }

    let rejoin = join_room(
        &mut service,
        3,
        "u1",
        "reset-takeover-room",
        GameId::LANDLORD,
    );
    assert!(has_response(&rejoin, Routes::JOIN, WsResponseCode::JOINED));
    assert_eq!(service.session_position(3), Some(0));
    let common = service
        .room_common_state("reset-takeover-room")
        .expect("common state after rejoin");
    let common = common.lock().unwrap();
    assert!(!common.is_away(0));
    assert!(!common.is_disconnected(0));
    assert!(!common.is_ai_takeover_position(0));
}

#[test]
fn official_session_can_be_resolved_by_room_position() {
    let mut service = RoomService::default();
    let _ = common_request(
        &mut service,
        1,
        GameId::LANDLORD,
        Routes::JOIN,
        serde_json::json!({
            "name": "member",
            "password": "official-session-room",
            "game_id": GameId::LANDLORD as i32,
            "session_id": "official-session-token"
        }),
    );

    assert_eq!(
        service.room_position_official_session_id("official-session-room", 0),
        Some("official-session-token".to_owned())
    );
    assert_eq!(
        service.room_position_official_session_id("official-session-room", 1),
        None
    );
}

#[test]
fn ai_players_do_not_keep_room_alive_after_last_human_disconnects() {
    let mut service = RoomService::default();
    let _ = join_room(
        &mut service,
        1,
        "owner",
        "ai-disconnect-room",
        GameId::LANDLORD,
    );
    let added = common_request(
        &mut service,
        1,
        GameId::LANDLORD,
        Routes::ADD_AI,
        serde_json::json!({ "count": 2 }),
    );
    assert!(has_response(&added, Routes::ADD_AI, WsResponseCode::OK));
    let common = service
        .room_common_state("ai-disconnect-room")
        .expect("room common state");

    let _ = service.disconnect(1);

    assert!(!service.room_exists("ai-disconnect-room"));
    let common = common.lock().unwrap();
    assert!(common.stop_requested());
    assert_eq!(common.players.len(), 3);
    assert_eq!(common.ai_positions.len(), 2);
}

#[test]
fn ai_players_do_not_keep_room_alive_after_last_human_quits() {
    fn four_player_settings() -> super::SettingsBuilderResult {
        (GameSettings::new(4, 4), HashMap::new())
    }

    let mut service = RoomService::default();
    let room_key = "ai-quit-room";
    let joined = service
        .handle_common_request(
            1,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({
                    "name": "owner",
                    "password": room_key,
                    "game_id": GameId::SHENYANG_MAHJONG as i32
                }),
            },
            GameId::SHENYANG_MAHJONG,
            four_player_settings,
        )
        .expect("join common route");
    assert!(has_response(&joined, Routes::JOIN, WsResponseCode::JOINED));

    let added = service
        .handle_common_request(
            1,
            &WsRequest {
                route: Routes::ADD_AI as i32,
                data: serde_json::json!({ "count": 3 }),
            },
            GameId::SHENYANG_MAHJONG,
            four_player_settings,
        )
        .expect("add ai common route");
    assert!(has_response(&added, Routes::ADD_AI, WsResponseCode::OK));
    let old_common = service
        .room_common_state(room_key)
        .expect("old room common state");
    {
        let common = old_common.lock().unwrap();
        assert_eq!(common.players.len(), 4);
        assert_eq!(common.ai_positions.len(), 3);
    }

    let quit = service
        .handle_common_request(
            1,
            &WsRequest {
                route: Routes::QUIT as i32,
                data: serde_json::json!({}),
            },
            GameId::SHENYANG_MAHJONG,
            four_player_settings,
        )
        .expect("quit common route");

    assert!(has_response(&quit, Routes::QUIT, WsResponseCode::OK));
    assert!(!service.room_exists(room_key));
    assert_eq!(service.room_count(), 0);
    assert_eq!(service.room_key_of(1), None);
    {
        let common = old_common.lock().unwrap();
        assert!(common.stop_requested());
        assert_eq!(common.turn_countdown, 0);
        assert_eq!(common.players.len(), 3);
        assert_eq!(common.ai_positions.len(), 3);
    }

    let recreated = service
        .handle_common_request(
            2,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({
                    "name": "new-owner",
                    "password": room_key,
                    "game_id": GameId::SHENYANG_MAHJONG as i32
                }),
            },
            GameId::SHENYANG_MAHJONG,
            four_player_settings,
        )
        .expect("recreate common route");
    assert!(has_response(
        &recreated,
        Routes::JOIN,
        WsResponseCode::JOINED
    ));
    let new_common = service
        .room_common_state(room_key)
        .expect("new room common state");
    assert!(!Arc::ptr_eq(&old_common, &new_common));
    let new_common = new_common.lock().unwrap();
    assert_eq!(new_common.players.len(), 1);
    assert!(new_common.ai_positions.is_empty());
}

#[test]
fn last_disconnect_releases_name_and_old_cleanup_cannot_clear_recreated_room() {
    let mut service = RoomService::default();
    let _ = join_room(
        &mut service,
        1,
        "old-owner",
        "recreated-room",
        GameId::LANDLORD,
    );
    let old_common = service
        .room_common_state("recreated-room")
        .expect("old room common state");

    let _ = service.disconnect(1);
    assert!(!service.room_exists("recreated-room"));

    let recreated = join_room(
        &mut service,
        2,
        "new-owner",
        "recreated-room",
        GameId::LANDLORD,
    );
    assert!(has_response(
        &recreated,
        Routes::JOIN,
        WsResponseCode::JOINED
    ));
    let new_common = service
        .room_common_state("recreated-room")
        .expect("new room common state");
    assert!(!Arc::ptr_eq(&old_common, &new_common));
    assert!(old_common.lock().unwrap().stop_requested());
    assert!(!new_common.lock().unwrap().stop_requested());

    service.set_room_game_state(
        "recreated-room",
        Box::new(NoAcceptState {
            common: Arc::clone(&new_common),
        }),
    );
    // Simulate the old loop's final cleanup after a new room with the same
    // name has already been created.
    service.clear_room_game_state_if_same("recreated-room", &old_common);

    let rejected = join_room(
        &mut service,
        3,
        "late-player",
        "recreated-room",
        GameId::LANDLORD,
    );
    assert!(has_response(
        &rejected,
        Routes::JOIN,
        WsResponseCode::NO_PERMISSION
    ));
    assert!(Arc::ptr_eq(
        &service
            .room_common_state("recreated-room")
            .expect("current room common state"),
        &new_common
    ));
}

#[test]
fn quit_permanently_removes_player_and_always_requests_loop_stop() {
    let mut service = RoomService::default();
    let _ = join_room(&mut service, 1, "quitter", "quit-room", GameId::LANDLORD);
    let _ = join_room(&mut service, 2, "remaining", "quit-room", GameId::LANDLORD);
    let common = service
        .room_common_state("quit-room")
        .expect("room common state");
    common.lock().unwrap().turn_countdown = 29;

    let quit = common_request(
        &mut service,
        1,
        GameId::LANDLORD,
        Routes::QUIT,
        serde_json::json!({}),
    );

    assert!(has_response(&quit, Routes::QUIT, WsResponseCode::OK));
    assert!(service.room_exists("quit-room"));
    assert_eq!(service.room_key_of(1), None);
    assert_eq!(service.session_position(1), None);
    let common = common.lock().unwrap();
    assert!(common.stop_requested());
    assert_eq!(common.turn_countdown, 0);
    assert_eq!(common.players.len(), 1);
    assert!(!common.players.values().any(|(_, name)| name == "quitter"));
}

#[test]
fn join_accepts_multiple_game_ids_but_room_keeps_created_game() {
    let mut service = RoomService::default();
    service.connect(1);
    service.connect(2);
    service.connect(3);

    let accepted = |game_id| {
        matches!(
            game_id,
            GameId::TEXAS_HOLD_EM | GameId::OPEN_HOLD_EM | GameId::OMAHA_HOLD_EM
        )
    };

    let texas_join = service
        .handle_common_request_with_game_acceptance(
            1,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({
                    "name": "u1",
                    "password": "poker-room",
                    "game_id": GameId::TEXAS_HOLD_EM as i32
                }),
            },
            accepted,
            settings,
        )
        .expect("join common");
    assert!(texas_join.messages.iter().any(|item| match &item.payload {
        OutboundPayload::Response(RequestResponse::WithData(resp)) => {
            item.recipient == 1 && resp.code as i32 == WsResponseCode::JOINED as i32
        }
        _ => false,
    }));
    assert_eq!(
        service.room_game_id("poker-room"),
        Some(GameId::TEXAS_HOLD_EM)
    );

    let mixed_game = service
        .handle_common_request_with_game_acceptance(
            2,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({
                    "name": "u2",
                    "password": "poker-room",
                    "game_id": GameId::OMAHA_HOLD_EM as i32
                }),
            },
            accepted,
            settings,
        )
        .expect("join common");
    assert!(mixed_game.messages.iter().any(|item| match &item.payload {
        OutboundPayload::Response(RequestResponse::WithoutData(resp)) => {
            item.recipient == 2 && resp.code as i32 == WsResponseCode::WRONG_GAME as i32
        }
        _ => false,
    }));

    let open_join = service
        .handle_common_request_with_game_acceptance(
            3,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({
                    "name": "u3",
                    "password": "open-room",
                    "game_id": GameId::OPEN_HOLD_EM as i32
                }),
            },
            accepted,
            settings,
        )
        .expect("join common");
    assert!(open_join.messages.iter().any(|item| match &item.payload {
        OutboundPayload::Response(RequestResponse::WithData(resp)) => {
            item.recipient == 3 && resp.code as i32 == WsResponseCode::JOINED as i32
        }
        _ => false,
    }));
    assert_eq!(
        service.room_game_id("open-room"),
        Some(GameId::OPEN_HOLD_EM)
    );
}

#[test]
fn join_idempotent_for_same_room_same_name() {
    let mut service = RoomService::default();
    service.connect(1);

    let _ = service.handle_common_request(
        1,
        &WsRequest {
            route: Routes::JOIN as i32,
            data: serde_json::json!({"name":"u1","password":"p1","game_id":GameId::LANDLORD as i32}),
        },
        GameId::LANDLORD,
        settings,
    );

    let rejoin = service
        .handle_common_request(
            1,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({"name":"u1","password":"p1","game_id":GameId::LANDLORD as i32}),
            },
            GameId::LANDLORD,
            settings,
        )
        .expect("join common");
    let rejoin_joined = rejoin.messages.iter().any(|item| match &item.payload {
        OutboundPayload::Response(RequestResponse::WithData(resp)) => {
            resp.code as i32 == WsResponseCode::JOINED as i32
        }
        _ => false,
    });
    assert!(rejoin_joined);

    let join_other_room = service
        .handle_common_request(
            1,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({"name":"u1","password":"p2","game_id":GameId::LANDLORD as i32}),
            },
            GameId::LANDLORD,
            settings,
        )
        .expect("join common");
    let join_other_room_denied = join_other_room
        .messages
        .iter()
        .any(|item| match &item.payload {
            OutboundPayload::Response(RequestResponse::WithoutData(resp)) => {
                resp.code as i32 == WsResponseCode::NO_PERMISSION as i32
            }
            _ => false,
        });
    assert!(join_other_room_denied);
}

#[test]
fn join_rejects_duplicate_name_and_overflow() {
    let mut service = RoomService::default();
    service.connect(1);
    service.connect(2);
    service.connect(3);
    service.connect(4);

    let _ = service.handle_common_request(
        1,
        &WsRequest {
            route: Routes::JOIN as i32,
            data: serde_json::json!({"name":"u1","password":"p1","game_id":GameId::LANDLORD as i32}),
        },
        GameId::LANDLORD,
        settings,
    );

    let duplicate = service
        .handle_common_request(
            2,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({"name":"u1","password":"p1","game_id":GameId::LANDLORD as i32}),
            },
            GameId::LANDLORD,
            settings,
        )
        .expect("join common");
    let duplicate_denied = duplicate.messages.iter().any(|item| match &item.payload {
        OutboundPayload::Response(RequestResponse::WithoutData(resp)) => {
            item.recipient == 2 && resp.code as i32 == WsResponseCode::NO_PERMISSION as i32
        }
        _ => false,
    });
    assert!(duplicate_denied);

    let _ = service.handle_common_request(
        2,
        &WsRequest {
            route: Routes::JOIN as i32,
            data: serde_json::json!({"name":"u2","password":"p1","game_id":GameId::LANDLORD as i32}),
        },
        GameId::LANDLORD,
        settings,
    );
    let _ = service.handle_common_request(
        3,
        &WsRequest {
            route: Routes::JOIN as i32,
            data: serde_json::json!({"name":"u3","password":"p1","game_id":GameId::LANDLORD as i32}),
        },
        GameId::LANDLORD,
        settings,
    );
    let overflow = service
        .handle_common_request(
            4,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({"name":"u4","password":"p1","game_id":GameId::LANDLORD as i32}),
            },
            GameId::LANDLORD,
            settings,
        )
        .expect("join common");
    let overflow_denied = overflow.messages.iter().any(|item| match &item.payload {
        OutboundPayload::Response(RequestResponse::WithoutData(resp)) => {
            item.recipient == 4 && resp.code as i32 == WsResponseCode::NO_PERMISSION as i32
        }
        _ => false,
    });
    assert!(overflow_denied);
}

#[test]
fn join_rejects_wrong_game_id() {
    let mut service = RoomService::default();
    service.connect(1);

    let dispatch = service
        .handle_common_request(
            1,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({
                    "name": "u1",
                    "password": "p1",
                    "game_id": GameId::SHENYANG_MAHJONG as i32
                }),
            },
            GameId::LANDLORD,
            settings,
        )
        .expect("join common");

    let wrong_game = dispatch.messages.iter().any(|item| match &item.payload {
        OutboundPayload::Response(RequestResponse::WithoutData(resp)) => {
            item.recipient == 1 && resp.code as i32 == WsResponseCode::WRONG_GAME as i32
        }
        _ => false,
    });
    assert!(wrong_game);
    assert!(service.room_key_of(1).is_none());
}

#[test]
fn message_pause_resume_go_to_other_only() {
    let mut service = RoomService::default();
    service.connect(1);
    service.connect(2);
    service.connect(3);

    let _ = service.handle_common_request(
        1,
        &WsRequest {
            route: Routes::JOIN as i32,
            data: serde_json::json!({"name":"u1","password":"p1","game_id":GameId::LANDLORD as i32}),
        },
        GameId::LANDLORD,
        settings,
    );
    let join_dispatch = service
        .handle_common_request(
            2,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({"name":"u2","password":"p1","game_id":GameId::LANDLORD as i32}),
            },
            GameId::LANDLORD,
            settings,
        )
        .expect("join common");
    let join_response_has_settings =
        join_dispatch
            .messages
            .iter()
            .any(|item| match &item.payload {
                OutboundPayload::Response(RequestResponse::WithData(resp)) => {
                    item.recipient == 2
                        && resp.code as i32 == WsResponseCode::JOINED as i32
                        && resp.data.get("current_configs").is_some()
                        && resp.data.get("name").is_none()
                }
                _ => false,
            });
    assert!(join_response_has_settings);
    let join_event_has_no_settings =
        join_dispatch
            .messages
            .iter()
            .any(|item| match &item.payload {
                OutboundPayload::Event(event) if event.code == WsCode::JOIN as i32 => {
                    event.data.get("settings").is_none() && event.data.get("position").is_some()
                }
                _ => false,
            });
    assert!(join_event_has_no_settings);
    let _ = service.handle_common_request(
        3,
        &WsRequest {
            route: Routes::JOIN as i32,
            data: serde_json::json!({"name":"u3","password":"p2","game_id":GameId::LANDLORD as i32}),
        },
        GameId::LANDLORD,
        settings,
    );

    let message = service
        .handle_common_request(
            1,
            &WsRequest {
                route: Routes::MESSAGE as i32,
                data: serde_json::json!({"message":"hi"}),
            },
            GameId::LANDLORD,
            settings,
        )
        .expect("message common");
    assert_eq!(
        recipients_of(WsCode::MESSAGE as i32, &message),
        [2_u64].into_iter().collect()
    );

    let pause = service
        .handle_common_request(
            1,
            &WsRequest {
                route: Routes::PAUSE as i32,
                data: serde_json::json!({}),
            },
            GameId::LANDLORD,
            settings,
        )
        .expect("pause common");
    assert_eq!(
        recipients_of(WsCode::PAUSE as i32, &pause),
        [2_u64].into_iter().collect()
    );

    let resume = service
        .handle_common_request(
            1,
            &WsRequest {
                route: Routes::RESUME as i32,
                data: serde_json::json!({}),
            },
            GameId::LANDLORD,
            settings,
        )
        .expect("resume common");
    assert_eq!(
        recipients_of(WsCode::RESUME as i32, &resume),
        [2_u64].into_iter().collect()
    );
}

#[test]
fn non_owner_join_receives_param_descriptions_for_viewing_settings() {
    let mut service = RoomService::default();
    service.connect(1);
    service.connect(2);

    let _ = service.handle_common_request(
        1,
        &WsRequest {
            route: Routes::JOIN as i32,
            data: serde_json::json!({"name":"u1","password":"p1","game_id":GameId::LANDLORD as i32}),
        },
        GameId::LANDLORD,
        settings,
    );

    let join = service
        .handle_common_request(
            2,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({"name":"u2","password":"p1","game_id":GameId::LANDLORD as i32}),
            },
            GameId::LANDLORD,
            settings,
        )
        .expect("join common");

    let non_owner_gets_param_descriptions = join.messages.iter().any(|item| match &item.payload {
        OutboundPayload::Response(RequestResponse::WithData(resp)) => {
            item.recipient == 2
                && resp.route == Routes::JOIN as i32
                && resp.code as i32 == WsResponseCode::JOINED as i32
                && resp
                    .data
                    .get("param_descriptions")
                    .and_then(|params| params.get("test_param"))
                    .is_some()
        }
        _ => false,
    });
    assert!(non_owner_gets_param_descriptions);
}

#[test]
fn pause_resume_must_follow_state() {
    let mut service = RoomService::default();
    service.connect(1);
    service.connect(2);

    let _ = service.handle_common_request(
        1,
        &WsRequest {
            route: Routes::JOIN as i32,
            data: serde_json::json!({"name":"u1","password":"p1","game_id":GameId::LANDLORD as i32}),
        },
        GameId::LANDLORD,
        settings,
    );
    let _ = service.handle_common_request(
        2,
        &WsRequest {
            route: Routes::JOIN as i32,
            data: serde_json::json!({"name":"u2","password":"p1","game_id":GameId::LANDLORD as i32}),
        },
        GameId::LANDLORD,
        settings,
    );

    let resume_before_pause = service
        .handle_common_request(
            1,
            &WsRequest {
                route: Routes::RESUME as i32,
                data: serde_json::json!({}),
            },
            GameId::LANDLORD,
            settings,
        )
        .expect("resume common");
    let resume_denied = resume_before_pause
        .messages
        .iter()
        .any(|item| match &item.payload {
            OutboundPayload::Response(RequestResponse::WithoutData(resp)) => {
                resp.code as i32 == WsResponseCode::NO_PERMISSION as i32
            }
            _ => false,
        });
    assert!(resume_denied);

    let _ = service.handle_common_request(
        1,
        &WsRequest {
            route: Routes::PAUSE as i32,
            data: serde_json::json!({}),
        },
        GameId::LANDLORD,
        settings,
    );
    let pause_again = service
        .handle_common_request(
            1,
            &WsRequest {
                route: Routes::PAUSE as i32,
                data: serde_json::json!({}),
            },
            GameId::LANDLORD,
            settings,
        )
        .expect("pause common");
    let pause_denied = pause_again.messages.iter().any(|item| match &item.payload {
        OutboundPayload::Response(RequestResponse::WithoutData(resp)) => {
            resp.code as i32 == WsResponseCode::NO_PERMISSION as i32
        }
        _ => false,
    });
    assert!(pause_denied);
}

#[test]
fn position_hole_reused_after_quit() {
    let mut service = RoomService::default();
    service.connect(1);
    service.connect(2);
    service.connect(3);
    service.connect(4);

    let _ = service.handle_common_request(
        1,
        &WsRequest {
            route: Routes::JOIN as i32,
            data: serde_json::json!({"name":"u1","password":"p1","game_id":GameId::LANDLORD as i32}),
        },
        GameId::LANDLORD,
        settings,
    );
    let _ = service.handle_common_request(
        2,
        &WsRequest {
            route: Routes::JOIN as i32,
            data: serde_json::json!({"name":"u2","password":"p1","game_id":GameId::LANDLORD as i32}),
        },
        GameId::LANDLORD,
        settings,
    );
    let _ = service.handle_common_request(
        3,
        &WsRequest {
            route: Routes::JOIN as i32,
            data: serde_json::json!({"name":"u3","password":"p1","game_id":GameId::LANDLORD as i32}),
        },
        GameId::LANDLORD,
        settings,
    );

    let _ = service.handle_common_request(
        2,
        &WsRequest {
            route: Routes::QUIT as i32,
            data: serde_json::json!({}),
        },
        GameId::LANDLORD,
        settings,
    );

    let join4 = service
        .handle_common_request(
            4,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({"name":"u4","password":"p1","game_id":GameId::LANDLORD as i32}),
            },
            GameId::LANDLORD,
            settings,
        )
        .expect("join common");

    let reused = join4.messages.iter().any(|item| match &item.payload {
        OutboundPayload::Event(event) if event.code == WsCode::JOIN as i32 => {
            event.data.get("position").and_then(|v| v.as_i64()) == Some(1)
        }
        _ => false,
    });
    assert!(reused);
}

fn recipients_of(code: i32, dispatch: &Dispatch) -> HashSet<u64> {
    dispatch
        .messages
        .iter()
        .filter_map(|item| match &item.payload {
            OutboundPayload::Event(event) if event.code == code => Some(item.recipient),
            _ => None,
        })
        .collect()
}

#[test]
fn setting_updates_room_broadcasts_and_affects_later_join() {
    let mut service = RoomService::default();
    service.connect(1);
    service.connect(2);
    service.connect(3);

    let _ = service.handle_common_request(
        1,
        &WsRequest {
            route: Routes::JOIN as i32,
            data: serde_json::json!({"name":"u1","password":"p1","game_id":GameId::LANDLORD as i32}),
        },
        GameId::LANDLORD,
        settings,
    );
    let _ = service.handle_common_request(
        2,
        &WsRequest {
            route: Routes::JOIN as i32,
            data: serde_json::json!({"name":"u2","password":"p1","game_id":GameId::LANDLORD as i32}),
        },
        GameId::LANDLORD,
        settings,
    );

    let setting = service
        .handle_common_request(
            1,
            &WsRequest {
                route: Routes::SETTING as i32,
                data: serde_json::json!({"current_configs":{"test_param":500}}),
            },
            GameId::LANDLORD,
            settings,
        )
        .expect("setting common");

    let owner_gets_current_configs = setting.messages.iter().any(|item| match &item.payload {
        OutboundPayload::Response(RequestResponse::WithData(resp)) => {
            item.recipient == 1
                && resp.route == Routes::SETTING as i32
                && resp.code as i32 == WsResponseCode::OK as i32
                && resp
                    .data
                    .get("current_configs")
                    .and_then(|configs| configs.get("test_param"))
                    .and_then(|value| value.as_i64())
                    == Some(500)
        }
        _ => false,
    });
    assert!(owner_gets_current_configs);

    let other_gets_setting_event = setting.messages.iter().any(|item| match &item.payload {
        OutboundPayload::Event(event) if event.code == WsCode::SETTING as i32 => {
            item.recipient == 2
                && event
                    .data
                    .get("current_configs")
                    .and_then(|configs| configs.get("test_param"))
                    .and_then(|value| value.as_i64())
                    == Some(500)
        }
        _ => false,
    });
    assert!(other_gets_setting_event);

    let later_join = service
        .handle_common_request(
            3,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({"name":"u3","password":"p1","game_id":GameId::LANDLORD as i32}),
            },
            GameId::LANDLORD,
            settings,
        )
        .expect("join common");
    let later_join_gets_updated_configs =
        later_join.messages.iter().any(|item| match &item.payload {
            OutboundPayload::Response(RequestResponse::WithData(resp)) => {
                item.recipient == 3
                    && resp.route == Routes::JOIN as i32
                    && resp
                        .data
                        .get("current_configs")
                        .and_then(|configs| configs.get("test_param"))
                        .and_then(|value| value.as_i64())
                        == Some(500)
            }
            _ => false,
        });
    assert!(later_join_gets_updated_configs);
}

fn settings() -> super::SettingsBuilderResult {
    let params: HashMap<String, GameParam> = [(
        "test_param".into(),
        GameParam::Range(GameParamRange {
            default: 200,
            min: 50,
            max: 2000,
        }),
    )]
    .into_iter()
    .collect();

    let mut s = GameSettings::new(3, 3);
    for (key, param) in &params {
        if let GameParam::Range(r) = param {
            s.values.insert(key.clone(), r.default);
        }
    }

    (s, params)
}

#[test]
fn official_games_can_swap_two_non_owner_players() {
    for game_id in [GameId::LANDLORD, GameId::SHENYANG_MAHJONG, GameId::TRACTOR] {
        let mut service = RoomService::default();
        service.connect(1);
        service.connect(2);
        service.connect(3);

        for (session_id, name) in [(1_u64, "u1"), (2, "u2"), (3, "u3")] {
            let _ = service.handle_common_request(
                session_id,
                &WsRequest {
                    route: Routes::JOIN as i32,
                    data: serde_json::json!({
                        "name": name,
                        "password": "p1",
                        "game_id": game_id as i32,
                        "session_id": format!("official-{session_id}")
                    }),
                },
                game_id,
                settings,
            );
        }

        let swap = service
            .handle_common_request(
                1,
                &WsRequest {
                    route: Routes::SWAP as i32,
                    data: serde_json::json!({ "a": 1, "b": 2 }),
                },
                game_id,
                settings,
            )
            .expect("swap common");

        assert_eq!(service.session_position(1), Some(0));
        assert_eq!(service.session_position(2), Some(2));
        assert_eq!(service.session_position(3), Some(1));

        let swap_event = swap.messages.iter().any(|item| match &item.payload {
            OutboundPayload::Event(event) if event.code == WsCode::SWAP as i32 => {
                event.data.get("a").and_then(|v| v.as_u64()) == Some(1)
                    && event.data.get("b").and_then(|v| v.as_u64()) == Some(2)
            }
            _ => false,
        });
        assert!(swap_event, "missing swap event for {game_id:?}");
    }
}

#[test]
fn swap_rejects_non_official_room() {
    let mut service = RoomService::default();
    service.connect(1);
    service.connect(2);

    for (session_id, name) in [(1_u64, "u1"), (2, "u2")] {
        let _ = service.handle_common_request(
            session_id,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({
                    "name": name,
                    "password": "p1",
                    "game_id": GameId::LANDLORD as i32
                }),
            },
            GameId::LANDLORD,
            settings,
        );
    }

    let swap = service
        .handle_common_request(
            1,
            &WsRequest {
                route: Routes::SWAP as i32,
                data: serde_json::json!({ "a": 0, "b": 1 }),
            },
            GameId::LANDLORD,
            settings,
        )
        .expect("swap common");

    assert!(swap.messages.iter().any(|item| matches!(
        &item.payload,
        OutboundPayload::Response(RequestResponse::WithoutData(resp))
            if resp.route == Routes::SWAP as i32
                && resp.code as i32 == WsResponseCode::NO_PERMISSION as i32
    )));
    assert_eq!(service.session_position(1), Some(0));
    assert_eq!(service.session_position(2), Some(1));
}

#[test]
fn swap_rejects_unsupported_official_game() {
    let mut service = RoomService::default();
    service.connect(1);
    service.connect(2);

    for (session_id, name) in [(1_u64, "u1"), (2, "u2")] {
        let _ = service.handle_common_request(
            session_id,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({
                    "name": name,
                    "password": "p1",
                    "game_id": GameId::TEXAS_HOLD_EM as i32,
                    "session_id": format!("official-{session_id}")
                }),
            },
            GameId::TEXAS_HOLD_EM,
            settings,
        );
    }

    let swap = service
        .handle_common_request(
            1,
            &WsRequest {
                route: Routes::SWAP as i32,
                data: serde_json::json!({ "a": 0, "b": 1 }),
            },
            GameId::TEXAS_HOLD_EM,
            settings,
        )
        .expect("swap common");

    assert!(swap.messages.iter().any(|item| matches!(
        &item.payload,
        OutboundPayload::Response(RequestResponse::WithoutData(resp))
            if resp.route == Routes::SWAP as i32
                && resp.code as i32 == WsResponseCode::NO_PERMISSION as i32
    )));
}

#[test]
fn swap_rejects_state_that_disallows_swap() {
    let mut service = RoomService::default();
    service.connect(1);
    service.connect(2);

    for (session_id, name) in [(1_u64, "u1"), (2, "u2")] {
        let _ = service.handle_common_request(
            session_id,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({
                    "name": name,
                    "password": "p1",
                    "game_id": GameId::LANDLORD as i32,
                    "session_id": format!("official-{session_id}")
                }),
            },
            GameId::LANDLORD,
            settings,
        );
    }

    let room_key = service.room_key_of(1).expect("room key");
    let common = service.room_common_state(&room_key).expect("common state");
    service.set_room_game_state(&room_key, Box::new(NoAcceptState { common }));

    let swap = service
        .handle_common_request(
            1,
            &WsRequest {
                route: Routes::SWAP as i32,
                data: serde_json::json!({ "a": 0, "b": 1 }),
            },
            GameId::LANDLORD,
            settings,
        )
        .expect("swap common");

    let rejected = swap.messages.iter().any(|item| match &item.payload {
        OutboundPayload::Response(RequestResponse::WithoutData(resp)) => {
            resp.route == Routes::SWAP as i32
                && resp.code as i32 == WsResponseCode::NO_PERMISSION as i32
        }
        _ => false,
    });
    assert!(rejected);
    assert_eq!(service.session_position(1), Some(0));
    assert_eq!(service.session_position(2), Some(1));
}

impl GameState for NoAcceptState {
    fn can_accept_players(&self) -> bool {
        false
    }

    fn shared_common_state(&self) -> Arc<Mutex<CommonGameState>> {
        Arc::clone(&self.common)
    }
}
