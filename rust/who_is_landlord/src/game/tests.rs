use super::*;
use serde_json::json;
fn request(
    handler: &mut WhoIsLandlordHandler,
    service: &mut RoomService,
    session: u64,
    route: i32,
    data: serde_json::Value,
) -> Dispatch {
    let request = share_type_public::WsRequest { route, data };
    if let Some(mut dispatch) =
        service.handle_common_request(session, &request, GameId::WHO_IS_LANDLORD, || {
            (GameSettings::new(3, 3), HashMap::new())
        })
    {
        handler.after_common_request(service, session, &request, &mut dispatch);
        dispatch
    } else {
        handler.handle_game_request(service, session, request)
    }
}
fn response(dispatch: &Dispatch, route: i32, code: WsResponseCode) -> bool {
    dispatch.messages.iter().any(|m| match &m.payload {
        OutboundPayload::Response(RequestResponse::WithData(r)) => {
            r.route == route && r.code as i32 == code as i32
        }
        OutboundPayload::Response(RequestResponse::WithoutData(r)) => {
            r.route == route && r.code as i32 == code as i32
        }
        _ => false,
    })
}
fn join(
    handler: &mut WhoIsLandlordHandler,
    service: &mut RoomService,
    session: u64,
    name: &str,
) -> Dispatch {
    request(
        handler,
        service,
        session,
        Routes::JOIN as i32,
        json!({"name":name,"password":"test-room","game_id":12}),
    )
}
#[test]
fn room_requires_three_players_owner_start_and_does_not_offer_ai_seats() {
    let mut handler = WhoIsLandlordHandler::default();
    let mut service = RoomService::default();
    assert!(!handler.supports_ai_players());
    assert!(response(
        &join(&mut handler, &mut service, 1, "owner"),
        Routes::JOIN as i32,
        WsResponseCode::JOINED
    ));
    assert!(response(
        &request(
            &mut handler,
            &mut service,
            1,
            Routes::START as i32,
            json!({})
        ),
        Routes::START as i32,
        WsResponseCode::NOT_IN_RANGE
    ));
    for i in 2..=3 {
        join(&mut handler, &mut service, i, &format!("p{i}"));
    }
    assert!(response(
        &request(
            &mut handler,
            &mut service,
            2,
            Routes::START as i32,
            json!({})
        ),
        Routes::START as i32,
        WsResponseCode::NO_PERMISSION
    ));
    let start = request(
        &mut handler,
        &mut service,
        1,
        Routes::START as i32,
        json!({}),
    );
    assert!(response(&start, Routes::START as i32, WsResponseCode::OK));
    let snapshots = start
        .messages
        .iter()
        .filter_map(|m| match &m.payload {
            OutboundPayload::Event(e) if e.code == WhoIsLandlordWsCode::STATE as i32 => Some((
                m.recipient,
                serde_json::from_value::<WhoIsLandlordStateEvent>(e.data.clone()).unwrap(),
            )),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(snapshots.len(), 3);
    for (recipient, state) in snapshots {
        assert_eq!(state.self_position as u64, recipient - 1);
    }
    assert!(response(
        &request(
            &mut handler,
            &mut service,
            1,
            Routes::START as i32,
            json!({})
        ),
        Routes::START as i32,
        WsResponseCode::NO_PERMISSION
    ));
}
#[test]
fn private_snapshot_survives_reconnect_and_new_hand_uses_new_lifecycle() {
    let mut handler = WhoIsLandlordHandler::default();
    let mut service = RoomService::default();
    for i in 1..=3 {
        join(&mut handler, &mut service, i, &format!("p{i}"));
    }
    request(
        &mut handler,
        &mut service,
        1,
        Routes::START as i32,
        json!({}),
    );
    let old_base = service.room_common_state("test-room").unwrap();
    let old_round = handler.states.lock().unwrap()["test-room"].clone();
    service.disconnect(2);
    let rejoin = join(&mut handler, &mut service, 99, "p2");
    assert!(response(
        &rejoin,
        Routes::JOIN as i32,
        WsResponseCode::JOINED
    ));
    assert!(rejoin.messages.iter().any(|m| m.recipient == 99 && matches!(&m.payload,OutboundPayload::Event(e) if e.code == 6001 && e.data["self_position"] == 1)));
    old_round.lock().unwrap().core.phase = WhoIsLandlordPhase::SETTLEMENT;
    let next = request(
        &mut handler,
        &mut service,
        1,
        Routes::START as i32,
        json!({}),
    );
    assert!(response(&next, Routes::START as i32, WsResponseCode::OK));
    assert!(!Arc::ptr_eq(
        &old_base,
        &service.room_common_state("test-room").unwrap()
    ));
    assert_eq!(
        handler.states.lock().unwrap()["test-room"]
            .lock()
            .unwrap()
            .core
            .phase,
        WhoIsLandlordPhase::BURY
    );
}
#[test]
fn quit_aborts_hand_and_a_replacement_can_join_and_restart() {
    let mut handler = WhoIsLandlordHandler::default();
    let mut service = RoomService::default();
    for i in 1..=3 {
        join(&mut handler, &mut service, i, &format!("p{i}"));
    }
    request(
        &mut handler,
        &mut service,
        1,
        Routes::START as i32,
        json!({}),
    );
    request(
        &mut handler,
        &mut service,
        2,
        Routes::QUIT as i32,
        json!({}),
    );
    assert!(response(
        &join(&mut handler, &mut service, 4, "replacement"),
        Routes::JOIN as i32,
        WsResponseCode::JOINED
    ));
    assert!(response(
        &request(
            &mut handler,
            &mut service,
            1,
            Routes::START as i32,
            json!({})
        ),
        Routes::START as i32,
        WsResponseCode::OK
    ));
}
