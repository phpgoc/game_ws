use super::*;

#[test]
fn perform_discard_rejects_during_claim_window() {
    let mut state = playable_state();
    state.current_position = 0;
    state.discards.insert(0, Vec::new());
    state
        .hands
        .insert(0, vec![3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31, 32, 33]);
    state.discards.insert(1, vec![35]);
    state.claim_window = Some(ClaimWindowState {
        tile: 35,
        from_position: 1,
        kind: ClaimWindowKind::Discard,
        eligible_positions: vec![0],
        responses: HashMap::new(),
    });
    let original_hand = state.hands.get(&0).cloned().unwrap();
    let mut dispatch = Dispatch::default();

    assert!(!perform_discard(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
        0,
        3,
    ));

    assert_eq!(state.hands.get(&0), Some(&original_hand));
    assert!(state.discards.get(&0).unwrap().is_empty());
    assert!(state.claim_window.is_some());
    assert!(dispatch.messages.is_empty());
}

#[test]
fn perform_discard_rejects_invalid_owned_tile() {
    let mut state = playable_state();
    state.current_position = 0;
    state.discards.insert(0, Vec::new());
    state
        .hands
        .insert(0, vec![4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31, 32, 33, 99]);
    state.wall = vec![36];
    state.last_drawn_tile = Some(99);
    let original_hand = state.hands.get(&0).cloned().unwrap();
    let mut dispatch = Dispatch::default();

    assert!(!perform_discard(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
        0,
        99,
    ));

    assert_eq!(state.hands.get(&0), Some(&original_hand));
    assert!(state.discards.get(&0).unwrap().is_empty());
    assert_eq!(state.wall, vec![36]);
    assert!(dispatch.messages.is_empty());
}

#[test]
fn perform_discard_rejects_malformed_owned_meld() {
    let mut state = playable_state();
    state.current_position = 0;
    state.discards.insert(0, Vec::new());
    state
        .hands
        .insert(0, vec![4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31, 32, 33, 34]);
    state.melds.insert(
        0,
        vec![build_meld(
            ShenyangMahjongMeldKind::PENG,
            vec![3, 3],
            Some(1),
        )],
    );
    state.wall = vec![36];
    state.last_drawn_tile = Some(4);
    let original_hand = state.hands.get(&0).cloned().unwrap();
    let mut dispatch = Dispatch::default();

    assert!(!perform_discard(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
        0,
        4,
    ));

    assert_eq!(state.hands.get(&0), Some(&original_hand));
    assert!(state.discards.get(&0).unwrap().is_empty());
    assert_eq!(state.wall, vec![36]);
    assert!(dispatch.messages.is_empty());
}

#[test]
fn perform_discard_rejects_outside_play_phase() {
    let mut state = playable_state();
    state.phase = ShenyangMahjongPhase::Settlement;
    state.discards.insert(0, Vec::new());
    state
        .hands
        .insert(0, vec![3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31, 32, 33]);
    state.wall = vec![36];
    let original_hand = state.hands.get(&0).cloned().unwrap();
    let mut dispatch = Dispatch::default();

    assert!(!perform_discard(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
        0,
        3,
    ));

    assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
    assert_eq!(state.hands.get(&0), Some(&original_hand));
    assert!(state.discards.get(&0).unwrap().is_empty());
    assert_eq!(state.wall, vec![36]);
    assert!(dispatch.messages.is_empty());
}

#[test]
fn perform_discard_rejects_public_fifth_copy() {
    let mut state = playable_state();
    state.current_position = 0;
    state.discards.insert(0, Vec::new());
    state.discards.insert(1, vec![3]);
    state
        .hands
        .insert(0, vec![3, 3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
    state.wall = vec![36];
    state.last_drawn_tile = Some(3);
    let original_hand = state.hands.get(&0).cloned().unwrap();
    let mut dispatch = Dispatch::default();

    assert_eq!(known_tile_count(&state, 3), 5);
    assert!(!perform_discard(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
        0,
        3,
    ));

    assert_eq!(state.hands.get(&0), Some(&original_hand));
    assert!(state.discards.get(&0).unwrap().is_empty());
    assert_eq!(state.discards.get(&1), Some(&vec![3]));
    assert_eq!(state.wall, vec![36]);
    assert!(dispatch.messages.is_empty());
}

#[test]
fn perform_discard_rejects_self_sourced_open_meld() {
    let mut state = playable_state();
    state.current_position = 0;
    state.discards.insert(0, Vec::new());
    state
        .hands
        .insert(0, vec![4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31]);
    state.melds.insert(
        0,
        vec![build_meld(
            ShenyangMahjongMeldKind::PENG,
            vec![3, 3, 3],
            Some(0),
        )],
    );
    state.wall = vec![36];
    state.last_drawn_tile = Some(4);
    let original_hand = state.hands.get(&0).cloned().unwrap();
    let mut dispatch = Dispatch::default();

    assert!(!perform_discard(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
        0,
        4,
    ));

    assert_eq!(state.hands.get(&0), Some(&original_hand));
    assert!(state.discards.get(&0).unwrap().is_empty());
    assert_eq!(state.wall, vec![36]);
    assert!(dispatch.messages.is_empty());
}

#[test]
fn perform_discard_rejects_valid_target_with_invalid_hand_tile() {
    let mut state = playable_state();
    state.current_position = 0;
    state.discards.insert(0, Vec::new());
    state
        .hands
        .insert(0, vec![4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31, 32, 33, 99]);
    state.wall = vec![36];
    state.last_drawn_tile = Some(4);
    let original_hand = state.hands.get(&0).cloned().unwrap();
    let mut dispatch = Dispatch::default();

    assert!(!perform_discard(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
        0,
        4,
    ));

    assert_eq!(state.hands.get(&0), Some(&original_hand));
    assert!(state.discards.get(&0).unwrap().is_empty());
    assert_eq!(state.wall, vec![36]);
    assert!(dispatch.messages.is_empty());
}

#[test]
fn perform_discard_requires_current_position() {
    let mut state = playable_state();
    state.current_position = 1;
    state.discards.insert(0, Vec::new());
    state
        .hands
        .insert(0, vec![3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31, 32, 33]);
    let original_hand = state.hands.get(&0).cloned().unwrap();
    let mut dispatch = Dispatch::default();

    assert!(!perform_discard(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
        0,
        3,
    ));

    assert_eq!(state.hands.get(&0), Some(&original_hand));
    assert!(state.discards.get(&0).unwrap().is_empty());
    assert!(dispatch.messages.is_empty());
}

#[test]
fn perform_discard_requires_fourteen_virtual_tiles() {
    let mut state = playable_state();
    state.current_position = 0;
    state.discards.insert(0, Vec::new());
    state
        .hands
        .insert(0, vec![3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 32, 33]);
    state.wall = vec![36];
    let original_hand = state.hands.get(&0).cloned().unwrap();
    let mut dispatch = Dispatch::default();

    assert!(!perform_discard(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
        0,
        3,
    ));

    assert_eq!(state.hands.get(&0), Some(&original_hand));
    assert!(state.discards.get(&0).unwrap().is_empty());
    assert_eq!(state.wall, vec![36]);
    assert!(dispatch.messages.is_empty());
}

#[test]
fn perform_self_draw_hu_rejects_during_claim_window() {
    let mut state = playable_state();
    state.current_position = 0;
    state
        .hands
        .insert(0, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
    state.last_drawn_tile = Some(35);
    state.claim_window = Some(ClaimWindowState {
        tile: 35,
        from_position: 0,
        kind: ClaimWindowKind::RobGang,
        eligible_positions: vec![1],
        responses: HashMap::new(),
    });
    let mut dispatch = Dispatch::default();

    assert!(!can_self_draw_hu_with_configs(
        &state,
        0,
        &default_configs()
    ));
    perform_self_draw_hu(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
        0,
    );

    assert!(state.settlement.is_none());
    assert!(state.claim_window.is_some());
    assert!(dispatch.messages.is_empty());
}

#[test]
fn perform_self_draw_hu_requires_current_position() {
    let mut state = playable_state();
    state.current_position = 1;
    state
        .hands
        .insert(0, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
    state.last_drawn_tile = Some(35);
    let mut dispatch = Dispatch::default();

    perform_self_draw_hu(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
        0,
    );

    assert!(state.settlement.is_none());
    assert!(dispatch.messages.is_empty());
}

#[test]
fn perform_self_draw_hu_requires_legal_win() {
    let mut state = playable_state();
    state.current_position = 0;
    state
        .hands
        .insert(0, vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
    state.last_drawn_tile = Some(35);
    let mut dispatch = Dispatch::default();

    assert!(!can_self_draw_hu_with_configs(
        &state,
        0,
        &default_configs()
    ));
    perform_self_draw_hu(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
        0,
    );

    assert!(state.settlement.is_none());
    assert!(dispatch.messages.is_empty());
}

#[test]
fn perform_self_draw_hu_requires_shenyang_rules() {
    let mut state = playable_state();
    state.current_position = 0;
    state
        .hands
        .insert(0, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
    state.last_drawn_tile = Some(35);
    let configs = default_configs();
    let mut dispatch = Dispatch::default();

    assert!(!can_self_draw_hu_with_configs(&state, 0, &configs));
    assert!(!can_self_draw_hu_with_configs(&state, 0, &configs));
    perform_self_draw_hu(
        &RoomService::default(),
        "room",
        &mut state,
        &configs,
        &mut dispatch,
        0,
    );

    assert!(state.settlement.is_none());
    assert!(dispatch.messages.is_empty());
}

#[test]
fn perform_self_gang_rejects_during_claim_window() {
    let mut state = playable_state();
    state.current_position = 0;
    state
        .hands
        .insert(0, vec![3, 3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
    state.wall = vec![35];
    state.last_drawn_tile = Some(3);
    state.claim_window = Some(ClaimWindowState {
        tile: 3,
        from_position: 0,
        kind: ClaimWindowKind::RobGang,
        eligible_positions: vec![1],
        responses: HashMap::new(),
    });
    let original_hand = state.hands.get(&0).cloned().unwrap();
    let mut dispatch = Dispatch::default();

    assert!(!can_self_gang(&state, 0, 3));
    assert!(!perform_self_gang(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
        0,
        3,
    ));

    assert_eq!(state.hands.get(&0), Some(&original_hand));
    assert!(state.melds.get(&0).is_none_or(Vec::is_empty));
    assert!(state.claim_window.is_some());
    assert!(dispatch.messages.is_empty());
}

#[test]
fn perform_self_gang_requires_current_position() {
    let mut state = playable_state();
    state.current_position = 1;
    state
        .hands
        .insert(0, vec![3, 3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
    state.wall = vec![35];
    state.last_drawn_tile = Some(3);
    let original_hand = state.hands.get(&0).cloned().unwrap();
    let mut dispatch = Dispatch::default();

    assert!(!perform_self_gang(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
        0,
        3,
    ));
    assert_eq!(state.hands.get(&0), Some(&original_hand));
    assert!(state.melds.get(&0).is_none_or(Vec::is_empty));
    assert!(dispatch.messages.is_empty());
}

#[test]
fn play_request_allows_multiple_hu_by_default() {
    let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
        }
        state.discards.insert(0, vec![3]);
        state
            .hands
            .insert(1, vec![1, 2, 11, 12, 13, 21, 22, 23, 35, 35]);
        state
            .hands
            .insert(2, vec![1, 2, 14, 15, 16, 24, 25, 26, 35, 35]);
        state.melds.insert(1, vec![open_peng_meld(31, 3)]);
        state.melds.insert(2, vec![open_peng_meld(32, 3)]);
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1, 2],
            responses: HashMap::new(),
        });
    }

    let first_hu = handler.handle_game_request(
        &mut room_service,
        2,
        play_request(ShenyangMahjongAction::HU, Vec::new(), Some(3), Some(0)),
    );
    let second_hu = handler.handle_game_request(
        &mut room_service,
        3,
        play_request(ShenyangMahjongAction::HU, Vec::new(), Some(3), Some(0)),
    );

    assert_eq!(
        response_code(&first_hu, 2, Routes::PLAY),
        Some(WsResponseCode::OK as i32)
    );
    assert!(!has_room_event(&first_hu, WsCode::GAME_OVER));
    assert_eq!(
        response_code(&second_hu, 3, Routes::PLAY),
        Some(WsResponseCode::OK as i32)
    );
    assert!(has_room_event(&second_hu, WsCode::GAME_OVER));
    let state = loop_state.lock().unwrap();
    let settlement = state.settlement.as_ref().expect("settlement");
    assert_eq!(settlement.winner_positions, vec![1, 2]);
    assert_eq!(settlement.from_position, Some(0));
    assert_eq!(settlement.win_tile, Some(3));
    assert_eq!(state.discards.get(&0), Some(&Vec::<i32>::new()));
}

#[test]
fn play_request_blocks_only_first_chi_when_configured() {
    let (mut room_service, mut handler, _room_key, loop_state) =
        setup_request_room_with_configs(serde_json::json!({"allow_first_chi":0}));
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
        }
        state.discards.insert(0, vec![3]);
        state
            .hands
            .insert(1, vec![1, 2, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1],
            responses: HashMap::new(),
        });
    }

    let response = handler.handle_game_request(
        &mut room_service,
        2,
        play_request(ShenyangMahjongAction::CHI, vec![1, 2], Some(3), Some(0)),
    );

    assert_eq!(
        response_code(&response, 2, Routes::PLAY),
        Some(WsResponseCode::NO_PERMISSION as i32)
    );
    let state = loop_state.lock().unwrap();
    assert!(state.claim_window.is_some());
    assert_eq!(state.discards.get(&0), Some(&vec![3]));
    assert!(state.melds.get(&1).unwrap().is_empty());
    drop(state);

    {
        let mut state = loop_state.lock().unwrap();
        state
            .hands
            .insert(1, vec![1, 2, 4, 5, 6, 11, 12, 13, 31, 35]);
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![21, 21, 21],
                Some(2),
            )],
        );
    }

    let response = handler.handle_game_request(
        &mut room_service,
        2,
        play_request(ShenyangMahjongAction::CHI, vec![1, 2], Some(3), Some(0)),
    );

    assert_eq!(
        response_code(&response, 2, Routes::PLAY),
        Some(WsResponseCode::OK as i32)
    );
    let state = loop_state.lock().unwrap();
    assert!(state.claim_window.is_none());
    assert_eq!(state.melds.get(&1).unwrap().len(), 2);
    assert_eq!(
        state.melds.get(&1).unwrap()[1].kind,
        ShenyangMahjongMeldKind::CHI
    );
}

#[test]
fn play_request_chi_allows_shenyang_rule() {
    let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
        }
        state.discards.insert(0, vec![3]);
        state
            .hands
            .insert(1, vec![1, 2, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1],
            responses: HashMap::new(),
        });
    }

    let response = handler.handle_game_request(
        &mut room_service,
        2,
        play_request(ShenyangMahjongAction::CHI, vec![1, 2], Some(3), Some(0)),
    );

    assert_eq!(
        response_code(&response, 2, Routes::PLAY),
        Some(WsResponseCode::OK as i32)
    );
    assert!(!has_room_event(&response, WsCode::GAME_OVER));
    let state = loop_state.lock().unwrap();
    assert_eq!(state.phase, ShenyangMahjongPhase::Play);
    assert!(state.claim_window.is_none());
    assert_eq!(state.current_position, 1);
    assert!(state.discards.get(&0).unwrap().is_empty());
    let meld = state.melds.get(&1).unwrap().first().unwrap();
    assert_eq!(meld.kind, ShenyangMahjongMeldKind::CHI);
    assert_eq!(meld.tiles, vec![1, 2, 3]);
}

#[test]
fn play_request_chi_consumes_tiles_and_keeps_turn_with_chi_player() {
    let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
        }
        state.discards.insert(0, vec![3]);
        state
            .hands
            .insert(1, vec![1, 2, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
        state.wall = vec![36];
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1],
            responses: HashMap::new(),
        });
    }

    let response = handler.handle_game_request(
        &mut room_service,
        2,
        ClientRequest {
            route: Routes::PLAY as i32,
            data: serde_json::json!({
                "action": ShenyangMahjongAction::CHI as i32,
                "tiles": [1, 2],
                "target_tile": 3,
                "from_position": 0,
            }),
        },
    );

    assert_eq!(
        response_code(&response, 2, Routes::PLAY),
        Some(WsResponseCode::OK as i32)
    );
    assert!(!has_room_event(&response, WsCode::GAME_OVER));
    let state = loop_state.lock().unwrap();
    assert_eq!(state.phase, ShenyangMahjongPhase::Play);
    assert!(state.claim_window.is_none());
    assert_eq!(state.current_position, 1);
    assert_eq!(state.last_drawn_tile, None);
    assert_eq!(state.wall, vec![36]);
    assert!(state.discards.get(&0).unwrap().is_empty());
    assert!(!state.hands.get(&1).unwrap().contains(&1));
    assert!(!state.hands.get(&1).unwrap().contains(&2));
    let meld = state.melds.get(&1).unwrap().first().unwrap();
    assert_eq!(meld.kind, ShenyangMahjongMeldKind::CHI);
    assert_eq!(meld.tiles, vec![1, 2, 3]);
    assert_eq!(meld.from_position, Some(0));
}

#[test]
fn play_request_chi_rejects_invalid_sequence() {
    let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
        }
        state.discards.insert(0, vec![3]);
        state
            .hands
            .insert(1, vec![1, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35, 36]);
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1],
            responses: HashMap::new(),
        });
    }

    let response = handler.handle_game_request(
        &mut room_service,
        2,
        ClientRequest {
            route: Routes::PLAY as i32,
            data: serde_json::json!({
                "action": ShenyangMahjongAction::CHI as i32,
                "tiles": [1, 4],
                "target_tile": 3,
                "from_position": 0,
            }),
        },
    );

    assert_eq!(
        response_code(&response, 2, Routes::PLAY),
        Some(WsResponseCode::NO_PERMISSION as i32)
    );
    let state = loop_state.lock().unwrap();
    assert!(state.claim_window.is_some());
    assert_eq!(state.discards.get(&0), Some(&vec![3]));
    assert!(state.melds.get(&1).unwrap().is_empty());
}

#[test]
fn play_request_chi_rejects_non_next_player() {
    let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
        }
        state.discards.insert(0, vec![3]);
        state
            .hands
            .insert(2, vec![1, 2, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![2],
            responses: HashMap::new(),
        });
    }

    let response = handler.handle_game_request(
        &mut room_service,
        3,
        ClientRequest {
            route: Routes::PLAY as i32,
            data: serde_json::json!({
                "action": ShenyangMahjongAction::CHI as i32,
                "tiles": [1, 2],
                "target_tile": 3,
                "from_position": 0,
            }),
        },
    );

    assert_eq!(
        response_code(&response, 3, Routes::PLAY),
        Some(WsResponseCode::NO_PERMISSION as i32)
    );
    let state = loop_state.lock().unwrap();
    assert!(state.claim_window.is_some());
    assert_eq!(state.discards.get(&0), Some(&vec![3]));
    assert!(state.melds.get(&2).unwrap().is_empty());
}

#[test]
fn play_request_declares_only_frozen_first_draw_xi_gang_option() {
    let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 1;
        state
            .hands
            .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 35, 36, 37]);
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
        }
        state.last_drawn_tile = Some(37);
        state.wall = vec![34];
        state.xi_gang_options.insert(1, vec![vec![35, 36, 37]]);
    }

    let response = handler.handle_game_request(
        &mut room_service,
        2,
        play_request(ShenyangMahjongAction::XI_GANG, vec![37, 35, 36], None, None),
    );
    assert_eq!(
        response_code(&response, 2, Routes::PLAY),
        Some(WsResponseCode::OK as i32)
    );
    {
        let state = loop_state.lock().unwrap();
        assert_eq!(state.melds.get(&1).unwrap().len(), 1);
        assert!(state.xi_gang_options_for_position(1).is_empty());
    }

    let duplicate = handler.handle_game_request(
        &mut room_service,
        2,
        play_request(ShenyangMahjongAction::XI_GANG, vec![35, 36, 37], None, None),
    );
    assert_eq!(
        response_code(&duplicate, 2, Routes::PLAY),
        Some(WsResponseCode::NO_PERMISSION as i32)
    );
}

#[test]
fn play_request_discard_opens_claim_window_when_claimable() {
    let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
        }
        state
            .hands
            .insert(0, vec![3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31, 32, 33]);
        state
            .hands
            .insert(1, vec![1, 2, 7, 9, 14, 16, 18, 24, 26, 28, 34, 35, 36]);
        state
            .hands
            .insert(2, vec![3, 3, 4, 6, 8, 11, 13, 15, 17, 21, 23, 25, 27]);
        state
            .hands
            .insert(3, vec![5, 7, 9, 12, 14, 16, 18, 22, 24, 26, 28, 32, 37]);
        state.wall = vec![36];
        state.last_drawn_tile = Some(3);
    }

    let response = handler.handle_game_request(
        &mut room_service,
        1,
        play_request(ShenyangMahjongAction::DISCARD, Vec::new(), Some(3), None),
    );

    assert_eq!(
        response_code(&response, 1, Routes::PLAY),
        Some(WsResponseCode::OK as i32)
    );
    assert!(has_room_event(&response, WsCode::CLAIM_WINDOW));
    for recipient in 1..=4 {
        let common_event = response
            .messages
            .iter()
            .find_map(|message| match &message.payload {
                OutboundPayload::Event(event)
                    if message.recipient == recipient
                        && event.code == WsCode::CLAIM_WINDOW as i32 =>
                {
                    Some(event)
                }
                _ => None,
            })
            .expect("each player should receive the public claim window");
        let event: WsShenyangMahjongClaimWindowEvent =
            serde_json::from_value(common_event.data.clone()).expect("claim window payload");
        let viewer_position = recipient as i32 - 1;
        assert_eq!(event.tile, 3);
        assert_eq!(event.from_position, 0);
        if matches!(viewer_position, 1 | 2) {
            assert_eq!(event.eligible_positions, vec![viewer_position]);
            assert_eq!(event.options.len(), 1);
            assert_eq!(event.options[0].position, viewer_position);
        } else {
            assert!(event.eligible_positions.is_empty());
            assert!(event.options.is_empty());
        }
    }
    let state = loop_state.lock().unwrap();
    let claim_window = state.claim_window.as_ref().expect("claim window");
    assert!(matches!(claim_window.kind, ClaimWindowKind::Discard));
    assert_eq!(claim_window.tile, 3);
    assert_eq!(claim_window.from_position, 0);
    assert_eq!(claim_window.eligible_positions, vec![1, 2]);
    assert_eq!(state.current_position, 0);
    assert_eq!(state.last_drawn_tile, None);
    assert_eq!(state.wall, vec![36]);
    assert_eq!(state.discards.get(&0), Some(&vec![3]));
}

#[test]
fn play_request_discard_rejects_invalid_owned_tile() {
    let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
        }
        state
            .hands
            .insert(0, vec![4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31, 32, 33, 99]);
        state.wall = vec![36];
        state.last_drawn_tile = Some(99);
    }

    let response = handler.handle_game_request(
        &mut room_service,
        1,
        play_request(ShenyangMahjongAction::DISCARD, Vec::new(), Some(99), None),
    );

    assert_eq!(
        response_code(&response, 1, Routes::PLAY),
        Some(WsResponseCode::NO_PERMISSION as i32)
    );
    let state = loop_state.lock().unwrap();
    assert!(state.discards.get(&0).unwrap().is_empty());
    assert!(state.hands.get(&0).unwrap().contains(&99));
    assert_eq!(state.wall, vec![36]);
}

#[test]
fn play_request_discard_rejects_public_fifth_copy() {
    let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
        }
        state.discards.insert(1, vec![3]);
        state
            .hands
            .insert(0, vec![3, 3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
        state.wall = vec![36];
        state.last_drawn_tile = Some(3);
    }

    let response = handler.handle_game_request(
        &mut room_service,
        1,
        play_request(ShenyangMahjongAction::DISCARD, Vec::new(), Some(3), None),
    );

    assert_eq!(
        response_code(&response, 1, Routes::PLAY),
        Some(WsResponseCode::NO_PERMISSION as i32)
    );
    let state = loop_state.lock().unwrap();
    assert_eq!(known_tile_count(&state, 3), 5);
    assert_eq!(state.discards.get(&1), Some(&vec![3]));
    assert!(state.discards.get(&0).unwrap().is_empty());
    assert_eq!(
        state
            .hands
            .get(&0)
            .unwrap()
            .iter()
            .filter(|&&tile| tile == 3)
            .count(),
        4
    );
    assert_eq!(state.wall, vec![36]);
}

#[test]
fn play_request_discard_rejects_tile_not_in_hand() {
    let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
        }
        state
            .hands
            .insert(0, vec![1, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31, 32, 33]);
        state.wall = vec![36];
        state.last_drawn_tile = Some(1);
    }

    let response = handler.handle_game_request(
        &mut room_service,
        1,
        play_request(ShenyangMahjongAction::DISCARD, Vec::new(), Some(9), None),
    );

    assert_eq!(
        response_code(&response, 1, Routes::PLAY),
        Some(WsResponseCode::NO_PERMISSION as i32)
    );
    let state = loop_state.lock().unwrap();
    assert!(state.claim_window.is_none());
    assert_eq!(state.current_position, 0);
    assert_eq!(state.wall, vec![36]);
    assert!(state.discards.get(&0).unwrap().is_empty());
    assert_eq!(state.hands.get(&0).unwrap().len(), 14);
}

#[test]
fn play_request_discard_without_claim_draws_next_player() {
    let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
        }
        state
            .hands
            .insert(0, vec![1, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31, 32, 33]);
        state
            .hands
            .insert(1, vec![2, 4, 7, 9, 14, 16, 18, 24, 26, 28, 34, 35, 37]);
        state
            .hands
            .insert(2, vec![3, 5, 8, 11, 13, 15, 17, 21, 23, 25, 27, 32, 36]);
        state
            .hands
            .insert(3, vec![6, 7, 9, 12, 14, 16, 18, 22, 24, 26, 28, 33, 34]);
        state.wall = vec![36];
        state.last_drawn_tile = Some(1);
    }

    let response = handler.handle_game_request(
        &mut room_service,
        1,
        play_request(ShenyangMahjongAction::DISCARD, Vec::new(), Some(1), None),
    );

    assert_eq!(
        response_code(&response, 1, Routes::PLAY),
        Some(WsResponseCode::OK as i32)
    );
    assert!(!has_room_event(&response, WsCode::CLAIM_WINDOW));
    assert!(!has_room_event(&response, WsCode::GAME_OVER));
    let state = loop_state.lock().unwrap();
    assert!(state.claim_window.is_none());
    assert_eq!(state.current_position, 1);
    assert_eq!(state.last_drawn_tile, Some(36));
    assert_eq!(state.wall_count(), 0);
    assert_eq!(state.discards.get(&0), Some(&vec![1]));
    assert!(!state.hands.get(&0).unwrap().contains(&1));
    assert!(state.hands.get(&1).unwrap().contains(&36));
}

#[test]
fn play_request_gang_consumes_triplet_and_draws_replacement() {
    let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
        }
        state.discards.insert(0, vec![3]);
        state
            .hands
            .insert(2, vec![3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
        state.wall = vec![36];
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![2],
            responses: HashMap::new(),
        });
    }

    let response = handler.handle_game_request(
        &mut room_service,
        3,
        ClientRequest {
            route: Routes::PLAY as i32,
            data: serde_json::json!({
                "action": ShenyangMahjongAction::GANG as i32,
                "tiles": [],
                "target_tile": 3,
                "from_position": 0,
            }),
        },
    );

    assert_eq!(
        response_code(&response, 3, Routes::PLAY),
        Some(WsResponseCode::OK as i32)
    );
    assert!(!has_room_event(&response, WsCode::GAME_OVER));
    let state = loop_state.lock().unwrap();
    assert_eq!(state.phase, ShenyangMahjongPhase::Play);
    assert!(state.claim_window.is_none());
    assert_eq!(state.current_position, 2);
    assert_eq!(state.last_drawn_tile, Some(36));
    assert_eq!(state.wall_count(), 0);
    assert!(state.discards.get(&0).unwrap().is_empty());
    assert_eq!(
        state
            .hands
            .get(&2)
            .unwrap()
            .iter()
            .filter(|&&tile| tile == 3)
            .count(),
        0,
    );
    assert!(state.hands.get(&2).unwrap().contains(&36));
    let meld = state.melds.get(&2).unwrap().first().unwrap();
    assert_eq!(meld.kind, ShenyangMahjongMeldKind::GANG);
    assert_eq!(meld.tiles, vec![3, 3, 3, 3]);
    assert_eq!(meld.from_position, Some(0));
}

#[test]
fn play_request_gang_rejects_when_replacement_tile_is_unavailable() {
    let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
        }
        state.discards.insert(0, vec![3]);
        state
            .hands
            .insert(2, vec![3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
        state.wall = Vec::new();
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![2],
            responses: HashMap::new(),
        });
    }

    let response = handler.handle_game_request(
        &mut room_service,
        3,
        play_request(ShenyangMahjongAction::GANG, Vec::new(), Some(3), Some(0)),
    );

    assert_eq!(
        response_code(&response, 3, Routes::PLAY),
        Some(WsResponseCode::NO_PERMISSION as i32)
    );
    assert!(!has_room_event(&response, WsCode::GAME_OVER));
    let state = loop_state.lock().unwrap();
    assert_eq!(state.phase, ShenyangMahjongPhase::Play);
    assert!(state.claim_window.is_some());
    assert_eq!(state.current_position, 0);
    assert_eq!(state.wall_count(), 0);
    assert_eq!(state.discards.get(&0), Some(&vec![3]));
    assert_eq!(
        state
            .hands
            .get(&2)
            .unwrap()
            .iter()
            .filter(|&&tile| tile == 3)
            .count(),
        3,
    );
    assert!(state.melds.get(&2).unwrap().is_empty());
    assert!(state.settlement.is_none());
}

#[test]
fn play_request_gang_rejects_without_triplet() {
    let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
        }
        state.discards.insert(0, vec![3]);
        state
            .hands
            .insert(2, vec![3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![2],
            responses: HashMap::new(),
        });
    }

    let response = handler.handle_game_request(
        &mut room_service,
        3,
        ClientRequest {
            route: Routes::PLAY as i32,
            data: serde_json::json!({
                "action": ShenyangMahjongAction::GANG as i32,
                "tiles": [],
                "target_tile": 3,
                "from_position": 0,
            }),
        },
    );

    assert_eq!(
        response_code(&response, 3, Routes::PLAY),
        Some(WsResponseCode::NO_PERMISSION as i32)
    );
    let state = loop_state.lock().unwrap();
    assert!(state.claim_window.is_some());
    assert_eq!(state.discards.get(&0), Some(&vec![3]));
    assert!(state.melds.get(&2).unwrap().is_empty());
}

#[test]
fn play_request_legacy_nearest_config_still_allows_multiple_hu() {
    let (mut room_service, mut handler, _room_key, loop_state) =
        setup_request_room_with_configs(serde_json::json!({"multi_hu_mode":0,"win_rule":0}));
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
        }
        state.discards.insert(0, vec![3]);
        state
            .hands
            .insert(1, vec![1, 2, 11, 12, 13, 21, 22, 23, 35, 35]);
        state
            .hands
            .insert(2, vec![1, 2, 14, 15, 16, 24, 25, 26, 35, 35]);
        state.melds.insert(1, vec![open_peng_meld(31, 3)]);
        state.melds.insert(2, vec![open_peng_meld(32, 3)]);
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1, 2],
            responses: HashMap::new(),
        });
    }

    let first_hu = handler.handle_game_request(
        &mut room_service,
        2,
        play_request(ShenyangMahjongAction::HU, Vec::new(), Some(3), Some(0)),
    );
    let second_hu = handler.handle_game_request(
        &mut room_service,
        3,
        play_request(ShenyangMahjongAction::HU, Vec::new(), Some(3), Some(0)),
    );

    assert_eq!(
        response_code(&first_hu, 2, Routes::PLAY),
        Some(WsResponseCode::OK as i32)
    );
    assert!(!has_room_event(&first_hu, WsCode::GAME_OVER));
    assert_eq!(
        response_code(&second_hu, 3, Routes::PLAY),
        Some(WsResponseCode::OK as i32)
    );
    assert!(has_room_event(&second_hu, WsCode::GAME_OVER));
    let state = loop_state.lock().unwrap();
    let settlement = state.settlement.as_ref().expect("settlement");
    assert_eq!(settlement.winner_positions, vec![1, 2]);
    assert_eq!(settlement.from_position, Some(0));
    assert_eq!(settlement.win_tile, Some(3));
    assert_eq!(state.discards.get(&0), Some(&Vec::<i32>::new()));
}

#[test]
fn play_request_pass_rejects_duplicate_claim_response() {
    let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
        }
        state.discards.insert(0, vec![3]);
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1, 2],
            responses: HashMap::from([(1, ClaimResponse::Pass)]),
        });
    }

    let response = handler.handle_game_request(
        &mut room_service,
        2,
        play_request(ShenyangMahjongAction::PASS, Vec::new(), Some(3), Some(0)),
    );

    assert_eq!(
        response_code(&response, 2, Routes::PLAY),
        Some(WsResponseCode::NO_PERMISSION as i32)
    );
    let state = loop_state.lock().unwrap();
    let claim_window = state.claim_window.as_ref().expect("claim window");
    assert_eq!(claim_window.responses.len(), 1);
    assert_eq!(state.discards.get(&0), Some(&vec![3]));
}

#[test]
fn play_request_pass_resolves_after_all_claims_and_draws_next_player() {
    let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
            state.hands.insert(
                position,
                vec![4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31, 32, 33],
            );
        }
        state.discards.insert(0, vec![3]);
        state.wall = vec![36];
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1, 2],
            responses: HashMap::new(),
        });
    }

    let first_pass = handler.handle_game_request(
        &mut room_service,
        2,
        play_request(ShenyangMahjongAction::PASS, Vec::new(), Some(3), Some(0)),
    );
    let second_pass = handler.handle_game_request(
        &mut room_service,
        3,
        play_request(ShenyangMahjongAction::PASS, Vec::new(), Some(3), Some(0)),
    );

    assert_eq!(
        response_code(&first_pass, 2, Routes::PLAY),
        Some(WsResponseCode::OK as i32)
    );
    assert_eq!(
        response_code(&second_pass, 3, Routes::PLAY),
        Some(WsResponseCode::OK as i32)
    );
    assert!(!has_room_event(&second_pass, WsCode::GAME_OVER));
    let state = loop_state.lock().unwrap();
    assert!(state.claim_window.is_none());
    assert_eq!(state.current_position, 1);
    assert_eq!(state.last_drawn_tile, Some(36));
    assert_eq!(state.wall_count(), 0);
    assert_eq!(state.discards.get(&0), Some(&vec![3]));
    assert!(state.hands.get(&1).unwrap().contains(&36));
    assert!(state.settlement.is_none());
}

#[test]
fn play_request_pass_resolves_to_draw_when_wall_is_empty() {
    let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
            state.hands.insert(
                position,
                vec![4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31, 32, 33],
            );
        }
        state.discards.insert(0, vec![3]);
        state.wall = Vec::new();
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1],
            responses: HashMap::new(),
        });
    }

    let response = handler.handle_game_request(
        &mut room_service,
        2,
        play_request(ShenyangMahjongAction::PASS, Vec::new(), Some(3), Some(0)),
    );

    assert_eq!(
        response_code(&response, 2, Routes::PLAY),
        Some(WsResponseCode::OK as i32)
    );
    assert!(has_room_event(&response, WsCode::GAME_OVER));
    let state = loop_state.lock().unwrap();
    assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
    assert!(state.claim_window.is_none());
    let settlement = state.settlement.as_ref().expect("settlement");
    assert!(settlement.winner_positions.is_empty());
    assert_eq!(settlement.from_position, None);
    assert_eq!(settlement.win_tile, None);
    assert!(!settlement.is_self_draw);
    assert_eq!(state.discards.get(&0), Some(&vec![3]));
}

#[test]
fn play_request_pass_waits_for_remaining_claim_responses() {
    let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
        }
        state.discards.insert(0, vec![3]);
        state.wall = vec![36];
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1, 2],
            responses: HashMap::new(),
        });
    }

    let response = handler.handle_game_request(
        &mut room_service,
        2,
        play_request(ShenyangMahjongAction::PASS, Vec::new(), Some(3), Some(0)),
    );

    assert_eq!(
        response_code(&response, 2, Routes::PLAY),
        Some(WsResponseCode::OK as i32)
    );
    assert!(!has_room_event(&response, WsCode::GAME_OVER));
    let state = loop_state.lock().unwrap();
    let claim_window = state.claim_window.as_ref().expect("claim window");
    assert_eq!(claim_window.responses.len(), 1);
    assert_eq!(state.current_position, 0);
    assert_eq!(state.wall, vec![36]);
    assert_eq!(state.discards.get(&0), Some(&vec![3]));
    assert!(state.settlement.is_none());
}

#[test]
fn play_request_peng_consumes_pair_and_keeps_turn_with_peng_player() {
    let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
        }
        state.discards.insert(0, vec![3]);
        state
            .hands
            .insert(2, vec![3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
        state.wall = vec![36];
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![2],
            responses: HashMap::new(),
        });
    }

    let response = handler.handle_game_request(
        &mut room_service,
        3,
        ClientRequest {
            route: Routes::PLAY as i32,
            data: serde_json::json!({
                "action": ShenyangMahjongAction::PENG as i32,
                "tiles": [],
                "target_tile": 3,
                "from_position": 0,
            }),
        },
    );

    assert_eq!(
        response_code(&response, 3, Routes::PLAY),
        Some(WsResponseCode::OK as i32)
    );
    assert!(!has_room_event(&response, WsCode::GAME_OVER));
    let state = loop_state.lock().unwrap();
    assert_eq!(state.phase, ShenyangMahjongPhase::Play);
    assert!(state.claim_window.is_none());
    assert_eq!(state.current_position, 2);
    assert_eq!(state.last_drawn_tile, None);
    assert_eq!(state.wall, vec![36]);
    assert!(state.discards.get(&0).unwrap().is_empty());
    assert_eq!(
        state
            .hands
            .get(&2)
            .unwrap()
            .iter()
            .filter(|&&tile| tile == 3)
            .count(),
        0,
    );
    let meld = state.melds.get(&2).unwrap().first().unwrap();
    assert_eq!(meld.kind, ShenyangMahjongMeldKind::PENG);
    assert_eq!(meld.tiles, vec![3, 3, 3]);
    assert_eq!(meld.from_position, Some(0));
}

#[test]
fn play_request_peng_rejects_without_pair() {
    let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
        }
        state.discards.insert(0, vec![3]);
        state
            .hands
            .insert(2, vec![3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35, 36]);
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![2],
            responses: HashMap::new(),
        });
    }

    let response = handler.handle_game_request(
        &mut room_service,
        3,
        ClientRequest {
            route: Routes::PLAY as i32,
            data: serde_json::json!({
                "action": ShenyangMahjongAction::PENG as i32,
                "tiles": [],
                "target_tile": 3,
                "from_position": 0,
            }),
        },
    );

    assert_eq!(
        response_code(&response, 3, Routes::PLAY),
        Some(WsResponseCode::NO_PERMISSION as i32)
    );
    let state = loop_state.lock().unwrap();
    assert!(state.claim_window.is_some());
    assert_eq!(state.discards.get(&0), Some(&vec![3]));
    assert!(state.melds.get(&2).unwrap().is_empty());
}

#[test]
fn play_request_peng_requires_thirteen_virtual_tiles() {
    let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
        }
        state.discards.insert(0, vec![3]);
        state.hands.insert(2, vec![3, 3]);
        state.wall = vec![36];
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![2],
            responses: HashMap::new(),
        });
    }

    let response = handler.handle_game_request(
        &mut room_service,
        3,
        play_request(ShenyangMahjongAction::PENG, Vec::new(), Some(3), Some(0)),
    );

    assert_eq!(
        response_code(&response, 3, Routes::PLAY),
        Some(WsResponseCode::NO_PERMISSION as i32)
    );
    let state = loop_state.lock().unwrap();
    let claim_window = state.claim_window.as_ref().expect("claim window");
    assert!(claim_window.responses.is_empty());
    assert_eq!(state.discards.get(&0), Some(&vec![3]));
    assert_eq!(state.hands.get(&2), Some(&vec![3, 3]));
    assert!(state.melds.get(&2).unwrap().is_empty());
}

#[test]
fn play_request_rejects_claim_from_source_position() {
    let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
        }
        state.discards.insert(0, vec![3]);
        state
            .hands
            .insert(0, vec![3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
        state.wall = vec![36];
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![0],
            responses: HashMap::new(),
        });
    }

    let response = handler.handle_game_request(
        &mut room_service,
        1,
        play_request(ShenyangMahjongAction::PENG, Vec::new(), Some(3), Some(0)),
    );

    assert_eq!(
        response_code(&response, 1, Routes::PLAY),
        Some(WsResponseCode::NO_PERMISSION as i32)
    );
    let state = loop_state.lock().unwrap();
    let claim_window = state.claim_window.as_ref().expect("claim window");
    assert!(claim_window.responses.is_empty());
    assert_eq!(state.discards.get(&0), Some(&vec![3]));
    assert!(state.melds.get(&0).unwrap().is_empty());
}

#[test]
fn play_request_rejects_claim_when_source_discard_does_not_match() {
    let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
        }
        state.discards.insert(0, vec![4]);
        state
            .hands
            .insert(2, vec![3, 3, 5, 6, 7, 11, 12, 13, 21, 22, 23, 31, 35]);
        state.wall = vec![36];
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![2],
            responses: HashMap::new(),
        });
    }

    let response = handler.handle_game_request(
        &mut room_service,
        3,
        play_request(ShenyangMahjongAction::PENG, Vec::new(), Some(3), Some(0)),
    );

    assert_eq!(
        response_code(&response, 3, Routes::PLAY),
        Some(WsResponseCode::NO_PERMISSION as i32)
    );
    let state = loop_state.lock().unwrap();
    let claim_window = state.claim_window.as_ref().expect("claim window");
    assert!(claim_window.responses.is_empty());
    assert_eq!(state.discards.get(&0), Some(&vec![4]));
    assert!(state.melds.get(&2).unwrap().is_empty());
}

#[test]
fn play_request_rejects_impossible_fifth_tile_peng_claim() {
    let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
        }
        state.discards.insert(0, vec![3]);
        state
            .hands
            .insert(1, vec![3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
        state
            .hands
            .insert(2, vec![3, 3, 7, 8, 9, 14, 15, 16, 24, 25, 26, 32, 36]);
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1],
            responses: HashMap::new(),
        });
    }

    let rejected_peng = handler.handle_game_request(
        &mut room_service,
        2,
        ClientRequest {
            route: Routes::PLAY as i32,
            data: serde_json::json!({
                "action": ShenyangMahjongAction::PENG as i32,
                "tiles": [],
                "target_tile": 3,
                "from_position": 0,
            }),
        },
    );

    assert_eq!(
        response_code(&rejected_peng, 2, Routes::PLAY),
        Some(WsResponseCode::NO_PERMISSION as i32)
    );
    let state = loop_state.lock().unwrap();
    assert!(state.claim_window.is_some());
    assert!(state.melds.get(&1).unwrap().is_empty());
}

#[test]
fn play_request_rejects_manual_action_while_away() {
    let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        state.base.lock().unwrap().mark_away(0);
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
        }
        state
            .hands
            .insert(0, vec![1, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31, 32, 33]);
        state.wall = vec![36];
        state.last_drawn_tile = Some(1);
    }

    let response = handler.handle_game_request(
        &mut room_service,
        1,
        play_request(ShenyangMahjongAction::DISCARD, Vec::new(), Some(1), None),
    );

    assert_eq!(
        response_code(&response, 1, Routes::PLAY),
        Some(WsResponseCode::NO_PERMISSION as i32)
    );
    let state = loop_state.lock().unwrap();
    assert_eq!(state.current_position, 0);
    assert_eq!(state.discards.get(&0), Some(&Vec::<i32>::new()));
    assert!(state.hands.get(&0).unwrap().contains(&1));
}

#[test]
fn play_request_rejects_manual_claim_response_while_away() {
    let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        state.base.lock().unwrap().mark_away(1);
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
        }
        state.discards.insert(0, vec![3]);
        state
            .hands
            .insert(1, vec![1, 2, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1],
            responses: HashMap::new(),
        });
    }

    let response = handler.handle_game_request(
        &mut room_service,
        2,
        play_request(ShenyangMahjongAction::HU, Vec::new(), Some(3), Some(0)),
    );

    assert_eq!(
        response_code(&response, 2, Routes::PLAY),
        Some(WsResponseCode::NO_PERMISSION as i32)
    );
    let state = loop_state.lock().unwrap();
    let claim_window = state.claim_window.as_ref().expect("claim window");
    assert!(claim_window.responses.is_empty());
    assert!(state.settlement.is_none());
    assert_eq!(state.discards.get(&0), Some(&vec![3]));
}

#[test]
fn play_request_rejects_melds_from_impossible_known_tile_state() {
    for (action, tiles, hand) in [
        (
            ShenyangMahjongAction::PENG,
            Vec::new(),
            vec![3, 3, 4, 5, 6, 9, 9, 9, 9, 11, 12, 13, 21],
        ),
        (
            ShenyangMahjongAction::GANG,
            Vec::new(),
            vec![3, 3, 3, 4, 5, 6, 9, 9, 9, 9, 11, 12, 13],
        ),
        (
            ShenyangMahjongAction::CHI,
            vec![1, 2],
            vec![1, 2, 4, 5, 6, 9, 9, 9, 9, 11, 12, 13, 21],
        ),
    ] {
        let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
        {
            let mut state = loop_state.lock().unwrap();
            state.phase = ShenyangMahjongPhase::Play;
            state.current_position = 0;
            for position in 0..4 {
                state.discards.insert(position, Vec::new());
                state.melds.insert(position, Vec::new());
            }
            state.hands.insert(1, hand.clone());
            state.discards.insert(0, vec![3]);
            state.discards.insert(2, vec![9]);
            state.wall = vec![37];
            state.claim_window = Some(ClaimWindowState {
                tile: 3,
                from_position: 0,
                kind: ClaimWindowKind::Discard,
                eligible_positions: vec![1],
                responses: HashMap::new(),
            });
            assert_eq!(known_tile_count(&state, 9), 5);
            assert!(position_has_impossible_known_tile_count(&state, 1));
        }

        let response = handler.handle_game_request(
            &mut room_service,
            2,
            play_request(action, tiles, Some(3), Some(0)),
        );

        assert_eq!(
            response_code(&response, 2, Routes::PLAY),
            Some(WsResponseCode::NO_PERMISSION as i32)
        );
        let state = loop_state.lock().unwrap();
        assert!(
            state
                .claim_window
                .as_ref()
                .is_some_and(|window| { window.responses.is_empty() })
        );
        assert_eq!(state.hands.get(&1), Some(&hand));
        assert!(state.melds.get(&1).unwrap().is_empty());
        assert_eq!(state.discards.get(&0), Some(&vec![3]));
    }
}

#[test]
fn play_request_rejects_public_fifth_copy_used_by_hu_winner() {
    let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
        }
        state
            .hands
            .insert(1, vec![1, 1, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6]);
        state.discards.insert(0, vec![6]);
        state.discards.insert(2, vec![1]);
        state.claim_window = Some(ClaimWindowState {
            tile: 6,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1],
            responses: HashMap::new(),
        });
    }

    let rejected_hu = handler.handle_game_request(
        &mut room_service,
        2,
        ClientRequest {
            route: Routes::PLAY as i32,
            data: serde_json::json!({
                "action": ShenyangMahjongAction::HU as i32,
                "tiles": [],
                "target_tile": 6,
                "from_position": 0,
            }),
        },
    );

    assert_eq!(
        response_code(&rejected_hu, 2, Routes::PLAY),
        Some(WsResponseCode::NO_PERMISSION as i32)
    );
    let state = loop_state.lock().unwrap();
    assert!(state.settlement.is_none());
    assert!(state.claim_window.is_some());
}

#[test]
fn play_request_rejects_self_hu_without_draw_and_accepts_after_draw() {
    let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
        }
        state
            .hands
            .insert(0, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35]);
    }

    let denied = handler.handle_game_request(
        &mut room_service,
        1,
        ClientRequest {
            route: Routes::PLAY as i32,
            data: serde_json::json!({
                "action": ShenyangMahjongAction::HU as i32,
                "tiles": [],
                "target_tile": null,
                "from_position": null,
            }),
        },
    );

    assert_eq!(
        response_code(&denied, 1, Routes::PLAY),
        Some(WsResponseCode::NO_PERMISSION as i32)
    );
    assert!(loop_state.lock().unwrap().settlement.is_none());

    {
        loop_state.lock().unwrap().last_drawn_tile = Some(35);
    }
    let accepted = handler.handle_game_request(
        &mut room_service,
        1,
        ClientRequest {
            route: Routes::PLAY as i32,
            data: serde_json::json!({
                "action": ShenyangMahjongAction::HU as i32,
                "tiles": [],
                "target_tile": null,
                "from_position": null,
            }),
        },
    );

    assert_eq!(
        response_code(&accepted, 1, Routes::PLAY),
        Some(WsResponseCode::OK as i32)
    );
    assert!(has_room_event(&accepted, WsCode::GAME_OVER));
    let state = loop_state.lock().unwrap();
    assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
    assert_eq!(
        state
            .settlement
            .as_ref()
            .map(|settlement| settlement.winner_positions.clone()),
        Some(vec![0])
    );
}

#[test]
fn play_request_respects_shenyang_win_rule() {
    let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
        }
        state.discards.insert(0, vec![35]);
        state
            .hands
            .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35]);
        state.claim_window = Some(ClaimWindowState {
            tile: 35,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1],
            responses: HashMap::new(),
        });
    }

    let denied = handler.handle_game_request(
        &mut room_service,
        2,
        play_request(ShenyangMahjongAction::HU, Vec::new(), Some(35), Some(0)),
    );

    assert_eq!(
        response_code(&denied, 2, Routes::PLAY),
        Some(WsResponseCode::NO_PERMISSION as i32)
    );
    assert!(loop_state.lock().unwrap().settlement.is_none());
}

#[test]
fn play_request_rob_gang_hu_requires_added_gang_source() {
    let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
        }
        state
            .hands
            .insert(0, vec![3, 4, 5, 6, 7, 8, 9, 11, 12, 13, 21, 22, 23, 31]);
        state
            .hands
            .insert(1, vec![1, 2, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
        state.wall = vec![36];
        state.last_drawn_tile = Some(3);
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::RobGang,
            eligible_positions: vec![1],
            responses: HashMap::new(),
        });
    }

    let response = handler.handle_game_request(
        &mut room_service,
        2,
        play_request(ShenyangMahjongAction::HU, Vec::new(), Some(3), Some(0)),
    );

    assert_eq!(
        response_code(&response, 2, Routes::PLAY),
        Some(WsResponseCode::NO_PERMISSION as i32)
    );
    let state = loop_state.lock().unwrap();
    assert!(state.settlement.is_none());
    let claim_window = state.claim_window.as_ref().expect("claim window");
    assert!(claim_window.responses.is_empty());
    assert!(state.hands.get(&0).unwrap().contains(&3));
    assert!(state.melds.get(&0).unwrap().is_empty());
}

#[test]
fn play_request_rob_gang_pass_finishes_added_gang() {
    let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
        }
        state
            .hands
            .insert(0, vec![3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
        state.melds.insert(
            0,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![3, 3, 3],
                Some(2),
            )],
        );
        state
            .hands
            .insert(1, vec![1, 2, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
        state.wall = vec![36];
        state.last_drawn_tile = Some(3);
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::RobGang,
            eligible_positions: vec![1],
            responses: HashMap::new(),
        });
    }

    let pass_response = handler.handle_game_request(
        &mut room_service,
        2,
        ClientRequest {
            route: Routes::PLAY as i32,
            data: serde_json::json!({
                "action": ShenyangMahjongAction::PASS as i32,
                "tiles": [],
                "target_tile": 3,
                "from_position": 0,
            }),
        },
    );

    assert_eq!(
        response_code(&pass_response, 2, Routes::PLAY),
        Some(WsResponseCode::OK as i32)
    );
    assert!(!has_room_event(&pass_response, WsCode::GAME_OVER));
    let state = loop_state.lock().unwrap();
    assert_eq!(state.phase, ShenyangMahjongPhase::Play);
    assert!(state.claim_window.is_none());
    assert_eq!(state.current_position, 0);
    assert_eq!(state.last_drawn_tile, Some(36));
    assert!(state.hands.get(&0).unwrap().contains(&36));
    assert!(!state.hands.get(&0).unwrap().contains(&3));
    assert!(state.settlement.is_none());
    let meld = state.melds.get(&0).unwrap().first().unwrap();
    assert_eq!(meld.kind, ShenyangMahjongMeldKind::GANG);
    assert_eq!(meld.tiles, vec![3, 3, 3, 3]);
}

#[test]
fn play_request_rob_gang_rejects_impossible_fifth_tile_hu() {
    let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
        }
        state
            .hands
            .insert(0, vec![3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
        state.melds.insert(
            0,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![3, 3, 3],
                Some(2),
            )],
        );
        state
            .hands
            .insert(1, vec![1, 2, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
        state.discards.insert(2, vec![3]);
        state.last_drawn_tile = Some(3);
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::RobGang,
            eligible_positions: vec![1],
            responses: HashMap::new(),
        });
    }

    let rejected_hu = handler.handle_game_request(
        &mut room_service,
        2,
        ClientRequest {
            route: Routes::PLAY as i32,
            data: serde_json::json!({
                "action": ShenyangMahjongAction::HU as i32,
                "tiles": [],
                "target_tile": 3,
                "from_position": 0,
            }),
        },
    );

    assert_eq!(
        response_code(&rejected_hu, 2, Routes::PLAY),
        Some(WsResponseCode::NO_PERMISSION as i32)
    );
    let state = loop_state.lock().unwrap();
    assert!(state.settlement.is_none());
    assert!(state.claim_window.is_some());
}

#[test]
fn play_request_rob_gang_rejects_peng_and_accepts_hu() {
    let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
        }
        state
            .hands
            .insert(0, vec![3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
        state.melds.insert(
            0,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![3, 3, 3],
                Some(2),
            )],
        );
        state
            .hands
            .insert(1, vec![1, 2, 11, 12, 13, 31, 31, 31, 35, 35]);
        state.melds.insert(1, vec![open_peng_meld(21, 3)]);
        state.wall = vec![36];
        state.last_drawn_tile = Some(3);
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::RobGang,
            eligible_positions: vec![1],
            responses: HashMap::new(),
        });
    }

    let rejected_peng = handler.handle_game_request(
        &mut room_service,
        2,
        ClientRequest {
            route: Routes::PLAY as i32,
            data: serde_json::json!({
                "action": ShenyangMahjongAction::PENG as i32,
                "tiles": [],
                "target_tile": 3,
                "from_position": 0,
            }),
        },
    );

    assert_eq!(
        response_code(&rejected_peng, 2, Routes::PLAY),
        Some(WsResponseCode::NO_PERMISSION as i32)
    );
    assert!(loop_state.lock().unwrap().claim_window.is_some());

    let accepted_hu = handler.handle_game_request(
        &mut room_service,
        2,
        ClientRequest {
            route: Routes::PLAY as i32,
            data: serde_json::json!({
                "action": ShenyangMahjongAction::HU as i32,
                "tiles": [],
                "target_tile": 3,
                "from_position": 0,
            }),
        },
    );

    assert_eq!(
        response_code(&accepted_hu, 2, Routes::PLAY),
        Some(WsResponseCode::OK as i32)
    );
    assert!(has_room_event(&accepted_hu, WsCode::GAME_OVER));
    let state = loop_state.lock().unwrap();
    let settlement = state.settlement.as_ref().expect("settlement");
    assert_eq!(settlement.winner_positions, vec![1]);
    assert_eq!(settlement.from_position, Some(0));
    assert_eq!(settlement.win_tile, Some(3));
    assert!(settlement.is_reverse_win);
    assert!(!state.hands.get(&0).unwrap().contains(&3));
    assert_eq!(
        state.melds.get(&0).unwrap().first().unwrap().kind,
        ShenyangMahjongMeldKind::PENG
    );
}

#[test]
fn play_request_rob_gang_allows_multiple_hu() {
    let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
        }
        state
            .hands
            .insert(0, vec![1, 2, 3, 6, 11, 12, 13, 21, 22, 23, 31]);
        state.melds.insert(
            0,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![6, 6, 6],
                Some(2),
            )],
        );
        state
            .hands
            .insert(1, vec![4, 5, 11, 12, 13, 21, 22, 23, 35, 35]);
        state
            .hands
            .insert(2, vec![4, 5, 14, 15, 16, 24, 25, 26, 35, 35]);
        state.melds.insert(1, vec![open_peng_meld(31, 3)]);
        state.melds.insert(2, vec![open_peng_meld(32, 3)]);
        state.wall = vec![36];
        state.last_drawn_tile = Some(6);
        state.claim_window = Some(ClaimWindowState {
            tile: 6,
            from_position: 0,
            kind: ClaimWindowKind::RobGang,
            eligible_positions: vec![1, 2],
            responses: HashMap::new(),
        });
    }

    let first_hu = handler.handle_game_request(
        &mut room_service,
        2,
        play_request(ShenyangMahjongAction::HU, Vec::new(), Some(6), Some(0)),
    );
    let second_hu = handler.handle_game_request(
        &mut room_service,
        3,
        play_request(ShenyangMahjongAction::HU, Vec::new(), Some(6), Some(0)),
    );

    assert_eq!(
        response_code(&first_hu, 2, Routes::PLAY),
        Some(WsResponseCode::OK as i32)
    );
    assert!(!has_room_event(&first_hu, WsCode::GAME_OVER));
    assert_eq!(
        response_code(&second_hu, 3, Routes::PLAY),
        Some(WsResponseCode::OK as i32)
    );
    assert!(has_room_event(&second_hu, WsCode::GAME_OVER));

    let state = loop_state.lock().unwrap();
    let settlement = state.settlement.as_ref().expect("settlement");
    assert_eq!(settlement.winner_positions, vec![1, 2]);
    assert_eq!(settlement.from_position, Some(0));
    assert_eq!(settlement.win_tile, Some(6));
    assert!(settlement.is_reverse_win);
    assert!(!state.hands.get(&0).unwrap().contains(&6));
    assert_eq!(
        state.melds.get(&0).unwrap().first().unwrap().kind,
        ShenyangMahjongMeldKind::PENG
    );
    assert_eq!(winner_hand_fan(&state, settlement, 1), 2);
    assert_eq!(winner_hand_fan(&state, settlement, 2), 2);
}

#[test]
fn play_request_waits_for_claims_and_hu_beats_peng() {
    let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
    {
        let mut state = loop_state.lock().unwrap();
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        for position in 0..4 {
            state.discards.insert(position, Vec::new());
            state.melds.insert(position, Vec::new());
        }
        state.discards.insert(0, vec![3]);
        state
            .hands
            .insert(1, vec![1, 2, 11, 12, 13, 31, 31, 31, 35, 35]);
        state.melds.insert(1, vec![open_peng_meld(21, 3)]);
        state
            .hands
            .insert(2, vec![3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1, 2],
            responses: HashMap::new(),
        });
    }

    let peng_response = handler.handle_game_request(
        &mut room_service,
        3,
        ClientRequest {
            route: Routes::PLAY as i32,
            data: serde_json::json!({
                "action": ShenyangMahjongAction::PENG as i32,
                "tiles": [],
                "target_tile": 3,
                "from_position": 0,
            }),
        },
    );

    assert_eq!(
        response_code(&peng_response, 3, Routes::PLAY),
        Some(WsResponseCode::OK as i32)
    );
    assert!(!has_room_event(&peng_response, WsCode::GAME_OVER));
    {
        let state = loop_state.lock().unwrap();
        assert!(state.claim_window.is_some());
        assert!(state.settlement.is_none());
        assert!(state.melds.get(&2).unwrap().is_empty());
    }

    let hu_response = handler.handle_game_request(
        &mut room_service,
        2,
        ClientRequest {
            route: Routes::PLAY as i32,
            data: serde_json::json!({
                "action": ShenyangMahjongAction::HU as i32,
                "tiles": [],
                "target_tile": 3,
                "from_position": 0,
            }),
        },
    );

    assert_eq!(
        response_code(&hu_response, 2, Routes::PLAY),
        Some(WsResponseCode::OK as i32)
    );
    assert!(has_room_event(&hu_response, WsCode::GAME_OVER));
    let state = loop_state.lock().unwrap();
    assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
    assert_eq!(
        state
            .settlement
            .as_ref()
            .map(|settlement| settlement.winner_positions.clone()),
        Some(vec![1])
    );
    assert!(state.discards.get(&0).unwrap().is_empty());
    assert!(state.melds.get(&2).unwrap().is_empty());
}
