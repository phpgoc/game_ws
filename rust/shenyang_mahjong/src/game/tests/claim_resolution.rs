use super::*;

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
fn rob_gang_hu_allows_configured_closed_sequence_dragon_pair_win() {
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
        .insert(1, vec![1, 2, 4, 5, 6, 11, 12, 13, 21, 22, 23, 35, 35]);
    state.wall = vec![36];
    state.last_drawn_tile = Some(3);
    state.claim_window = Some(ClaimWindowState {
        tile: 3,
        from_position: 0,
        kind: ClaimWindowKind::RobGang,
        eligible_positions: vec![1],
        responses: HashMap::from([(1, ClaimResponse::Hu)]),
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
