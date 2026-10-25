use super::*;
use serde_json::json;
fn request(
    handler: &mut NiangniangHandler,
    service: &mut RoomService,
    session: u64,
    route: i32,
    data: serde_json::Value,
) -> Dispatch {
    let request = share_type_public::WsRequest { route, data };
    if let Some(mut dispatch) =
        service.handle_common_request(session, &request, GameId::NIANGNIANG, || {
            (GameSettings::new(4, 4), HashMap::new())
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
    handler: &mut NiangniangHandler,
    service: &mut RoomService,
    session: u64,
    name: &str,
) -> Dispatch {
    request(
        handler,
        service,
        session,
        Routes::JOIN as i32,
        json!({"name":name,"password":"test-room","game_id":13}),
    )
}
#[test]
fn room_requires_four_players_owner_start_and_official_only_ai_seats() {
    let mut handler = NiangniangHandler::default();
    let mut service = RoomService::default();
    assert_eq!(handler.supports_ai_players(), cfg!(feature = "official"));
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
    for i in 2..=4 {
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
            OutboundPayload::Event(e) if e.code == NiangniangWsCode::STATE as i32 => Some((
                m.recipient,
                serde_json::from_value::<NiangniangStateEvent>(e.data.clone()).unwrap(),
            )),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(snapshots.len(), 4);
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
    let mut handler = NiangniangHandler::default();
    let mut service = RoomService::default();
    for i in 1..=4 {
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
    assert!(rejoin.messages.iter().any(|m| m.recipient == 99 && matches!(&m.payload,OutboundPayload::Event(e) if e.code == 7001 && e.data["self_position"] == 1)));
    old_round.lock().unwrap().core.phase = NiangniangPhase::MATCH_OVER;
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
        NiangniangPhase::PLAY
    );
}
#[test]
fn quit_aborts_hand_and_a_replacement_can_join_and_restart() {
    let mut handler = NiangniangHandler::default();
    let mut service = RoomService::default();
    for i in 1..=4 {
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
        &join(&mut handler, &mut service, 5, "replacement"),
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

#[cfg(feature = "official")]
#[test]
fn official_host_can_start_with_three_ai_seats() {
    let mut handler = NiangniangHandler::default();
    let mut service = RoomService::with_ai_players_enabled(handler.supports_ai_players());
    join(&mut handler, &mut service, 1, "host");
    let added = request(
        &mut handler,
        &mut service,
        1,
        Routes::ADD_AI as i32,
        json!({ "count": 3 }),
    );
    assert!(response(&added, Routes::ADD_AI as i32, WsResponseCode::OK));
    let start = request(
        &mut handler,
        &mut service,
        1,
        Routes::START as i32,
        json!({}),
    );
    assert!(response(&start, Routes::START as i32, WsResponseCode::OK));
    let common = service.room_common_state("test-room").unwrap();
    let common = common.lock().unwrap();
    assert!(common.is_ai_position(1) && common.is_ai_position(2));
    let round = handler.states.lock().unwrap()["test-room"].clone();
    let round = round.lock().unwrap();
    for viewer in 0..4 {
        let state = round.core.state_for(viewer);
        assert_eq!(state.players.len(), 4);
        assert!(!state.hand.is_empty());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn automatic_loop_plays_offline_seats_and_stops_with_the_room() {
    let mut handler = NiangniangHandler::default();
    let mut service = RoomService::default();
    for session in 1..=4 {
        join(
            &mut handler,
            &mut service,
            session,
            &format!("player-{session}"),
        );
    }
    let started = request(
        &mut handler,
        &mut service,
        1,
        Routes::START as i32,
        json!({}),
    );
    assert!(response(&started, Routes::START as i32, WsResponseCode::OK));
    let common = service.room_common_state("test-room").unwrap();
    let round = handler.states.lock().unwrap()["test-room"].clone();
    {
        let mut base = common.lock().unwrap();
        for seat in 0..4 {
            base.mark_away(seat);
            if cfg!(feature = "official") {
                base.mark_ai_takeover_position(seat);
            }
        }
    }
    let service = Arc::new(AsyncMutex::new(service));
    let senders = Arc::new(AsyncMutex::new(HashMap::new()));
    start_loop(
        "test-room".into(),
        Arc::clone(&common),
        Arc::clone(&round),
        Arc::clone(&service),
        senders,
        Arc::clone(&handler.states),
    );
    let before = round.lock().unwrap().core.state_for(0).hand.len();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if round.lock().unwrap().core.state_for(0).hand.len() < before {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("automatic loop must play a card without blocking the service");
    drop(
        tokio::time::timeout(Duration::from_secs(1), service.lock())
            .await
            .expect("service lock remains available"),
    );
    common.lock().unwrap().stop_requested = true;
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if !handler.states.lock().unwrap().contains_key("test-room") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("old automatic loop stops");
}

#[test]
fn completed_match_does_not_reveal_previous_hand_to_new_roster() {
    let mut handler = NiangniangHandler::default();
    let mut service = RoomService::default();
    for session in 1..=4 {
        join(&mut handler, &mut service, session, &format!("p{session}"));
    }
    request(
        &mut handler,
        &mut service,
        1,
        Routes::START as i32,
        json!({}),
    );
    let round = handler.states.lock().unwrap()["test-room"].clone();
    {
        let mut round = round.lock().unwrap();
        round.core.phase = NiangniangPhase::MATCH_OVER;
        settle(&mut service, "test-room", &mut round);
    }
    service.disconnect(2);
    let dispatch = join(&mut handler, &mut service, 99, "replacement");
    assert!(response(
        &dispatch,
        Routes::JOIN as i32,
        WsResponseCode::JOINED
    ));
    assert!(!dispatch.messages.iter().any(|m| m.recipient == 99 && matches!(&m.payload, OutboundPayload::Event(e) if e.code == NiangniangWsCode::STATE as i32)));
    assert!(dispatch.messages.iter().any(
        |m| matches!(&m.payload, OutboundPayload::Event(e) if e.code == WsCode::GAME_OVER as i32)
    ));
    assert!(!handler.states.lock().unwrap().contains_key("test-room"));
}
