use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::json;
use share_type_public::settings::GameParamEnum;
use share_type_public::{
    GameId, GameParam, GameParamRange, Routes, WsCode, WsRequest, WsResponseCode,
};

use crate::{
    CommonGameState, Dispatch, GameSettings, OutboundPayload, RequestResponse, RoomService,
    SettingsBuilderResult,
};

struct LockedGameState {
    common: Arc<Mutex<CommonGameState>>,
}

impl crate::GameState for LockedGameState {
    fn can_accept_players(&self) -> bool {
        false
    }

    fn can_join_players(&self) -> bool {
        false
    }

    fn position_reserved_for_join(&self, _position: usize) -> bool {
        true
    }

    fn shared_common_state(&self) -> Arc<Mutex<CommonGameState>> {
        Arc::clone(&self.common)
    }
}

fn settings() -> SettingsBuilderResult {
    (GameSettings::new(1, 4), HashMap::new())
}

fn settings_with_options() -> SettingsBuilderResult {
    (
        GameSettings {
            min_players: 1,
            max_players: 4,
            values: HashMap::from([
                ("rounds".to_owned(), 2),
                ("mode".to_owned(), 0),
                ("settlement_time".to_owned(), 9),
            ]),
        },
        HashMap::from([
            (
                "rounds".to_owned(),
                GameParam::Range(GameParamRange {
                    default: 2,
                    min: 1,
                    max: 4,
                }),
            ),
            (
                "mode".to_owned(),
                GameParam::Enum(GameParamEnum {
                    default: 0,
                    options: vec!["normal".to_owned(), "fast".to_owned()],
                }),
            ),
            (
                "settlement_time".to_owned(),
                GameParam::Range(GameParamRange {
                    default: 9,
                    min: 1,
                    max: 30,
                }),
            ),
        ]),
    )
}

fn join_official_owner(service: &mut RoomService) {
    service
        .handle_common_request(
            1,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: json!({
                    "name": "owner",
                    "password": "query-room",
                    "game_id": GameId::LANDLORD as i32,
                    "session_id": "official-owner-session",
                    "avatar_url": "https://example.com/owner.png",
                }),
            },
            GameId::LANDLORD,
            settings,
        )
        .expect("JOIN is a common request");
}

fn join_member(
    service: &mut RoomService,
    session_id: u64,
    name: &str,
    room_key: &str,
    official_session_id: &str,
) {
    let dispatch = service
        .handle_common_request(
            session_id,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: json!({
                    "name": name,
                    "password": room_key,
                    "game_id": GameId::LANDLORD as i32,
                    "session_id": official_session_id,
                    "avatar_url": format!("https://example.com/{name}.png"),
                }),
            },
            GameId::LANDLORD,
            settings_with_options,
        )
        .expect("JOIN is a common request");
    assert!(has_response(
        &dispatch,
        Routes::JOIN,
        WsResponseCode::JOINED
    ));
}

fn common_request(
    service: &mut RoomService,
    session_id: u64,
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
            GameId::LANDLORD,
            settings,
        )
        .expect("common room route")
}

fn has_response(
    dispatch: &Dispatch,
    route: Routes,
    code: share_type_public::WsResponseCode,
) -> bool {
    dispatch
        .messages
        .iter()
        .any(|delivery| match &delivery.payload {
            OutboundPayload::Response(RequestResponse::WithoutData(response)) => {
                response.route == route as i32 && response.code as i32 == code as i32
            }
            OutboundPayload::Response(RequestResponse::WithData(response)) => {
                response.route == route as i32 && response.code as i32 == code as i32
            }
            OutboundPayload::Event(_) => false,
        })
}

fn has_event(dispatch: &Dispatch, code: share_type_public::WsCode) -> bool {
    dispatch.messages.iter().any(|delivery| {
        matches!(&delivery.payload, OutboundPayload::Event(event) if event.code == code as i32)
    })
}

#[test]
fn room_settings_chat_ai_and_owner_swap_keep_the_common_contract() {
    let mut service = RoomService::with_ai_players_enabled(true);
    join_member(&mut service, 1, "owner", "contract-room", "owner-session");
    join_member(&mut service, 2, "guest", "contract-room", "guest-session");
    assert!(service.room_supports_official_swap("contract-room"));

    let message = common_request(
        &mut service,
        1,
        Routes::MESSAGE,
        json!({"message": "hello room"}),
    );
    assert!(has_response(&message, Routes::MESSAGE, WsResponseCode::OK));
    assert!(has_event(&message, WsCode::MESSAGE));
    assert!(
        message
            .messages
            .iter()
            .any(|delivery| delivery.recipient == 2)
    );

    let settings = common_request(
        &mut service,
        1,
        Routes::SETTING,
        json!({"current_configs": {"rounds": 4, "mode": 1, "settlement_time": 7}}),
    );
    assert!(has_response(&settings, Routes::SETTING, WsResponseCode::OK));
    assert!(has_event(&settings, WsCode::SETTING));
    assert_eq!(
        service.room_configs("contract-room"),
        Some(HashMap::from([
            ("rounds".to_owned(), 4),
            ("mode".to_owned(), 1),
            ("settlement_time".to_owned(), 7),
        ]))
    );
    assert!(has_response(
        &common_request(
            &mut service,
            1,
            Routes::SETTING,
            json!({"current_configs": {"rounds": 5}}),
        ),
        Routes::SETTING,
        WsResponseCode::ERROR_FORMAT,
    ));
    assert!(has_response(
        &common_request(
            &mut service,
            1,
            Routes::SETTING,
            json!({"current_configs": {"unknown": 1}}),
        ),
        Routes::SETTING,
        WsResponseCode::ERROR_FORMAT,
    ));

    let add_ai = common_request(&mut service, 1, Routes::ADD_AI, json!({"count": 8}));
    assert!(has_response(&add_ai, Routes::ADD_AI, WsResponseCode::OK));
    assert_eq!(service.room_members("contract-room").len(), 4);
    let remove_ai = common_request(&mut service, 1, Routes::REMOVE_AI, json!({"position": 2}));
    assert!(has_response(
        &remove_ai,
        Routes::REMOVE_AI,
        WsResponseCode::OK
    ));
    assert!(has_event(&remove_ai, WsCode::QUIT));

    let swap = common_request(&mut service, 1, Routes::SWAP, json!({"a": 0, "b": 1}));
    assert!(has_response(&swap, Routes::SWAP, WsResponseCode::OK));
    assert!(has_event(&swap, WsCode::SWAP));
    assert_eq!(service.session_position(1), Some(1));
    assert_eq!(service.session_position(2), Some(0));

    let disband = common_request(&mut service, 2, Routes::DISBAND, serde_json::Value::Null);
    assert!(has_response(&disband, Routes::DISBAND, WsResponseCode::OK));
    assert!(!service.room_exists("contract-room"));
    assert_eq!(service.room_key_of(1), None);
}

#[test]
fn common_router_handles_unknown_routes_and_missing_room_state() {
    let mut service = RoomService::default();
    assert!(
        service
            .handle_common_request(
                99,
                &WsRequest {
                    route: -1,
                    data: serde_json::Value::Null,
                },
                GameId::LANDLORD,
                settings,
            )
            .is_none()
    );
    assert_eq!(service.session_name(99), "");
    assert_eq!(service.session_position(99), None);
    assert_eq!(service.room_key_of(99), None);
    assert!(!service.is_room_paused("missing"));
    assert!(
        service
            .reset_room_common_state_for_new_game("missing")
            .is_none()
    );
    service.clear_room_game_state("missing");
    service.clear_room_game_state_if_same(
        "missing",
        &std::sync::Arc::new(std::sync::Mutex::new(crate::CommonGameState::new())),
    );
}

#[test]
fn room_queries_broadcasts_and_official_metadata_stay_in_sync() {
    let mut service = RoomService::default();
    join_official_owner(&mut service);

    let entry = service.rooms.get("query-room").expect("room entry exists");
    let debug = format!("{entry:?}");
    assert!(debug.contains("RoomEntry"));
    assert!(debug.contains("LANDLORD"));

    assert_eq!(service.room_count(), 1);
    assert!(service.room_exists("query-room"));
    assert!(!service.room_exists("missing-room"));
    assert_eq!(service.room_game_id("query-room"), Some(GameId::LANDLORD));
    assert!(service.room_is_ready_to_start("query-room"));
    assert!(!service.room_is_ready_to_start("missing-room"));
    assert_eq!(service.room_key_of(1).as_deref(), Some("query-room"));
    assert_eq!(service.session_name(1), "owner");
    assert_eq!(service.session_position(1), Some(0));
    assert_eq!(service.connected_session_ids("query-room"), vec![1]);
    assert_eq!(
        service.connected_session_ids_for_position("query-room", 0),
        vec![1]
    );
    assert!(
        service
            .connected_session_ids_for_position("query-room", 1)
            .is_empty()
    );
    assert_eq!(service.room_configs("query-room"), Some(HashMap::new()));
    assert_eq!(service.room_members("missing-room"), Vec::new());
    assert_eq!(
        service.room_members("query-room"),
        vec![(
            1,
            "owner".to_owned(),
            0,
            "https://example.com/owner.png".to_owned(),
        )]
    );

    let mut dispatch = Dispatch::default();
    service.broadcast("query-room", 100, json!({"kind": "all"}), &mut dispatch);
    service.broadcast_connected(
        "query-room",
        101,
        json!({"kind": "connected"}),
        &mut dispatch,
    );
    service.broadcast_except(
        "query-room",
        1,
        102,
        json!({"kind": "except-owner"}),
        &mut dispatch,
    );
    assert_eq!(dispatch.messages.len(), 2);
    assert!(
        dispatch
            .messages
            .iter()
            .all(|delivery| delivery.recipient == 1)
    );

    assert!(!service.set_session_active_membership(99, true));
    assert!(service.set_session_active_membership(1, true));
    assert!(service.room_position_has_active_membership("query-room", 0));
    assert!(!service.room_position_has_active_membership("query-room", 1));
    assert!(!service.room_position_is_ai_takeover("query-room", 0));
    assert!(service.room_supports_official_swap("query-room"));
    assert!(!service.room_supports_official_swap("missing-room"));

    service.set_room_official_match("query-room", 77, HashMap::from([(0, 42)]));
    assert_eq!(service.room_official_match_id("query-room"), Some(77));
    assert_eq!(service.room_official_user_id("query-room", 0), Some(42));
    assert_eq!(service.room_official_user_id("query-room", 1), None);
    assert_eq!(
        service.room_official_player_sessions("query-room"),
        vec![crate::OfficialPlayerSession {
            position: 0,
            session_id: "official-owner-session".to_owned(),
        }]
    );

    let before_reset = service
        .room_common_state("query-room")
        .expect("room common state before reset");
    let after_reset = service
        .reset_room_common_state_for_new_game("query-room")
        .expect("reset existing room state");
    assert!(!std::sync::Arc::ptr_eq(&before_reset, &after_reset));
    assert_eq!(after_reset.lock().unwrap().player_name(0), "owner");
    assert_eq!(service.room_official_match_id("query-room"), Some(77));
}

#[test]
fn room_lifecycle_keeps_disconnected_rosters_and_sanitizes_official_state() {
    let mut service = RoomService::with_ai_players_enabled(true);
    service.connect(99);
    assert!(service.disconnect(99).messages.is_empty());

    join_member(&mut service, 1, "owner", "lifecycle-room", "owner-session");
    join_member(&mut service, 2, "guest", "lifecycle-room", "");
    assert!(!service.room_supports_official_swap("lifecycle-room"));
    assert_eq!(
        service.room_official_player_sessions("lifecycle-room"),
        vec![crate::OfficialPlayerSession {
            position: 0,
            session_id: "owner-session".to_owned(),
        }]
    );

    let add_ai = common_request(&mut service, 1, Routes::ADD_AI, json!({"count": 1}));
    assert!(has_response(&add_ai, Routes::ADD_AI, WsResponseCode::OK));
    let ai_session = service
        .room_members("lifecycle-room")
        .into_iter()
        .find(|(_, name, _, _)| name.starts_with("AI "))
        .expect("AI member is added");
    assert!(!service.set_session_active_membership(ai_session.0, true));

    service.set_room_official_match("lifecycle-room", 88, HashMap::from([(0, 101), (1, 202)]));
    let retained_common = service
        .room_common_state("lifecycle-room")
        .expect("room common state");
    let unrelated_common =
        std::sync::Arc::new(std::sync::Mutex::new(crate::CommonGameState::new()));
    service.clear_room_game_state_if_same("lifecycle-room", &unrelated_common);
    assert!(std::sync::Arc::ptr_eq(
        &retained_common,
        &service
            .room_common_state("lifecycle-room")
            .expect("unchanged room common state"),
    ));
    assert_eq!(service.room_official_match_id("lifecycle-room"), Some(88));

    service.clear_room_game_state("lifecycle-room");
    assert_eq!(service.room_members("lifecycle-room").len(), 3);
    assert_eq!(service.room_official_match_id("lifecycle-room"), None);
    assert_eq!(service.room_official_user_id("lifecycle-room", 0), None);

    assert!(service.set_session_active_membership(1, true));
    let disconnect = service.disconnect(1);
    assert!(has_event(&disconnect, WsCode::JOIN));
    assert!(service.room_position_is_ai_takeover("lifecycle-room", 0));
    assert!(
        service
            .room_official_player_sessions("lifecycle-room")
            .is_empty()
    );
    assert!(service.room_exists("lifecycle-room"));

    service.disconnect(2);
    assert!(!service.room_exists("lifecycle-room"));
}

#[test]
fn running_game_locks_room_controls_but_keeps_existing_roster_intact() {
    let mut service = RoomService::with_ai_players_enabled(true);
    join_member(&mut service, 1, "owner", "locked-room", "owner-session");
    let common = service
        .room_common_state("locked-room")
        .expect("room common state before game starts");
    service.set_room_game_state(
        "locked-room",
        Box::new(LockedGameState {
            common: Arc::clone(&common),
        }),
    );

    assert!(has_response(
        &common_request(
            &mut service,
            1,
            Routes::SETTING,
            json!({"current_configs": {"rounds": 3}}),
        ),
        Routes::SETTING,
        WsResponseCode::ERROR_FORMAT,
    ));
    assert!(has_response(
        &common_request(&mut service, 1, Routes::ADD_AI, json!({"count": 1})),
        Routes::ADD_AI,
        WsResponseCode::NO_PERMISSION,
    ));
    assert!(has_response(
        &common_request(&mut service, 1, Routes::REMOVE_AI, json!({"position": 1})),
        Routes::REMOVE_AI,
        WsResponseCode::NO_PERMISSION,
    ));
    assert!(has_response(
        &common_request(&mut service, 1, Routes::SWAP, json!({"a": 0, "b": 1})),
        Routes::SWAP,
        WsResponseCode::NO_PERMISSION,
    ));

    let join = service
        .handle_common_request(
            2,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: json!({
                    "name": "late guest",
                    "password": "locked-room",
                    "game_id": GameId::LANDLORD as i32,
                }),
            },
            GameId::LANDLORD,
            settings_with_options,
        )
        .expect("JOIN stays a common request");
    assert!(has_response(
        &join,
        Routes::JOIN,
        WsResponseCode::NO_PERMISSION,
    ));
    assert_eq!(service.room_members("locked-room").len(), 1);
}

#[test]
fn common_room_operations_reject_invalid_state_without_mutating_the_room() {
    let mut unjoined = RoomService::with_ai_players_enabled(true);
    for route in [
        Routes::QUIT,
        Routes::DISBAND,
        Routes::SETTING,
        Routes::MESSAGE,
        Routes::PAUSE,
        Routes::RESUME,
        Routes::AWAY,
        Routes::BACK,
        Routes::ADD_AI,
        Routes::REMOVE_AI,
        Routes::SWAP,
    ] {
        let dispatch = common_request(&mut unjoined, 99, route, serde_json::Value::Null);
        assert!(has_response(
            &dispatch,
            route,
            share_type_public::WsResponseCode::NOT_LOGIN
        ));
    }

    let mut service = RoomService::with_ai_players_enabled(true);
    join_official_owner(&mut service);
    assert!(has_response(
        &common_request(&mut service, 1, Routes::ADD_AI, serde_json::Value::Null),
        Routes::ADD_AI,
        share_type_public::WsResponseCode::ERROR_FORMAT,
    ));
    assert!(has_response(
        &common_request(&mut service, 1, Routes::ADD_AI, json!({"count": 0})),
        Routes::ADD_AI,
        share_type_public::WsResponseCode::OK,
    ));
    assert!(has_response(
        &common_request(&mut service, 1, Routes::REMOVE_AI, serde_json::Value::Null),
        Routes::REMOVE_AI,
        share_type_public::WsResponseCode::ERROR_FORMAT,
    ));
    assert!(has_response(
        &common_request(&mut service, 1, Routes::REMOVE_AI, json!({"position": -1})),
        Routes::REMOVE_AI,
        share_type_public::WsResponseCode::NOT_IN_RANGE,
    ));
    assert!(has_response(
        &common_request(&mut service, 1, Routes::REMOVE_AI, json!({"position": 3})),
        Routes::REMOVE_AI,
        share_type_public::WsResponseCode::NO_PERMISSION,
    ));

    assert!(has_response(
        &common_request(&mut service, 1, Routes::BACK, serde_json::Value::Null),
        Routes::BACK,
        share_type_public::WsResponseCode::NO_PERMISSION,
    ));
    assert!(service.set_session_active_membership(1, true));
    assert!(has_event(
        &common_request(&mut service, 1, Routes::AWAY, serde_json::Value::Null),
        share_type_public::WsCode::AWAY,
    ));
    assert!(service.room_position_is_ai_takeover("query-room", 0));
    assert!(has_response(
        &common_request(&mut service, 1, Routes::AWAY, serde_json::Value::Null),
        Routes::AWAY,
        share_type_public::WsResponseCode::NO_PERMISSION,
    ));
    assert!(has_event(
        &common_request(&mut service, 1, Routes::BACK, serde_json::Value::Null),
        share_type_public::WsCode::BACK,
    ));
    assert!(!service.room_position_is_ai_takeover("query-room", 0));

    assert!(has_response(
        &common_request(&mut service, 1, Routes::MESSAGE, serde_json::Value::Null),
        Routes::MESSAGE,
        share_type_public::WsResponseCode::ERROR_FORMAT,
    ));
    assert!(has_response(
        &common_request(&mut service, 1, Routes::PAUSE, serde_json::Value::Null),
        Routes::PAUSE,
        share_type_public::WsResponseCode::OK,
    ));
    assert!(has_response(
        &common_request(&mut service, 1, Routes::PAUSE, serde_json::Value::Null),
        Routes::PAUSE,
        share_type_public::WsResponseCode::NO_PERMISSION,
    ));
    assert!(has_response(
        &common_request(&mut service, 1, Routes::RESUME, serde_json::Value::Null),
        Routes::RESUME,
        share_type_public::WsResponseCode::OK,
    ));
    assert!(has_response(
        &common_request(&mut service, 1, Routes::RESUME, serde_json::Value::Null),
        Routes::RESUME,
        share_type_public::WsResponseCode::NO_PERMISSION,
    ));
    assert!(has_response(
        &common_request(&mut service, 1, Routes::SETTING, serde_json::Value::Null),
        Routes::SETTING,
        share_type_public::WsResponseCode::ERROR_FORMAT,
    ));
    assert!(has_response(
        &common_request(&mut service, 1, Routes::SWAP, json!({"a": 0, "b": 0})),
        Routes::SWAP,
        share_type_public::WsResponseCode::ERROR_FORMAT,
    ));
}
