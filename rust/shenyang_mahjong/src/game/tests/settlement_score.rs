use super::*;

#[test]
fn self_draw_converts_each_payers_final_fan_to_score() {
    let mut state = playable_state();
    state.dealer_position = 0;
    state
        .hands
        .insert(3, vec![2, 3, 5, 6, 7, 11, 12, 13, 21, 22, 23, 35, 35, 4]);
    state.enter_settlement(vec![3], None, Some(4), true);
    let settlement = state.settlement.as_ref().expect("settlement");
    let configs = HashMap::from([
        ("allow_first_chi".to_owned(), 0),
        ("max_fan".to_owned(), 50),
    ]);

    assert_eq!(
        winner_hand_fan_with_configs(&state, settlement, 3, &configs),
        1
    );
    assert_eq!(
        settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &configs)
            .into_iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, -16), (1, -8), (2, -8), (3, 32)]
    );
}

#[test]
fn self_draw_broadcasts_exponential_score_changes_to_clients() {
    let (room_service, _handler, room_key, loop_state) =
        setup_request_room_with_configs(serde_json::json!({
            "allow_first_chi": 0,
            "max_fan": 50
        }));
    let configs = room_service.room_configs(&room_key).expect("room configs");
    let mut state = loop_state.lock().unwrap();
    state.phase = ShenyangMahjongPhase::Play;
    state.current_position = 3;
    state.dealer_position = 0;
    state
        .hands
        .insert(3, vec![2, 3, 4, 5, 6, 7, 11, 12, 13, 21, 22, 23, 35, 35]);
    state.wall = vec![31];
    state.last_drawn_tile = Some(4);
    let mut dispatch = Dispatch::default();

    perform_self_draw_hu(
        &room_service,
        &room_key,
        &mut state,
        &configs,
        &mut dispatch,
        3,
    );

    let event = dispatch
        .messages
        .iter()
        .find_map(|message| match &message.payload {
            OutboundPayload::Event(event) if event.code == WsCode::GAME_OVER as i32 => {
                serde_json::from_value::<WsShenyangMahjongSettlementEvent>(event.data.clone()).ok()
            }
            _ => None,
        })
        .expect("self draw should broadcast a settlement event");
    assert_eq!(
        event
            .score_changes
            .iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, -16), (1, -8), (2, -8), (3, 32)]
    );
    assert_eq!(event.winner_details.len(), 1);
    assert_eq!(event.winner_details[0].score, 32);
}

#[test]
fn self_draw_broadcast_caps_six_and_seven_payment_fan_at_fifty() {
    let (room_service, _handler, room_key, loop_state) =
        setup_request_room_with_configs(serde_json::json!({
            "max_fan": 50
        }));
    let configs = room_service.room_configs(&room_key).expect("room configs");
    let mut state = loop_state.lock().unwrap();
    state.phase = ShenyangMahjongPhase::Play;
    state.current_position = 3;
    state.dealer_position = 0;
    state
        .hands
        .insert(3, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35]);
    state.wall = vec![31];
    state.last_drawn_tile = Some(35);
    let mut dispatch = Dispatch::default();

    perform_self_draw_hu(
        &room_service,
        &room_key,
        &mut state,
        &configs,
        &mut dispatch,
        3,
    );

    let event = dispatch
        .messages
        .iter()
        .find_map(|message| match &message.payload {
            OutboundPayload::Event(event) if event.code == WsCode::GAME_OVER as i32 => {
                serde_json::from_value::<WsShenyangMahjongSettlementEvent>(event.data.clone()).ok()
            }
            _ => None,
        })
        .expect("self draw should broadcast a settlement event");
    assert_eq!(event.winner_details.len(), 1);
    assert_eq!(
        event.winner_details[0].pattern,
        ShenyangMahjongWinPattern::SevenPairs
    );
    assert_eq!(event.winner_details[0].score, 150);
    assert_eq!(
        event
            .score_changes
            .iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, -50), (1, -50), (2, -50), (3, 150)]
    );
}

#[test]
fn discard_multi_hu_caps_each_winner_payment_separately() {
    let mut state = playable_state();
    state.dealer_position = 0;
    state
        .hands
        .insert(1, vec![2, 3, 11, 12, 13, 21, 22, 23, 35, 35]);
    state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::CHI,
            vec![6, 7, 8],
            Some(0),
        )],
    );
    state
        .hands
        .insert(2, vec![1, 1, 2, 2, 4, 11, 11, 12, 12, 21, 21, 22, 22]);
    state.discards.insert(0, vec![4]);
    state.enter_settlement(vec![1, 2], Some(0), Some(4), false);
    let settlement = state.settlement.as_ref().expect("settlement");
    let configs = HashMap::from([("max_fan".to_owned(), 50)]);

    assert_eq!(
        winner_hand_fan_with_configs(&state, settlement, 1, &configs),
        1
    );
    assert_eq!(
        winner_hand_fan_with_configs(&state, settlement, 2, &configs),
        5
    );
    assert_eq!(
        settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &configs)
            .into_iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, -58), (1, 8), (2, 50), (3, 0)]
    );

    let event =
        build_settlement_event_with_configs(&state, &configs).expect("multi-hu settlement event");
    assert_eq!(event.winner_positions, vec![1, 2]);
    assert_eq!(
        event
            .winner_details
            .iter()
            .map(|detail| (detail.position, detail.score))
            .collect::<Vec<_>>(),
        vec![(1, 8), (2, 50)]
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
        vec![(0, -4), (1, 4), (2, 0), (3, 0)]
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
            vec![(0, -4), (1, 4), (2, 0), (3, 0)]
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
        vec![(0, -4), (1, 4), (2, 0), (3, 0)]
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
        vec![(0, -4), (1, 4), (2, 0), (3, 0)]
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
        vec![(0, -2), (1, 2), (2, 0), (3, 0)]
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
        vec![(0, -256), (1, -256), (2, 768), (3, -256)]
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
        vec![(0, -4), (1, 4), (2, 0), (3, 0)]
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
        vec![(0, 64), (1, -64), (2, 0), (3, 0)]
    );
}

#[test]
fn settlement_caps_after_adding_dealer_payer_fan() {
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
    let configs = HashMap::from([("max_fan".to_owned(), 50)]);

    assert_eq!(winner_hand_fan(&state, settlement, 1), 5);
    assert_eq!(
        settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &configs)
            .into_iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, 0), (1, 50), (2, -50), (3, 0)]
    );
}

#[test]
fn settlement_treats_max_fan_as_payment_score_cap() {
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

    assert_eq!(winner_hand_fan(&state, settlement, 1), 5);
    assert_eq!(
        settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &configs)
            .into_iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, 0), (1, 4), (2, -4), (3, 0)]
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
        vec![(0, -4), (1, 4), (2, 0), (3, 0)]
    );
}

#[test]
fn settlement_score_counts_xi_gang_discard_payer_as_closed() {
    let mut state = playable_state();
    state.dealer_position = 2;
    state
        .hands
        .insert(1, vec![2, 3, 5, 6, 7, 11, 12, 13, 35, 35]);
    state.melds.insert(
        0,
        vec![build_meld(
            ShenyangMahjongMeldKind::XI_GANG,
            vec![31, 32, 33, 34],
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
    state.melds.insert(3, vec![open_peng_meld(14, 2)]);
    state.enter_settlement_with_reverse_win(vec![1], Some(0), Some(4), false, false, false, false);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(winner_hand_fan(&state, settlement, 1), 1);
    assert_eq!(
        settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &HashMap::new())
            .into_iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, -4), (1, 4), (2, 0), (3, 0)]
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
        vec![(0, -8), (1, 8), (2, 0), (3, 0)]
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
        vec![(0, -64), (1, 128), (2, -32), (3, -32)]
    );
    let event = build_settlement_event_with_configs(&state, &disabled_configs)
        .expect("configured closed win settlement event");
    assert_eq!(event.winner_details.len(), 1);
    assert_eq!(
        event.winner_details[0].pattern,
        ShenyangMahjongWinPattern::Standard
    );
    assert_eq!(event.winner_details[0].score, 128);
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
        vec![(0, -256), (1, -128), (2, 512), (3, -128)]
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
        vec![(0, -256), (1, -128), (2, 512), (3, -128)]
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
        vec![(0, -256), (1, -128), (2, 512), (3, -128)]
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
        vec![(0, -128), (1, -32), (2, 224), (3, -64)]
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
        vec![(0, -128), (1, -32), (2, 224), (3, -64)]
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
    assert_eq!(event.winner_details[0].score, 32);
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
    assert_eq!(event.winner_details[0].score, 64);
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
    assert_eq!(event.winner_details[0].score, 512);
    assert_eq!(
        event
            .score_changes
            .iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, -256), (1, -128), (2, 512), (3, -128)]
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
    assert_eq!(event.winner_details[0].score, 8);
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
    assert_eq!(event.winner_details[0].score, 16);
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
    assert_eq!(empty_config_event.winner_details[0].score, 64);
}
