use super::*;

#[test]
fn self_draw_closed_sequence_dragon_pair_win_stays_available_after_xi_gang() {
    let mut state = playable_state();
    state.current_position = 1;
    state
        .hands
        .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 35, 35]);
    state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::XI_GANG,
            vec![31, 32, 33, 34],
            None,
        )],
    );
    state.last_drawn_tile = Some(35);
    let default_configs = HashMap::new();
    let disabled_configs = HashMap::from([("allow_first_chi".to_owned(), 0)]);

    assert!(!can_self_draw_hu_with_configs(&state, 1, &default_configs));
    assert!(can_self_draw_hu_with_configs(&state, 1, &disabled_configs));
}

#[test]
fn self_draw_hu_allows_closed_sequence_dragon_pair_win_when_first_chi_disabled() {
    let mut state = playable_state();
    state.current_position = 0;
    state
        .hands
        .insert(0, vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 35, 35]);
    state.last_drawn_tile = Some(35);
    let default_configs = HashMap::new();
    let disabled_configs = HashMap::from([("allow_first_chi".to_owned(), 0)]);

    assert!(!can_self_draw_hu_with_configs(&state, 0, &default_configs));
    assert!(can_self_draw_hu_with_configs(&state, 0, &disabled_configs));
}

#[test]
fn concealed_gang_does_not_count_as_open_for_self_draw() {
    let mut state = playable_state();
    state
        .hands
        .insert(0, vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
    state.melds.insert(
        0,
        vec![build_meld(
            ShenyangMahjongMeldKind::GANG,
            vec![1, 1, 1, 1],
            None,
        )],
    );
    state.last_drawn_tile = Some(35);
    let configs = default_configs();

    assert!(!can_self_draw_hu_with_configs(&state, 0, &configs));
    assert!(!can_self_draw_hu_with_configs(&state, 0, &configs));
}

#[test]
fn self_draw_hu_rejects_chi_from_non_previous_position() {
    let mut state = playable_state();
    state
        .hands
        .insert(0, vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
    state.melds.insert(
        0,
        vec![build_meld(
            ShenyangMahjongMeldKind::CHI,
            vec![1, 2, 3],
            Some(1),
        )],
    );
    state.last_drawn_tile = Some(35);
    let configs = HashMap::new();

    assert!(is_complete_win_with_configs(
        state.hands.get(&0).unwrap(),
        state.melds.get(&0).unwrap(),
        &configs
    ));
    assert!(!can_self_draw_hu_with_configs(&state, 0, &configs));
}

#[test]
fn self_draw_hu_rejects_complete_open_hand_without_draw() {
    let mut state = playable_state();
    state
        .hands
        .insert(0, vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
    state.melds.insert(
        0,
        vec![build_meld(
            ShenyangMahjongMeldKind::PENG,
            vec![1, 1, 1],
            Some(2),
        )],
    );

    assert!(!can_self_draw_hu(&state, 0));
}

#[test]
fn self_draw_hu_rejects_outside_play_phase() {
    let mut state = playable_state();
    state.phase = ShenyangMahjongPhase::Start;
    state
        .hands
        .insert(0, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
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

    assert_eq!(state.phase, ShenyangMahjongPhase::Start);
    assert!(state.settlement.is_none());
    assert!(dispatch.messages.is_empty());
}

#[test]
fn self_draw_hu_rejects_public_fifth_copy() {
    let mut state = playable_state();
    state
        .hands
        .insert(0, vec![1, 1, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6]);
    state.discards.insert(1, vec![1]);
    state.last_drawn_tile = Some(6);
    let mut dispatch = Dispatch::default();

    assert!(is_seven_pairs_win(state.hands.get(&0).unwrap()));
    assert_eq!(known_tile_count(&state, 1), 5);
    assert!(position_has_impossible_known_tile_count(&state, 0));
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

    state.discards.insert(1, vec![9, 9, 9, 9, 9]);
    assert_eq!(known_tile_count(&state, 9), 5);
    assert!(!position_has_impossible_known_tile_count(&state, 0));
    assert!(can_self_draw_hu_with_configs(&state, 0, &default_configs()));
}

#[test]
fn self_draw_hu_rejects_self_sourced_open_meld() {
    let mut state = playable_state();
    state
        .hands
        .insert(0, vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
    state.melds.insert(
        0,
        vec![build_meld(
            ShenyangMahjongMeldKind::CHI,
            vec![1, 2, 3],
            Some(0),
        )],
    );
    state.last_drawn_tile = Some(35);
    let configs = HashMap::new();

    assert!(is_complete_win_with_configs(
        state.hands.get(&0).unwrap(),
        state.melds.get(&0).unwrap(),
        &configs
    ));
    assert!(!can_self_draw_hu_with_configs(&state, 0, &configs));

    state.enter_settlement(vec![0], None, Some(35), true);
    let settlement = state.settlement.as_ref().expect("settlement");
    assert_eq!(winner_hand_fan(&state, settlement, 0), 0);
}

#[test]
fn self_draw_hu_requires_a_drawn_turn() {
    let mut state = playable_state();
    state
        .hands
        .insert(0, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35]);

    assert!(!can_self_draw_hu_with_configs(
        &state,
        0,
        &default_configs()
    ));

    state.last_drawn_tile = Some(9);

    assert!(!can_self_draw_hu_with_configs(
        &state,
        0,
        &default_configs()
    ));

    state.last_drawn_tile = Some(35);

    assert!(can_self_draw_hu_with_configs(&state, 0, &default_configs()));
}

#[test]
fn self_draw_hu_requires_current_position() {
    let mut state = playable_state();
    state.current_position = 1;
    state
        .hands
        .insert(0, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
    state.last_drawn_tile = Some(35);

    assert!(!can_self_draw_hu(&state, 0));
}

#[test]
fn self_draw_hu_respects_shenyang_win_rule() {
    let mut state = playable_state();
    state
        .hands
        .insert(0, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
    state.last_drawn_tile = Some(35);
    let configs = HashMap::new();

    assert!(!can_self_draw_hu_with_configs(&state, 0, &configs));

    state
        .hands
        .insert(0, vec![1, 2, 3, 2, 3, 4, 4, 5, 6, 7, 7, 7, 9, 9]);
    state.melds.insert(0, Vec::new());
    state.last_drawn_tile = Some(9);

    assert!(can_self_draw_hu_with_configs(&state, 0, &configs));

    state.hands.insert(0, vec![3, 4, 5, 4, 5, 6, 5, 6, 7, 8, 8]);
    state.melds.insert(
        0,
        vec![build_meld(
            ShenyangMahjongMeldKind::CHI,
            vec![2, 3, 4],
            Some(3),
        )],
    );
    state.last_drawn_tile = Some(8);

    assert!(can_self_draw_hu_with_configs(&state, 0, &configs));

    state
        .hands
        .insert(0, vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
    state.melds.insert(
        0,
        vec![build_meld(
            ShenyangMahjongMeldKind::CHI,
            vec![1, 2, 3],
            Some(3),
        )],
    );
    state.last_drawn_tile = Some(35);

    assert!(can_self_draw_hu_with_configs(&state, 0, &configs));
    let first_chi_disabled_configs = HashMap::from([("allow_first_chi".to_owned(), 0)]);
    assert!(can_self_draw_hu_with_configs(
        &state,
        0,
        &first_chi_disabled_configs
    ));
}

#[test]
fn self_draw_last_wall_tile_counts_haidilao_without_gang_draw() {
    let mut state = playable_state();
    state
        .hands
        .insert(0, vec![2, 3, 4, 11, 12, 13, 31, 31, 31, 35]);
    state.melds.insert(0, vec![open_peng_meld(21, 2)]);
    state.wall = vec![35];

    assert_eq!(state.draw_for_position(0), Some(35));
    assert_eq!(state.wall_count(), 0);
    assert_eq!(state.last_drawn_tile, Some(35));
    assert!(!state.pending_gang_draw);
    assert!(can_self_draw_hu_with_configs(&state, 0, &default_configs()));

    let mut dispatch = Dispatch::default();
    perform_self_draw_hu(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
        0,
    );

    let settlement = state.settlement.as_ref().expect("settlement");
    assert!(settlement.is_self_draw);
    assert!(!settlement.is_gang_draw);
    assert!(settlement.is_haidilao);
    assert_eq!(settlement.win_tile, Some(35));
    assert_eq!(winner_hand_fan(&state, settlement, 0), 3);

    let event = build_settlement_event_with_configs(&state, &default_configs()).unwrap();
    assert!(!event.is_gang_draw);
    assert!(event.is_haidilao);
    assert_eq!(event.winner_details.len(), 1);
    assert!(!event.winner_details[0].is_gang_draw);
    assert!(event.winner_details[0].is_haidilao);
}

#[test]
fn self_gang_consumes_four_tiles_and_draws_replacement() {
    let mut state = playable_state();
    state
        .hands
        .insert(0, vec![3, 3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
    state.wall = vec![35];
    state.last_drawn_tile = Some(3);
    let mut dispatch = Dispatch::default();

    assert!(can_self_gang(&state, 0, 3));
    assert!(perform_self_gang(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
        0,
        3,
    ));

    assert_eq!(state.current_position, 0);
    assert_eq!(state.wall_count(), 0);
    assert_eq!(state.last_drawn_tile, Some(35));
    assert!(state.hands.get(&0).unwrap().contains(&35));
    assert_eq!(
        state
            .hands
            .get(&0)
            .unwrap()
            .iter()
            .filter(|&&tile| tile == 3)
            .count(),
        0,
    );

    let meld = state.melds.get(&0).unwrap().first().unwrap();
    assert_eq!(meld.kind, ShenyangMahjongMeldKind::GANG);
    assert_eq!(meld.tiles, vec![3, 3, 3, 3]);
    assert_eq!(meld.from_position, None);
}

#[test]
fn self_gang_last_replacement_self_draw_counts_gang_draw_and_haidilao() {
    let mut state = playable_state();
    state
        .hands
        .insert(0, vec![3, 3, 3, 3, 11, 12, 13, 31, 31, 31, 35]);
    state.melds.insert(0, vec![open_peng_meld(21, 2)]);
    state.wall = vec![35];
    state.last_drawn_tile = Some(3);
    let mut dispatch = Dispatch::default();

    assert!(perform_self_gang(
        &RoomService::default(),
        "room",
        &mut state,
        &HashMap::new(),
        &mut dispatch,
        0,
        3,
    ));
    assert_eq!(state.wall_count(), 0);
    assert_eq!(state.last_drawn_tile, Some(35));
    assert!(state.pending_gang_draw);
    assert!(can_self_draw_hu_with_configs(&state, 0, &default_configs()));

    perform_self_draw_hu(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
        0,
    );

    let settlement = state.settlement.as_ref().expect("settlement");
    assert!(settlement.is_self_draw);
    assert!(settlement.is_gang_draw);
    assert!(settlement.is_haidilao);
    assert_eq!(settlement.win_tile, Some(35));
    assert_eq!(winner_hand_fan(&state, settlement, 0), 6);

    let event = build_settlement_event_with_configs(&state, &default_configs()).unwrap();
    assert!(event.is_gang_draw);
    assert!(event.is_haidilao);
    assert_eq!(event.winner_details.len(), 1);
    assert!(event.winner_details[0].is_gang_draw);
    assert!(event.winner_details[0].is_haidilao);
}

#[test]
fn self_gang_rejects_malformed_owned_meld() {
    let mut state = playable_state();
    state
        .hands
        .insert(0, vec![3, 3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
    state.melds.insert(
        0,
        vec![build_meld(
            ShenyangMahjongMeldKind::PENG,
            vec![9, 9],
            Some(1),
        )],
    );
    state.wall = vec![35];
    state.last_drawn_tile = Some(3);
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
    let melds = state.melds.get(&0).unwrap();
    assert_eq!(melds.len(), 1);
    assert_eq!(melds[0].kind, ShenyangMahjongMeldKind::PENG);
    assert_eq!(melds[0].tiles, vec![9, 9]);
    assert_eq!(melds[0].from_position, Some(1));
    assert_eq!(state.wall, vec![35]);
    assert!(dispatch.messages.is_empty());
}

#[test]
fn self_gang_rejects_outside_play_phase() {
    let mut state = playable_state();
    state.phase = ShenyangMahjongPhase::Settlement;
    state
        .hands
        .insert(0, vec![3, 3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
    state.wall = vec![35];
    state.last_drawn_tile = Some(31);
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

    assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
    assert_eq!(state.hands.get(&0), Some(&original_hand));
    assert!(state.melds.get(&0).is_none_or(Vec::is_empty));
    assert_eq!(state.wall, vec![35]);
    assert!(dispatch.messages.is_empty());
}

#[test]
fn self_gang_rejects_public_fifth_copy() {
    let mut concealed_gang_state = playable_state();
    concealed_gang_state
        .hands
        .insert(0, vec![3, 3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
    concealed_gang_state.discards.insert(1, vec![3]);
    concealed_gang_state.wall = vec![35];
    concealed_gang_state.last_drawn_tile = Some(3);
    let concealed_hand = concealed_gang_state.hands.get(&0).cloned().unwrap();
    let mut concealed_dispatch = Dispatch::default();

    assert_eq!(known_tile_count(&concealed_gang_state, 3), 5);
    assert!(!can_self_gang(&concealed_gang_state, 0, 3));
    assert!(!perform_self_gang(
        &RoomService::default(),
        "room",
        &mut concealed_gang_state,
        &HashMap::new(),
        &mut concealed_dispatch,
        0,
        3,
    ));
    assert_eq!(concealed_gang_state.hands.get(&0), Some(&concealed_hand));
    assert!(concealed_gang_state.melds.get(&0).is_none_or(Vec::is_empty));
    assert_eq!(concealed_gang_state.wall, vec![35]);
    assert!(concealed_dispatch.messages.is_empty());

    let mut added_gang_state = playable_state();
    added_gang_state
        .hands
        .insert(0, vec![3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
    added_gang_state.melds.insert(
        0,
        vec![build_meld(
            ShenyangMahjongMeldKind::PENG,
            vec![3, 3, 3],
            Some(2),
        )],
    );
    added_gang_state.discards.insert(1, vec![3]);
    added_gang_state.wall = vec![35];
    added_gang_state.last_drawn_tile = Some(3);
    let added_hand = added_gang_state.hands.get(&0).cloned().unwrap();
    let added_melds = added_gang_state.melds.get(&0).cloned().unwrap();
    let mut added_dispatch = Dispatch::default();

    assert_eq!(known_tile_count(&added_gang_state, 3), 5);
    assert!(!can_self_gang(&added_gang_state, 0, 3));
    assert!(!perform_self_gang(
        &RoomService::default(),
        "room",
        &mut added_gang_state,
        &HashMap::new(),
        &mut added_dispatch,
        0,
        3,
    ));
    assert_eq!(added_gang_state.hands.get(&0), Some(&added_hand));
    let actual_melds = added_gang_state.melds.get(&0).expect("melds should stay");
    assert_eq!(actual_melds.len(), added_melds.len());
    assert_eq!(actual_melds[0].kind, added_melds[0].kind);
    assert_eq!(actual_melds[0].tiles, added_melds[0].tiles);
    assert_eq!(actual_melds[0].from_position, added_melds[0].from_position);
    assert_eq!(added_gang_state.wall, vec![35]);
    assert!(added_dispatch.messages.is_empty());
}

#[test]
fn self_gang_rejects_unrelated_invalid_hand_tile() {
    let mut state = playable_state();
    state
        .hands
        .insert(0, vec![3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 99]);
    state.melds.insert(
        0,
        vec![build_meld(
            ShenyangMahjongMeldKind::PENG,
            vec![3, 3, 3],
            Some(2),
        )],
    );
    state.wall = vec![35];
    state.last_drawn_tile = Some(3);
    let original_hand = state.hands.get(&0).cloned().unwrap();
    let original_melds = state.melds.get(&0).cloned().unwrap();
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
    let actual_melds = state.melds.get(&0).expect("melds should stay");
    assert_eq!(actual_melds.len(), original_melds.len());
    assert_eq!(actual_melds[0].kind, original_melds[0].kind);
    assert_eq!(actual_melds[0].tiles, original_melds[0].tiles);
    assert_eq!(
        actual_melds[0].from_position,
        original_melds[0].from_position
    );
    assert_eq!(state.wall, vec![35]);
    assert!(dispatch.messages.is_empty());
}

#[test]
fn self_gang_rejects_unrelated_public_fifth_copy() {
    let mut state = playable_state();
    state
        .hands
        .insert(0, vec![3, 3, 3, 3, 4, 5, 6, 9, 11, 12, 13, 21, 22, 23]);
    state.discards.insert(1, vec![9, 9, 9, 9]);
    state.wall = vec![35];
    state.last_drawn_tile = Some(3);
    let original_hand = state.hands.get(&0).cloned().unwrap();
    let mut dispatch = Dispatch::default();

    assert_eq!(known_tile_count(&state, 3), 4);
    assert_eq!(known_tile_count(&state, 9), 5);
    assert!(position_has_impossible_known_tile_count(&state, 0));
    assert!(!can_self_gang(&state, 0, 3));
    assert!(!perform_self_gang(
        &RoomService::default(),
        "room",
        &mut state,
        &HashMap::new(),
        &mut dispatch,
        0,
        3,
    ));

    assert_eq!(state.hands.get(&0), Some(&original_hand));
    assert!(state.melds.get(&0).is_none_or(Vec::is_empty));
    assert_eq!(state.wall, vec![35]);
    assert!(dispatch.messages.is_empty());
}

#[test]
fn self_gang_rejects_when_replacement_tile_is_unavailable() {
    let mut state = playable_state();
    state
        .hands
        .insert(0, vec![3, 3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
    state.wall.clear();
    state.last_drawn_tile = Some(31);
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
    assert!(state.settlement.is_none());
    assert!(dispatch.messages.is_empty());
}

#[test]
fn self_gang_requires_current_position() {
    let mut state = playable_state();
    state.current_position = 1;
    state
        .hands
        .insert(0, vec![3, 3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
    state.last_drawn_tile = Some(3);

    assert!(!can_self_gang(&state, 0, 3));
}

#[test]
fn self_gang_requires_fourteen_virtual_tiles() {
    let mut state = playable_state();
    state.hands.insert(0, vec![3, 3, 3, 3]);
    state.wall = vec![35];
    state.last_drawn_tile = Some(3);
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
    assert_eq!(state.wall, vec![35]);
    assert!(dispatch.messages.is_empty());
}

#[test]
fn self_gang_requires_owned_last_drawn_tile() {
    let mut state = playable_state();
    state.wall = vec![35];
    state
        .hands
        .insert(0, vec![3, 3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
    state.last_drawn_tile = Some(9);
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
    assert_eq!(state.wall, vec![35]);
    assert!(dispatch.messages.is_empty());

    state.last_drawn_tile = Some(31);

    assert!(can_self_gang(&state, 0, 3));
}
