use super::*;

#[test]
fn claim_chi_can_fill_missing_third_suit() {
    let mut table = table_with_discards(3, Vec::new());
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 22,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 23, 31, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Chi {
            consume_tiles: vec![21, 23]
        })
    );
}

#[test]
fn claim_chi_passes_when_disabled() {
    let mut table = table_with_discards(3, Vec::new());
    table.wall_count = 40;
    table.allow_chi = false;
    table.claim_window = Some(AiClaimView {
        tile: 22,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 23, 31, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_chi_starts_after_half_round_when_reaching_ready() {
    let mut table = table_with_discards(3, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 3,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 4, 5, 6, 7, 8, 9, 11, 12, 13, 31, 35];

    table.wall_count = LATE_PRESSURE_WALL_COUNT + 1;
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Pass)
    );

    table.wall_count = LATE_PRESSURE_WALL_COUNT;
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Chi {
            consume_tiles: vec![1, 2]
        })
    );
}

#[test]
fn defensive_chi_open_starts_after_half_round() {
    let mut table = table_with_discards(3, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 3,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 5, 8, 11, 14, 17, 21, 24, 31, 32, 33];

    table.wall_count = LATE_PRESSURE_WALL_COUNT + 1;
    assert!(!should_claim_chi_to_open_broken_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_RELAXED
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Pass)
    );

    table.wall_count = LATE_PRESSURE_WALL_COUNT;
    assert!(should_claim_chi_to_open_broken_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_RELAXED
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Chi {
            consume_tiles: vec![1, 2]
        })
    );
}

#[test]
fn claim_chi_can_use_claim_tile_as_low_edge() {
    let mut table = table_with_discards(3, Vec::new());
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 2, 3, 5, 8, 11, 14, 17, 21, 24, 31, 32, 33];

    assert!(should_claim_chi_to_open_broken_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_RELAXED
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Chi {
            consume_tiles: vec![2, 3]
        })
    );
}

#[test]
fn claim_chi_does_not_fake_open_door_when_configured_off() {
    let mut table = table_with_discards(3, Vec::new());
    table.wall_count = 40;
    table.chi_opens_door = false;
    table.claim_window = Some(AiClaimView {
        tile: 3,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn relaxed_claim_chi_does_not_fake_defensive_open_when_configured_off() {
    let mut table = table_with_discards(3, Vec::new());
    table.wall_count = 52;
    table.chi_opens_door = false;
    table.claim_window = Some(AiClaimView {
        tile: 3,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 5, 8, 11, 14, 17, 21, 24, 31, 32, 33];

    assert!(!should_claim_chi_to_open_broken_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_RELAXED
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_chi_passes_first_half_even_when_it_reaches_basic_ready() {
    let mut table = table_with_discards(3, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 3,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35];

    assert!(!is_mid_opening_round(&table));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_chi_opens_late_broken_hand_for_defense() {
    let mut table = table_with_discards(3, Vec::new());
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 3,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 5, 8, 11, 14, 17, 21, 24, 31, 32, 33];

    assert!(should_claim_chi_to_open_broken_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_RELAXED
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Chi {
            consume_tiles: vec![1, 2]
        })
    );
}

#[test]
fn claim_chi_passes_before_half_for_broken_hand() {
    let mut table = table_with_discards(3, Vec::new());
    table.wall_count = 52;
    table.claim_window = Some(AiClaimView {
        tile: 3,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 5, 8, 11, 14, 17, 21, 24, 31, 32, 33];

    assert!(!is_late_round(&table));
    assert!(!should_claim_chi_to_open_broken_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_RELAXED
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_chi_passes_first_half_when_filling_missing_suit_reaches_ready() {
    let mut table = table_with_discards(3, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 22,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 23, 31, 35];

    assert!(!is_mid_opening_round(&table));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_chi_passes_early_when_it_does_not_make_ready() {
    let mut table = table_with_discards(3, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 3,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 5, 5, 5, 9, 9, 9, 11, 14, 17, 21, 24];

    assert!(!is_mid_opening_round(&table));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_chi_passes_for_four_pair_piao_candidate_in_relaxed_rule() {
    let mut table = table_with_discards(3, Vec::new());
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 7,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 3, 3, 4, 4, 5, 6, 11, 12, 21];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_chi_passes_for_open_triplet_two_pair_piao_route_even_when_chi_reaches_ready() {
    let mut table = table_with_discards(3, Vec::new());
    table.wall_count = 40;
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    table.claim_window = Some(AiClaimView {
        tile: 22,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![11, 11, 12, 13, 14, 21, 21, 23, 24, 31];

    assert!(should_preserve_piao_plan_for_chi(
        &hand,
        table.seats.get(&0).unwrap().melds.as_slice(),
        &table,
        0,
        WIN_RULE_RELAXED
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_chi_passes_for_three_pair_piao_candidate_even_when_chi_reaches_ready() {
    let mut table = table_with_discards(3, Vec::new());
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 27,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 5, 5, 11, 12, 13, 22, 23, 24, 24, 28, 29];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_chi_passes_late_ready_hand() {
    let mut table = table_with_discards(3, Vec::new());
    table.wall_count = 36;
    table.claim_window = Some(AiClaimView {
        tile: 3,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 31, 31];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_chi_passes_mid_round_when_it_does_not_make_ready_or_defensive_open() {
    let mut table = table_with_discards(3, Vec::new());
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 3,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 5, 5, 5, 9, 9, 9, 11, 14, 17, 21, 24];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_chi_passes_when_five_pairs_missing_suit_can_chase_seven_pairs() {
    let mut table = table_with_discards(3, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 23,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 3, 3, 11, 11, 21, 21, 22, 31, 32];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_chi_passes_when_piao_plan_is_stronger() {
    let mut table = table_with_discards(3, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![1, 1, 1],
        from_position: Some(2),
    }];
    table.claim_window = Some(AiClaimView {
        tile: 22,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![11, 11, 21, 23, 31, 31, 35, 35, 36, 37];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_chi_preserves_pure_one_suit_plan_from_off_suit_chi() {
    let mut table = table_with_discards(3, Vec::new());
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 13,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 11, 12, 35];

    assert!(pure_one_suit_plan_score_for_context(&hand, &[], &table, 0, WIN_RULE_RELAXED,) > 0.0);
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_chi_passes_before_half_when_ready_without_defensive_open() {
    let mut table = table_with_discards(3, Vec::new());
    table.wall_count = 52;
    table.claim_window = Some(AiClaimView {
        tile: 3,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 4, 5, 6, 7, 8, 9, 11, 12, 13, 31, 35];

    assert!(is_mid_opening_round(&table));
    assert!(!is_late_round(&table));
    assert!(!should_claim_chi_to_open_broken_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_RELAXED
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_chi_takes_mid_round_when_it_reaches_ready() {
    let mut table = table_with_discards(3, Vec::new());
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 3,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 4, 5, 6, 7, 8, 9, 11, 12, 13, 31, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Chi {
            consume_tiles: vec![1, 2]
        })
    );
}

#[test]
fn claim_chi_takes_shenyang_basic_rule_when_it_reaches_ready() {
    let mut table = table_with_discards(3, Vec::new());
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 3,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Chi {
            consume_tiles: vec![1, 2]
        })
    );
}

#[test]
fn dealer_claim_chi_passes_before_half_for_broken_hand() {
    let mut table = table_with_discards(3, Vec::new());
    table.dealer_position = 0;
    table.wall_count = 52;
    table.claim_window = Some(AiClaimView {
        tile: 3,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 5, 8, 11, 14, 17, 21, 24, 31, 32, 33];

    assert!(!is_late_round(&table));
    assert!(!should_claim_chi_to_open_broken_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_RELAXED
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn threatening_dealer_makes_post_chi_discard_choose_wider_wait() {
    let mut table = table_with_discards(3, Vec::new());
    table.wall_count = 30;
    table.seats.insert(
        1,
        AiSeatView {
            position: 1,
            hand_count: 13,
            discards: vec![1, 9, 19, 29, 31, 32],
            melds: Vec::new(),
        },
    );
    let hand = vec![4, 5, 7, 11, 12, 13, 18, 19, 21, 22, 23, 35, 35];
    table.claim_window = Some(AiClaimView {
        tile: 17,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();

    table.dealer_position = 3;
    assert!(!dealer_opponent_has_major_threat(
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Chi {
            consume_tiles: vec![18, 19]
        })
    );
    table.dealer_position = 1;
    assert!(dealer_opponent_has_major_threat(
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Chi {
            consume_tiles: vec![18, 19]
        })
    );

    table.claim_window = None;
    table.seats.get_mut(&0).unwrap().melds = vec![WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::CHI,
        tiles: vec![17, 18, 19],
        from_position: Some(3),
    }];
    let post_chi_hand = vec![4, 5, 7, 11, 12, 13, 21, 22, 23, 35, 35];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();
    let narrow_wait = remove_n_tiles(&post_chi_hand, 4, 1);
    let wide_wait = remove_n_tiles(&post_chi_hand, 7, 1);
    assert_eq!(
        ready_live_tile_count_after_discard(
            &narrow_wait,
            melds,
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC,
            4,
        ),
        4
    );
    assert_eq!(
        ready_live_tile_count_after_discard(
            &wide_wait,
            melds,
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC,
            7,
        ),
        8
    );

    table.dealer_position = 3;
    assert_eq!(
        choose_discard_from_view(&post_chi_hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(4)
    );
    table.dealer_position = 1;
    assert_eq!(
        choose_discard_from_view(&post_chi_hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(7)
    );
}
