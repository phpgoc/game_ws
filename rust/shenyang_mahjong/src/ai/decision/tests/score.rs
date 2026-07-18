use super::*;

#[test]
fn capped_ready_score_keeps_wind_shape_as_seven_pairs_tiebreaker() {
    let mut table = table_with_discards(1, Vec::new());
    table.score_cap = Some(16);
    let wind_wait = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 31];
    let middle_wait = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 21, 21, 22, 22];

    assert!(
        ready_tile_score(&wind_wait, &[], &table, 0)
            > ready_tile_score(&middle_wait, &[], &table, 0)
    );
}

#[test]
fn capped_ready_score_prefers_live_middle_over_public_wind_wait() {
    let mut table = table_with_discards(1, vec![31]);
    table.score_cap = Some(16);
    let wind_wait = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 31];
    let middle_wait = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 21, 21, 22, 22];

    assert!(
        ready_tile_score(&middle_wait, &[], &table, 0)
            > ready_tile_score(&wind_wait, &[], &table, 0)
    );
}

#[test]
fn ready_cap_counts_winner_dealer_payment_fan() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_peng_meld(14)],
        },
    );
    table.seats.insert(
        3,
        AiSeatView {
            position: 3,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_peng_meld(24)],
        },
    );
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(4)];
    table.seats.get_mut(&0).unwrap().melds = vec![test_chi_meld(1)];
    table.score_cap = Some(8);
    let hand = vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35];
    let melds = table.seats.get(&0).unwrap().melds.clone();

    table.dealer_position = 3;
    assert!(!ready_hand_visible_fan_reaches_cap(
        &hand, &melds, &table, 0, 8
    ));

    table.dealer_position = 0;
    assert!(ready_hand_visible_fan_reaches_cap(
        &hand, &melds, &table, 0, 8
    ));

    table.score_cap = Some(15);
    table.dealer_position = 3;
    assert!(!ready_visible_fan_exceeds_half_cap(
        &hand, &melds, &table, 0
    ));

    table.dealer_position = 0;
    assert!(ready_visible_fan_exceeds_half_cap(&hand, &melds, &table, 0));
}

#[test]
fn ready_cap_requires_every_potential_payer_to_reach_cap() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 13,
            discards: Vec::new(),
            melds: Vec::new(),
        },
    );
    table.seats.insert(
        3,
        AiSeatView {
            position: 3,
            hand_count: 13,
            discards: Vec::new(),
            melds: Vec::new(),
        },
    );
    table.dealer_position = 3;
    table.score_cap = Some(16);
    table.seats.get_mut(&0).unwrap().melds = vec![test_chi_meld(1)];
    let hand = vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35];
    let melds = table.seats.get(&0).unwrap().melds.clone();

    assert!(ready_hand_visible_fan_reaches_cap(
        &hand, &melds, &table, 0, 16
    ));

    table.seats.get_mut(&2).unwrap().melds = vec![test_peng_meld(14)];
    table.seats.get_mut(&2).unwrap().hand_count = 10;

    assert!(!ready_hand_visible_fan_reaches_cap(
        &hand, &melds, &table, 0, 16
    ));
}

#[test]
fn normal_route_cap_counts_winner_dealer_payment_fan() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_peng_meld(14)],
        },
    );
    table.seats.insert(
        3,
        AiSeatView {
            position: 3,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_peng_meld(24)],
        },
    );
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(4)];
    table.dealer_position = 0;
    table.score_cap = Some(8);
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 11, 21, 35, 35, 35];

    assert!(has_normal_route_foundation(&hand, &[]));
    assert_eq!(estimated_visible_bonus_fan(&hand, &[]), 1);
    assert!(capped_normal_route_visible_fan_reaches_cap(
        &hand,
        &[],
        &table,
        0
    ));

    table.score_cap = Some(15);
    assert!(capped_normal_route_visible_fan_exceeds_half_cap(
        &hand,
        &[],
        &table,
        0
    ));
}

#[test]
fn estimated_fan_counts_honor_single_wait_once() {
    let win_hand = vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35];
    let melds = vec![test_chi_meld(1)];

    assert_eq!(estimated_fan_with_wait(&win_hand, &melds, 35), 2);
}

#[test]
fn estimated_fan_counts_terminal_single_wait_once() {
    let win_hand = vec![11, 11, 14, 15, 15, 16, 16, 17, 17, 17, 17];
    let melds = vec![test_chi_meld(12)];

    assert_eq!(estimated_fan_with_wait(&win_hand, &melds, 11), 6);
}

#[test]
fn estimated_fan_counts_terminal_single_wait_when_public_discards_exhaust_other_wait() {
    let table = table_with_discards(1, vec![4, 4, 4, 4]);
    let hand_after_discard = vec![2, 3, 21, 22, 23, 25, 25, 31, 31, 31];
    let win_hand = vec![1, 2, 3, 21, 22, 23, 25, 25, 31, 31, 31];
    let melds = vec![test_chi_meld(11)];
    let known_unavailable = known_unavailable_tiles_with_simulated_discards(&table, 0, &melds, &[]);

    assert_eq!(estimated_fan_with_wait(&win_hand, &melds, 1), 1);
    assert_eq!(
        estimated_fan_with_known_unavailable_wait(&win_hand, &melds, 1, &known_unavailable,),
        2
    );
    assert!(ready_tile_score(&hand_after_discard, &melds, &table, 0,) > 100.0);
}

#[test]
fn estimated_fan_rejects_invalid_meld_for_single_wait() {
    let win_hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 35, 35];
    let invalid_meld = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![99, 99, 99],
        from_position: Some(1),
    };

    assert_eq!(estimated_fan_with_wait(&win_hand, &[invalid_meld], 35), 0);
}

#[test]
fn estimated_four_gui_yi_ignores_malformed_melds() {
    let short_peng = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![2, 2],
        from_position: Some(1),
    };
    let invalid_chi = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::CHI,
        tiles: vec![2, 2, 2],
        from_position: Some(1),
    };
    let invalid_tile_peng = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![99, 99, 99],
        from_position: Some(1),
    };

    assert_eq!(estimated_four_gui_yi_fan(&[2, 2], &[short_peng]), 0);
    assert_eq!(estimated_four_gui_yi_fan(&[2], &[test_peng_meld(2)]), 1);
    assert_eq!(estimated_four_gui_yi_fan(&[2], &[invalid_chi]), 0);
    assert_eq!(estimated_four_gui_yi_fan(&[99], &[invalid_tile_peng]), 0);
    assert_eq!(estimated_four_gui_yi_fan(&[99, 99, 99, 99], &[]), 0);
    assert_eq!(
        estimated_four_gui_yi_fan(&[2, 2, 2], &[test_chi_meld(2)]),
        1
    );
}

#[test]
fn estimated_meld_fan_ignores_short_dragon_melds() {
    let short_gang = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::GANG,
        tiles: vec![35, 35, 35],
        from_position: None,
    };
    let short_peng = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![35, 35],
        from_position: Some(1),
    };
    let invalid_tile_gang = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::GANG,
        tiles: vec![99, 99, 99, 99],
        from_position: None,
    };

    assert_eq!(estimated_meld_fan(&[short_gang]), 0);
    assert_eq!(estimated_meld_fan(&[short_peng]), 0);
    assert_eq!(estimated_meld_fan(&[invalid_tile_gang]), 0);
}

#[test]
fn estimated_visible_fan_accepts_closed_piao_with_dragon_pair() {
    let closed_triplet_hand = vec![1, 1, 1, 11, 11, 11, 21, 21, 21, 31, 31, 31, 35, 35];

    assert_eq!(
        estimated_visible_fan_without_wait(&closed_triplet_hand, &[]),
        3
    );
    assert_eq!(
        estimated_visible_fan_without_wait(&closed_triplet_hand, &[]),
        3
    );
}

#[test]
fn estimated_fan_counts_xi_gang_for_closed_piao_single_wait() {
    let win_hand = vec![1, 1, 1, 11, 11, 11, 21, 21, 21, 35, 35];
    let melds = vec![WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::XI_GANG,
        tiles: vec![35, 36, 37],
        from_position: None,
    }];

    assert!(!has_open_meld(&melds));
    assert_eq!(estimated_visible_fan_without_wait(&win_hand, &melds), 4);
    assert_eq!(estimated_fan_with_wait(&win_hand, &melds, 35), 5);
}

#[test]
fn estimated_visible_fan_counts_concealed_dragon_triplet() {
    let win_hand = vec![11, 12, 13, 21, 22, 23, 31, 31, 35, 35, 35];
    let melds = vec![test_chi_meld(1)];

    assert_eq!(estimated_visible_fan_without_wait(&win_hand, &melds), 2);
}

#[test]
fn estimated_visible_fan_counts_four_concealed_dragons_as_triplet_and_four_gui_yi() {
    let win_hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 35, 35, 35, 35];

    assert_eq!(estimated_visible_fan_without_wait(&win_hand, &[]), 6);
}

#[test]
fn estimated_visible_fan_counts_four_gui_yi_before_wait_fan() {
    let win_hand = vec![2, 3, 4, 11, 12, 13, 21, 22, 23, 35, 35];
    let melds = vec![test_peng_meld(2)];

    assert_eq!(estimated_visible_fan_without_wait(&win_hand, &melds), 2);
}

#[test]
fn estimated_visible_fan_does_not_add_closed_winner_fan() {
    let closed_pure_one_suit = vec![1, 2, 3, 2, 3, 4, 4, 5, 6, 7, 7, 7, 9, 9];
    let closed_seven_pairs = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35];

    assert_eq!(
        estimated_visible_fan_without_wait(&closed_pure_one_suit, &[]),
        4
    );
    assert_eq!(
        estimated_visible_fan_without_wait(&closed_seven_pairs, &[]),
        4
    );
}

#[test]
fn estimated_visible_fan_does_not_count_piao_shou_ba_yi_without_wait_tile() {
    let win_hand = vec![35, 35];
    let melds = vec![
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(21),
        test_peng_meld(31),
    ];

    assert_eq!(estimated_visible_fan_without_wait(&win_hand, &melds), 3);
    assert_eq!(estimated_fan_with_wait(&win_hand, &melds, 35), 5);
}

#[test]
fn estimated_visible_fan_rejects_closed_piao_with_non_dragon_pair() {
    let closed_triplet_hand = vec![1, 1, 1, 11, 11, 11, 21, 21, 21, 31, 31, 35, 35, 35];

    assert_eq!(
        estimated_visible_fan_without_wait(&closed_triplet_hand, &[]),
        0
    );
}

#[test]
fn estimated_visible_fan_uses_shenyang_rule_for_closed_pure_one_suit() {
    let win_hand = vec![1, 2, 3, 2, 3, 4, 4, 5, 6, 7, 7, 7, 9, 9];

    assert_eq!(estimated_visible_fan_without_wait(&win_hand, &[]), 4);
    assert_eq!(estimated_visible_fan_without_wait(&win_hand, &[]), 4);
}

#[test]
fn fan_wait_bias_counts_middle_tile_seven_pairs_single_wait() {
    let mut table = table_with_discards(1, Vec::new());
    table.score_cap = Some(256);
    let win_hand = vec![1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 11, 11, 21, 21];

    assert!(fan_wait_bias(&win_hand, &[], &table, 0, 5, 3, &[]) > 0.0);

    table.dealer_position = 0;
    assert_eq!(fan_wait_bias(&win_hand, &[], &table, 0, 5, 3, &[],), 0.0);
}

#[test]
fn fan_wait_bias_counts_single_wait_cap_when_visible_fan_is_half_cap() {
    let mut table = table_with_discards(1, Vec::new());
    table.score_cap = Some(4);
    let win_hand = vec![2, 2, 5, 6, 7, 11, 12, 13, 21, 22, 23];
    let melds = vec![test_peng_meld(31)];

    assert_eq!(estimated_visible_fan_without_wait(&win_hand, &melds), 1);
    assert_eq!(estimated_fan_with_wait(&win_hand, &melds, 6), 2);
    assert_eq!(
        fan_wait_bias(&win_hand, &melds, &table, 0, 6, 4, &[],),
        14.0
    );
}

#[test]
fn fan_wait_bias_rewards_edge_wait_decomposition() {
    let table = table_with_discards(1, Vec::new());
    let melds = vec![test_peng_meld(11), test_chi_meld(21)];
    let edge_win = vec![1, 2, 3, 4, 4, 6, 7, 8];
    let closed_middle_win = vec![1, 1, 2, 3, 4, 6, 7, 8];

    assert!(has_edge_wait_decomposition(&edge_win, 3));
    assert!(!has_edge_wait_decomposition(&closed_middle_win, 3));
    let edge_score = fan_wait_bias(&edge_win, &melds, &table, 0, 3, 4, &[]);
    let closed_middle_score = fan_wait_bias(&closed_middle_win, &melds, &table, 0, 3, 4, &[]);

    assert!(edge_score > closed_middle_score);

    let mut speed_first_table = table.clone();
    speed_first_table.dealer_position = 0;
    assert_eq!(
        fan_wait_bias(&edge_win, &melds, &speed_first_table, 0, 3, 4, &[],),
        0.0
    );

    speed_first_table.dealer_position = 1;
    speed_first_table.wall_count = FINAL_DEFENSE_WALL_COUNT;
    assert_eq!(
        fan_wait_bias(&edge_win, &melds, &speed_first_table, 0, 3, 4, &[],),
        0.0
    );
}

#[test]
fn fan_wait_bias_stops_when_closed_payers_exceed_half_cap() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 13,
            discards: Vec::new(),
            melds: Vec::new(),
        },
    );
    table.seats.insert(
        3,
        AiSeatView {
            position: 3,
            hand_count: 13,
            discards: Vec::new(),
            melds: Vec::new(),
        },
    );
    table.dealer_position = 3;
    table.score_cap = Some(15);
    let melds = vec![test_peng_meld(11), test_chi_meld(21)];
    let edge_win = vec![1, 2, 3, 4, 4, 6, 7, 8];

    assert_eq!(estimated_visible_fan_without_wait(&edge_win, &melds), 1);
    assert_eq!(
        minimum_potential_payment_fan(1, &table, 0),
        3,
        "three closed payers should add two fan to every payment",
    );
    assert_eq!(fan_wait_bias(&edge_win, &melds, &table, 0, 3, 4, &[]), 0.0);
}

#[test]
fn fan_wait_bias_stops_piao_shou_ba_yi_when_visible_fan_exceeds_half_cap() {
    let mut table = table_with_discards(1, Vec::new());
    table.score_cap = Some(15);
    let win_hand = vec![35, 35];
    let melds = vec![
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(21),
        test_peng_meld(31),
    ];

    assert_eq!(estimated_visible_fan_without_wait(&win_hand, &melds), 3);
    assert_eq!(estimated_fan_with_wait(&win_hand, &melds, 35), 5);
    assert!(shenyang_fan_score_exceeds_half_cap(
        3,
        table.score_cap.unwrap()
    ));
    assert_eq!(
        fan_wait_bias(&win_hand, &melds, &table, 0, 35, 2, &[],),
        0.0
    );
}

#[test]
fn fan_wait_bias_stops_terminal_single_wait_when_visible_fan_exceeds_half_cap() {
    let mut table = table_with_discards(1, Vec::new());
    table.score_cap = Some(50);
    let win_hand = vec![11, 11, 14, 15, 15, 16, 16, 17, 17, 17, 17];
    let melds = vec![test_chi_meld(12)];

    assert_eq!(estimated_visible_fan_without_wait(&win_hand, &melds), 5);
    assert_eq!(estimated_fan_with_wait(&win_hand, &melds, 11), 6);
    assert!(shenyang_fan_score_exceeds_half_cap(
        5,
        table.score_cap.unwrap()
    ));

    assert_eq!(
        fan_wait_bias(&win_hand, &melds, &table, 0, 11, 3, &[],),
        0.0
    );
}

#[test]
fn fan_wait_bias_rejects_closed_illegal_basic_hand() {
    let table = table_with_discards(1, Vec::new());
    let win_hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35];

    assert_eq!(fan_wait_bias(&win_hand, &[], &table, 0, 35, 2, &[],), 0.0);
}

#[test]
fn known_unavailable_tiles_ignore_invalid_discards() {
    let table = table_with_discards(1, vec![4, 4, 99]);
    let known_unavailable =
        known_unavailable_tiles_with_simulated_discards(&table, 0, &[], &[4, 31, 99]);

    assert_eq!(
        known_unavailable.iter().filter(|tile| **tile == 4).count(),
        3
    );
    assert_eq!(
        known_unavailable.iter().filter(|tile| **tile == 31).count(),
        1
    );
    assert!(!known_unavailable.contains(&99));
}

#[test]
fn one_step_wait_potential_rejects_illegal_near_ready_shape() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 31, 35];

    assert_eq!(one_step_wait_potential(&hand, &[], &table, 0), 0.0);
}

#[test]
fn one_step_wait_potential_values_open_basic_route_foundation() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();
    let hand = vec![1, 2, 3, 5, 11, 12, 13, 21, 35, 35];

    assert!(hand_power(&hand) < 50.0);
    assert!(pair_count(&hand) < 4);
    assert!(has_open_meld(melds));
    assert!(missing_suits(&hand, melds).is_empty());
    assert!(has_terminal_or_honor_with_extra(&hand, melds, None));
    assert!(has_triplet_or_dragon_pair(&hand, melds));
    assert!(
        one_step_wait_potential(&hand, melds, &table, 0) > 0.0,
        "open basic hand with all hard requirements should value one-step ready draws"
    );
}

#[test]
fn one_step_wait_potential_values_first_chi_disabled_closed_route_after_xi_gang() {
    let mut table = table_with_discards(1, Vec::new());
    table.allow_first_chi = false;
    table.seats.get_mut(&0).unwrap().melds = vec![WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::XI_GANG,
        tiles: vec![31, 32, 33, 34],
        from_position: None,
    }];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();
    let hand = vec![1, 2, 3, 5, 11, 12, 13, 21, 35, 35];

    assert!(hand_power(&hand) < 50.0);
    assert!(pair_count(&hand) < 4);
    assert!(!has_open_meld(melds));
    assert!(missing_suits(&hand, melds).is_empty());
    assert!(has_terminal_or_honor_with_extra(&hand, melds, None));

    let mut after_draw = hand.clone();
    after_draw.push(22);
    let after_discard = remove_n_tiles(&after_draw, 5, 1);
    assert!(ready_tile_score_after_discard(&after_discard, melds, &table, 0, 5,) > 0.0);
    assert!(
        one_step_wait_potential(&hand, melds, &table, 0) > 0.0,
        "the configured closed route can draw 22, discard 5, and wait on 23"
    );

    let mut first_chi_allowed = table.clone();
    first_chi_allowed.allow_first_chi = true;
    let first_chi_allowed_melds = first_chi_allowed.seats.get(&0).unwrap().melds.as_slice();
    assert_eq!(
        one_step_wait_potential(&hand, first_chi_allowed_melds, &first_chi_allowed, 0),
        0.0,
    );

    let mut concealed_gang_table = table.clone();
    concealed_gang_table.seats.get_mut(&0).unwrap().melds = vec![WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::GANG,
        tiles: vec![31, 31, 31, 31],
        from_position: None,
    }];
    let concealed_gang_melds = concealed_gang_table.seats.get(&0).unwrap().melds.as_slice();
    assert_eq!(
        one_step_wait_potential(&hand, concealed_gang_melds, &concealed_gang_table, 0),
        0.0,
    );
}

#[test]
fn ready_cap_counts_single_wait_fan() {
    let mut table = table_with_discards(1, Vec::new());
    table.score_cap = Some(4);
    table.seats.get_mut(&0).unwrap().melds = vec![test_chi_meld(1)];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();
    let hand = vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35];

    assert!(ready_visible_fan_reaches_cap(&hand, melds, &table, 0));
}

#[test]
fn ready_score_allows_closed_sequence_dragon_pair_win_when_first_chi_disabled() {
    let mut table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 35];
    let mut win_hand = hand.clone();
    win_hand.push(35);
    sort_tiles(&mut win_hand);

    assert_eq!(ready_tile_score(&hand, &[], &table, 0), 0.0);
    assert_eq!(
        estimated_visible_fan_without_wait_for_table(&win_hand, &[], &table),
        0
    );

    table.allow_first_chi = false;
    assert!(ready_tile_score(&hand, &[], &table, 0) > 0.0);
    assert_eq!(
        estimated_visible_fan_without_wait_for_table(&win_hand, &[], &table),
        1
    );
}

#[test]
fn ready_score_counts_chi_as_opening_meld() {
    let table = table_with_discards(1, Vec::new());
    let melds = vec![test_chi_meld(1)];
    let hand = vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35];

    assert!(ready_tile_score(&hand, &melds, &table, 0) > 0.0);

    assert!(ready_tile_score(&hand, &melds, &table, 0) > 0.0);
}

#[test]
fn ready_score_counts_projected_meld_tiles_as_dead() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    let melds = vec![test_chi_meld(2)];
    let hand = vec![1, 2, 11, 12, 13, 21, 21, 21, 35, 35];

    assert_eq!(remaining_tile_count(&hand, &table, 0, 3), 4);
    assert_eq!(
        remaining_tile_count_with_melds(&hand, &melds, &table, 0, 3),
        3
    );
    assert_eq!(ready_tile_score(&hand, &melds, &table, 0), 43.0);
}

#[test]
fn ready_score_counts_simulated_discarded_wait_tile_as_dead() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    let melds = vec![test_chi_meld(1)];
    let hand_after_discard = vec![11, 12, 13, 21, 22, 23, 35, 35, 35, 31];

    assert_eq!(remaining_tile_count(&hand_after_discard, &table, 0, 31), 3);
    assert_eq!(
        remaining_tile_count_with_melds_after_discards(
            &hand_after_discard,
            &melds,
            &table,
            0,
            31,
            &[31]
        ),
        2
    );
    assert_eq!(
        ready_tile_score(&hand_after_discard, &melds, &table, 0),
        43.0
    );
    assert_eq!(
        ready_tile_score_after_discard(&hand_after_discard, &melds, &table, 0, 31),
        38.0
    );
}

#[test]
fn ready_score_does_not_double_count_visible_claim_tile_in_projected_meld() {
    let mut table = table_with_discards(1, vec![3]);
    table.dealer_position = 0;
    table.claim_window = Some(AiClaimView {
        tile: 3,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let melds = vec![test_peng_meld(3)];
    let hand = vec![1, 2, 4, 5, 6, 11, 12, 13, 21, 21];

    assert_eq!(visible_tile_count(&table, 3), 1);
    assert_eq!(
        remaining_tile_count_with_melds(&hand, &melds, &table, 0, 3),
        1
    );
    assert_eq!(ready_tile_score(&hand, &melds, &table, 0), 33.0);
}

#[test]
fn ready_score_keeps_closed_sequence_dragon_pair_route_after_xi_gang() {
    let mut table = table_with_discards(1, Vec::new());
    let xi_gang = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::XI_GANG,
        tiles: vec![31, 32, 33, 34],
        from_position: None,
    };
    table.seats.get_mut(&0).unwrap().melds = vec![xi_gang];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();
    let hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 35];
    let mut win_hand = hand.clone();
    win_hand.push(35);
    sort_tiles(&mut win_hand);

    assert_eq!(ready_tile_score(&hand, melds, &table, 0), 0.0);
    table.allow_first_chi = false;
    assert!(ready_tile_score(&hand, melds, &table, 0) > 0.0);
    assert_eq!(
        estimated_visible_fan_without_wait_for_table(&win_hand, melds, &table),
        2
    );
}

#[test]
fn ready_score_values_live_wind_over_middle_for_dealer_seven_pairs() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    let wind_wait = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 31];
    let middle_wait = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 21, 21, 22, 22];

    assert!(
        ready_tile_score(&wind_wait, &[], &table, 0)
            > ready_tile_score(&middle_wait, &[], &table, 0)
    );
}

#[test]
fn ready_visible_cap_counts_concealed_dragon_triplet() {
    let mut table = table_with_discards(1, Vec::new());
    table.score_cap = Some(4);
    table.seats.get_mut(&0).unwrap().melds = vec![test_chi_meld(1)];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();
    let hand = vec![11, 12, 13, 21, 22, 23, 31, 35, 35, 35];

    assert!(ready_visible_fan_reaches_cap(&hand, melds, &table, 0));
}

#[test]
fn ready_visible_cap_counts_four_gui_yi() {
    let mut table = table_with_discards(1, Vec::new());
    table.score_cap = Some(4);
    table.seats.get_mut(&0).unwrap().melds = vec![test_chi_meld(11)];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();
    let hand = vec![1, 2, 3, 7, 8, 9, 9, 9, 9, 21];

    assert!(ready_visible_fan_reaches_cap(&hand, melds, &table, 0));
}

#[test]
fn ready_visible_cap_counts_piao_shou_ba_yi() {
    let mut table = table_with_discards(1, Vec::new());
    table.score_cap = Some(16);
    table.seats.get_mut(&0).unwrap().melds = vec![
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(21),
        test_peng_meld(31),
    ];
    let hand = vec![35];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();

    assert!(ready_visible_fan_reaches_cap(&hand, melds, &table, 0));
}

#[test]
fn remaining_tile_count_counts_own_public_tiles() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().discards = vec![31];
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];

    assert_eq!(remaining_tile_count(&[], &table, 0, 31), 0);
}
