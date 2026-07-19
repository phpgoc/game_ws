use super::*;

#[test]
fn ting_candidates_are_human_only_and_declaration_is_recorded() {
    let mut state = playable_state();
    state.hands.insert(0, seven_pairs_ting_hand());
    state.last_drawn_tile = Some(32);
    assert_eq!(
        ting_discard_tiles_for_position(&state, 0, &default_configs()),
        vec![31, 32]
    );

    state.base.lock().unwrap().mark_ai_position(0);
    assert!(ting_discard_tiles_for_position(&state, 0, &default_configs()).is_empty());

    let (room_service, _handler, room_key, loop_state) = setup_request_room();
    let mut human_state = loop_state.lock().unwrap();
    human_state.phase = ShenyangMahjongPhase::Play;
    human_state.current_position = 0;
    human_state.dealer_position = 0;
    human_state.hands.insert(0, seven_pairs_ting_hand());
    human_state.last_drawn_tile = Some(32);
    let mut dispatch = Dispatch::default();
    assert!(perform_discard_with_ting(
        &room_service,
        &room_key,
        &mut human_state,
        &default_configs(),
        &mut dispatch,
        0,
        32,
        true,
    ));
    assert!(human_state.is_ting(0));
    let declared_event = dispatch.messages.iter().find_map(|message| {
        let OutboundPayload::Event(event) = &message.payload else {
            return None;
        };
        (event.code == WsCode::PLAY as i32)
            .then(|| serde_json::from_value::<WsShenyangMahjongPlayEvent>(event.data.clone()).ok())
            .flatten()
            .filter(|event| event.action == ShenyangMahjongAction::DISCARD)
    });
    assert_eq!(declared_event.and_then(|event| event.is_ting), Some(true));
}

#[test]
fn ting_candidates_follow_human_control_across_ai_takeover() {
    let mut state = playable_state();
    state.hands.insert(0, seven_pairs_ting_hand());
    state.last_drawn_tile = Some(32);

    assert_eq!(
        ting_discard_tiles_for_position(&state, 0, &default_configs()),
        vec![31, 32]
    );

    {
        let mut base = state.base.lock().unwrap();
        base.mark_away(0);
        base.mark_ai_takeover_position(0);
    }
    assert!(!state.is_ai_position(0));
    assert!(state.is_ai_controlled_position(0));
    assert!(ting_discard_tiles_for_position(&state, 0, &default_configs()).is_empty());

    state.base.lock().unwrap().clear_away();
    assert!(!state.is_ai_controlled_position(0));
    assert_eq!(
        ting_discard_tiles_for_position(&state, 0, &default_configs()),
        vec![31, 32]
    );
}

#[test]
fn ting_candidates_do_not_reveal_wait_tiles_in_opponent_hands() {
    let mut state = playable_state();
    state
        .hands
        .insert(0, vec![1, 1, 2, 2, 3, 3, 5, 11, 11, 12, 12, 21, 21, 31]);
    state.hands.insert(1, vec![31]);
    state.hands.insert(2, vec![31]);
    state.hands.insert(3, vec![31]);
    state.last_drawn_tile = Some(5);

    assert_eq!(state.known_tile_count(31), 4);
    assert!(ting_shape_wait_tiles_after_discard(&state, 0, 5, &default_configs()).contains(&31));
    assert!(ting_discard_tiles_for_position(&state, 0, &default_configs()).contains(&5));
}

#[test]
fn declared_ting_locks_future_discard_to_the_drawn_tile() {
    let mut state = playable_state();
    let mut hand = seven_pairs_ting_hand();
    hand.retain(|tile| *tile != 32);
    hand.push(5);
    hand.sort_unstable();
    state.hands.insert(0, hand);
    state.last_drawn_tile = Some(5);
    state.declare_ting(0);
    let mut dispatch = Dispatch::default();

    assert!(!perform_discard(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
        0,
        1,
    ));
    assert!(perform_discard(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
        0,
        5,
    ));
}

#[test]
fn declared_ting_can_discard_drawn_tile_after_wait_becomes_dead() {
    let mut state = playable_state();
    state
        .hands
        .insert(0, vec![1, 1, 2, 2, 3, 3, 5, 11, 11, 12, 12, 21, 21, 31]);
    state.discards.insert(1, vec![31]);
    state.discards.insert(2, vec![31]);
    state.discards.insert(3, vec![31]);
    state.wall = vec![6];
    state.last_drawn_tile = Some(5);
    state.declare_ting(0);
    let mut dispatch = Dispatch::default();

    assert!(ting_wait_tiles_after_discard(&state, 0, 5, &default_configs()).is_empty());
    assert!(perform_discard(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
        0,
        5,
    ));
    assert_eq!(state.discards.get(&0).unwrap().last(), Some(&5));
}

#[test]
fn declared_ting_claim_options_allow_only_hu() {
    let mut state = playable_state();
    state.current_position = 0;
    state.discards.insert(0, vec![5]);
    state
        .hands
        .insert(1, vec![4, 5, 5, 5, 6, 11, 12, 13, 35, 35]);
    state.melds.insert(1, vec![open_chi_meld(21)]);
    state.wall = vec![31];

    let before_ting = build_claim_options(&state, 5, 0, &default_configs());
    let before_option = before_ting
        .iter()
        .find(|option| option.position == 1)
        .expect("position 1 claim option before ting");
    assert!(before_option.can_hu);
    assert!(before_option.can_peng);
    assert!(before_option.can_gang);
    assert_eq!(before_option.chi_options, vec![vec![4, 6]]);

    state.declare_ting(1);

    let after_ting = build_claim_options(&state, 5, 0, &default_configs());
    let after_option = after_ting
        .iter()
        .find(|option| option.position == 1)
        .expect("ting position should retain its hu option");
    assert!(after_option.can_hu);
    assert!(!after_option.can_peng);
    assert!(!after_option.can_gang);
    assert!(after_option.chi_options.is_empty());
}

#[test]
fn declared_ting_blocks_self_gang_and_xi_gang() {
    let mut state = playable_state();
    state.current_position = 1;
    state
        .hands
        .insert(1, vec![3, 3, 3, 3, 4, 5, 6, 11, 12, 13, 31, 32, 33, 34]);
    state.wall = vec![35];
    state.last_drawn_tile = Some(3);
    state.xi_gang_options.insert(1, vec![vec![31, 32, 33, 34]]);

    assert!(can_self_gang(&state, 1, 3));
    assert!(can_declare_xi_gang(&state, 1, &[31, 32, 33, 34]));

    state.declare_ting(1);

    assert!(!can_self_gang(&state, 1, 3));
    assert!(!can_declare_xi_gang(&state, 1, &[31, 32, 33, 34]));
}

#[test]
fn enabled_ting_setting_adds_one_fan_before_the_cap() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![2, 3, 5, 6, 7, 11, 12, 13, 31, 31]);
    state.melds.insert(1, vec![open_peng_meld(21, 0)]);
    state.declare_ting(1);
    state.enter_settlement_with_reverse_win(vec![1], Some(0), Some(4), false, false, false, false);
    let settlement = state.settlement.as_ref().expect("settlement");
    let disabled = HashMap::from([("ting_fan".to_owned(), 0)]);
    let enabled = HashMap::from([("ting_fan".to_owned(), 1)]);
    let disabled_fan = winner_hand_fan_with_configs(&state, settlement, 1, &disabled);

    assert!(disabled_fan > 0);
    assert_eq!(
        winner_hand_fan_with_configs(&state, settlement, 1, &enabled),
        disabled_fan + 1,
    );
}

#[test]
fn enabled_ting_fan_does_not_restore_an_invalid_winner() {
    let mut state = playable_state();
    state.hands.insert(1, vec![1, 2, 3]);
    state.declare_ting(1);
    state.enter_settlement(vec![1], Some(0), Some(4), false);
    let settlement = state.settlement.as_ref().expect("settlement");
    let configs = HashMap::from([("ting_fan".to_owned(), 1)]);

    assert_eq!(
        winner_hand_fan_with_configs(&state, settlement, 1, &configs),
        0
    );
    let event = build_settlement_event_with_configs(&state, &configs)
        .expect("invalid winner settlement event");
    assert!(event.winner_positions.is_empty());
    assert!(event.winner_details.is_empty());
    assert!(event.score_changes.iter().all(|change| change.score == 0));
}

#[test]
fn pregame_quit_does_not_poison_the_next_start() {
    let mut room_service = RoomService::default();
    let mut handler = ShenyangMahjongGameHandler::default();
    for session_id in 1..=4 {
        let _ = room_service.handle_common_request(
            session_id,
            &ClientRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({
                    "name": format!("P{session_id}"),
                    "password": "pregame-quit",
                    "game_id": GameId::SHENYANG_MAHJONG as i32
                }),
            },
            GameId::SHENYANG_MAHJONG,
            build_shenyang_mahjong_settings,
        );
    }
    let quit_request = ClientRequest {
        route: Routes::QUIT as i32,
        data: Value::Null,
    };
    let mut quit_dispatch = room_service
        .handle_common_request(
            2,
            &quit_request,
            GameId::SHENYANG_MAHJONG,
            build_shenyang_mahjong_settings,
        )
        .expect("common quit route");
    handler.after_common_request(&mut room_service, 2, &quit_request, &mut quit_dispatch);
    assert!(
        room_service
            .room_common_state("pregame-quit")
            .expect("stopped pregame state")
            .lock()
            .unwrap()
            .stop_requested()
    );
    let _ = room_service.handle_common_request(
        5,
        &ClientRequest {
            route: Routes::JOIN as i32,
            data: serde_json::json!({
                "name": "P5",
                "password": "pregame-quit",
                "game_id": GameId::SHENYANG_MAHJONG as i32
            }),
        },
        GameId::SHENYANG_MAHJONG,
        build_shenyang_mahjong_settings,
    );

    let started = handler.handle_start(&mut room_service, 1);

    assert_eq!(
        response_code(&started, 1, Routes::START),
        Some(WsResponseCode::OK as i32)
    );
    let state = handler
        .loop_state("pregame-quit")
        .expect("started loop state");
    assert!(!state.lock().unwrap().stop_requested());
}

#[test]
fn pruning_stopped_loop_state_restores_room_acceptance() {
    let mut room_service = RoomService::default();
    for session_id in 1..=3 {
        room_service.connect(session_id);
    }
    for (session_id, name) in [(1_u64, "P1"), (2, "P2")] {
        let _ = room_service.handle_common_request(
            session_id,
            &ClientRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({
                    "name": name,
                    "password": "room",
                    "game_id": GameId::SHENYANG_MAHJONG as i32
                }),
            },
            GameId::SHENYANG_MAHJONG,
            build_shenyang_mahjong_settings,
        );
    }
    let room_key = room_service.room_key_of(1).expect("room key");
    let common = room_service
        .room_common_state(&room_key)
        .expect("common state");
    let loop_state = Arc::new(StdMutex::new(ShenyangMahjongLoopState::new(Arc::clone(
        &common,
    ))));
    room_service.set_room_game_state(
        &room_key,
        Box::new(ShenyangMahjongGameState::from_loop_state(Arc::clone(
            &loop_state,
        ))),
    );
    let handler = ShenyangMahjongGameHandler::default();
    handler
        .loop_states
        .lock()
        .unwrap()
        .insert(room_key.clone(), Arc::clone(&loop_state));
    loop_state.lock().unwrap().request_stop();

    handler.prune_stopped_loop_states(&mut room_service);
    let join_after_prune = room_service
        .handle_common_request(
            3,
            &ClientRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({
                    "name": "P3",
                    "password": "room",
                    "game_id": GameId::SHENYANG_MAHJONG as i32
                }),
            },
            GameId::SHENYANG_MAHJONG,
            build_shenyang_mahjong_settings,
        )
        .expect("join common");
    let joined = join_after_prune
        .messages
        .iter()
        .any(|item| match &item.payload {
            OutboundPayload::Response(RequestResponse::WithData(response)) => {
                response.code as i32 == WsResponseCode::JOINED as i32
            }
            _ => false,
        });

    assert!(joined);
    assert_eq!(room_service.session_position(3), Some(2));
}

#[test]
fn redeal_uses_only_positive_score_winners_for_dealer_rotation() {
    let mut state = playable_state();
    state.dealer_position = 0;
    state
        .hands
        .insert(0, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 35]);
    state.melds.insert(
        0,
        vec![build_meld(
            ShenyangMahjongMeldKind::PENG,
            vec![99, 99, 99],
            Some(2),
        )],
    );
    state.hands.insert(1, vec![1, 1, 35, 35]);
    state.melds.insert(
        1,
        vec![
            build_meld(ShenyangMahjongMeldKind::PENG, vec![11, 11, 11], Some(0)),
            build_meld(ShenyangMahjongMeldKind::PENG, vec![21, 21, 21], Some(2)),
            build_meld(ShenyangMahjongMeldKind::PENG, vec![31, 31, 31], Some(3)),
        ],
    );
    state.enter_settlement_with_reverse_win(
        vec![0, 1],
        Some(2),
        Some(1),
        false,
        false,
        false,
        false,
    );
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(winner_hand_fan(&state, settlement, 0), 0);
    assert!(winner_hand_fan(&state, settlement, 1) > 0);
    assert_eq!(
        positive_winner_positions_for_state(&state, settlement, &HashMap::new()),
        vec![1]
    );

    redeal_after_settlement_with_configs(&mut state, &HashMap::new());

    assert_eq!(state.dealer_position, 1);
    assert_eq!(state.current_position, 1);
    assert!(state.settlement.is_none());
}
