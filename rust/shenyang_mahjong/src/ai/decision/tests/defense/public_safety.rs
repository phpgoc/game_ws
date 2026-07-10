use super::*;

#[test]
fn late_defense_can_follow_exposed_terminal_over_live_wind() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().hand_count = 13;
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_peng_meld(9)],
        },
    );
    let hand = vec![2, 4, 6, 8, 9, 12, 14, 16, 18, 22, 24, 26, 28, 31];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(9)
    );
}

#[test]
fn late_defense_avoids_breaking_cold_terminal_pair_against_closed_opponent() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().hand_count = 13;
    let hand = vec![2, 4, 6, 8, 9, 9, 12, 14, 16, 18, 19, 22, 24, 26];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(19)
    );
}

#[test]
fn mid_round_non_dealer_piao_single_wait_prefers_wider_middle_without_wind_extra_fan() {
    let mut table = table_with_discards(1, vec![31]);
    table.wall_count = 32;
    table.seats.get_mut(&0).unwrap().melds = vec![
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(21),
        test_peng_meld(35),
    ];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();

    assert_eq!(remaining_tile_count(&[31], &table, 0, 31), 2);
    assert_eq!(remaining_tile_count(&[5], &table, 0, 5), 3);
    assert!(
        piao_single_wait_tile_score(5, &[5], melds, &table, 0, WIN_RULE_SHENYANG_BASIC)
            > piao_single_wait_tile_score(31, &[31], melds, &table, 0, WIN_RULE_SHENYANG_BASIC)
    );
}

#[test]
fn late_defense_breaks_locked_five_pairs_for_only_public_tile() {
    let mut table = table_with_discards(1, vec![1]);
    table.wall_count = 20;
    let hand = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 14, 21, 21, 31, 35];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(1)
    );
}

#[test]
fn late_defense_preserves_locked_five_pairs_without_public_tile() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 20;
    let hand = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 14, 21, 21, 31, 35];

    assert!(!matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(1 | 2 | 11 | 12 | 21)
    ));
}

#[test]
fn late_defense_locked_five_pairs_follows_public_singleton_without_breaking_pairs() {
    let mut table = table_with_discards(1, vec![5]);
    table.wall_count = 20;
    let hand = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 14, 21, 21, 31, 35];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(5)
    );
}

#[test]
fn mid_round_discard_follows_public_honor_over_live_dragon() {
    let mut table = table_with_discards(1, vec![31]);
    table.wall_count = 46;
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 36];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(31)
    );
}

#[test]
fn mid_round_discard_follows_public_dragon_over_multiple_public_terminal() {
    let mut table = table_with_discards(1, vec![9, 9, 35]);
    table.wall_count = 46;
    let hand = vec![1, 2, 3, 9, 11, 12, 14, 16, 18, 21, 22, 24, 26, 35];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(35)
    );
}

#[test]
fn mid_round_public_honor_stays_safer_than_four_public_middle_tiles() {
    let mut table = table_with_discards(1, vec![14, 14, 31]);
    table.wall_count = 46;
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: vec![14],
            melds: Vec::new(),
        },
    );
    table.seats.insert(
        3,
        AiSeatView {
            position: 3,
            hand_count: 10,
            discards: vec![14],
            melds: Vec::new(),
        },
    );

    assert_eq!(public_discard_count(&table, 14), 4);
    assert_eq!(public_discard_count(&table, 31), 1);
    assert!(
        mid_round_public_discard_bias(&table, 0, 31) > mid_round_public_discard_bias(&table, 0, 14)
    );
}

#[test]
fn public_table_counts_ignore_invalid_discards_and_queries() {
    let mut table = table_with_discards(1, vec![14, 99, 99]);
    table.seats.get_mut(&0).unwrap().discards = vec![14, 99];

    assert_eq!(public_discard_count(&table, 14), 2);
    assert_eq!(public_discard_seat_count(&table, 14), 2);
    assert_eq!(own_previous_discard_count(&table, 0, 14), 1);
    assert_eq!(visible_tile_count(&table, 14), 2);

    assert_eq!(public_discard_count(&table, 99), 0);
    assert_eq!(public_discard_seat_count(&table, 99), 0);
    assert_eq!(own_previous_discard_count(&table, 0, 99), 0);
    assert_eq!(visible_tile_count(&table, 99), 0);
    assert_eq!(remaining_tile_count(&[14, 99], &table, 0, 99), 0);
    assert_eq!(
        remaining_tile_count_with_melds_after_discards(&[14], &[], &table, 0, 99, &[99]),
        0
    );
}

#[test]
fn mid_round_discard_follows_public_middle_before_late_round() {
    let mut table = table_with_discards(1, vec![14]);
    table.wall_count = 55;
    let hand = vec![1, 2, 3, 9, 11, 12, 14, 16, 18, 21, 22, 24, 26, 35];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(14)
    );
}

#[test]
fn mid_round_live_dragon_risk_grows_when_opponents_are_open() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 42;
    let base = mid_round_live_honor_risk_bias(&table, 0, 35, 1);
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(9)];
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_peng_meld(16)],
        },
    );

    assert!(mid_round_live_honor_risk_bias(&table, 0, 35, 1) < base);
}

#[test]
fn mid_round_live_dragon_risk_ignores_concealed_gang_opponent() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 42;
    let base = mid_round_live_honor_risk_bias(&table, 0, 35, 1);

    table.seats.get_mut(&1).unwrap().melds = vec![test_concealed_gang_meld(9)];
    assert_eq!(mid_round_live_honor_risk_bias(&table, 0, 35, 1), base);

    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(9)];
    assert!(mid_round_live_honor_risk_bias(&table, 0, 35, 1) < base);
}

#[test]
fn mid_round_open_dragon_meld_does_not_add_live_dragon_pressure() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 42;
    let base = mid_round_live_honor_risk_bias(&table, 0, 35, 1);

    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(35)];
    assert_eq!(open_opponent_live_dragon_risk(&table, 0, 35), 0.0);
    assert_eq!(mid_round_live_honor_risk_bias(&table, 0, 35, 1), 0.0);

    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(9)];
    assert!(open_opponent_live_dragon_risk(&table, 0, 35) > 0.0);
    assert!(mid_round_live_honor_risk_bias(&table, 0, 35, 1) < base);
}

#[test]
fn mid_round_live_dragon_risk_discounts_exposed_meld_tiles() {
    let mut exposed_table = table_with_discards(1, Vec::new());
    exposed_table.wall_count = 42;
    exposed_table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(9)];
    exposed_table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_peng_meld(35)],
        },
    );

    let mut live_table = exposed_table.clone();
    live_table.seats.get_mut(&2).unwrap().melds = vec![test_peng_meld(16)];

    assert!(live_risk_exposure_scale(&exposed_table, 35) < 1.0);
    assert!(
        open_opponent_live_dragon_risk(&exposed_table, 0, 35)
            < open_opponent_live_dragon_risk(&live_table, 0, 35)
    );
}

#[test]
fn mid_round_live_dragon_risk_ignores_tile_fully_accounted_by_meld_and_own_tile() {
    let mut accounted_table = table_with_discards(1, Vec::new());
    accounted_table.wall_count = 42;
    accounted_table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(35)];

    let mut live_table = accounted_table.clone();
    live_table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(9)];

    assert_eq!(public_discard_count(&accounted_table, 35), 0);
    assert_eq!(exposed_meld_tile_count(&accounted_table, 35), 3);
    assert_eq!(
        mid_round_live_honor_risk_bias(&accounted_table, 0, 35, 1),
        0.0
    );
    assert!(mid_round_live_honor_risk_bias(&live_table, 0, 35, 1) < 0.0);
}

#[test]
fn mid_round_open_honor_meld_tile_is_safer_than_live_dragon() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 42;
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(35)];

    let exposed_dragon_safety = mid_round_open_meld_safety_bias(&table, 35);
    let live_dragon_safety = mid_round_open_meld_safety_bias(&table, 36);
    assert!(exposed_dragon_safety > 0.0);
    assert_eq!(live_dragon_safety, 0.0);

    let exposed_dragon_score =
        exposed_dragon_safety + mid_round_live_honor_risk_bias(&table, 0, 35, 1);
    let live_dragon_score = live_dragon_safety + mid_round_live_honor_risk_bias(&table, 0, 36, 1);
    assert!(exposed_dragon_score > live_dragon_score);
}

#[test]
fn mid_round_discard_avoids_live_dragon_against_open_opponent() {
    let mut table = table_with_discards(1, vec![14]);
    table.wall_count = 42;
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(9)];
    let hand = vec![1, 2, 3, 11, 12, 13, 14, 16, 18, 21, 22, 23, 31, 35];

    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(35)
    );
}

#[test]
fn mid_round_discard_follows_public_middle_over_live_terminal() {
    let mut table = table_with_discards(1, vec![14]);
    table.wall_count = 37;
    let hand = vec![1, 2, 3, 9, 11, 12, 14, 16, 18, 21, 22, 24, 26, 31];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(14)
    );
}

#[test]
fn mid_round_discard_follows_public_middle_over_cold_wind_against_closed_opponent() {
    let mut table = table_with_discards(1, vec![14]);
    table.wall_count = 37;
    table.seats.get_mut(&1).unwrap().hand_count = 13;
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 14, 21, 22, 23, 31];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(14)
    );
}

#[test]
fn mid_round_live_suited_risk_grows_when_opponents_are_open() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 37;
    let hand = vec![1, 2, 3, 9, 11, 12, 14, 16, 18, 21, 22, 24, 26, 31];
    let base = mid_round_live_suited_risk_bias(&hand, &[], &table, 0, 9, 1, WIN_RULE_RELAXED);
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(16)];

    assert!(mid_round_live_suited_risk_bias(&hand, &[], &table, 0, 9, 1, WIN_RULE_RELAXED) < base);
}

#[test]
fn mid_round_live_suited_risk_ignores_concealed_gang_opponent() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 37;
    let hand = vec![1, 2, 3, 9, 11, 12, 14, 16, 18, 21, 22, 24, 26, 31];
    let base = mid_round_live_suited_risk_bias(&hand, &[], &table, 0, 9, 1, WIN_RULE_RELAXED);

    table.seats.get_mut(&1).unwrap().melds = vec![test_concealed_gang_meld(16)];
    assert_eq!(
        mid_round_live_suited_risk_bias(&hand, &[], &table, 0, 9, 1, WIN_RULE_RELAXED),
        base
    );

    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(16)];
    assert!(mid_round_live_suited_risk_bias(&hand, &[], &table, 0, 9, 1, WIN_RULE_RELAXED) < base);
}

#[test]
fn mid_round_open_meld_tile_is_safer_than_live_suited_tile() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 37;
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(14)];
    let hand = vec![1, 2, 3, 9, 11, 12, 14, 16, 18, 21, 22, 24, 26, 28];

    assert!(mid_round_open_meld_safety_bias(&table, 14) > 0.0);
    assert_eq!(
        open_opponent_live_suited_risk(&table, 0, 14),
        0.0,
        "an opponent who already opened this tile should not add live-tile pressure for it"
    );
    assert!(
        mid_round_open_meld_safety_bias(&table, 14) > mid_round_open_meld_safety_bias(&table, 9)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(14)
    );
}

#[test]
fn mid_round_live_suited_risk_discounts_exposed_meld_tiles() {
    let mut exposed_table = table_with_discards(1, Vec::new());
    exposed_table.wall_count = 37;
    exposed_table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(16)];
    exposed_table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_peng_meld(9)],
        },
    );
    let hand = vec![1, 2, 3, 9, 11, 12, 14, 16, 18, 21, 22, 24, 26, 31];

    let mut live_table = exposed_table.clone();
    live_table.seats.get_mut(&2).unwrap().melds = vec![test_peng_meld(35)];

    assert!(live_risk_exposure_scale(&exposed_table, 9) < 1.0);
    assert!(
        mid_round_live_suited_risk_bias(&hand, &[], &exposed_table, 0, 9, 1, WIN_RULE_RELAXED)
            > mid_round_live_suited_risk_bias(&hand, &[], &live_table, 0, 9, 1, WIN_RULE_RELAXED)
    );
}

#[test]
fn mid_round_live_suited_risk_ignores_tile_fully_accounted_by_meld_and_own_tile() {
    let mut accounted_table = table_with_discards(1, Vec::new());
    accounted_table.wall_count = 37;
    accounted_table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(9)];
    let hand = vec![1, 2, 3, 9, 11, 12, 14, 16, 18, 21, 22, 24, 26, 31];

    let mut live_table = accounted_table.clone();
    live_table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(16)];

    assert_eq!(public_discard_count(&accounted_table, 9), 0);
    assert_eq!(exposed_meld_tile_count(&accounted_table, 9), 3);
    assert_eq!(
        mid_round_live_suited_risk_bias(&hand, &[], &accounted_table, 0, 9, 1, WIN_RULE_RELAXED),
        0.0
    );
    assert!(
        mid_round_live_suited_risk_bias(&hand, &[], &live_table, 0, 9, 1, WIN_RULE_RELAXED) < 0.0
    );
}

#[test]
fn mid_round_values_two_open_meld_tiles_over_live_dragon() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 37;
    table.seats.get_mut(&1).unwrap().melds = vec![test_chi_meld(4)];
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_chi_meld(6)],
        },
    );
    let hand = vec![1, 2, 3, 6, 9, 11, 12, 14, 16, 18, 21, 22, 24, 35];

    assert_eq!(open_meld_tile_count(&table, 6), 2);
    assert!(
        mid_round_open_meld_safety_bias(&table, 6)
            + mid_round_live_honor_risk_bias(&table, 0, 6, 1)
            > mid_round_open_meld_safety_bias(&table, 35)
                + mid_round_live_honor_risk_bias(&table, 0, 35, 1)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(6)
    );
}

#[test]
fn open_meld_tile_count_ignores_malformed_meld_tiles() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&1).unwrap().melds = vec![
        WsShenyangMahjongMeld {
            kind: ShenyangMahjongMeldKind::PENG,
            tiles: vec![14, 14],
            from_position: Some(0),
        },
        test_peng_meld(14),
    ];

    assert_eq!(open_meld_tile_count(&table, 14), 3);
}

#[test]
fn open_opponent_exists_ignores_tile_from_its_open_meld() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 37;
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(14)];

    assert!(!open_opponent_exists_for_tile(&table, 0, 14));
    assert!(open_opponent_exists_for_tile(&table, 0, 15));
}

#[test]
fn own_open_live_suited_pressure_ignores_opponent_open_meld_tile() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 37;
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(14)];
    let melds = vec![test_peng_meld(1), test_peng_meld(11)];

    assert_eq!(own_open_live_suited_pressure(&melds, &table, 0, 14, 1), 0.0);
    assert!(own_open_live_suited_pressure(&melds, &table, 0, 15, 1) > 0.0);
}

#[test]
fn own_open_live_suited_pressure_ignores_fully_accounted_tile() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 37;
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(16)];
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_peng_meld(9)],
        },
    );
    let melds = vec![test_peng_meld(1), test_peng_meld(11)];

    assert_eq!(public_discard_count(&table, 9), 0);
    assert_eq!(exposed_meld_tile_count(&table, 9), 3);
    assert_eq!(own_open_live_suited_pressure(&melds, &table, 0, 9, 1), 0.0);
    assert!(own_open_live_suited_pressure(&melds, &table, 0, 15, 1) > 0.0);
}

#[test]
fn own_open_public_safety_starts_after_first_open_meld() {
    let mut table = table_with_discards(1, vec![14]);
    table.wall_count = 37;
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_peng_meld(16)],
        },
    );
    let closed_melds = Vec::new();
    let open_melds = vec![test_peng_meld(1)];

    assert_eq!(
        own_open_public_safety_bias(&closed_melds, &table, 0, 14),
        0.0
    );
    assert!(own_open_public_safety_bias(&open_melds, &table, 0, 14) > 0.0);
}

#[test]
fn mid_round_discard_avoids_live_terminal_against_open_opponent() {
    let mut table = table_with_discards(1, vec![14]);
    table.wall_count = 37;
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(16)];
    let hand = vec![1, 2, 3, 9, 11, 12, 14, 16, 18, 21, 22, 24, 26, 31];

    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(9)
    );
}
