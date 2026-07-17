use std::sync::{Arc, Mutex as StdMutex};

use ws_common::CommonGameState;

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

fn has_room_event(dispatch: &Dispatch, code: WsCode) -> bool {
    dispatch.messages.iter().any(
        |item| matches!(&item.payload, OutboundPayload::Event(event) if event.code == code as i32),
    )
}

fn open_peng_meld(tile: i32, from_position: usize) -> WsShenyangMahjongMeld {
    build_meld(
        ShenyangMahjongMeldKind::PENG,
        vec![tile, tile, tile],
        Some(from_position),
    )
}

fn open_chi_meld(start_tile: i32) -> WsShenyangMahjongMeld {
    build_meld(
        ShenyangMahjongMeldKind::CHI,
        vec![start_tile, start_tile + 1, start_tile + 2],
        Some(0),
    )
}

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

fn play_request(
    action: ShenyangMahjongAction,
    tiles: Vec<i32>,
    target_tile: Option<i32>,
    from_position: Option<usize>,
) -> ClientRequest {
    ClientRequest {
        route: Routes::PLAY as i32,
        data: serde_json::json!({
            "action": action as i32,
            "tiles": tiles,
            "target_tile": target_tile,
            "from_position": from_position,
        }),
    }
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

fn playable_state() -> ShenyangMahjongLoopState {
    let base = Arc::new(StdMutex::new(CommonGameState::default()));
    {
        let mut common = base.lock().unwrap();
        for position in 0..4 {
            common.add_player(position, position as u64 + 1, &format!("P{}", position));
        }
    }
    let mut state = ShenyangMahjongLoopState::new(base);
    state.phase = ShenyangMahjongPhase::Play;
    state.current_position = 0;
    state.dealer_position = 0;
    state
}

fn seven_pairs_ting_hand() -> Vec<i32> {
    vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 12, 21, 21, 31, 32]
}

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
fn enabled_ting_setting_adds_one_fan_before_the_cap() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![2, 3, 5, 6, 7, 11, 12, 13, 21, 22, 23, 31, 31]);
    state.declare_ting(1);
    state.enter_settlement_with_reverse_win(vec![1], Some(0), Some(4), false, false, false, false);
    let settlement = state.settlement.as_ref().expect("settlement");
    let disabled = HashMap::from([("ting_fan".to_owned(), 0)]);
    let enabled = HashMap::from([("ting_fan".to_owned(), 1)]);

    assert_eq!(
        winner_hand_fan_with_configs(&state, settlement, 1, &enabled),
        winner_hand_fan_with_configs(&state, settlement, 1, &disabled) + 1,
    );
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

fn default_configs() -> HashMap<String, i32> {
    HashMap::new()
}

#[test]
fn resolve_claim_window_allows_chi_for_shenyang_rule() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![1, 2, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
    state.discards.insert(0, vec![3]);
    state.wall = vec![36];
    state.current_position = 0;
    state.claim_window = Some(ClaimWindowState {
        tile: 3,
        from_position: 0,
        kind: ClaimWindowKind::Discard,
        eligible_positions: vec![1],
        responses: HashMap::from([(
            1,
            ClaimResponse::Chi {
                consume_tiles: vec![1, 2],
            },
        )]),
    });
    let configs = HashMap::new();
    let mut dispatch = Dispatch::default();

    resolve_claim_window(
        &RoomService::default(),
        "room",
        &mut state,
        &configs,
        &mut dispatch,
    );

    assert!(state.claim_window.is_none());
    assert_eq!(state.current_position, 1);
    assert!(state.discards.get(&0).unwrap().is_empty());
    assert!(!state.hands.get(&1).unwrap().contains(&1));
    assert!(!state.hands.get(&1).unwrap().contains(&2));
    assert!(!state.hands.get(&1).unwrap().contains(&36));
    let meld = state.melds.get(&1).unwrap().first().unwrap();
    assert_eq!(meld.kind, ShenyangMahjongMeldKind::CHI);
    assert_eq!(meld.tiles, vec![1, 2, 3]);
}

#[test]
fn resolve_claim_window_blocks_only_first_chi_when_configured() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![1, 2, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
    state.discards.insert(0, vec![3]);
    state.wall = vec![36];
    state.current_position = 0;
    state.claim_window = Some(ClaimWindowState {
        tile: 3,
        from_position: 0,
        kind: ClaimWindowKind::Discard,
        eligible_positions: vec![1],
        responses: HashMap::from([(
            1,
            ClaimResponse::Chi {
                consume_tiles: vec![1, 2],
            },
        )]),
    });
    let configs = HashMap::from([("allow_first_chi".to_owned(), 0)]);
    let mut dispatch = Dispatch::default();

    resolve_claim_window(
        &RoomService::default(),
        "room",
        &mut state,
        &configs,
        &mut dispatch,
    );

    assert!(state.claim_window.is_none());
    assert_eq!(state.current_position, 1);
    assert_eq!(state.discards.get(&0), Some(&vec![3]));
    assert!(state.melds.get(&1).map(Vec::is_empty).unwrap_or(true));
    assert!(state.hands.get(&1).unwrap().contains(&36));

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
    state.discards.insert(0, vec![3]);
    state.wall = vec![36];
    state.current_position = 0;
    state.claim_window = Some(ClaimWindowState {
        tile: 3,
        from_position: 0,
        kind: ClaimWindowKind::Discard,
        eligible_positions: vec![1],
        responses: HashMap::from([(
            1,
            ClaimResponse::Chi {
                consume_tiles: vec![1, 2],
            },
        )]),
    });

    resolve_claim_window(
        &RoomService::default(),
        "room",
        &mut state,
        &configs,
        &mut dispatch,
    );

    assert!(state.claim_window.is_none());
    assert!(state.discards.get(&0).unwrap().is_empty());
    assert_eq!(state.melds.get(&1).unwrap().len(), 2);
    assert_eq!(
        state.melds.get(&1).unwrap()[1].kind,
        ShenyangMahjongMeldKind::CHI
    );
}

#[test]
fn resolve_claim_window_gang_consumes_three_tiles_and_draws_replacement() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
    state.discards.insert(0, vec![3]);
    state.wall = vec![35];
    state.current_position = 0;
    state.claim_window = Some(ClaimWindowState {
        tile: 3,
        from_position: 0,
        kind: ClaimWindowKind::Discard,
        eligible_positions: vec![1],
        responses: HashMap::from([(1, ClaimResponse::Gang)]),
    });
    let mut dispatch = Dispatch::default();

    resolve_claim_window(
        &RoomService::default(),
        "room",
        &mut state,
        &HashMap::new(),
        &mut dispatch,
    );

    assert!(state.claim_window.is_none());
    assert_eq!(state.current_position, 1);
    assert_eq!(state.wall_count(), 0);
    assert_eq!(
        state
            .hands
            .get(&1)
            .unwrap()
            .iter()
            .filter(|&&tile| tile == 3)
            .count(),
        0,
    );
    assert!(state.hands.get(&1).unwrap().contains(&35));
    assert!(state.discards.get(&0).unwrap().is_empty());

    let meld = state.melds.get(&1).unwrap().first().unwrap();
    assert_eq!(meld.kind, ShenyangMahjongMeldKind::GANG);
    assert_eq!(meld.tiles, vec![3, 3, 3, 3]);
}

#[test]
fn resolve_claim_window_ignores_gang_without_replacement_tile() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
    state.discards.insert(0, vec![3]);
    state.wall.clear();
    state.current_position = 0;
    state.claim_window = Some(ClaimWindowState {
        tile: 3,
        from_position: 0,
        kind: ClaimWindowKind::Discard,
        eligible_positions: vec![1],
        responses: HashMap::from([(1, ClaimResponse::Gang)]),
    });
    let original_hand = state.hands.get(&1).cloned().unwrap();
    let mut dispatch = Dispatch::default();

    resolve_claim_window(
        &RoomService::default(),
        "room",
        &mut state,
        &HashMap::new(),
        &mut dispatch,
    );

    assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
    assert!(state.claim_window.is_none());
    assert_eq!(state.discards.get(&0), Some(&vec![3]));
    assert_eq!(state.hands.get(&1), Some(&original_hand));
    assert!(state.melds.get(&1).is_none_or(Vec::is_empty));
    assert!(
        state
            .settlement
            .as_ref()
            .is_some_and(|settlement| settlement.winner_positions.is_empty())
    );
}

#[test]
fn resolve_claim_window_ignores_illegal_gang_response() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
    state.discards.insert(0, vec![3]);
    state.wall = vec![36];
    state.current_position = 0;
    state.claim_window = Some(ClaimWindowState {
        tile: 3,
        from_position: 0,
        kind: ClaimWindowKind::Discard,
        eligible_positions: vec![1],
        responses: HashMap::from([(1, ClaimResponse::Gang)]),
    });
    let mut dispatch = Dispatch::default();

    resolve_claim_window(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
    );

    assert!(state.claim_window.is_none());
    assert_eq!(state.current_position, 1);
    assert_eq!(state.discards.get(&0), Some(&vec![3]));
    assert!(state.melds.get(&1).map(Vec::is_empty).unwrap_or(true));
    assert_eq!(
        state
            .hands
            .get(&1)
            .unwrap()
            .iter()
            .filter(|&&tile| tile == 3)
            .count(),
        2,
    );
    assert!(state.hands.get(&1).unwrap().contains(&36));
}

#[test]
fn resolve_claim_window_ignores_illegal_hu_response() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
    state.discards.insert(0, vec![35]);
    state.wall = vec![36];
    state.current_position = 0;
    state.claim_window = Some(ClaimWindowState {
        tile: 35,
        from_position: 0,
        kind: ClaimWindowKind::Discard,
        eligible_positions: vec![1],
        responses: HashMap::from([(1, ClaimResponse::Hu)]),
    });
    let mut dispatch = Dispatch::default();

    resolve_claim_window(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
    );

    assert!(state.settlement.is_none());
    assert!(state.claim_window.is_none());
    assert_eq!(state.current_position, 1);
    assert_eq!(state.discards.get(&0), Some(&vec![35]));
    assert!(state.hands.get(&1).unwrap().contains(&36));
}

#[test]
fn resolve_claim_window_ignores_illegal_peng_response() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35, 35]);
    state.discards.insert(0, vec![3]);
    state.wall = vec![36];
    state.current_position = 0;
    state.claim_window = Some(ClaimWindowState {
        tile: 3,
        from_position: 0,
        kind: ClaimWindowKind::Discard,
        eligible_positions: vec![1],
        responses: HashMap::from([(1, ClaimResponse::Peng)]),
    });
    let mut dispatch = Dispatch::default();

    resolve_claim_window(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
    );

    assert!(state.claim_window.is_none());
    assert_eq!(state.current_position, 1);
    assert_eq!(state.discards.get(&0), Some(&vec![3]));
    assert!(state.melds.get(&1).map(Vec::is_empty).unwrap_or(true));
    assert_eq!(
        state
            .hands
            .get(&1)
            .unwrap()
            .iter()
            .filter(|&&tile| tile == 3)
            .count(),
        1,
    );
    assert!(state.hands.get(&1).unwrap().contains(&36));
}

#[test]
fn resolve_claim_window_ignores_impossible_fifth_tile_peng_response() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
    state
        .hands
        .insert(2, vec![3, 3, 7, 8, 9, 14, 15, 16, 24, 25, 26, 32, 36]);
    state.discards.insert(0, vec![3]);
    state.wall = vec![37];
    state.current_position = 0;
    state.claim_window = Some(ClaimWindowState {
        tile: 3,
        from_position: 0,
        kind: ClaimWindowKind::Discard,
        eligible_positions: vec![1],
        responses: HashMap::from([(1, ClaimResponse::Peng)]),
    });
    let mut dispatch = Dispatch::default();

    resolve_claim_window(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
    );

    assert!(state.claim_window.is_none());
    assert_eq!(state.current_position, 1);
    assert_eq!(state.discards.get(&0), Some(&vec![3]));
    assert!(state.melds.get(&1).map(Vec::is_empty).unwrap_or(true));
    assert!(state.hands.get(&1).unwrap().contains(&37));
}

#[test]
fn resolve_claim_window_ignores_invalid_chi_sequence() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![1, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35, 35]);
    state.discards.insert(0, vec![3]);
    state.wall = vec![36];
    state.current_position = 0;
    state.claim_window = Some(ClaimWindowState {
        tile: 3,
        from_position: 0,
        kind: ClaimWindowKind::Discard,
        eligible_positions: vec![1],
        responses: HashMap::from([(
            1,
            ClaimResponse::Chi {
                consume_tiles: vec![1, 4],
            },
        )]),
    });
    let mut dispatch = Dispatch::default();

    resolve_claim_window(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
    );

    assert!(state.claim_window.is_none());
    assert_eq!(state.current_position, 1);
    assert_eq!(state.discards.get(&0), Some(&vec![3]));
    assert!(state.melds.get(&1).map(Vec::is_empty).unwrap_or(true));
    assert!(state.hands.get(&1).unwrap().contains(&1));
    assert!(state.hands.get(&1).unwrap().contains(&4));
    assert!(state.hands.get(&1).unwrap().contains(&36));
}

#[test]
fn resolve_claim_window_ignores_meld_responses_without_thirteen_virtual_tiles() {
    for (hand, response) in [
        (vec![3, 3], ClaimResponse::Peng),
        (vec![3, 3, 3], ClaimResponse::Gang),
        (
            vec![1, 2],
            ClaimResponse::Chi {
                consume_tiles: vec![1, 2],
            },
        ),
    ] {
        let mut state = playable_state();
        state.hands.insert(1, hand);
        state.discards.insert(0, vec![3]);
        state.wall = vec![36];
        state.current_position = 0;
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1],
            responses: HashMap::from([(1, response)]),
        });
        let mut dispatch = Dispatch::default();

        resolve_claim_window(
            &RoomService::default(),
            "room",
            &mut state,
            &default_configs(),
            &mut dispatch,
        );

        assert!(state.claim_window.is_none());
        assert_eq!(state.discards.get(&0), Some(&vec![3]));
        assert!(state.melds.get(&1).is_none_or(Vec::is_empty));
    }
}

#[test]
fn resolve_claim_window_ignores_melds_from_impossible_known_tile_state() {
    for (hand, response) in [
        (
            vec![3, 3, 4, 5, 6, 9, 9, 9, 9, 11, 12, 13, 21],
            ClaimResponse::Peng,
        ),
        (
            vec![3, 3, 3, 4, 5, 6, 9, 9, 9, 9, 11, 12, 13],
            ClaimResponse::Gang,
        ),
        (
            vec![1, 2, 4, 5, 6, 9, 9, 9, 9, 11, 12, 13, 21],
            ClaimResponse::Chi {
                consume_tiles: vec![1, 2],
            },
        ),
    ] {
        let mut state = playable_state();
        state.hands.insert(1, hand.clone());
        state.discards.insert(0, vec![3]);
        state.discards.insert(2, vec![9]);
        state.wall = vec![37];
        state.current_position = 0;
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1],
            responses: HashMap::from([(1, response)]),
        });
        let mut dispatch = Dispatch::default();

        assert_eq!(known_tile_count(&state, 9), 5);
        assert!(position_has_impossible_known_tile_count(&state, 1));
        resolve_claim_window(
            &RoomService::default(),
            "room",
            &mut state,
            &default_configs(),
            &mut dispatch,
        );

        assert!(state.claim_window.is_none());
        assert_eq!(state.discards.get(&0), Some(&vec![3]));
        assert!(state.melds.get(&1).is_none_or(Vec::is_empty));
        assert!(
            hand.iter()
                .all(|tile| state.hands.get(&1).unwrap().contains(tile))
        );
    }
}

#[test]
fn resolve_claim_window_ignores_mismatched_source_discard() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![3, 3, 5, 6, 7, 11, 12, 13, 21, 22, 23, 31, 35]);
    state.discards.insert(0, vec![4]);
    state.wall = vec![36];
    state.current_position = 0;
    state.claim_window = Some(ClaimWindowState {
        tile: 3,
        from_position: 0,
        kind: ClaimWindowKind::Discard,
        eligible_positions: vec![1],
        responses: HashMap::from([(1, ClaimResponse::Peng)]),
    });
    let mut dispatch = Dispatch::default();

    resolve_claim_window(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
    );

    assert!(state.claim_window.is_none());
    assert_eq!(state.current_position, 1);
    assert_eq!(state.discards.get(&0), Some(&vec![4]));
    assert!(state.melds.get(&1).is_none_or(Vec::is_empty));
    assert!(state.hands.get(&1).unwrap().contains(&36));
}

#[test]
fn resolve_claim_window_ignores_public_fifth_copy_used_by_hu_winner() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![1, 1, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6]);
    state.discards.insert(0, vec![6]);
    state.discards.insert(2, vec![1]);
    state.wall = vec![36];
    state.current_position = 0;
    state.claim_window = Some(ClaimWindowState {
        tile: 6,
        from_position: 0,
        kind: ClaimWindowKind::Discard,
        eligible_positions: vec![1],
        responses: HashMap::from([(1, ClaimResponse::Hu)]),
    });
    let mut dispatch = Dispatch::default();

    assert_eq!(known_tile_count(&state, 1), 5);
    resolve_claim_window(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
    );

    assert!(state.settlement.is_none());
    assert!(state.claim_window.is_none());
    assert_eq!(state.current_position, 1);
    assert_eq!(state.discards.get(&0), Some(&vec![6]));
    assert!(state.hands.get(&1).unwrap().contains(&36));
}

#[test]
fn resolve_claim_window_ignores_response_from_source_position() {
    let mut state = playable_state();
    state
        .hands
        .insert(0, vec![3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
    state.discards.insert(0, vec![3]);
    state.wall = vec![36];
    state.current_position = 0;
    state.claim_window = Some(ClaimWindowState {
        tile: 3,
        from_position: 0,
        kind: ClaimWindowKind::Discard,
        eligible_positions: vec![0],
        responses: HashMap::from([(0, ClaimResponse::Peng)]),
    });
    let original_hand = state.hands.get(&0).cloned().unwrap();
    let mut dispatch = Dispatch::default();

    resolve_claim_window(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
    );

    assert!(state.claim_window.is_none());
    assert_eq!(state.current_position, 1);
    assert_eq!(state.discards.get(&0), Some(&vec![3]));
    assert_eq!(state.hands.get(&0), Some(&original_hand));
    assert!(state.melds.get(&0).is_none_or(Vec::is_empty));
}

#[test]
fn resolve_claim_window_recovers_from_unknown_source() {
    let mut state = playable_state();
    state.current_position = 0;
    state
        .hands
        .insert(1, vec![3, 3, 5, 6, 7, 11, 12, 13, 21, 22, 23, 31, 35]);
    state.discards.insert(9, vec![3]);
    state.wall = vec![36];
    state.claim_window = Some(ClaimWindowState {
        tile: 3,
        from_position: 9,
        kind: ClaimWindowKind::Discard,
        eligible_positions: vec![1],
        responses: HashMap::from([(1, ClaimResponse::Peng)]),
    });
    let original_hand = state.hands.get(&1).cloned().unwrap();
    let mut dispatch = Dispatch::default();

    resolve_claim_window(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
    );

    assert!(state.claim_window.is_none());
    assert_eq!(state.current_position, 1);
    assert_eq!(state.discards.get(&9), Some(&vec![3]));
    assert!(state.melds.get(&1).is_none_or(Vec::is_empty));
    assert_eq!(state.hands.get(&1).unwrap().len(), original_hand.len() + 1);
    assert!(state.hands.get(&1).unwrap().contains(&36));
}

#[test]
fn resolve_claim_window_rejects_outside_play_phase() {
    let mut state = playable_state();
    state.phase = ShenyangMahjongPhase::Settlement;
    state
        .hands
        .insert(1, vec![3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
    state.discards.insert(0, vec![3]);
    state.wall = vec![36];
    state.claim_window = Some(ClaimWindowState {
        tile: 3,
        from_position: 0,
        kind: ClaimWindowKind::Discard,
        eligible_positions: vec![1],
        responses: HashMap::from([(1, ClaimResponse::Peng)]),
    });
    let original_hand = state.hands.get(&1).cloned().unwrap();
    let mut dispatch = Dispatch::default();

    resolve_claim_window(
        &RoomService::default(),
        "room",
        &mut state,
        &HashMap::new(),
        &mut dispatch,
    );

    assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
    assert_eq!(state.hands.get(&1), Some(&original_hand));
    assert_eq!(state.discards.get(&0), Some(&vec![3]));
    assert!(state.melds.get(&1).is_none_or(Vec::is_empty));
    assert!(
        state.claim_window.as_ref().is_some_and(|window| {
            matches!(window.responses.get(&1), Some(ClaimResponse::Peng))
        })
    );
    assert_eq!(state.wall, vec![36]);
    assert!(dispatch.messages.is_empty());
}

fn response_code(dispatch: &Dispatch, recipient: SessionId, route: Routes) -> Option<i32> {
    dispatch
        .messages
        .iter()
        .find_map(|item| match &item.payload {
            OutboundPayload::Response(RequestResponse::WithData(response))
                if item.recipient == recipient && response.route == route as i32 =>
            {
                Some(response.code as i32)
            }
            OutboundPayload::Response(RequestResponse::WithoutData(response))
                if item.recipient == recipient && response.route == route as i32 =>
            {
                Some(response.code as i32)
            }
            _ => None,
        })
}

#[test]
fn rob_gang_claim_pass_finishes_added_gang_and_draws_replacement() {
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
    state.wall = vec![36];
    state.last_drawn_tile = Some(3);
    state.claim_window = Some(ClaimWindowState {
        tile: 3,
        from_position: 0,
        kind: ClaimWindowKind::RobGang,
        eligible_positions: vec![1],
        responses: HashMap::from([(1, ClaimResponse::Pass)]),
    });
    let mut dispatch = Dispatch::default();

    resolve_claim_window(
        &RoomService::default(),
        "room",
        &mut state,
        &HashMap::new(),
        &mut dispatch,
    );

    assert!(state.claim_window.is_none());
    assert_eq!(state.current_position, 0);
    assert_eq!(state.last_drawn_tile, Some(36));
    assert!(!state.hands.get(&0).unwrap().contains(&3));
    let meld = state.melds.get(&0).unwrap().first().unwrap();
    assert_eq!(meld.kind, ShenyangMahjongMeldKind::GANG);
    assert_eq!(meld.tiles, vec![3, 3, 3, 3]);
}

#[test]
fn rob_gang_hu_ignores_invalid_added_gang_source() {
    let mut state = playable_state();
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
        responses: HashMap::from([(1, ClaimResponse::Hu)]),
    });
    let original_hand = state.hands.get(&0).cloned().unwrap();
    let mut dispatch = Dispatch::default();

    resolve_claim_window(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
    );

    assert!(state.settlement.is_none());
    assert!(state.claim_window.is_none());
    assert_eq!(state.phase, ShenyangMahjongPhase::Play);
    assert_eq!(state.current_position, 0);
    assert_eq!(state.hands.get(&0), Some(&original_hand));
    assert_eq!(state.wall, vec![36]);
}

#[test]
fn rob_gang_hu_rejects_impossible_fifth_tile_response() {
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
        .insert(1, vec![1, 2, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
    state.discards.insert(2, vec![3]);
    state.wall = vec![36];
    state.last_drawn_tile = Some(3);
    state.claim_window = Some(ClaimWindowState {
        tile: 3,
        from_position: 0,
        kind: ClaimWindowKind::RobGang,
        eligible_positions: vec![1],
        responses: HashMap::from([(1, ClaimResponse::Hu)]),
    });
    let mut dispatch = Dispatch::default();

    resolve_claim_window(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
    );

    assert!(state.settlement.is_none());
    assert!(state.claim_window.is_none());
    assert_eq!(state.current_position, 0);
    assert_eq!(state.last_drawn_tile, Some(3));
    assert!(state.hands.get(&0).unwrap().contains(&3));
    assert_eq!(state.wall, vec![36]);
    assert_eq!(
        state.melds.get(&0).unwrap().first().unwrap().kind,
        ShenyangMahjongMeldKind::PENG
    );
}

#[test]
fn rob_gang_hu_respects_shenyang_open_requirement() {
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
        .insert(1, vec![1, 2, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
    state.wall = vec![36];
    state.last_drawn_tile = Some(3);
    state.claim_window = Some(ClaimWindowState {
        tile: 3,
        from_position: 0,
        kind: ClaimWindowKind::RobGang,
        eligible_positions: vec![1],
        responses: HashMap::from([(1, ClaimResponse::Hu)]),
    });
    let mut dispatch = Dispatch::default();

    resolve_claim_window(
        &RoomService::default(),
        "room",
        &mut state,
        &HashMap::new(),
        &mut dispatch,
    );

    assert!(state.settlement.is_none());
    assert!(state.claim_window.is_none());
    assert_eq!(state.current_position, 0);
    assert_eq!(state.last_drawn_tile, Some(36));
    assert_eq!(
        state.melds.get(&0).unwrap().first().unwrap().kind,
        ShenyangMahjongMeldKind::GANG
    );
}

#[test]
fn rob_gang_hu_settles_without_upgrading_peng() {
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
    state.claim_window = Some(ClaimWindowState {
        tile: 3,
        from_position: 0,
        kind: ClaimWindowKind::RobGang,
        eligible_positions: vec![1],
        responses: HashMap::from([(1, ClaimResponse::Hu)]),
    });
    let mut dispatch = Dispatch::default();

    resolve_claim_window(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
    );

    let settlement = state.settlement.as_ref().unwrap();
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
fn concealed_gang_does_not_count_as_open_for_rob_gang() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![1, 2, 11, 12, 13, 21, 22, 23, 35, 35]);
    state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::GANG,
            vec![31, 31, 31, 31],
            None,
        )],
    );
    let default_event = build_rob_gang_claim_window_event(&state, 3, 0, 5, &default_configs());
    let empty_config_event = build_rob_gang_claim_window_event(&state, 3, 0, 5, &HashMap::new());

    assert!(!default_event.eligible_positions.contains(&1));
    assert!(!empty_config_event.eligible_positions.contains(&1));
}

#[test]
fn rob_gang_options_reject_impossible_fifth_tile() {
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
        .insert(1, vec![1, 2, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
    state.discards.insert(2, vec![3]);

    let claim_window = build_rob_gang_claim_window_event(&state, 3, 0, 5, &default_configs());

    assert!(!claim_window.eligible_positions.contains(&1));
    assert!(claim_window.options.is_empty());
}

#[test]
fn rob_gang_options_reject_public_fifth_copy_used_by_winner() {
    let mut state = playable_state();
    state
        .hands
        .insert(0, vec![6, 7, 8, 11, 12, 13, 21, 22, 23, 31, 35]);
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
        .insert(1, vec![1, 1, 1, 1, 2, 3, 7, 8, 11, 12, 13, 35, 35]);
    state.discards.insert(2, vec![1]);

    let claim_window = build_rob_gang_claim_window_event(&state, 6, 0, 5, &default_configs());

    assert_eq!(known_tile_count(&state, 1), 5);
    assert!(!claim_window.eligible_positions.contains(&1));
    assert!(claim_window.options.is_empty());
}

#[test]
fn rob_gang_pass_clears_invalid_added_gang_source() {
    let mut state = playable_state();
    state
        .hands
        .insert(0, vec![3, 4, 5, 6, 7, 8, 9, 11, 12, 13, 21, 22, 23, 31]);
    state.wall = vec![36];
    state.last_drawn_tile = Some(3);
    state.claim_window = Some(ClaimWindowState {
        tile: 3,
        from_position: 0,
        kind: ClaimWindowKind::RobGang,
        eligible_positions: vec![1],
        responses: HashMap::from([(1, ClaimResponse::Pass)]),
    });
    let original_hand = state.hands.get(&0).cloned().unwrap();
    let mut dispatch = Dispatch::default();

    resolve_claim_window(
        &RoomService::default(),
        "room",
        &mut state,
        &default_configs(),
        &mut dispatch,
    );

    assert!(state.claim_window.is_none());
    assert_eq!(state.phase, ShenyangMahjongPhase::Play);
    assert_eq!(state.current_position, 0);
    assert_eq!(state.hands.get(&0), Some(&original_hand));
    assert_eq!(state.wall, vec![36]);
}

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

#[test]
fn settlement_deduplicates_restored_winner_positions() {
    let mut state = playable_state();
    state.dealer_position = 2;
    state
        .hands
        .insert(2, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35]);
    state.enter_settlement(vec![2, 2], None, Some(35), true);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(
        settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &HashMap::new())
            .into_iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, -8), (1, -8), (2, 24), (3, -8)]
    );

    let event = build_settlement_event(&state).expect("settlement event");
    assert_eq!(event.winner_positions, vec![2]);
    assert_eq!(event.winner_details.len(), 1);
    assert_eq!(event.winner_details[0].position, 2);
}

#[test]
fn settlement_event_normalizes_invalid_gang_haidilao_as_draw() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![2, 3, 5, 6, 8, 11, 12, 13, 31, 31, 31]);
    state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::GANG,
            vec![35, 35, 35, 35],
            None,
        )],
    );
    state.wall.clear();
    state.enter_settlement_with_reverse_win(vec![1], None, Some(31), true, false, true, true);

    let event =
        build_settlement_event_with_configs(&state, &default_configs()).expect("settlement event");

    assert!(event.winner_positions.is_empty());
    assert!(event.winner_details.is_empty());
    assert_eq!(event.from_position, None);
    assert_eq!(event.win_tile, None);
    assert!(!event.is_self_draw);
    assert!(!event.is_reverse_win);
    assert!(!event.is_gang_draw);
    assert!(!event.is_haidilao);
}

#[test]
fn settlement_event_normalizes_invalid_reverse_win_as_draw() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![2, 3, 5, 6, 8, 11, 12, 13, 21, 22, 23, 31, 31]);
    state.melds.insert(
        0,
        vec![build_meld(
            ShenyangMahjongMeldKind::PENG,
            vec![4, 4, 4],
            Some(2),
        )],
    );
    state.enter_settlement_with_reverse_win(vec![1], Some(0), Some(4), false, true, false, false);

    let event =
        build_settlement_event_with_configs(&state, &default_configs()).expect("settlement event");

    assert!(event.winner_positions.is_empty());
    assert!(event.winner_details.is_empty());
    assert_eq!(event.from_position, None);
    assert_eq!(event.win_tile, None);
    assert!(!event.is_self_draw);
    assert!(!event.is_reverse_win);
    assert!(!event.is_gang_draw);
    assert!(!event.is_haidilao);
}

#[test]
fn settlement_event_skips_zero_score_winners() {
    let mut state = playable_state();
    state.hands.insert(1, vec![1, 1, 35, 35]);
    state.melds.insert(
        1,
        vec![
            build_meld(ShenyangMahjongMeldKind::PENG, vec![11, 11, 11], Some(0)),
            build_meld(ShenyangMahjongMeldKind::PENG, vec![21, 21, 21], Some(2)),
            build_meld(ShenyangMahjongMeldKind::PENG, vec![31, 31, 31], Some(3)),
        ],
    );
    state
        .hands
        .insert(2, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 35]);
    state.melds.insert(
        2,
        vec![build_meld(
            ShenyangMahjongMeldKind::PENG,
            vec![99, 99, 99],
            Some(0),
        )],
    );
    state.enter_settlement_with_reverse_win(
        vec![1, 2],
        Some(0),
        Some(1),
        false,
        false,
        false,
        false,
    );

    let event = build_settlement_event(&state).expect("settlement event");

    assert_eq!(event.winner_positions, vec![1]);
    assert_eq!(event.winner_details.len(), 1);
    assert_eq!(event.winner_details[0].position, 1);
    assert!(event.winner_details[0].score > 0);
    assert_eq!(
        event
            .score_changes
            .iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, -5), (1, 5), (2, 0), (3, 0)]
    );

    let valid_winner_snapshot = event
        .players
        .iter()
        .find(|player| player.position == 1)
        .expect("valid winner snapshot");
    assert_eq!(
        valid_winner_snapshot
            .hand_tiles
            .iter()
            .filter(|tile| **tile == 1)
            .count(),
        3
    );

    let invalid_winner_snapshot = event
        .players
        .iter()
        .find(|player| player.position == 2)
        .expect("invalid winner snapshot");
    assert_eq!(
        invalid_winner_snapshot
            .hand_tiles
            .iter()
            .filter(|tile| **tile == 1)
            .count(),
        1
    );
}

#[test]
fn settlement_fan_accepts_only_dragon_pair_for_closed_piao() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![1, 1, 1, 11, 11, 11, 21, 21, 21, 31, 31, 31, 35, 35]);
    state.enter_settlement(vec![1], None, None, true);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(
        winner_pattern_with_context(
            state.hands.get(&1).unwrap(),
            &[],
            ShenyangMahjongWinContext::new()
        ),
        ShenyangMahjongWinPattern::PiaoHu
    );
    assert_eq!(winner_hand_fan(&state, settlement, 1), 3);

    state
        .hands
        .insert(1, vec![1, 1, 1, 11, 11, 11, 21, 21, 21, 31, 31, 35, 35, 35]);
    assert_eq!(winner_hand_fan(&state, settlement, 1), 0);
}

#[test]
fn settlement_fan_counts_chi_as_opening_meld() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35]);
    state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::CHI,
            vec![1, 2, 3],
            Some(0),
        )],
    );
    state.enter_settlement(vec![1], Some(0), Some(35), false);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(winner_hand_fan(&state, settlement, 1), 2);
}

#[test]
fn settlement_fan_counts_concealed_dragon_triplet() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![11, 12, 13, 21, 22, 23, 31, 31, 35, 35, 35]);
    state.melds.insert(1, vec![open_chi_meld(1)]);
    state.enter_settlement(vec![1], None, None, true);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(winner_hand_fan(&state, settlement, 1), 2);
}

#[test]
fn settlement_fan_counts_configured_closed_sequence_dragon_pair_win_as_standard() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 35, 35]);
    state.enter_settlement(vec![1], None, None, true);
    let settlement = state.settlement.as_ref().expect("settlement");
    let default_configs = HashMap::new();
    let disabled_configs = HashMap::from([("allow_first_chi".to_owned(), 0)]);

    assert_eq!(
        winner_hand_fan_with_configs(&state, settlement, 1, &default_configs),
        0
    );
    assert_eq!(
        winner_hand_fan_with_configs(&state, settlement, 1, &disabled_configs),
        1
    );
    assert_eq!(
        shenyang_win_pattern(state.hands.get(&1).unwrap(), &[]),
        ShenyangMahjongWinPattern::Standard
    );
}

#[test]
fn settlement_fan_counts_dragon_concealed_gang() {
    let mut state = playable_state();
    state.hands.insert(1, vec![11, 12, 13, 21, 22, 23, 31, 31]);
    state.melds.insert(
        1,
        vec![
            open_chi_meld(4),
            build_meld(ShenyangMahjongMeldKind::GANG, vec![35, 35, 35, 35], None),
        ],
    );
    state.enter_settlement(vec![1], None, None, true);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(winner_hand_fan(&state, settlement, 1), 5);
}

#[test]
fn settlement_fan_counts_dragon_open_gang() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31]);
    state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::GANG,
            vec![35, 35, 35, 35],
            Some(0),
        )],
    );
    state.enter_settlement(vec![1], None, None, true);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(winner_hand_fan(&state, settlement, 1), 3);
}

#[test]
fn settlement_fan_counts_dragon_peng() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31]);
    state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::PENG,
            vec![35, 35, 35],
            Some(0),
        )],
    );
    state.enter_settlement(vec![1], None, None, true);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(winner_hand_fan(&state, settlement, 1), 2);
}

#[test]
fn settlement_fan_counts_four_concealed_dragons_as_triplet_and_four_gui_yi() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 35, 35, 35, 35]);
    state.enter_settlement(vec![1], None, None, true);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(winner_hand_fan(&state, settlement, 1), 6);
}

#[test]
fn settlement_fan_counts_four_gui_yi_across_chi_meld_and_hand() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![2, 2, 2, 11, 12, 13, 21, 22, 23, 35, 35]);
    state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::CHI,
            vec![2, 3, 4],
            Some(0),
        )],
    );
    state.enter_settlement(vec![1], None, None, true);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(winner_hand_fan(&state, settlement, 1), 2);
}

#[test]
fn settlement_fan_counts_four_gui_yi_across_peng_meld_and_hand() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![2, 3, 4, 11, 12, 13, 21, 22, 23, 35, 35]);
    state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::PENG,
            vec![2, 2, 2],
            Some(0),
        )],
    );
    state.enter_settlement(vec![1], None, Some(35), true);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(winner_hand_fan(&state, settlement, 1), 3);
}

#[test]
fn settlement_fan_counts_four_gui_yi_and_single_wait_on_seven_pairs() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![1, 1, 1, 1, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35]);
    state.enter_settlement(vec![1], None, Some(35), true);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(winner_hand_fan(&state, settlement, 1), 6);
}

#[test]
fn settlement_fan_counts_honor_single_wait_once() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35]);
    state.melds.insert(1, vec![open_chi_meld(1)]);
    state.enter_settlement_with_reverse_win(vec![1], Some(0), Some(35), false, false, false, false);
    let settlement = state.settlement.as_ref().expect("settlement");
    let hand_tiles = winner_final_hand_tiles(&state, settlement, 1);
    let melds = state.melds.get(&1).map(Vec::as_slice).unwrap_or(&[]);

    assert!(is_single_wait_win(&hand_tiles, melds, settlement.win_tile));
    assert_eq!(winner_hand_fan(&state, settlement, 1), 2);
}

#[test]
fn settlement_fan_counts_middle_tile_single_wait_on_seven_pairs() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![1, 1, 2, 2, 3, 3, 4, 4, 5, 11, 11, 21, 21]);
    state.enter_settlement_with_reverse_win(vec![1], Some(0), Some(5), false, false, false, false);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(winner_hand_fan(&state, settlement, 1), 5);
    assert_eq!(winner_hand_fan(&state, settlement, 1), 5);
}

#[test]
fn settlement_fan_counts_ordinary_concealed_gang() {
    let mut state = playable_state();
    state.hands.insert(1, vec![11, 12, 13, 21, 22, 23, 31, 31]);
    state.melds.insert(
        1,
        vec![
            open_chi_meld(4),
            build_meld(ShenyangMahjongMeldKind::GANG, vec![2, 2, 2, 2], None),
        ],
    );
    state.enter_settlement(vec![1], None, None, true);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(winner_hand_fan(&state, settlement, 1), 3);
}

#[test]
fn settlement_fan_counts_ordinary_open_gang() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![3, 4, 5, 11, 12, 13, 21, 22, 23, 31, 31]);
    state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::GANG,
            vec![2, 2, 2, 2],
            Some(0),
        )],
    );
    state.enter_settlement(vec![1], None, None, true);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(winner_hand_fan(&state, settlement, 1), 2);
}

#[test]
fn settlement_fan_counts_piao_hu_with_concealed_gang() {
    let mut state = playable_state();
    state.hands.insert(1, vec![35, 35]);
    state.melds.insert(
        1,
        vec![
            build_meld(ShenyangMahjongMeldKind::GANG, vec![1, 1, 1, 1], None),
            build_meld(ShenyangMahjongMeldKind::PENG, vec![11, 11, 11], Some(2)),
            build_meld(ShenyangMahjongMeldKind::PENG, vec![21, 21, 21], Some(3)),
            build_meld(ShenyangMahjongMeldKind::PENG, vec![31, 31, 31], Some(0)),
        ],
    );
    state.enter_settlement(vec![1], None, None, true);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(winner_hand_fan(&state, settlement, 1), 5);
}

#[test]
fn settlement_fan_counts_piao_hu_with_open_gang() {
    let mut state = playable_state();
    state.hands.insert(1, vec![35, 35]);
    state.melds.insert(
        1,
        vec![
            build_meld(ShenyangMahjongMeldKind::GANG, vec![1, 1, 1, 1], Some(0)),
            build_meld(ShenyangMahjongMeldKind::PENG, vec![11, 11, 11], Some(2)),
            build_meld(ShenyangMahjongMeldKind::PENG, vec![21, 21, 21], Some(3)),
            build_meld(ShenyangMahjongMeldKind::PENG, vec![31, 31, 31], Some(0)),
        ],
    );
    state.enter_settlement(vec![1], None, None, true);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(winner_hand_fan(&state, settlement, 1), 4);
}

#[test]
fn settlement_fan_counts_pure_one_suit_with_concealed_gang_and_single_wait() {
    let mut state = playable_state();
    state.hands.insert(1, vec![5, 5, 6, 7, 8, 9, 9, 9]);
    state.melds.insert(
        1,
        vec![
            build_meld(ShenyangMahjongMeldKind::GANG, vec![1, 1, 1, 1], None),
            build_meld(ShenyangMahjongMeldKind::CHI, vec![2, 3, 4], Some(0)),
        ],
    );
    state.enter_settlement(vec![1], None, Some(7), true);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(winner_hand_fan(&state, settlement, 1), 7);
    assert_eq!(winner_hand_fan(&state, settlement, 1), 7);
}

#[test]
fn settlement_fan_counts_shou_ba_yi_for_piao_hu() {
    let mut state = playable_state();
    state.hands.insert(1, vec![35]);
    state.melds.insert(
        1,
        vec![
            build_meld(ShenyangMahjongMeldKind::PENG, vec![1, 1, 1], Some(0)),
            build_meld(ShenyangMahjongMeldKind::PENG, vec![11, 11, 11], Some(2)),
            build_meld(ShenyangMahjongMeldKind::PENG, vec![21, 21, 21], Some(3)),
            build_meld(ShenyangMahjongMeldKind::PENG, vec![31, 31, 31], Some(0)),
        ],
    );
    state.enter_settlement(vec![1], Some(0), Some(35), false);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(winner_hand_fan(&state, settlement, 1), 5);
}

#[test]
fn settlement_fan_counts_single_middle_pair_wait() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![11, 12, 13, 21, 22, 23, 25, 31, 31, 31]);
    state.melds.insert(1, vec![open_chi_meld(1)]);
    state.enter_settlement_with_reverse_win(vec![1], Some(0), Some(25), false, false, false, false);
    let settlement = state.settlement.as_ref().expect("settlement");
    let hand_tiles = winner_final_hand_tiles(&state, settlement, 1);
    let melds = state.melds.get(&1).map(Vec::as_slice).unwrap_or(&[]);

    assert_eq!(hand_tiles, vec![11, 12, 13, 21, 22, 23, 25, 25, 31, 31, 31]);
    assert!(is_single_wait_win(&hand_tiles, melds, settlement.win_tile));
    assert_eq!(winner_hand_fan(&state, settlement, 1), 2);
}

#[test]
fn settlement_fan_counts_terminal_single_wait_once() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![11, 11, 13, 14, 15, 16, 17, 17, 17, 17, 18, 18, 19]);
    state.enter_settlement_with_reverse_win(vec![1], Some(0), Some(11), false, false, false, false);
    let settlement = state.settlement.as_ref().expect("settlement");
    let hand_tiles = winner_final_hand_tiles(&state, settlement, 1);

    assert!(is_single_wait_win(&hand_tiles, &[], settlement.win_tile));
    assert_eq!(winner_hand_fan(&state, settlement, 1), 6);
}

#[test]
fn settlement_fan_counts_terminal_single_wait_when_other_wait_is_discarded_out() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![2, 3, 21, 22, 23, 25, 25, 31, 31, 31]);
    state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::CHI,
            vec![11, 12, 13],
            Some(0),
        )],
    );
    for position in 0..4 {
        state.discards.insert(position, vec![4]);
    }
    state.enter_settlement_with_reverse_win(vec![1], Some(0), Some(1), false, false, false, false);
    let settlement = state.settlement.as_ref().expect("settlement");
    let hand_tiles = winner_final_hand_tiles(&state, settlement, 1);
    let melds = state.melds.get(&1).map(Vec::as_slice).unwrap_or(&[]);
    let public_unavailable = public_unavailable_tiles_for_winner(&state, 1);

    assert!(!is_single_wait_win(&hand_tiles, melds, settlement.win_tile));
    assert_eq!(
        public_unavailable.iter().filter(|tile| **tile == 4).count(),
        4
    );
    assert!(is_single_wait_win_with_known_unavailable_tiles(
        &hand_tiles,
        melds,
        settlement.win_tile,
        &public_unavailable
    ));
    assert_eq!(winner_hand_fan(&state, settlement, 1), 2);
}

#[test]
fn settlement_fan_counts_terminal_single_wait_when_other_wait_is_exhausted() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![2, 3, 11, 12, 13, 21, 22, 23, 35, 35]);
    state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::GANG,
            vec![4, 4, 4, 4],
            Some(0),
        )],
    );
    state.enter_settlement_with_reverse_win(vec![1], Some(0), Some(1), false, false, false, false);
    let settlement = state.settlement.as_ref().expect("settlement");
    let hand_tiles = winner_final_hand_tiles(&state, settlement, 1);
    let melds = state.melds.get(&1).map(Vec::as_slice).unwrap_or(&[]);

    assert!(is_single_wait_win(&hand_tiles, melds, settlement.win_tile));
    assert_eq!(winner_hand_fan(&state, settlement, 1), 3);
}

#[test]
fn settlement_fan_does_not_count_closed_middle_shape_with_multiple_waits() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![6, 7, 7, 8, 9, 15, 15, 15, 22, 22]);
    state.melds.insert(1, vec![open_chi_meld(11)]);
    state.enter_settlement_with_reverse_win(vec![1], Some(0), Some(8), false, false, false, false);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(winner_hand_fan(&state, settlement, 1), 1);
}

#[test]
fn settlement_fan_does_not_count_four_gui_yi_for_gang_meld() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![3, 4, 5, 11, 12, 13, 21, 22, 23, 35, 35]);
    state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::GANG,
            vec![2, 2, 2, 2],
            Some(0),
        )],
    );
    state.enter_settlement(vec![1], None, None, true);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(winner_hand_fan(&state, settlement, 1), 2);
}

#[test]
fn settlement_fan_does_not_count_open_two_sided_wait_as_single_wait() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![2, 3, 5, 6, 7, 21, 22, 23, 35, 35]);
    state.melds.insert(1, vec![open_chi_meld(11)]);
    state.enter_settlement_with_reverse_win(vec![1], Some(0), Some(4), false, false, false, false);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(winner_hand_fan(&state, settlement, 1), 1);
}

#[test]
fn settlement_fan_does_not_count_shou_ba_yi_for_standard_hand() {
    let mut state = playable_state();
    state.hands.insert(1, vec![35]);
    state.melds.insert(
        1,
        vec![
            build_meld(ShenyangMahjongMeldKind::CHI, vec![1, 2, 3], Some(0)),
            build_meld(ShenyangMahjongMeldKind::CHI, vec![11, 12, 13], Some(0)),
            build_meld(ShenyangMahjongMeldKind::CHI, vec![21, 22, 23], Some(0)),
            build_meld(ShenyangMahjongMeldKind::PENG, vec![31, 31, 31], Some(0)),
        ],
    );
    state.enter_settlement(vec![1], Some(0), Some(35), false);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(winner_hand_fan(&state, settlement, 1), 2);
}

#[test]
fn settlement_fan_does_not_count_terminal_triplet_completion_as_single_wait() {
    let mut state = playable_state();
    state.hands.insert(1, vec![1, 1, 4, 5, 6, 7, 8, 9, 21, 21]);
    state.melds.insert(1, vec![open_chi_meld(11)]);
    state.enter_settlement_with_reverse_win(vec![1], Some(0), Some(1), false, false, false, false);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(winner_hand_fan(&state, settlement, 1), 1);
}

#[test]
fn settlement_fan_ignores_gang_draw_flag_on_discard_win() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![2, 3, 11, 12, 13, 31, 31, 31, 35, 35]);
    state.melds.insert(1, vec![open_peng_meld(21, 3)]);
    state.enter_settlement_with_reverse_win(vec![1], Some(0), Some(4), false, false, true, false);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(winner_hand_fan(&state, settlement, 1), 1);
    let event = build_settlement_event_with_configs(&state, &default_configs()).unwrap();
    assert!(!event.is_gang_draw);
    assert!(!event.winner_details[0].is_gang_draw);
}

#[test]
fn settlement_fan_ignores_haidilao_flag_on_discard_win() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![2, 3, 11, 12, 13, 31, 31, 31, 35, 35]);
    state.melds.insert(1, vec![open_peng_meld(21, 3)]);
    state.enter_settlement_with_reverse_win(vec![1], Some(0), Some(4), false, false, false, true);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(winner_hand_fan(&state, settlement, 1), 1);
    let event = build_settlement_event_with_configs(&state, &default_configs()).unwrap();
    assert!(!event.is_haidilao);
    assert!(!event.winner_details[0].is_haidilao);
}

#[test]
fn settlement_fan_ignores_invalid_source_melds_for_single_wait() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![2, 3, 21, 22, 23, 25, 25, 31, 31, 31]);
    state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::CHI,
            vec![11, 12, 13],
            Some(0),
        )],
    );
    state.melds.insert(
        2,
        vec![build_meld(
            ShenyangMahjongMeldKind::GANG,
            vec![4, 4, 4, 4],
            Some(2),
        )],
    );
    state.enter_settlement_with_reverse_win(vec![1], Some(0), Some(1), false, false, false, false);
    let settlement = state.settlement.as_ref().expect("settlement");
    let hand_tiles = winner_final_hand_tiles(&state, settlement, 1);
    let melds = state.melds.get(&1).map(Vec::as_slice).unwrap_or(&[]);
    let public_unavailable = public_unavailable_tiles_for_winner(&state, 1);

    assert_eq!(
        public_unavailable.iter().filter(|tile| **tile == 4).count(),
        0
    );
    assert!(!is_single_wait_win_with_known_unavailable_tiles(
        &hand_tiles,
        melds,
        settlement.win_tile,
        &public_unavailable
    ));
    assert_eq!(winner_hand_fan(&state, settlement, 1), 1);
}

#[test]
fn settlement_fan_ignores_malformed_melds_for_four_gui_yi() {
    assert_eq!(
        four_gui_yi_fan(
            &[2, 2],
            &[build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![2, 2],
                Some(0)
            )]
        ),
        0
    );
    assert_eq!(
        four_gui_yi_fan(
            &[2],
            &[build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![2, 2, 2],
                Some(0)
            )]
        ),
        1
    );
    assert_eq!(
        four_gui_yi_fan(
            &[2],
            &[build_meld(
                ShenyangMahjongMeldKind::CHI,
                vec![2, 2, 2],
                Some(0)
            )]
        ),
        0
    );
    assert_eq!(
        four_gui_yi_fan(
            &[2, 2, 2],
            &[build_meld(
                ShenyangMahjongMeldKind::CHI,
                vec![2, 3, 4],
                Some(0)
            )]
        ),
        1
    );
    assert_eq!(
        four_gui_yi_fan(
            &[99],
            &[build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![99, 99, 99],
                Some(0)
            )]
        ),
        0
    );
    assert_eq!(four_gui_yi_fan(&[99, 99, 99, 99], &[]), 0);
}

#[test]
fn settlement_fan_ignores_reverse_win_flag_on_self_draw() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![2, 3, 4, 11, 12, 13, 31, 31, 31, 35, 35]);
    state.melds.insert(1, vec![open_peng_meld(21, 3)]);
    state.enter_settlement_with_reverse_win(vec![1], Some(0), None, true, true, false, false);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(winner_hand_fan(&state, settlement, 1), 1);
    let event = build_settlement_event_with_configs(&state, &default_configs()).unwrap();
    assert_eq!(event.from_position, None);
    assert!(!event.is_reverse_win);
    assert!(!event.winner_details[0].is_reverse_win);
}

#[test]
fn settlement_fan_rejects_invalid_meld_for_single_wait() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 35]);
    state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::PENG,
            vec![99, 99, 99],
            Some(0),
        )],
    );
    state.enter_settlement_with_reverse_win(vec![1], Some(0), Some(35), false, false, false, false);
    let settlement = state.settlement.as_ref().expect("settlement");
    let hand_tiles = winner_final_hand_tiles(&state, settlement, 1);
    let melds = state.melds.get(&1).map(Vec::as_slice).unwrap_or(&[]);

    assert!(!is_single_wait_win(&hand_tiles, melds, settlement.win_tile));
    assert_eq!(winner_hand_fan(&state, settlement, 1), 0);
}

#[test]
fn settlement_fan_rejects_invalid_tile_melds() {
    let mut invalid_gang_state = playable_state();
    invalid_gang_state
        .hands
        .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31]);
    invalid_gang_state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::GANG,
            vec![99, 99, 99, 99],
            None,
        )],
    );
    invalid_gang_state.enter_settlement(vec![1], None, None, true);
    let invalid_gang_settlement = invalid_gang_state.settlement.as_ref().expect("settlement");

    assert_eq!(
        winner_hand_fan(&invalid_gang_state, invalid_gang_settlement, 1),
        0
    );
}

#[test]
fn settlement_fan_rejects_short_dragon_melds() {
    let mut short_gang_state = playable_state();
    short_gang_state
        .hands
        .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31]);
    short_gang_state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::GANG,
            vec![35, 35, 35],
            None,
        )],
    );
    short_gang_state.enter_settlement(vec![1], None, None, true);
    let short_gang_settlement = short_gang_state.settlement.as_ref().expect("settlement");

    assert_eq!(
        winner_hand_fan(&short_gang_state, short_gang_settlement, 1),
        0
    );

    let mut short_peng_state = playable_state();
    short_peng_state
        .hands
        .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31]);
    short_peng_state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::PENG,
            vec![35, 35],
            Some(0),
        )],
    );
    short_peng_state.enter_settlement(vec![1], None, None, true);
    let short_peng_settlement = short_peng_state.settlement.as_ref().expect("settlement");

    assert_eq!(
        winner_hand_fan(&short_peng_state, short_peng_settlement, 1),
        0
    );
}

#[test]
fn settlement_fan_requires_gang_meld_and_empty_wall_for_draw_bonuses() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![2, 3, 4, 11, 12, 13, 31, 31, 31, 35, 35]);
    state.melds.insert(1, vec![open_peng_meld(21, 3)]);
    state.enter_settlement_with_reverse_win(vec![1], None, None, true, false, true, true);
    let settlement = state.settlement.clone().expect("settlement");

    assert_eq!(winner_hand_fan(&state, &settlement, 1), 2);
    let no_gang_event = build_settlement_event_with_configs(&state, &default_configs()).unwrap();
    assert!(!no_gang_event.is_gang_draw);
    assert!(no_gang_event.is_haidilao);
    assert!(!no_gang_event.winner_details[0].is_gang_draw);
    assert!(no_gang_event.winner_details[0].is_haidilao);

    state.hands.insert(1, vec![11, 12, 13, 31, 31, 31, 35, 35]);
    state.melds.insert(
        1,
        vec![
            open_peng_meld(21, 3),
            build_meld(ShenyangMahjongMeldKind::GANG, vec![2, 2, 2, 2], None),
        ],
    );

    assert_eq!(winner_hand_fan(&state, &settlement, 1), 5);
    let valid_event = build_settlement_event_with_configs(&state, &default_configs()).unwrap();
    assert!(valid_event.is_gang_draw);
    assert!(valid_event.is_haidilao);
    assert!(valid_event.winner_details[0].is_gang_draw);
    assert!(valid_event.winner_details[0].is_haidilao);

    state.wall = vec![35];

    assert_eq!(winner_hand_fan(&state, &settlement, 1), 4);
    let nonempty_wall_event =
        build_settlement_event_with_configs(&state, &default_configs()).unwrap();
    assert!(nonempty_wall_event.is_gang_draw);
    assert!(!nonempty_wall_event.is_haidilao);
    assert!(nonempty_wall_event.winner_details[0].is_gang_draw);
    assert!(!nonempty_wall_event.winner_details[0].is_haidilao);
}

#[test]
fn settlement_fan_requires_open_peng_source_for_rob_gang() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![2, 3, 11, 12, 13, 31, 31, 31, 35, 35]);
    state.melds.insert(1, vec![open_peng_meld(21, 3)]);
    state.enter_settlement_with_reverse_win(vec![1], Some(0), Some(4), false, true, false, false);
    let settlement = state.settlement.clone().expect("settlement");

    assert_eq!(winner_hand_fan(&state, &settlement, 1), 1);
    let invalid_event = build_settlement_event_with_configs(&state, &default_configs()).unwrap();
    assert!(!invalid_event.is_reverse_win);
    assert!(!invalid_event.winner_details[0].is_reverse_win);

    state.melds.insert(
        0,
        vec![build_meld(
            ShenyangMahjongMeldKind::PENG,
            vec![4, 4, 4],
            Some(2),
        )],
    );

    assert_eq!(winner_hand_fan(&state, &settlement, 1), 2);
    let valid_event = build_settlement_event_with_configs(&state, &default_configs()).unwrap();
    assert!(valid_event.is_reverse_win);
    assert!(valid_event.winner_details[0].is_reverse_win);
    assert!(!valid_event.is_gang_draw);
}

#[test]
fn settlement_fan_requires_win_tile_for_shou_ba_yi() {
    let mut state = playable_state();
    state.hands.insert(1, vec![35, 35]);
    state.melds.insert(
        1,
        vec![
            build_meld(ShenyangMahjongMeldKind::PENG, vec![1, 1, 1], Some(0)),
            build_meld(ShenyangMahjongMeldKind::PENG, vec![11, 11, 11], Some(2)),
            build_meld(ShenyangMahjongMeldKind::PENG, vec![21, 21, 21], Some(3)),
            build_meld(ShenyangMahjongMeldKind::PENG, vec![31, 31, 31], Some(0)),
        ],
    );
    state.enter_settlement(vec![1], None, None, true);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(winner_hand_fan(&state, settlement, 1), 3);
}

#[test]
fn settlement_fan_uses_shenyang_rules_for_single_wait() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35]);
    state.melds.insert(1, vec![open_chi_meld(1)]);
    state.enter_settlement_with_reverse_win(vec![1], Some(0), Some(35), false, false, false, false);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(winner_hand_fan(&state, settlement, 1), 2);
    assert_eq!(winner_hand_fan(&state, settlement, 1), 2);
}

#[test]
fn settlement_rejects_missing_discard_win_tile() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35]);
    state.enter_settlement(vec![1], Some(0), None, false);
    let invalid_settlement = state.settlement.clone().expect("settlement");

    assert_eq!(winner_hand_fan(&state, &invalid_settlement, 1), 0);
    assert!(
        build_settlement_event(&state)
            .expect("settlement event")
            .winner_positions
            .is_empty()
    );

    state.hands.get_mut(&1).unwrap().pop();
    state.settlement.as_mut().unwrap().win_tile = Some(35);
    let valid_settlement = state.settlement.as_ref().expect("settlement");

    assert!(winner_hand_fan(&state, valid_settlement, 1) > 0);
    assert_eq!(
        build_settlement_event(&state)
            .expect("settlement event")
            .winner_positions,
        vec![1]
    );
}

#[test]
fn settlement_rejects_multiple_self_draw_winners() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35]);
    state
        .hands
        .insert(2, vec![3, 3, 4, 4, 13, 13, 14, 14, 23, 23, 24, 24, 35, 35]);
    state.enter_settlement(vec![1, 2], None, Some(35), true);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(
        settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &HashMap::new())
            .into_iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, 0), (1, 0), (2, 0), (3, 0)]
    );
    assert!(
        build_settlement_event(&state)
            .expect("settlement event")
            .winner_positions
            .is_empty()
    );
}

#[test]
fn settlement_rejects_public_fifth_claim_tile() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![4, 5, 11, 12, 13, 21, 22, 23, 35, 35]);
    state.melds.insert(1, vec![open_chi_meld(1)]);
    state.melds.insert(
        2,
        vec![build_meld(
            ShenyangMahjongMeldKind::GANG,
            vec![6, 6, 6, 6],
            None,
        )],
    );
    state.enter_settlement_with_reverse_win(vec![1], Some(0), Some(6), false, false, false, false);
    let settlement = state.settlement.clone().expect("settlement");

    assert_eq!(known_tile_count(&state, 6), 4);
    assert!(!position_has_impossible_known_tile_count(&state, 1));
    assert!(winner_has_impossible_known_tile_count(
        &state,
        &settlement,
        1
    ));
    assert_eq!(winner_hand_fan(&state, &settlement, 1), 0);
    assert_eq!(
        settlement_score_changes_for_state(&state, &[0, 1, 2, 3], &settlement, &HashMap::new())
            .into_iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, 0), (1, 0), (2, 0), (3, 0)]
    );

    state.melds.insert(
        2,
        vec![build_meld(
            ShenyangMahjongMeldKind::PENG,
            vec![6, 6, 6],
            Some(3),
        )],
    );
    state.discards.insert(0, vec![6]);

    assert_eq!(known_tile_count(&state, 6), 4);
    assert!(!winner_has_impossible_known_tile_count(
        &state,
        &settlement,
        1
    ));
    assert!(winner_hand_fan(&state, &settlement, 1) > 0);
}

#[test]
fn settlement_rejects_public_fifth_copy_used_by_self_draw_winner() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![1, 1, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6]);
    state.discards.insert(2, vec![1]);
    state.enter_settlement(vec![1], None, Some(6), true);
    let settlement = state.settlement.clone().expect("settlement");

    assert_eq!(known_tile_count(&state, 1), 5);
    assert!(position_has_impossible_known_tile_count(&state, 1));
    assert!(winner_has_impossible_known_tile_count(
        &state,
        &settlement,
        1
    ));
    assert_eq!(winner_hand_fan(&state, &settlement, 1), 0);
    assert_eq!(
        settlement_score_changes_for_state(&state, &[0, 1, 2, 3], &settlement, &HashMap::new())
            .into_iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, 0), (1, 0), (2, 0), (3, 0)]
    );
    assert!(
        build_settlement_event(&state)
            .expect("settlement event")
            .winner_positions
            .is_empty()
    );

    state.discards.insert(2, vec![9, 9, 9, 9, 9]);

    assert_eq!(known_tile_count(&state, 9), 5);
    assert!(!position_has_impossible_known_tile_count(&state, 1));
    assert!(!winner_has_impossible_known_tile_count(
        &state,
        &settlement,
        1
    ));
    assert!(winner_hand_fan(&state, &settlement, 1) > 0);
    assert_eq!(
        build_settlement_event(&state)
            .expect("settlement event")
            .winner_positions,
        vec![1]
    );
}

#[test]
fn settlement_rejects_unknown_discard_payer() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35]);
    state.enter_settlement(vec![1], Some(9), Some(35), false);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(
        settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &HashMap::new())
            .into_iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, 0), (1, 0), (2, 0), (3, 0)]
    );
    assert!(
        build_settlement_event(&state)
            .expect("settlement event")
            .winner_positions
            .is_empty()
    );
}

#[test]
fn settlement_rejects_unknown_self_draw_winner() {
    let mut state = playable_state();
    state
        .hands
        .insert(9, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35]);
    state.enter_settlement(vec![9], None, Some(35), true);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(
        settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &HashMap::new())
            .into_iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, 0), (1, 0), (2, 0), (3, 0)]
    );
    assert!(
        build_settlement_event(&state)
            .expect("settlement event")
            .winner_positions
            .is_empty()
    );
}

#[test]
fn settlement_rejects_unowned_self_draw_win_tile() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35]);
    state.enter_settlement(vec![1], None, Some(9), true);
    let invalid_settlement = state.settlement.clone().expect("settlement");

    assert_eq!(winner_hand_fan(&state, &invalid_settlement, 1), 0);
    assert!(
        build_settlement_event(&state)
            .expect("settlement event")
            .winner_positions
            .is_empty()
    );

    state.settlement.as_mut().unwrap().win_tile = Some(35);
    let valid_settlement = state.settlement.as_ref().expect("settlement");

    assert!(winner_hand_fan(&state, valid_settlement, 1) > 0);
    assert_eq!(
        build_settlement_event(&state)
            .expect("settlement event")
            .winner_positions,
        vec![1]
    );
}

#[test]
fn settlement_score_adds_closed_fan_when_discard_payer_has_not_opened() {
    let open_non_payer_meld = || vec![open_peng_meld(31, 2)];
    let mut closed_payer_state = playable_state();
    closed_payer_state.dealer_position = 2;
    closed_payer_state.melds.insert(3, open_non_payer_meld());
    closed_payer_state
        .hands
        .insert(1, vec![2, 3, 5, 6, 7, 11, 12, 13, 35, 35]);
    closed_payer_state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::CHI,
            vec![21, 22, 23],
            Some(0),
        )],
    );
    closed_payer_state.enter_settlement_with_reverse_win(
        vec![1],
        Some(0),
        Some(4),
        false,
        false,
        false,
        false,
    );
    let closed_settlement = closed_payer_state.settlement.as_ref().expect("settlement");

    assert_eq!(
        winner_hand_fan(&closed_payer_state, closed_settlement, 1),
        1
    );
    assert_eq!(
        settlement_score_changes_for_state(
            &closed_payer_state,
            &[0, 1, 2, 3],
            closed_settlement,
            &HashMap::new()
        )
        .into_iter()
        .map(|change| (change.position, change.score))
        .collect::<Vec<_>>(),
        vec![(0, -2), (1, 2), (2, 0), (3, 0)]
    );

    for invalid_source in [0, 9] {
        let mut invalid_source_state = playable_state();
        invalid_source_state.dealer_position = 2;
        invalid_source_state.melds.insert(3, open_non_payer_meld());
        invalid_source_state
            .hands
            .insert(1, vec![2, 3, 5, 6, 7, 11, 12, 13, 35, 35]);
        invalid_source_state.melds.insert(
            0,
            vec![build_meld(
                ShenyangMahjongMeldKind::CHI,
                vec![1, 2, 3],
                Some(invalid_source),
            )],
        );
        invalid_source_state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::CHI,
                vec![21, 22, 23],
                Some(0),
            )],
        );
        invalid_source_state.enter_settlement_with_reverse_win(
            vec![1],
            Some(0),
            Some(4),
            false,
            false,
            false,
            false,
        );
        let settlement = invalid_source_state
            .settlement
            .as_ref()
            .expect("settlement");

        assert_eq!(
            settlement_score_changes_for_state(
                &invalid_source_state,
                &[0, 1, 2, 3],
                settlement,
                &HashMap::new()
            )
            .into_iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
            vec![(0, -2), (1, 2), (2, 0), (3, 0)]
        );
    }

    let mut malformed_open_payer_state = playable_state();
    malformed_open_payer_state.dealer_position = 2;
    malformed_open_payer_state
        .melds
        .insert(3, open_non_payer_meld());
    malformed_open_payer_state
        .hands
        .insert(1, vec![2, 3, 5, 6, 7, 11, 12, 13, 35, 35]);
    malformed_open_payer_state.melds.insert(
        0,
        vec![build_meld(
            ShenyangMahjongMeldKind::PENG,
            vec![1, 1],
            Some(1),
        )],
    );
    malformed_open_payer_state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::CHI,
            vec![21, 22, 23],
            Some(0),
        )],
    );
    malformed_open_payer_state.enter_settlement_with_reverse_win(
        vec![1],
        Some(0),
        Some(4),
        false,
        false,
        false,
        false,
    );
    let malformed_open_settlement = malformed_open_payer_state
        .settlement
        .as_ref()
        .expect("settlement");

    assert_eq!(
        settlement_score_changes_for_state(
            &malformed_open_payer_state,
            &[0, 1, 2, 3],
            malformed_open_settlement,
            &HashMap::new()
        )
        .into_iter()
        .map(|change| (change.position, change.score))
        .collect::<Vec<_>>(),
        vec![(0, -2), (1, 2), (2, 0), (3, 0)]
    );

    let mut invalid_tile_open_payer_state = playable_state();
    invalid_tile_open_payer_state.dealer_position = 2;
    invalid_tile_open_payer_state
        .melds
        .insert(3, open_non_payer_meld());
    invalid_tile_open_payer_state
        .hands
        .insert(1, vec![2, 3, 5, 6, 7, 11, 12, 13, 35, 35]);
    invalid_tile_open_payer_state.melds.insert(
        0,
        vec![build_meld(
            ShenyangMahjongMeldKind::PENG,
            vec![99, 99, 99],
            Some(1),
        )],
    );
    invalid_tile_open_payer_state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::CHI,
            vec![21, 22, 23],
            Some(0),
        )],
    );
    invalid_tile_open_payer_state.enter_settlement_with_reverse_win(
        vec![1],
        Some(0),
        Some(4),
        false,
        false,
        false,
        false,
    );
    let invalid_tile_open_settlement = invalid_tile_open_payer_state
        .settlement
        .as_ref()
        .expect("settlement");

    assert_eq!(
        settlement_score_changes_for_state(
            &invalid_tile_open_payer_state,
            &[0, 1, 2, 3],
            invalid_tile_open_settlement,
            &HashMap::new()
        )
        .into_iter()
        .map(|change| (change.position, change.score))
        .collect::<Vec<_>>(),
        vec![(0, -2), (1, 2), (2, 0), (3, 0)]
    );

    let mut open_payer_state = playable_state();
    open_payer_state.dealer_position = 2;
    open_payer_state.melds.insert(3, open_non_payer_meld());
    open_payer_state
        .hands
        .insert(1, vec![2, 3, 5, 6, 7, 11, 12, 13, 35, 35]);
    open_payer_state.melds.insert(
        0,
        vec![build_meld(
            ShenyangMahjongMeldKind::CHI,
            vec![1, 2, 3],
            Some(3),
        )],
    );
    open_payer_state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::CHI,
            vec![21, 22, 23],
            Some(0),
        )],
    );
    open_payer_state.enter_settlement_with_reverse_win(
        vec![1],
        Some(0),
        Some(4),
        false,
        false,
        false,
        false,
    );
    let open_settlement = open_payer_state.settlement.as_ref().expect("settlement");

    assert_eq!(winner_hand_fan(&open_payer_state, open_settlement, 1), 1);
    assert_eq!(
        settlement_score_changes_for_state(
            &open_payer_state,
            &[0, 1, 2, 3],
            open_settlement,
            &HashMap::new()
        )
        .into_iter()
        .map(|change| (change.position, change.score))
        .collect::<Vec<_>>(),
        vec![(0, -1), (1, 1), (2, 0), (3, 0)]
    );
}

#[test]
fn settlement_score_adds_dealer_fan_when_dealer_self_draws() {
    let mut state = playable_state();
    state.dealer_position = 2;
    state
        .hands
        .insert(2, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35]);
    state.enter_settlement(vec![2], None, Some(35), true);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(
        settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &HashMap::new())
            .into_iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, -8), (1, -8), (2, 24), (3, -8)]
    );
}

#[test]
fn settlement_score_adds_dealer_fan_when_payer_is_open_dealer() {
    let mut state = playable_state();
    state.dealer_position = 0;
    state
        .hands
        .insert(1, vec![2, 3, 5, 6, 7, 11, 12, 13, 35, 35]);
    state.melds.insert(
        0,
        vec![build_meld(
            ShenyangMahjongMeldKind::PENG,
            vec![9, 9, 9],
            Some(1),
        )],
    );
    state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::CHI,
            vec![21, 22, 23],
            Some(0),
        )],
    );
    state.enter_settlement_with_reverse_win(vec![1], Some(0), Some(4), false, false, false, false);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(winner_hand_fan(&state, settlement, 1), 1);
    assert_eq!(
        settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &HashMap::new())
            .into_iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, -2), (1, 2), (2, 0), (3, 0)]
    );
}

#[test]
fn settlement_score_adds_dealer_fan_when_winner_is_dealer() {
    let mut state = playable_state();
    state.dealer_position = 0;
    state
        .hands
        .insert(0, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35]);
    state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::PENG,
            vec![31, 31, 31],
            Some(0),
        )],
    );
    state.enter_settlement(vec![0], Some(1), Some(35), false);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(winner_hand_fan(&state, settlement, 0), 5);
    assert_eq!(
        settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &HashMap::new())
            .into_iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, 6), (1, -6), (2, 0), (3, 0)]
    );
}

#[test]
fn settlement_score_adds_payer_state_after_hand_fan_cap() {
    let mut state = playable_state();
    state.dealer_position = 2;
    state.hands.insert(1, vec![35]);
    state.melds.insert(
        1,
        vec![
            build_meld(ShenyangMahjongMeldKind::PENG, vec![1, 1, 1], Some(0)),
            build_meld(ShenyangMahjongMeldKind::PENG, vec![11, 11, 11], Some(2)),
            build_meld(ShenyangMahjongMeldKind::PENG, vec![21, 21, 21], Some(3)),
            build_meld(ShenyangMahjongMeldKind::PENG, vec![31, 31, 31], Some(0)),
        ],
    );
    state.melds.insert(
        2,
        vec![build_meld(
            ShenyangMahjongMeldKind::PENG,
            vec![9, 9, 9],
            Some(0),
        )],
    );
    state.enter_settlement(vec![1], Some(2), Some(35), false);
    let settlement = state.settlement.as_ref().expect("settlement");
    let configs = HashMap::from([("max_fan".to_owned(), 4)]);

    assert_eq!(
        settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &configs)
            .into_iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, 0), (1, 5), (2, -5), (3, 0)]
    );
}

#[test]
fn settlement_score_caps_winner_hand_fan() {
    let mut state = playable_state();
    state.hands.insert(1, vec![35]);
    state.melds.insert(
        1,
        vec![
            build_meld(ShenyangMahjongMeldKind::PENG, vec![1, 1, 1], Some(0)),
            build_meld(ShenyangMahjongMeldKind::PENG, vec![11, 11, 11], Some(2)),
            build_meld(ShenyangMahjongMeldKind::PENG, vec![21, 21, 21], Some(3)),
            build_meld(ShenyangMahjongMeldKind::PENG, vec![31, 31, 31], Some(0)),
        ],
    );
    state.melds.insert(
        2,
        vec![build_meld(
            ShenyangMahjongMeldKind::PENG,
            vec![9, 9, 9],
            Some(0),
        )],
    );
    state.enter_settlement(vec![1], Some(2), Some(35), false);
    let settlement = state.settlement.as_ref().expect("settlement");
    let configs = HashMap::from([("max_fan".to_owned(), 4)]);

    assert_eq!(
        settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &configs)
            .into_iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, 0), (1, 4), (2, -4), (3, 0)]
    );
}

#[test]
fn settlement_score_changes_cover_discard_self_draw_and_draw() {
    assert_eq!(
        settlement_score_changes_for_positions(&[0, 1, 2, 3], &[0, 2], Some(1), false)
            .into_iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, 1), (1, -2), (2, 1), (3, 0)]
    );
    assert_eq!(
        settlement_score_changes_for_positions(&[0, 1, 2, 3], &[2], None, true)
            .into_iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, -1), (1, -1), (2, 3), (3, -1)]
    );
    assert_eq!(
        settlement_score_changes_for_positions(&[0, 1, 2, 3], &[], None, false)
            .into_iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, 0), (1, 0), (2, 0), (3, 0)]
    );
}

#[test]
fn settlement_score_counts_concealed_gang_discard_payer_as_closed() {
    let mut state = playable_state();
    state.dealer_position = 2;
    state
        .hands
        .insert(1, vec![2, 3, 5, 6, 7, 11, 12, 13, 35, 35]);
    state.melds.insert(
        0,
        vec![build_meld(
            ShenyangMahjongMeldKind::GANG,
            vec![31, 31, 31, 31],
            None,
        )],
    );
    state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::CHI,
            vec![21, 22, 23],
            Some(0),
        )],
    );
    state.melds.insert(3, vec![open_peng_meld(34, 2)]);
    state.enter_settlement_with_reverse_win(vec![1], Some(0), Some(4), false, false, false, false);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(winner_hand_fan(&state, settlement, 1), 1);
    assert_eq!(
        settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &HashMap::new())
            .into_iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, -2), (1, 2), (2, 0), (3, 0)]
    );
}

#[test]
fn settlement_score_counts_three_closed_losers_on_discard_win() {
    let mut state = playable_state();
    state.dealer_position = 2;
    state
        .hands
        .insert(1, vec![2, 3, 5, 6, 7, 11, 12, 13, 35, 35]);
    state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::CHI,
            vec![21, 22, 23],
            Some(0),
        )],
    );
    state.enter_settlement(vec![1], Some(0), Some(4), false);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(winner_hand_fan(&state, settlement, 1), 1);
    assert_eq!(
        settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &HashMap::new())
            .into_iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, -3), (1, 3), (2, 0), (3, 0)]
    );
}

#[test]
fn settlement_score_ignores_illegal_winner_hand() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 35]);
    state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::PENG,
            vec![99, 99, 99],
            Some(0),
        )],
    );
    state.enter_settlement_with_reverse_win(vec![1], Some(0), Some(35), false, false, false, false);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(winner_hand_fan(&state, settlement, 1), 0);
    assert_eq!(
        settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &HashMap::new())
            .into_iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, 0), (1, 0), (2, 0), (3, 0)]
    );
}

#[test]
fn settlement_scores_closed_sequence_dragon_pair_winner_after_xi_gang() {
    let mut state = playable_state();
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
    state.enter_settlement(vec![1], None, Some(35), true);
    let settlement = state.settlement.as_ref().expect("settlement");
    let default_configs = HashMap::new();
    let disabled_configs = HashMap::from([("allow_first_chi".to_owned(), 0)]);

    assert!(
        settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &default_configs)
            .iter()
            .all(|change| change.score == 0)
    );
    assert_eq!(
        settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &disabled_configs)
            .into_iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, -6), (1, 16), (2, -5), (3, -5)]
    );
    let event = build_settlement_event_with_configs(&state, &disabled_configs)
        .expect("configured closed win settlement event");
    assert_eq!(event.winner_details.len(), 1);
    assert_eq!(
        event.winner_details[0].pattern,
        ShenyangMahjongWinPattern::Standard
    );
    assert_eq!(event.winner_details[0].score, 16);
}

#[test]
fn settlement_self_draw_counts_all_three_closed_payers() {
    let mut state = playable_state();
    state
        .hands
        .insert(2, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35]);
    state.enter_settlement(vec![2], None, Some(35), true);

    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(
        settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &HashMap::new())
            .into_iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, -8), (1, -7), (2, 22), (3, -7)]
    );
}

#[test]
fn settlement_self_draw_counts_concealed_gang_payer_as_closed() {
    let mut state = playable_state();
    state
        .hands
        .insert(2, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35]);
    state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::GANG,
            vec![31, 31, 31, 31],
            None,
        )],
    );
    state.enter_settlement(vec![2], None, Some(35), true);

    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(
        settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &HashMap::new())
            .into_iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, -8), (1, -7), (2, 22), (3, -7)]
    );
}

#[test]
fn settlement_self_draw_counts_xi_gang_payer_as_closed() {
    let mut state = playable_state();
    state
        .hands
        .insert(2, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35]);
    state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::XI_GANG,
            vec![31, 32, 33, 34],
            None,
        )],
    );
    state.enter_settlement(vec![2], None, Some(35), true);

    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(
        settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &HashMap::new())
            .into_iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, -8), (1, -7), (2, 22), (3, -7)]
    );
}

#[test]
fn settlement_self_draw_treats_chi_only_payer_as_open() {
    let mut state = playable_state();
    state
        .hands
        .insert(2, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35]);
    state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::CHI,
            vec![11, 12, 13],
            Some(0),
        )],
    );
    state.enter_settlement(vec![2], None, Some(35), true);

    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(
        settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &HashMap::new())
            .into_iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, -7), (1, -5), (2, 18), (3, -6)]
    );
}

#[test]
fn settlement_self_draw_uses_single_closed_fan_when_any_payer_opened() {
    let mut state = playable_state();
    state
        .hands
        .insert(2, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35]);
    state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::PENG,
            vec![31, 31, 31],
            Some(0),
        )],
    );
    state.enter_settlement(vec![2], None, Some(35), true);

    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(
        settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &HashMap::new())
            .into_iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, -7), (1, -5), (2, 18), (3, -6)]
    );
}

#[test]
fn settlement_winner_details_describe_piao_hu() {
    let mut state = playable_state();
    state.hands.insert(1, vec![1, 1, 35, 35]);
    state.melds.insert(
        1,
        vec![
            build_meld(ShenyangMahjongMeldKind::PENG, vec![11, 11, 11], Some(0)),
            build_meld(ShenyangMahjongMeldKind::PENG, vec![21, 21, 21], Some(2)),
            build_meld(ShenyangMahjongMeldKind::PENG, vec![31, 31, 31], Some(3)),
        ],
    );
    state.melds.insert(3, vec![open_peng_meld(34, 2)]);
    state.enter_settlement_with_reverse_win(vec![1], Some(0), Some(1), false, false, false, false);

    let event = build_settlement_event_with_configs(&state, &default_configs()).unwrap();

    assert_eq!(event.winner_details.len(), 1);
    assert_eq!(
        event.winner_details[0].pattern,
        ShenyangMahjongWinPattern::PiaoHu
    );
    assert_eq!(event.winner_details[0].score, 5);
}

#[test]
fn settlement_winner_details_describe_pure_one_suit() {
    let mut state = playable_state();
    state.hands.insert(1, vec![1, 2, 3, 4, 5, 6, 7]);
    state.melds.insert(
        1,
        vec![
            build_meld(ShenyangMahjongMeldKind::CHI, vec![2, 3, 4], Some(0)),
            build_meld(ShenyangMahjongMeldKind::PENG, vec![9, 9, 9], Some(2)),
        ],
    );
    state.melds.insert(3, vec![open_peng_meld(34, 2)]);
    state.enter_settlement_with_reverse_win(vec![1], Some(0), Some(7), false, false, false, false);

    let event = build_settlement_event_with_configs(&state, &default_configs()).unwrap();

    assert_eq!(event.winner_details.len(), 1);
    assert_eq!(
        event.winner_details[0].pattern,
        ShenyangMahjongWinPattern::PureOneSuit
    );
    assert_eq!(event.winner_details[0].score, 6);
}

#[test]
fn settlement_winner_details_describe_seven_pairs_self_draw() {
    let mut state = playable_state();
    state
        .hands
        .insert(2, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35]);
    state.enter_settlement(vec![2], None, Some(35), true);

    let event = build_settlement_event(&state).unwrap();

    assert_eq!(event.winner_details.len(), 1);
    assert_eq!(event.winner_details[0].position, 2);
    assert_eq!(
        event.winner_details[0].pattern,
        ShenyangMahjongWinPattern::SevenPairs
    );
    assert!(event.winner_details[0].is_self_draw);
    assert_eq!(event.winner_details[0].score, 22);
    assert_eq!(
        event
            .score_changes
            .iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, -8), (1, -7), (2, 22), (3, -7)]
    );
}

#[test]
fn settlement_winner_details_do_not_describe_sequence_remainder_as_piao_hu() {
    let mut state = playable_state();
    state.dealer_position = 2;
    state.hands.insert(1, vec![1, 1, 2, 3, 35, 35, 35]);
    state.melds.insert(
        1,
        vec![
            build_meld(ShenyangMahjongMeldKind::PENG, vec![11, 11, 11], Some(0)),
            build_meld(ShenyangMahjongMeldKind::PENG, vec![21, 21, 21], Some(2)),
        ],
    );
    state.melds.insert(3, vec![open_peng_meld(34, 2)]);
    state.enter_settlement_with_reverse_win(vec![1], Some(0), Some(4), false, false, false, false);

    let settlement = state.settlement.as_ref().expect("settlement");
    let event = build_settlement_event_with_configs(&state, &default_configs()).unwrap();

    assert_eq!(winner_hand_fan(&state, settlement, 1), 2);
    assert_eq!(event.winner_details.len(), 1);
    assert_eq!(
        event.winner_details[0].pattern,
        ShenyangMahjongWinPattern::Standard
    );
    assert_eq!(event.winner_details[0].score, 3);
}

#[test]
fn settlement_winner_details_include_reverse_win_and_score() {
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

    let event = build_settlement_event_with_configs(&state, &default_configs()).unwrap();

    assert_eq!(event.winner_details.len(), 1);
    assert_eq!(event.winner_details[0].position, 1);
    assert_eq!(
        event.winner_details[0].pattern,
        ShenyangMahjongWinPattern::Standard
    );
    assert!(event.winner_details[0].is_reverse_win);
    assert_eq!(event.winner_details[0].score, 4);
}

#[test]
fn settlement_winner_details_use_shenyang_rules_for_closed_pure_one_suit() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![1, 2, 3, 2, 3, 4, 4, 5, 6, 7, 7, 7, 9]);
    state.melds.insert(3, vec![open_peng_meld(34, 2)]);
    state.enter_settlement_with_reverse_win(vec![1], Some(0), Some(9), false, false, false, false);

    let default_event = build_settlement_event_with_configs(&state, &default_configs()).unwrap();
    let empty_config_event = build_settlement_event_with_configs(&state, &HashMap::new()).unwrap();

    assert_eq!(
        default_event.winner_details[0].pattern,
        ShenyangMahjongWinPattern::PureOneSuit
    );
    assert_eq!(
        empty_config_event.winner_details[0].pattern,
        ShenyangMahjongWinPattern::PureOneSuit
    );
    assert_eq!(
        winner_hand_fan(&state, state.settlement.as_ref().expect("settlement"), 1),
        4
    );
    assert_eq!(empty_config_event.winner_details[0].score, 6);
}

fn setup_request_room() -> (
    RoomService,
    ShenyangMahjongGameHandler,
    String,
    LoopStateHandle,
) {
    setup_request_room_with_configs(serde_json::json!({}))
}

fn setup_request_room_with_configs(
    configs: serde_json::Value,
) -> (
    RoomService,
    ShenyangMahjongGameHandler,
    String,
    LoopStateHandle,
) {
    let mut room_service = RoomService::default();
    for session_id in 1..=4 {
        room_service.connect(session_id);
        let _ = room_service.handle_common_request(
            session_id,
            &ClientRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({
                    "name": format!("P{}", session_id),
                    "password": "mahjong-request-room",
                    "game_id": GameId::SHENYANG_MAHJONG as i32
                }),
            },
            GameId::SHENYANG_MAHJONG,
            build_shenyang_mahjong_settings,
        );
    }
    if configs.as_object().is_some_and(|items| !items.is_empty()) {
        let _ = room_service.handle_common_request(
            1,
            &ClientRequest {
                route: Routes::SETTING as i32,
                data: serde_json::json!({ "current_configs": configs }),
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

    (room_service, handler, room_key, loop_state)
}

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
    assert_eq!(settlement.winner_details[0].score, 4);
    assert_eq!(
        settlement
            .score_changes
            .iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, -4), (1, 4), (2, 0), (3, 0)]
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
