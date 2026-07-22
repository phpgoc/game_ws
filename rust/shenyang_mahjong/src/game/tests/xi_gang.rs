use super::*;

#[test]
fn two_xi_gangs_stack_two_fan_and_keep_hand_closed() {
    let mut state = playable_state();
    state.current_position = 1;
    state
        .hands
        .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 31, 32, 33, 34, 35, 36, 37]);
    state.melds.insert(1, Vec::new());
    state.last_drawn_tile = Some(37);
    state.wall = vec![22];
    state
        .xi_gang_options
        .insert(1, vec![vec![31, 32, 33, 34], vec![35, 36, 37]]);
    let mut dispatch = Dispatch::default();

    assert!(perform_xi_gang(
        &RoomService::default(),
        "room",
        &mut state,
        &HashMap::new(),
        &mut dispatch,
        1,
        &[31, 32, 33, 34],
    ));
    assert!(perform_xi_gang(
        &RoomService::default(),
        "room",
        &mut state,
        &HashMap::new(),
        &mut dispatch,
        1,
        &[35, 36, 37],
    ));

    let melds = state.melds.get(&1).unwrap();
    assert_eq!(melds.len(), 2);
    assert_eq!(shenyang_score_meld_fan(melds), 2);
    assert!(!position_has_open_meld(&state, 1));
    assert_eq!(state.hands.get(&1).unwrap().len(), 8);
}

#[test]
fn two_xi_gangs_can_be_declared_dragon_first() {
    let mut state = playable_state();
    state.current_position = 1;
    state
        .hands
        .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 31, 32, 33, 34, 35, 36, 37]);
    state.melds.insert(1, Vec::new());
    state.last_drawn_tile = Some(37);
    state.wall = vec![22];
    state
        .xi_gang_options
        .insert(1, vec![vec![31, 32, 33, 34], vec![35, 36, 37]]);
    let mut dispatch = Dispatch::default();

    assert!(perform_xi_gang(
        &RoomService::default(),
        "room",
        &mut state,
        &HashMap::new(),
        &mut dispatch,
        1,
        &[35, 36, 37],
    ));
    assert_eq!(state.hands.get(&1).unwrap().len(), 11);
    assert_eq!(state.last_drawn_tile, Some(37));

    assert!(perform_xi_gang(
        &RoomService::default(),
        "room",
        &mut state,
        &HashMap::new(),
        &mut dispatch,
        1,
        &[31, 32, 33, 34],
    ));

    let melds = state.melds.get(&1).unwrap();
    assert_eq!(melds.len(), 2);
    assert_eq!(shenyang_score_meld_fan(melds), 2);
    assert!(!position_has_open_meld(&state, 1));
    assert_eq!(state.hands.get(&1).unwrap().len(), 8);
    assert_eq!(state.last_drawn_tile, Some(22));
    assert!(state.wall.is_empty());
    assert!(!state.pending_gang_draw);
}

#[test]
fn first_discard_closes_undeclared_xi_gang_window_permanently() {
    let mut state = playable_state();
    state.current_position = 1;
    state
        .hands
        .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 22, 31, 32, 33, 34, 35]);
    state.melds.insert(1, Vec::new());
    state.wall = vec![8, 9, 36];

    assert_eq!(state.draw_for_next_turn(1), Some(36));
    assert_eq!(
        state.xi_gang_options_for_position(1),
        vec![vec![31, 32, 33, 34]]
    );

    let mut dispatch = Dispatch::default();
    assert!(perform_discard(
        &RoomService::default(),
        "room",
        &mut state,
        &HashMap::new(),
        &mut dispatch,
        1,
        35,
    ));
    assert!(state.xi_gang_options_for_position(1).is_empty());

    assert_eq!(state.draw_for_next_turn(1), Some(8));
    assert!(state.xi_gang_options_for_position(1).is_empty());
    assert!(!can_declare_xi_gang(&state, 1, &[31, 32, 33, 34]));
}

#[test]
fn drawn_dragon_moved_into_xi_gang_can_complete_self_draw_without_gang_draw() {
    let mut state = playable_state();
    state.current_position = 1;
    state
        .hands
        .insert(1, vec![1, 1, 1, 11, 11, 11, 21, 21, 21, 35, 35, 35, 36, 37]);
    state.melds.insert(1, Vec::new());
    state.last_drawn_tile = Some(37);
    state.wall = vec![31];
    state.xi_gang_options.insert(1, vec![vec![35, 36, 37]]);
    let mut dispatch = Dispatch::default();

    assert!(perform_xi_gang(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
        1,
        &[35, 36, 37],
    ));
    assert_eq!(state.last_drawn_tile, Some(37));
    assert!(!state.hands.get(&1).unwrap().contains(&37));
    assert!(!state.pending_gang_draw);
    assert!(can_self_draw_hu_with_configs(&state, 1, &default_configs()));

    perform_self_draw_hu(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
        1,
    );

    let settlement = state.settlement.as_ref().expect("settlement");
    assert!(settlement.is_self_draw);
    assert!(!settlement.is_gang_draw);
    assert_eq!(settlement.win_tile, Some(37));
    assert_eq!(winner_hand_fan(&state, settlement, 1), 4);
    let event = build_settlement_event(&state).expect("settlement event");
    assert!(!event.is_gang_draw);
    assert!(!event.winner_details[0].is_gang_draw);
}

#[test]
fn wind_xi_gang_draws_replacement_without_creating_gang_draw() {
    let mut state = playable_state();
    state.current_position = 1;
    state
        .hands
        .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 35, 31, 32, 33, 34]);
    state.melds.insert(1, Vec::new());
    state.last_drawn_tile = Some(34);
    state.pending_gang_draw = true;
    state.wall = vec![36];
    state.xi_gang_options.insert(1, vec![vec![31, 32, 33, 34]]);
    let mut dispatch = Dispatch::default();

    assert!(perform_xi_gang(
        &RoomService::default(),
        "room",
        &mut state,
        &HashMap::new(),
        &mut dispatch,
        1,
        &[31, 32, 33, 34],
    ));

    assert!(state.wall.is_empty());
    assert_eq!(state.last_drawn_tile, Some(36));
    assert!(!state.pending_gang_draw);
    assert_eq!(state.hands.get(&1).unwrap().len(), 11);
    assert!(state.hands.get(&1).unwrap().contains(&36));
    assert!(!position_has_open_meld(&state, 1));
    assert!(dispatch.messages.iter().any(|message| {
            matches!(
                &message.payload,
                OutboundPayload::Event(event)
                    if event.code == WsCode::PLAY as i32
                        && event.data.get("action") == Some(&json!(ShenyangMahjongAction::XI_GANG as i32))
            )
    }));
}

#[test]
fn dragon_xi_gang_clears_prior_regular_gang_draw_state() {
    let mut state = playable_state();
    state.current_position = 1;
    state
        .hands
        .insert(1, vec![1, 1, 1, 1, 11, 11, 11, 21, 21, 35, 35, 35, 36, 37]);
    state.melds.insert(1, Vec::new());
    state.last_drawn_tile = Some(37);
    state.wall = vec![31, 21];
    state.xi_gang_options.insert(1, vec![vec![35, 36, 37]]);
    let mut dispatch = Dispatch::default();

    assert!(perform_self_gang(
        &RoomService::default(),
        "room",
        &mut state,
        &HashMap::new(),
        &mut dispatch,
        1,
        1,
    ));
    assert_eq!(state.last_drawn_tile, Some(21));
    assert!(state.pending_gang_draw);
    assert!(perform_xi_gang(
        &RoomService::default(),
        "room",
        &mut state,
        &HashMap::new(),
        &mut dispatch,
        1,
        &[35, 36, 37],
    ));

    assert_eq!(state.last_drawn_tile, Some(21));
    assert!(!state.pending_gang_draw);
    assert!(can_self_draw_hu_with_configs(&state, 1, &default_configs()));
    perform_self_draw_hu(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
        1,
    );

    let settlement = state.settlement.as_ref().expect("settlement");
    assert!(!settlement.is_gang_draw);
    assert_eq!(winner_hand_fan(&state, settlement, 1), 7);
    let event = build_settlement_event(&state).expect("settlement event");
    assert!(!event.is_gang_draw);
    assert!(!event.winner_details[0].is_gang_draw);
}

#[test]
fn wind_xi_gang_last_replacement_win_is_haidilao_not_gang_draw() {
    let mut state = playable_state();
    state.current_position = 1;
    state
        .hands
        .insert(1, vec![1, 2, 3, 11, 12, 13, 22, 23, 31, 32, 33, 34, 35, 35]);
    state.melds.insert(1, Vec::new());
    state.last_drawn_tile = Some(34);
    state.wall = vec![24];
    state.xi_gang_options.insert(1, vec![vec![31, 32, 33, 34]]);
    let configs = HashMap::from([("allow_first_chi".to_owned(), 0)]);
    let mut dispatch = Dispatch::default();

    assert!(perform_xi_gang(
        &RoomService::default(),
        "room",
        &mut state,
        &configs,
        &mut dispatch,
        1,
        &[31, 32, 33, 34],
    ));
    assert!(can_self_draw_hu_with_configs(&state, 1, &configs));
    perform_self_draw_hu(
        &RoomService::default(),
        "room",
        &mut state,
        &configs,
        &mut dispatch,
        1,
    );

    let settlement = state.settlement.as_ref().expect("settlement");
    assert!(settlement.is_haidilao);
    assert!(!settlement.is_gang_draw);
}
