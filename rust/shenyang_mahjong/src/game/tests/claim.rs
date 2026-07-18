use super::*;

#[test]
fn added_gang_opens_rob_gang_claim_window_before_replacement_draw() {
    let mut state = playable_state();
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
    let mut dispatch = Dispatch::default();

    assert!(perform_self_gang(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
        0,
        3,
    ));

    let claim_window = state.claim_window.as_ref().unwrap();
    assert!(matches!(claim_window.kind, ClaimWindowKind::RobGang));
    assert_eq!(claim_window.tile, 3);
    assert_eq!(claim_window.from_position, 0);
    assert_eq!(claim_window.eligible_positions, vec![1]);
    assert_eq!(state.last_drawn_tile, Some(3));
    assert!(state.hands.get(&0).unwrap().contains(&3));
    assert_eq!(
        state.melds.get(&0).unwrap().first().unwrap().kind,
        ShenyangMahjongMeldKind::PENG
    );
}

#[test]
fn added_gang_rejects_concealed_peng_source() {
    let mut state = playable_state();
    state
        .hands
        .insert(0, vec![3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
    state.melds.insert(
        0,
        vec![build_meld(
            ShenyangMahjongMeldKind::PENG,
            vec![3, 3, 3],
            None,
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
        &HashMap::new(),
        &mut dispatch,
        0,
        3,
    ));

    assert_eq!(state.hands.get(&0), Some(&original_hand));
    let melds = state.melds.get(&0).expect("melds should stay");
    assert_eq!(melds.len(), original_melds.len());
    assert_eq!(melds[0].kind, original_melds[0].kind);
    assert_eq!(melds[0].tiles, original_melds[0].tiles);
    assert_eq!(melds[0].from_position, original_melds[0].from_position);
    assert_eq!(state.wall, vec![35]);
    assert!(dispatch.messages.is_empty());
}

#[test]
fn added_gang_rejects_extra_copy_after_peng() {
    let mut state = playable_state();
    state
        .hands
        .insert(0, vec![3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
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
        &HashMap::new(),
        &mut dispatch,
        0,
        3,
    ));

    assert_eq!(state.hands.get(&0), Some(&original_hand));
    let melds = state.melds.get(&0).expect("melds should stay");
    assert_eq!(melds.len(), original_melds.len());
    assert_eq!(melds[0].kind, original_melds[0].kind);
    assert_eq!(melds[0].tiles, original_melds[0].tiles);
    assert_eq!(melds[0].from_position, original_melds[0].from_position);
    assert_eq!(state.wall, vec![35]);
    assert!(dispatch.messages.is_empty());
}

#[test]
fn added_gang_upgrades_peng_and_draws_replacement() {
    let mut state = playable_state();
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
    state.wall = vec![35];
    state.last_drawn_tile = Some(3);
    let mut dispatch = Dispatch::default();

    assert!(can_self_gang(&state, 0, 3));
    assert!(perform_self_gang(
        &RoomService::default(),
        "room",
        &mut state,
        &HashMap::new(),
        &mut dispatch,
        0,
        3,
    ));

    assert_eq!(state.last_drawn_tile, Some(35));
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
    assert_eq!(meld.from_position, Some(2));
}

#[test]
fn claim_options_allow_closed_sequence_dragon_pair_win_when_first_chi_disabled() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 35]);
    let default_configs = HashMap::new();
    let disabled_configs = HashMap::from([("allow_first_chi".to_owned(), 0)]);

    let default_options = build_claim_options(&state, 35, 0, &default_configs);
    let disabled_options = build_claim_options(&state, 35, 0, &disabled_configs);

    assert!(!default_options.iter().any(|option| option.position == 1));
    assert!(
        disabled_options
            .iter()
            .any(|option| option.position == 1 && option.can_hu)
    );
}

#[test]
fn claim_options_allow_closed_pure_one_suit() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![1, 2, 3, 2, 3, 4, 4, 5, 6, 7, 7, 7, 9]);
    let configs = HashMap::new();

    let options = build_claim_options(&state, 9, 0, &configs);

    let player = options
        .iter()
        .find(|option| option.position == 1)
        .expect("closed pure one suit should be allowed");
    assert!(player.can_hu);
}

#[test]
fn claim_options_allow_hu_after_open_meld() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35]);
    state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::PENG,
            vec![1, 1, 1],
            Some(2),
        )],
    );

    let options = build_claim_options(&state, 35, 0, &HashMap::new());
    let player = options
        .iter()
        .find(|option| option.position == 1)
        .expect("open meld player should be able to hu with remaining hand");

    assert!(player.can_hu);
}

#[test]
fn claim_options_allow_open_pure_one_suit() {
    let mut state = playable_state();
    state.hands.insert(1, vec![3, 4, 5, 4, 5, 6, 5, 6, 7, 8]);
    state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::CHI,
            vec![2, 3, 4],
            Some(0),
        )],
    );
    let configs = HashMap::new();

    let options = build_claim_options(&state, 8, 0, &configs);
    let player = options
        .iter()
        .find(|option| option.position == 1)
        .expect("open pure one suit player should be able to hu");

    assert!(player.can_hu);
}

#[test]
fn claim_options_allow_seven_pairs_hu() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35]);

    let options = build_claim_options(&state, 35, 0, &HashMap::new());
    let player = options
        .iter()
        .find(|option| option.position == 1)
        .expect("seven pairs player should be able to hu");

    assert!(player.can_hu);
}

#[test]
fn claim_options_block_only_first_chi_when_configured() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![1, 2, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
    let configs = HashMap::from([("allow_first_chi".to_owned(), 0)]);

    let options = build_claim_options(&state, 3, 0, &configs);

    assert!(
        options
            .iter()
            .all(|option| option.position != 1 || option.chi_options.is_empty())
    );

    state
        .hands
        .insert(1, vec![1, 2, 4, 5, 6, 11, 12, 13, 21, 31]);
    state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::XI_GANG,
            vec![35, 36, 37],
            None,
        )],
    );

    let options = build_claim_options(&state, 3, 0, &configs);
    assert!(
        options
            .iter()
            .all(|option| option.position != 1 || option.chi_options.is_empty())
    );

    state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::GANG,
            vec![9, 9, 9, 9],
            Some(2),
        )],
    );

    let options = build_claim_options(&state, 3, 0, &configs);
    let gang_opened_player = options
        .iter()
        .find(|option| option.position == 1)
        .expect("open gang next player should retain chi options");
    assert!(gang_opened_player.chi_options.contains(&vec![1, 2]));

    state
        .hands
        .insert(1, vec![1, 2, 4, 5, 6, 11, 12, 13, 31, 35]);
    state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::GANG,
            vec![21, 21, 21, 21],
            None,
        )],
    );

    let options = build_claim_options(&state, 3, 0, &configs);
    assert!(
        options
            .iter()
            .all(|option| option.position != 1 || option.chi_options.is_empty())
    );

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

    let options = build_claim_options(&state, 3, 0, &configs);
    let opened_player = options
        .iter()
        .find(|option| option.position == 1)
        .expect("opened next player should retain chi options");
    assert!(opened_player.chi_options.contains(&vec![1, 2]));
}

#[test]
fn claim_options_count_existing_gang_as_three_virtual_tiles() {
    let mut state = playable_state();
    state.discards.insert(0, vec![3]);
    state
        .hands
        .insert(1, vec![3, 3, 4, 5, 6, 12, 13, 21, 22, 23]);
    state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::GANG,
            vec![11, 11, 11, 11],
            Some(2),
        )],
    );

    let options = build_claim_options(&state, 3, 0, &default_configs());
    let option = options
        .iter()
        .find(|option| option.position == 1)
        .expect("existing Gang should count as one virtual set");

    assert!(option.can_peng);
}

#[test]
fn claim_options_do_not_count_concealed_gang_as_open() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35]);
    state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::GANG,
            vec![1, 1, 1, 1],
            None,
        )],
    );
    let configs = HashMap::new();

    let options = build_claim_options(&state, 35, 0, &configs);

    assert!(!options.iter().any(|option| option.position == 1));
}

#[test]
fn claim_options_hide_gang_when_only_impossible_fifth_wall_copy_remains() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
    state.discards.insert(2, vec![9, 9, 9, 9]);
    state.wall = vec![9];

    let options = build_claim_options(&state, 3, 0, &HashMap::new());
    let player = options
        .iter()
        .find(|option| option.position == 1)
        .expect("player should still be able to peng");

    assert!(player.can_peng);
    assert!(!player.can_gang);
}

#[test]
fn claim_options_hide_gang_when_only_invalid_wall_tiles_remain() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
    state.wall = vec![99, -1];

    let options = build_claim_options(&state, 3, 0, &HashMap::new());
    let player = options
        .iter()
        .find(|option| option.position == 1)
        .expect("player should still be able to peng");

    assert!(player.can_peng);
    assert!(!player.can_gang);
}

#[test]
fn claim_options_hide_gang_when_replacement_tile_is_unavailable() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
    state.wall.clear();

    let options = build_claim_options(&state, 3, 0, &HashMap::new());
    let player = options
        .iter()
        .find(|option| option.position == 1)
        .expect("player should still be able to peng");

    assert!(player.can_peng);
    assert!(!player.can_gang);
}

#[test]
fn claim_options_ignore_malformed_melds_for_known_tile_count() {
    let mut state = playable_state();
    state.discards.insert(0, vec![3]);
    state
        .hands
        .insert(1, vec![3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
    state.melds.insert(
        2,
        vec![build_meld(
            ShenyangMahjongMeldKind::CHI,
            vec![3, 3, 3],
            Some(1),
        )],
    );

    let options = build_claim_options(&state, 3, 0, &default_configs());
    let player = options
        .iter()
        .find(|option| option.position == 1)
        .expect("malformed meld should not block legal claim options");

    assert_eq!(known_tile_count(&state, 3), 3);
    assert!(player.can_peng);
}

#[test]
fn claim_options_ignore_melds_with_invalid_sources_for_known_tile_count() {
    let mut state = playable_state();
    state.discards.insert(0, vec![3]);
    state
        .hands
        .insert(1, vec![3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
    state.melds.insert(
        2,
        vec![build_meld(
            ShenyangMahjongMeldKind::PENG,
            vec![3, 3, 3],
            Some(2),
        )],
    );

    let options = build_claim_options(&state, 3, 0, &default_configs());
    let player = options
        .iter()
        .find(|option| option.position == 1)
        .expect("invalid-source meld should not block legal claim options");

    assert_eq!(known_tile_count(&state, 3), 3);
    assert!(player.can_peng);
}

#[test]
fn claim_options_list_chi_for_shenyang_rule() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![1, 2, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
    let configs = HashMap::new();

    let options = build_claim_options(&state, 3, 0, &configs);
    let next_player = options
        .iter()
        .find(|option| option.position == 1)
        .expect("next player should have chi options");

    assert!(next_player.chi_options.contains(&vec![1, 2]));
    assert!(next_player.chi_options.contains(&vec![2, 4]));
}

#[test]
fn claim_options_list_concrete_actions() {
    let mut state = playable_state();
    state.wall = vec![36];
    state.discards.insert(0, vec![3]);
    state
        .hands
        .insert(1, vec![1, 2, 2, 3, 3, 3, 4, 11, 12, 13, 21, 22, 23]);
    state
        .hands
        .insert(2, vec![4, 5, 6, 7, 8, 11, 12, 13, 21, 22, 23, 31, 31]);
    state
        .hands
        .insert(3, vec![1, 5, 7, 9, 11, 13, 15, 17, 21, 23, 25, 31, 35]);

    let options = build_claim_options(&state, 3, 0, &default_configs());
    let next_player = options
        .iter()
        .find(|option| option.position == 1)
        .expect("next player should have claim options");

    assert!(next_player.can_peng);
    assert!(next_player.can_gang);
    assert!(next_player.chi_options.contains(&vec![1, 2]));
    assert!(next_player.chi_options.contains(&vec![2, 4]));
    assert!(!options.iter().any(|option| option.position == 3));
}

#[test]
fn claim_options_reject_impossible_fifth_tile_chi() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![1, 2, 3, 3, 3, 3, 7, 8, 9, 11, 12, 13, 21]);

    let options = build_claim_options(&state, 3, 0, &default_configs());

    assert!(!options.iter().any(|option| option.position == 1));
}

#[test]
fn claim_options_reject_impossible_fifth_tile_claims() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![3, 3, 3, 3, 7, 8, 9, 11, 12, 13, 21, 22, 31]);

    let options = build_claim_options(&state, 3, 0, &default_configs());

    assert!(!options.iter().any(|option| option.position == 1));
}

#[test]
fn claim_options_reject_impossible_table_known_tile_claims() {
    let mut state = playable_state();
    state.discards.insert(0, vec![3]);
    state
        .hands
        .insert(1, vec![3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
    state
        .hands
        .insert(2, vec![3, 3, 7, 8, 9, 14, 15, 16, 24, 25, 26, 32, 36]);

    let options = build_claim_options(&state, 3, 0, &default_configs());

    assert!(known_tile_count(&state, 3) > 4);
    assert!(options.is_empty());
}

#[test]
fn claim_options_reject_melds_from_impossible_known_tile_state() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![3, 3, 3, 4, 5, 6, 9, 9, 9, 9, 11, 12, 13]);
    state.discards.insert(0, vec![3]);
    state.discards.insert(2, vec![9]);
    state.wall = vec![37];

    let options = build_claim_options(&state, 3, 0, &default_configs());

    assert_eq!(known_tile_count(&state, 9), 5);
    assert!(position_has_impossible_known_tile_count(&state, 1));
    assert!(!options.iter().any(|option| option.position == 1));
}

#[test]
fn claim_options_reject_player_with_invalid_hand_tile() {
    let mut state = playable_state();
    state.discards.insert(0, vec![3]);
    state
        .hands
        .insert(1, vec![3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 99]);

    let options = build_claim_options(&state, 3, 0, &default_configs());

    assert!(!options.iter().any(|option| option.position == 1));
}

#[test]
fn claim_options_reject_player_with_malformed_owned_meld() {
    let mut state = playable_state();
    state.discards.insert(0, vec![3]);
    state
        .hands
        .insert(1, vec![3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
    state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::PENG,
            vec![9, 9],
            Some(0),
        )],
    );

    let options = build_claim_options(&state, 3, 0, &default_configs());

    assert!(!options.iter().any(|option| option.position == 1));
}

#[test]
fn claim_options_reject_public_fifth_copy_used_by_winner() {
    let mut state = playable_state();
    state
        .hands
        .insert(2, vec![1, 1, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6]);
    state.discards.insert(0, vec![6]);
    state.discards.insert(3, vec![1]);

    let invalid_options = build_claim_options(&state, 6, 0, &default_configs());

    assert_eq!(known_tile_count(&state, 1), 5);
    assert!(position_has_impossible_known_tile_count(&state, 2));
    assert!(
        !invalid_options
            .iter()
            .any(|option| option.position == 2 && option.can_hu)
    );

    state.discards.insert(3, vec![9, 9, 9, 9, 9]);
    let unrelated_options = build_claim_options(&state, 6, 0, &default_configs());

    assert_eq!(known_tile_count(&state, 9), 5);
    assert!(!position_has_impossible_known_tile_count(&state, 2));
    assert!(
        unrelated_options
            .iter()
            .any(|option| option.position == 2 && option.can_hu)
    );
}

#[test]
fn claim_options_require_thirteen_virtual_tiles_for_melds() {
    let mut state = playable_state();
    state.wall = vec![36];
    state.discards.insert(0, vec![3]);
    state.hands.insert(1, vec![1, 2, 3, 3, 3]);

    let options = build_claim_options(&state, 3, 0, &default_configs());

    assert!(!options.iter().any(|option| option.position == 1));
}

#[test]
fn claim_options_respect_shenyang_win_rule() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35]);
    let configs = HashMap::new();

    let options = build_claim_options(&state, 35, 0, &configs);

    assert!(!options.iter().any(|option| option.position == 1));
}

#[test]
fn claim_window_rejects_impossible_fifth_copy_with_matching_discard() {
    let mut state = playable_state();
    state.current_position = 0;
    state.discards.insert(0, vec![3]);
    state.hands.insert(1, vec![3, 3, 3, 3]);
    let claim_window = ClaimWindowState {
        tile: 3,
        from_position: 0,
        kind: ClaimWindowKind::Discard,
        eligible_positions: vec![1],
        responses: HashMap::new(),
    };

    assert!(has_impossible_known_tile_count(&state, 3));
    assert!(!claim_window_matches_source(&state, &claim_window));
}

#[test]
fn claim_window_rejects_invalid_tile_with_matching_discard() {
    let mut state = playable_state();
    state.current_position = 0;
    state.discards.insert(0, vec![99]);
    let claim_window = ClaimWindowState {
        tile: 99,
        from_position: 0,
        kind: ClaimWindowKind::Discard,
        eligible_positions: vec![1],
        responses: HashMap::new(),
    };

    assert!(!claim_window_matches_source(&state, &claim_window));
}

#[test]
fn claim_window_rejects_malformed_participants() {
    let mut state = playable_state();
    state.current_position = 0;
    state.discards.insert(0, vec![3]);

    for (eligible_positions, responses) in [
        (vec![], HashMap::new()),
        (vec![0], HashMap::new()),
        (vec![1, 1], HashMap::new()),
        (vec![1, 9], HashMap::new()),
        (vec![1], HashMap::from([(2, ClaimResponse::Pass)])),
    ] {
        let claim_window = ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions,
            responses,
        };

        assert!(!claim_window_matches_source(&state, &claim_window));
    }
}

#[test]
fn claim_window_rejects_non_current_source_with_matching_discard() {
    let mut state = playable_state();
    state.current_position = 0;
    state.discards.insert(1, vec![3]);
    let claim_window = ClaimWindowState {
        tile: 3,
        from_position: 1,
        kind: ClaimWindowKind::Discard,
        eligible_positions: vec![2],
        responses: HashMap::new(),
    };

    assert!(!claim_window_matches_source(&state, &claim_window));
}

#[test]
fn claim_window_rejects_unknown_source_with_matching_discard() {
    let mut state = playable_state();
    state.discards.insert(9, vec![3]);
    let claim_window = ClaimWindowState {
        tile: 3,
        from_position: 9,
        kind: ClaimWindowKind::Discard,
        eligible_positions: vec![1],
        responses: HashMap::new(),
    };

    assert!(!claim_window_matches_source(&state, &claim_window));
}

#[test]
fn dragon_xi_gang_is_exposed_without_opening_or_replacement_draw() {
    let mut state = playable_state();
    state.current_position = 1;
    state
        .hands
        .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 35, 35, 35, 36, 37]);
    state.melds.insert(1, Vec::new());
    state.last_drawn_tile = Some(37);
    state.wall = vec![34];
    state.xi_gang_options.insert(1, vec![vec![35, 36, 37]]);
    let configs = HashMap::from([("allow_first_chi".to_owned(), 0)]);
    let mut dispatch = Dispatch::default();

    assert!(perform_xi_gang(
        &RoomService::default(),
        "room",
        &mut state,
        &configs,
        &mut dispatch,
        1,
        &[37, 35, 36],
    ));

    assert_eq!(state.wall, vec![34]);
    assert_eq!(state.last_drawn_tile, Some(37));
    assert_eq!(state.hands.get(&1).unwrap().len(), 11);
    assert_eq!(state.melds.get(&1).unwrap().len(), 1);
    assert_eq!(
        state.melds.get(&1).unwrap()[0].kind,
        ShenyangMahjongMeldKind::XI_GANG
    );
    assert!(!position_has_open_meld(&state, 1));
    assert!(state.xi_gang_options_for_position(1).is_empty());
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
    assert!(!settlement.is_gang_draw);
    assert_eq!(
        winner_hand_fan_with_configs(&state, settlement, 1, &configs),
        2
    );
}

#[test]
fn draw_event_hides_tile_from_other_players() {
    let mut state = playable_state();
    state.xi_gang_options.insert(1, vec![vec![35, 36, 37]]);
    let mut dispatch = Dispatch::default();

    push_draw_events(
        &RoomService::default(),
        "room",
        &state,
        &HashMap::new(),
        &mut dispatch,
        1,
        35,
    );

    assert_eq!(dispatch.messages.len(), 4);
    for message in &dispatch.messages {
        let OutboundPayload::Event(common_event) = &message.payload else {
            panic!("draw delivery should be an event");
        };
        assert_eq!(common_event.code, WsCode::PLAY as i32);
        let event: WsShenyangMahjongPlayEvent =
            serde_json::from_value(common_event.data.clone()).expect("draw event payload");
        assert_eq!(event.action, ShenyangMahjongAction::DRAW);
        assert_eq!(event.position, 1);
        assert_eq!(event.wall_count, state.wall_count() as i32);
        if message.recipient == 2 {
            assert_eq!(event.tiles, vec![35]);
            assert_eq!(event.target_tile, Some(35));
            assert_eq!(event.xi_gang_options, vec![vec![35, 36, 37]]);
        } else {
            assert!(event.tiles.is_empty());
            assert_eq!(event.target_tile, None);
            assert!(event.xi_gang_options.is_empty());
        }
    }
}
