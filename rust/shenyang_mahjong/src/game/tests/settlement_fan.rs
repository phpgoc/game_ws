use super::*;

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
        vec![(0, -256), (1, -256), (2, 768), (3, -256)]
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
fn settlement_event_skips_invalid_winner_without_suppressing_three_closed() {
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
    assert_eq!(event.winner_details[0].score, 64);
    assert_eq!(
        event
            .score_changes
            .iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, -64), (1, 64), (2, 0), (3, 0)]
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
fn settlement_fan_counts_closed_piao_single_wait_after_xi_gang() {
    let mut state = playable_state();
    state
        .hands
        .insert(1, vec![1, 1, 1, 11, 11, 11, 21, 21, 21, 35, 35]);
    state.melds.insert(
        1,
        vec![build_meld(
            ShenyangMahjongMeldKind::XI_GANG,
            vec![35, 36, 37],
            None,
        )],
    );
    state.enter_settlement(vec![1], None, Some(35), true);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert!(!position_has_open_meld(&state, 1));
    assert_eq!(
        winner_pattern_with_context(
            state.hands.get(&1).unwrap(),
            state.melds.get(&1).map(Vec::as_slice).unwrap_or(&[]),
            ShenyangMahjongWinContext::new(),
        ),
        ShenyangMahjongWinPattern::PiaoHu,
    );
    assert_eq!(winner_hand_fan(&state, settlement, 1), 5);
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
fn settlement_fan_counts_four_gui_yi_across_xi_gang_and_hand() {
    let mut state = playable_state();
    state.hands.insert(1, vec![21, 21, 37, 37, 37]);
    state.melds.insert(
        1,
        vec![
            build_meld(ShenyangMahjongMeldKind::XI_GANG, vec![35, 36, 37], None),
            build_meld(ShenyangMahjongMeldKind::CHI, vec![1, 2, 3], Some(0)),
            build_meld(ShenyangMahjongMeldKind::CHI, vec![11, 12, 13], Some(0)),
        ],
    );
    state.last_drawn_tile = Some(21);
    state.enter_settlement(vec![1], None, Some(21), true);
    let settlement = state.settlement.as_ref().expect("settlement");

    assert_eq!(winner_hand_fan(&state, settlement, 1), 5);
    let event = build_settlement_event(&state).expect("settlement event");
    assert_eq!(event.winner_details.len(), 1);
    assert_eq!(event.winner_details[0].score, 512);
    assert_eq!(
        event
            .score_changes
            .iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
        vec![(0, -256), (1, 512), (2, -128), (3, -128)]
    );
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
fn ordinary_self_draw_does_not_add_fan_over_the_same_discard_win() {
    let complete_hand = vec![2, 3, 4, 11, 12, 13, 31, 31, 31, 35, 35];

    let mut self_draw_state = playable_state();
    self_draw_state.hands.insert(1, complete_hand.clone());
    self_draw_state.melds.insert(1, vec![open_peng_meld(21, 3)]);
    self_draw_state.enter_settlement(vec![1], None, Some(4), true);
    let self_draw_fan = winner_hand_fan(
        &self_draw_state,
        self_draw_state.settlement.as_ref().expect("settlement"),
        1,
    );

    let mut discard_state = playable_state();
    let mut waiting_hand = complete_hand;
    waiting_hand.remove(waiting_hand.iter().position(|tile| *tile == 4).unwrap());
    discard_state.hands.insert(1, waiting_hand);
    discard_state.melds.insert(1, vec![open_peng_meld(21, 3)]);
    discard_state.discards.insert(0, vec![4]);
    discard_state.enter_settlement(vec![1], Some(0), Some(4), false);
    let discard_fan = winner_hand_fan(
        &discard_state,
        discard_state.settlement.as_ref().expect("settlement"),
        1,
    );

    assert_eq!(self_draw_fan, 1);
    assert_eq!(discard_fan, 1);
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
