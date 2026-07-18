use super::*;

#[test]
fn stale_same_name_loop_state_does_not_block_recreated_room_start() {
    let mut room_service = RoomService::default();
    let join = |room: &mut RoomService, session_id: SessionId, prefix: &str| {
        room.handle_common_request(
            session_id,
            &ClientRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({
                    "name": format!("{prefix}-{session_id}"),
                    "password": "same-name",
                    "game_id": GameId::SHENYANG_MAHJONG as i32
                }),
            },
            GameId::SHENYANG_MAHJONG,
            build_shenyang_mahjong_settings,
        )
    };
    for session_id in 1..=4 {
        let _ = join(&mut room_service, session_id, "old");
    }
    let old_common = room_service
        .room_common_state("same-name")
        .expect("old room common state");
    let old_loop_state = Arc::new(StdMutex::new(ShenyangMahjongLoopState::new(Arc::clone(
        &old_common,
    ))));
    room_service.set_room_game_state(
        "same-name",
        Box::new(ShenyangMahjongGameState::from_loop_state(Arc::clone(
            &old_loop_state,
        ))),
    );
    let mut handler = ShenyangMahjongGameHandler::default();
    handler
        .loop_states
        .lock()
        .unwrap()
        .insert("same-name".to_string(), Arc::clone(&old_loop_state));

    for session_id in 1..=4 {
        let _ = room_service.disconnect(session_id);
    }
    assert!(!room_service.room_exists("same-name"));
    assert!(old_common.lock().unwrap().stop_requested());

    for session_id in 5..=8 {
        let _ = join(&mut room_service, session_id, "new");
    }
    let recreated_common = room_service
        .room_common_state("same-name")
        .expect("recreated room common state");
    assert!(!Arc::ptr_eq(&old_common, &recreated_common));
    assert!(
        handler
            .current_loop_state(&room_service, "same-name")
            .is_none()
    );

    let started = handler.handle_start(&mut room_service, 5);

    assert_eq!(
        response_code(&started, 5, Routes::START),
        Some(WsResponseCode::OK as i32)
    );
    let new_state = handler
        .loop_state("same-name")
        .expect("new mahjong loop state");
    let new_common = Arc::clone(&new_state.lock().unwrap().base);
    assert!(Arc::ptr_eq(
        &new_common,
        &room_service
            .room_common_state("same-name")
            .expect("current room common state")
    ));
    assert!(!Arc::ptr_eq(&old_common, &new_common));
}

#[test]
fn table_and_settlement_snapshots_filter_invalid_discards() {
    let mut state = playable_state();
    state.discards.insert(1, vec![3, 99, 35, -1]);
    state.enter_settlement(Vec::new(), None, None, false);

    let snapshot = build_table_snapshot_event_with_configs(&state, 0, &default_configs());
    let public_discards = &snapshot
        .players
        .iter()
        .find(|player| player.position == 1)
        .expect("public seat 1")
        .discards;
    let settlement = snapshot.settlement.expect("settlement");
    let settlement_discards = &settlement
        .players
        .iter()
        .find(|player| player.position == 1)
        .expect("settlement seat 1")
        .discards;

    assert_eq!(public_discards, &vec![3, 35]);
    assert_eq!(settlement_discards, &vec![3, 35]);
}

#[test]
fn table_snapshot_exposes_xi_gang_options_only_to_current_player() {
    let mut state = playable_state();
    state.current_position = 1;
    state.xi_gang_options.insert(1, vec![vec![35, 36, 37]]);

    let owner = build_table_snapshot_event_with_configs(&state, 1, &HashMap::new());
    let opponent = build_table_snapshot_event_with_configs(&state, 0, &HashMap::new());

    assert_eq!(owner.xi_gang_options, vec![vec![35, 36, 37]]);
    assert!(opponent.xi_gang_options.is_empty());
}

#[test]
fn table_snapshot_filters_drawn_tile_and_claim_options() {
    let mut state = playable_state();
    state.current_position = 0;
    state.last_drawn_tile = Some(9);
    state.wall = vec![36];
    state
        .hands
        .insert(0, vec![1, 2, 4, 5, 6, 7, 9, 11, 12, 13, 21, 22, 23, 31]);
    state
        .hands
        .insert(1, vec![1, 2, 3, 3, 3, 4, 11, 12, 13, 21, 22, 23, 31]);
    state
        .hands
        .insert(2, vec![1, 2, 11, 12, 13, 32, 32, 32, 35, 35]);
    state.melds.insert(2, vec![open_peng_meld(29, 3)]);
    state.discards.insert(0, vec![3]);
    state.claim_window = Some(ClaimWindowState {
        tile: 3,
        from_position: 0,
        kind: ClaimWindowKind::Discard,
        eligible_positions: vec![1, 2],
        responses: HashMap::new(),
    });
    state.set_turn_countdown(4);

    let drawer_snapshot = build_table_snapshot_event_with_configs(&state, 0, &default_configs());
    let snapshot = build_table_snapshot_event_with_configs(&state, 1, &default_configs());
    let claim_window = snapshot.claim_window.expect("claim window");
    let option = claim_window
        .options
        .iter()
        .find(|option| option.position == 1)
        .expect("claim option");

    assert_eq!(drawer_snapshot.last_drawn_tile, Some(9));
    assert_eq!(snapshot.last_drawn_tile, None);
    assert_eq!(claim_window.tile, 3);
    assert_eq!(claim_window.from_position, 0);
    assert_eq!(claim_window.eligible_positions, vec![1]);
    assert_eq!(claim_window.seconds, 4);
    assert!(!claim_window.is_rob_gang);
    assert!(option.can_peng);
    assert!(option.can_gang);
    assert!(option.chi_options.contains(&vec![1, 2]));
    assert!(option.chi_options.contains(&vec![2, 4]));
    assert_eq!(claim_window.options.len(), 1);

    let observer_snapshot = build_table_snapshot_event_with_configs(&state, 3, &default_configs());
    let observer_claim_window = observer_snapshot.claim_window.expect("claim window");
    assert_eq!(observer_claim_window.tile, 3);
    assert_eq!(observer_claim_window.from_position, 0);
    assert!(observer_claim_window.eligible_positions.is_empty());
    assert!(observer_claim_window.options.is_empty());

    state
        .claim_window
        .as_mut()
        .unwrap()
        .responses
        .insert(1, ClaimResponse::Pass);
    let responded_snapshot = build_table_snapshot_event_with_configs(&state, 1, &default_configs());
    let responded_claim_window = responded_snapshot.claim_window.expect("claim window");
    assert_eq!(responded_claim_window.tile, 3);
    assert!(responded_claim_window.eligible_positions.is_empty());
    assert!(responded_claim_window.options.is_empty());

    let pending_snapshot = build_table_snapshot_event_with_configs(&state, 2, &default_configs());
    let pending_claim_window = pending_snapshot.claim_window.expect("claim window");
    assert_eq!(pending_claim_window.eligible_positions, vec![2]);
    assert_eq!(pending_claim_window.options.len(), 1);
    assert_eq!(pending_claim_window.options[0].position, 2);
    assert!(pending_claim_window.options[0].can_hu);

    state.claim_window.as_mut().unwrap().eligible_positions = vec![1];
    let excluded_snapshot = build_table_snapshot_event_with_configs(&state, 2, &default_configs());
    let excluded_claim_window = excluded_snapshot.claim_window.expect("claim window");
    assert!(excluded_claim_window.eligible_positions.is_empty());
    assert!(excluded_claim_window.options.is_empty());
}

#[test]
fn table_snapshot_filters_malformed_meld_shapes() {
    let mut state = playable_state();
    state.melds.insert(
        1,
        vec![
            build_meld(ShenyangMahjongMeldKind::PENG, vec![3, 3], Some(0)),
            build_meld(ShenyangMahjongMeldKind::PENG, vec![4, 4, 4], Some(2)),
        ],
    );
    state.enter_settlement(Vec::new(), None, None, false);

    let snapshot = build_table_snapshot_event_with_configs(&state, 0, &default_configs());
    let public_melds = &snapshot
        .players
        .iter()
        .find(|player| player.position == 1)
        .expect("public seat 1")
        .melds;
    let settlement = snapshot.settlement.as_ref().expect("settlement");
    let settlement_melds = &settlement
        .players
        .iter()
        .find(|player| player.position == 1)
        .expect("settlement seat 1")
        .melds;

    assert_eq!(public_melds.len(), 1);
    assert_eq!(public_melds[0].tiles, vec![4, 4, 4]);
    assert_eq!(settlement_melds.len(), 1);
    assert_eq!(settlement_melds[0].tiles, vec![4, 4, 4]);
}

#[test]
fn table_snapshot_filters_melds_with_invalid_source_positions() {
    let mut state = playable_state();
    state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::PENG,
            vec![3, 3, 3],
            Some(1),
        )],
    );
    state.melds.insert(
        2,
        vec![build_meld(
            ShenyangMahjongMeldKind::PENG,
            vec![4, 4, 4],
            Some(3),
        )],
    );
    state.enter_settlement(Vec::new(), None, None, false);

    let snapshot = build_table_snapshot_event_with_configs(&state, 0, &default_configs());
    let public_invalid = snapshot
        .players
        .iter()
        .find(|player| player.position == 1)
        .expect("public seat 1");
    let public_valid = snapshot
        .players
        .iter()
        .find(|player| player.position == 2)
        .expect("public seat 2");
    let settlement = snapshot.settlement.expect("settlement");
    let settlement_invalid = settlement
        .players
        .iter()
        .find(|player| player.position == 1)
        .expect("settlement seat 1");
    let settlement_valid = settlement
        .players
        .iter()
        .find(|player| player.position == 2)
        .expect("settlement seat 2");

    assert!(public_invalid.melds.is_empty());
    assert_eq!(public_valid.melds.len(), 1);
    assert!(settlement_invalid.melds.is_empty());
    assert_eq!(settlement_valid.melds.len(), 1);
}

#[test]
fn table_snapshot_hides_claim_window_outside_play_phase() {
    let mut state = playable_state();
    state.current_position = 0;
    state.discards.insert(0, vec![3]);
    state.claim_window = Some(ClaimWindowState {
        tile: 3,
        from_position: 0,
        kind: ClaimWindowKind::Discard,
        eligible_positions: vec![1],
        responses: HashMap::new(),
    });
    state.phase = ShenyangMahjongPhase::Settlement;

    let snapshot = build_table_snapshot_event_with_configs(&state, 1, &default_configs());

    assert!(snapshot.claim_window.is_none());
}

#[test]
fn table_snapshot_hides_claim_window_with_invalid_source() {
    let mut state = playable_state();
    state.current_position = 0;
    state.discards.insert(9, vec![3]);
    state.claim_window = Some(ClaimWindowState {
        tile: 3,
        from_position: 9,
        kind: ClaimWindowKind::Discard,
        eligible_positions: vec![1],
        responses: HashMap::new(),
    });

    let snapshot = build_table_snapshot_event_with_configs(&state, 1, &default_configs());

    assert!(snapshot.claim_window.is_none());
}

#[test]
fn table_snapshot_hides_claim_window_with_invalid_tile() {
    let mut state = playable_state();
    state.current_position = 0;
    state.discards.insert(0, vec![99]);
    state.claim_window = Some(ClaimWindowState {
        tile: 99,
        from_position: 0,
        kind: ClaimWindowKind::Discard,
        eligible_positions: vec![1],
        responses: HashMap::new(),
    });

    let snapshot = build_table_snapshot_event_with_configs(&state, 1, &default_configs());

    assert!(snapshot.claim_window.is_none());
}

#[test]
fn table_snapshot_hides_claim_window_with_malformed_participants() {
    let mut state = playable_state();
    state.current_position = 0;
    state.discards.insert(0, vec![3]);
    state.claim_window = Some(ClaimWindowState {
        tile: 3,
        from_position: 0,
        kind: ClaimWindowKind::Discard,
        eligible_positions: vec![1, 9],
        responses: HashMap::new(),
    });

    let snapshot = build_table_snapshot_event_with_configs(&state, 1, &default_configs());

    assert!(snapshot.claim_window.is_none());
}

#[test]
fn table_snapshot_includes_settlement_for_rejoin() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![1, 2, 11, 12, 13, 31, 31, 31, 35, 35]);
    state.melds.insert(1, vec![open_peng_meld(21, 3)]);
    state.melds.insert(
        0,
        vec![build_meld(
            ShenyangMahjongMeldKind::PENG,
            vec![3, 3, 3],
            Some(2),
        )],
    );
    state.enter_settlement_with_reverse_win(vec![1], Some(0), Some(3), false, true, false, false);

    let snapshot = build_table_snapshot_event_with_configs(&state, 1, &default_configs());
    let settlement = snapshot.settlement.expect("settlement");

    assert_eq!(snapshot.phase, ShenyangMahjongPhase::Settlement);
    assert_eq!(settlement.winner_positions, vec![1]);
    assert_eq!(settlement.from_position, Some(0));
    assert_eq!(settlement.win_tile, Some(3));
    assert!(settlement.is_reverse_win);
    assert_eq!(settlement.winner_details.len(), 1);
    assert_eq!(settlement.winner_details[0].position, 1);
    assert_eq!(settlement.winner_details[0].score, 16);
    assert_eq!(
        settlement
            .score_changes
            .iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, -16), (1, 16), (2, 0), (3, 0)]
    );
}

#[test]
fn table_snapshot_marks_disconnected_player_as_away() {
    let state = playable_state();
    state.base.lock().unwrap().mark_disconnected(2);

    let snapshot = build_table_snapshot_event_with_configs(&state, 1, &default_configs());
    let player = snapshot
        .players
        .iter()
        .find(|player| player.position == 2)
        .expect("player snapshot");

    assert!(player.away);
    assert!(!player.is_ai);
}
