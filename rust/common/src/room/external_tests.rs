use std::collections::HashMap;

use serde_json::json;
use share_type_public::{GameId, Routes, WsRequest};

use crate::{
    Dispatch, GameSettings, OutboundPayload, RequestResponse, RoomService, SettingsBuilderResult,
};

fn settings() -> SettingsBuilderResult {
    (GameSettings::new(1, 4), HashMap::new())
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

fn has_response(dispatch: &Dispatch, route: Routes, code: share_type_public::WsResponseCode) -> bool {
    dispatch.messages.iter().any(|delivery| match &delivery.payload {
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
fn room_queries_broadcasts_and_official_metadata_stay_in_sync() {
    let mut service = RoomService::default();
    join_official_owner(&mut service);

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
    assert!(service.connected_session_ids_for_position("query-room", 1).is_empty());
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
    assert!(dispatch.messages.iter().all(|delivery| delivery.recipient == 1));

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
        assert!(has_response(&dispatch, route, share_type_public::WsResponseCode::NOT_LOGIN));
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
