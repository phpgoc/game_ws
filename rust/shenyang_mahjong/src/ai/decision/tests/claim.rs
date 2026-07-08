use super::*;

#[test]
fn claim_peng_passes_raw_piao_shape_without_terminal_or_honor() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(2)];
    table.claim_window = Some(AiClaimView {
        tile: 13,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![12, 13, 13, 14, 15, 22, 22, 23, 25, 25];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();

    assert!(piao_plan_score(&hand, melds) >= 32.0);
    assert_eq!(piao_plan_score_for_context(&hand, melds, &table, 0), 0.0);
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_ready_hand_pengs_dragon_when_it_keeps_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![11, 12, 13, 21, 22, 23, 24, 25, 35, 35];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();
    let current_ready_score = ready_tile_score(&hand, melds, &table, 0, WIN_RULE_SHENYANG_BASIC);

    assert!(current_ready_score > 0.0);
    assert!(!is_complete_win_with_melds(
        &[11, 12, 13, 21, 22, 23, 24, 25, 35, 35, 35],
        melds,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert!(should_claim_ready_dragon_peng_from_discard(
        &hand,
        melds,
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC,
        35,
        1,
        current_ready_score
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_ready_hand_passes_dragon_peng_when_visible_fan_is_capped() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(1);
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![11, 12, 13, 21, 22, 23, 24, 25, 35, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

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
fn claim_chi_takes_mid_round_ready_without_defensive_open() {
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
    assert!(!should_claim_chi_to_open_broken_hand_for_defense(
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

    assert!(pure_one_suit_plan_score_for_context(&hand, &[], &table, 0) > 0.0);
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
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
fn claim_chi_opens_mid_broken_hand_for_defense() {
    let mut table = table_with_discards(3, Vec::new());
    table.wall_count = 52;
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
fn claim_chi_does_not_rush_opening_closed_basic_hand_early() {
    let mut table = table_with_discards(3, Vec::new());
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
fn claim_chi_passes_early_even_when_it_can_fill_missing_third_suit() {
    let mut table = table_with_discards(3, Vec::new());
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
fn claim_chi_passes_for_shenyang_basic_rule_even_late() {
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
        0
    ));
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
fn claim_gang_beats_peng_when_not_winning() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 35, 35, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Gang)
    );
}

#[test]
fn claim_gang_takes_dragon_gang_to_open_basic_hand_before_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 7, 11, 12, 14, 21, 35, 35, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Gang)
    );
}

#[test]
fn one_fan_capped_claim_gang_penges_dragon_for_speed_before_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(1);
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 7, 11, 12, 14, 21, 35, 35, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_gang_delays_open_piao_plain_gang_until_ready_and_pengs() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    table.claim_window = Some(AiClaimView {
        tile: 21,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![11, 11, 21, 21, 21, 31, 31, 35, 35, 36];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_gang_delays_open_plain_gang_when_not_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_chi_meld(1)];
    table.claim_window = Some(AiClaimView {
        tile: 9,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![4, 5, 6, 9, 9, 9, 11, 12, 14, 21];

    assert_ne!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Gang)
    );
}

#[test]
fn claim_gang_opens_closed_plain_basic_hand_before_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 3,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![3, 3, 3, 4, 5, 7, 8, 11, 12, 14, 21, 22, 31];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Gang)
    );
}

#[test]
fn claim_gang_penges_closed_early_piao_candidate() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 1, 4, 5, 6, 11, 11, 12, 13, 21, 21, 22];

    assert!(is_closed_early_piao_candidate(&hand, &[], &table, 0));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_gang_opens_broken_closed_hand_late_for_defense() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 2,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 2, 2, 4, 7, 12, 14, 17, 31, 32, 33, 34, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Gang)
    );
}

#[test]
fn dealer_claim_gang_opens_broken_closed_hand_for_speed() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 2,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 2, 2, 4, 7, 12, 14, 17, 31, 32, 33, 34, 35];

    assert!(should_open_broken_closed_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Gang)
    );
}

#[test]
fn claim_gang_passes_final_unready_broken_hand_for_defense() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.claim_window = Some(AiClaimView {
        tile: 2,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 2, 2, 4, 7, 12, 14, 17, 31, 32, 33, 34, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_gang_opens_broken_closed_hand_for_defense_in_relaxed_rule() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 31,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 5, 8, 12, 14, 17, 21, 24, 27, 31, 31, 31, 34];

    assert!(should_open_broken_closed_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_RELAXED
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Gang)
    );
}

#[test]
fn claim_gang_opens_mid_missing_suit_no_terminal_hand_for_defense() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 52;
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 3, 5, 5, 5, 7, 8, 12, 14, 15, 16, 17, 18];

    assert!(should_open_broken_closed_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Gang)
    );
}

#[test]
fn claim_gang_passes_when_it_breaks_locked_pure_one_suit_plan() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 11,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 11, 11, 11];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_gang_passes_dragon_when_pure_one_suit_plan_starts_at_eight_tiles() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 11, 21, 35, 35, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_gang_passes_closed_pure_one_suit_plan_before_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 11];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_gang_takes_ready_main_suit_pure_one_suit_when_not_capped() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 9];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Gang)
    );
}

#[test]
fn claim_gang_passes_ready_pure_one_suit_when_visible_fan_capped() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(4);
    table.seats.get_mut(&0).unwrap().melds = vec![test_chi_meld(2)];
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 1, 5, 6, 7, 8, 8, 9, 9];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_gang_passes_capped_closed_pure_one_suit_wait() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(4);
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 9];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_gang_passes_when_it_breaks_locked_seven_pairs_plan() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 31];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_gang_preserves_five_pairs_even_for_dragon_gang() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 22, 35, 35, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_gang_skips_plain_gang_when_ready_fan_already_capped() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(3);
    table.seats.get_mut(&0).unwrap().melds = vec![test_gang_meld(35)];
    table.claim_window = Some(AiClaimView {
        tile: 9,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 9, 9, 9, 11, 12, 13, 21];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_gang_takes_open_plain_gang_when_it_reaches_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_chi_meld(1)];
    table.claim_window = Some(AiClaimView {
        tile: 9,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![4, 5, 6, 9, 9, 9, 11, 12, 13, 21];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Gang)
    );
}

#[test]
fn claim_gang_penges_to_preserve_four_gui_yi_when_peng_stays_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    table.seats.get_mut(&0).unwrap().melds = vec![test_chi_meld(11)];
    table.claim_window = Some(AiClaimView {
        tile: 4,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 2, 2, 4, 4, 4, 5, 21, 21, 21];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_gang_takes_late_ready_dragon_gang_when_it_keeps_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 36;
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 11, 12, 13, 21, 35, 35, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Gang)
    );
}

#[test]
fn claim_gang_passes_late_ready_hand_when_gang_breaks_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 36;
    table.claim_window = Some(AiClaimView {
        tile: 6,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 4, 6, 6, 6, 7, 8, 13, 14, 15, 23, 24, 25];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_hu_accepts_open_meld_remainder() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    table.seats.get_mut(&0).unwrap().melds = vec![
        share_type_public::games::shenyang_mahjong::WsShenyangMahjongMeld {
            kind: share_type_public::games::shenyang_mahjong::ShenyangMahjongMeldKind::PENG,
            tiles: vec![1, 1, 1],
            from_position: Some(2),
        },
    ];
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Hu)
    );
}

#[test]
fn claim_hu_accepts_seven_pairs() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Hu)
    );
}

#[test]
fn claim_hu_beats_other_claims() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Hu)
    );
}

#[test]
fn claim_hu_still_wins_during_final_defense() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Hu)
    );
}

#[test]
fn claim_peng_allows_dragon_when_missing_suit_can_still_be_recovered() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 31, 35, 35, 36];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_passes_main_suit_when_closed_pure_one_suit_plan_is_strong() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 11, 12];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_passes_nine_tile_pure_one_suit_plan() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 3, 4, 5, 6, 7, 8, 11, 12, 31, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_passes_main_suit_pure_one_suit_when_opening_is_not_required() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 11, 12];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_takes_open_main_suit_pure_one_suit_when_it_reaches_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    table.claim_window = Some(AiClaimView {
        tile: 2,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 2, 3, 3, 3, 3, 4, 4, 7];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();

    assert!(pure_one_suit_plan_score_for_context(&hand, melds, &table, 0) > 0.0);
    assert_eq!(
        ready_tile_score(&hand, melds, &table, 0, WIN_RULE_SHENYANG_BASIC),
        0.0
    );
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_passes_weak_main_suit_pure_one_suit_start() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 3, 4, 5, 6, 7, 11, 12, 21, 31, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_preserves_pure_one_suit_seven_pairs_wait() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 8];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_opens_broken_closed_hand_late_for_defense() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 2,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 2, 4, 7, 12, 14, 17, 31, 32, 33, 34, 35, 36];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_passes_final_unready_broken_hand_for_defense() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.claim_window = Some(AiClaimView {
        tile: 2,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 2, 4, 7, 12, 14, 17, 31, 32, 33, 34, 35, 36];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_opens_mid_severely_broken_closed_hand_for_defense() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 52;
    table.claim_window = Some(AiClaimView {
        tile: 31,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 5, 8, 11, 14, 17, 19, 31, 31, 33, 35, 36, 37];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_opens_missing_suit_basic_hand_despite_relaxed_near_ready_shape() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 52;
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 3, 4, 5, 6, 11, 12, 13, 14, 15, 31];

    assert!(
        one_step_wait_potential(&hand, &[], &table, 0, WIN_RULE_RELAXED) > 0.0,
        "the relaxed shape is close enough that it used to block defensive opening"
    );
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_opens_broken_closed_hand_for_defense_in_relaxed_rule() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 31,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 5, 8, 12, 14, 17, 21, 24, 27, 31, 31, 33, 34];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_passes_dragon_when_pure_one_suit_plan_is_strong() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 35, 35, 36];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_passes_dragon_when_pure_one_suit_plan_starts_at_eight_tiles() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 11, 21, 35, 35, 36];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn dealer_claim_peng_can_ignore_early_eight_tile_pure_one_suit_plan() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 11, 21, 35, 35, 36];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_passes_when_five_pairs_missing_suit_can_chase_seven_pairs() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 21,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 12, 22, 31, 32];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_preserves_quad_as_two_pairs_seven_pairs_route() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 2,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 1, 1, 2, 2, 3, 3, 11, 11, 12, 31, 35];

    assert_eq!(pair_count(&hand), 5);
    assert!(should_lock_seven_pairs_plan(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_passes_when_it_breaks_locked_pure_one_suit_plan() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 11,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 11, 11];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_passes_when_it_breaks_seven_pairs_shape() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 6,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 31];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_passes_when_missing_suit_is_unrecoverable_even_for_dragon() {
    let dead_bamboo = (21..=29)
        .flat_map(|tile| std::iter::repeat_n(tile, 4))
        .collect::<Vec<_>>();
    let mut table = table_with_discards(1, dead_bamboo);
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 31, 35, 35, 36];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_opens_late_broken_missing_suit_hand_even_for_dragon() {
    let dead_bamboo = (21..=29)
        .flat_map(|tile| std::iter::repeat_n(tile, 4))
        .collect::<Vec<_>>();
    let mut table = table_with_discards(1, dead_bamboo);
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 4, 6, 8, 11, 13, 16, 19, 31, 35, 35, 36];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_passes_when_terminal_or_honor_is_unrecoverable_for_basic() {
    let mut table = table_with_discards(1, dead_terminal_or_honor_discards());
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 4, 5, 5, 12, 13, 14, 15, 16, 22, 23, 24, 25];

    assert!(claim_leaves_unrecoverable_terminal_or_honor(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC,
        ShenyangMahjongMeldKind::PENG,
        5,
        1
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_gang_passes_when_terminal_or_honor_is_unrecoverable_for_basic() {
    let mut table = table_with_discards(1, dead_terminal_or_honor_discards());
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 4, 5, 5, 5, 12, 13, 14, 15, 16, 22, 23, 24];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_opens_late_broken_no_terminal_hand_for_defense() {
    let mut table = table_with_discards(1, dead_terminal_or_honor_discards());
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 4, 5, 5, 7, 12, 14, 16, 18, 22, 24, 26, 28];

    assert!(should_open_broken_closed_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_opens_mid_unrecoverable_no_terminal_hand_for_defense() {
    let mut table = table_with_discards(1, dead_terminal_or_honor_discards());
    table.wall_count = 52;
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 3, 4, 5, 5, 6, 7, 12, 13, 14, 22, 23, 24];

    assert_eq!(
        unrecoverable_basic_rule_requirement_count(&hand, &[], &table),
        1
    );
    assert!(should_open_broken_closed_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_takes_late_ready_dragon_when_it_keeps_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 36;
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 35, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_takes_ready_dragon_before_late_round_when_it_keeps_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 35, 35];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();

    assert!(ready_tile_score(&hand, melds, &table, 0, WIN_RULE_SHENYANG_BASIC) > 0.0);
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_preserves_five_pairs_even_with_three_suits() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 21,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 31, 32];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_preserves_pinghu_sequence_when_open_and_heng_is_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![4, 5, 5, 6, 11, 12, 21, 22, 35, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_pursues_piao_plan_after_open_triplet() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![1, 1, 1],
        from_position: Some(2),
    }];
    table.claim_window = Some(AiClaimView {
        tile: 21,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![11, 11, 21, 21, 31, 31, 35, 35, 36, 37];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_still_opens_closed_basic_hand_despite_sequence_shape() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![4, 5, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_still_preserves_locked_seven_pairs_over_dragon_pair() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 12, 35, 35, 36];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_takes_dragon_pair_for_open_and_fan() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 4, 5, 7, 9, 11, 12, 14, 17, 21, 35, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_takes_four_pair_three_suit_piao_start() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 21,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 13, 21, 21, 22, 31, 32];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn relaxed_claim_peng_takes_closed_early_piao_candidate_over_sequence_shape() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 4, 5, 5, 6, 11, 12, 13, 21, 21, 35, 35];

    assert!(tile_is_middle_of_sequence(&hand, 5));
    assert!(should_claim_peng_for_closed_early_piao_candidate(
        &hand,
        &[],
        &table,
        0,
        5,
        1
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_takes_fourth_piao_meld_to_set_up_shou_ba_yi() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds =
        vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![35, 35, 36, 37];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_takes_three_pair_three_suit_piao_start() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 4, 5, 6, 11, 11, 12, 13, 21, 21, 22, 23];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_takes_basic_heng_and_opening_when_no_heng() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 4, 5, 5, 7, 8, 11, 13, 15, 21, 24, 31];

    assert!(!has_open_meld(
        table.seats.get(&0).unwrap().melds.as_slice()
    ));
    assert!(!has_triplet_or_dragon_pair(&hand, &[]));
    assert_eq!(
        ready_tile_score(&hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC),
        0.0
    );
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_opens_mid_basic_hand_with_existing_heng() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 52;
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 1, 2, 3, 5, 5, 11, 12, 13, 21, 22, 23];

    assert!(has_triplet_or_dragon_pair(&hand, &[]));
    assert!(!has_open_meld(
        table.seats.get(&0).unwrap().melds.as_slice()
    ));
    assert_eq!(
        ready_tile_score(&hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC),
        0.0
    );
    assert!(!claim_leaves_unrecoverable_basic_requirement(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC,
        ShenyangMahjongMeldKind::PENG,
        5,
        1
    ));
    assert!(should_claim_peng_to_open_mid_basic_hand(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC,
        5,
        1
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_preserves_closed_mid_basic_sequence_when_heng_exists() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 52;
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 1, 4, 5, 5, 6, 11, 12, 13, 21, 22, 23];

    assert!(has_triplet_or_dragon_pair(&hand, &[]));
    assert!(has_triplet_like_group(&hand, &[]));
    assert!(tile_is_middle_of_sequence(&hand, 5));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_opens_later_closed_basic_hand_over_sequence_preservation() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 42;
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 1, 4, 5, 5, 6, 11, 12, 13, 21, 22, 23];

    assert!(has_triplet_or_dragon_pair(&hand, &[]));
    assert!(tile_is_middle_of_sequence(&hand, 5));
    assert!(should_claim_peng_to_open_mid_basic_hand(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC,
        5,
        1
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn dealer_claim_chi_passes_for_shenyang_basic_rule() {
    let mut table = table_with_discards(3, Vec::new());
    table.dealer_position = 0;
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
fn dealer_claim_peng_does_not_chase_early_pure_one_suit_plan() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 3, 4, 5, 6, 7, 8, 11, 12, 31, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn dealer_claim_peng_preserves_five_pairs_when_basic_hand_is_missing_suit() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    table.claim_window = Some(AiClaimView {
        tile: 11,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 12, 31, 32, 33];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn dealer_claim_peng_uses_dragon_pair_for_speed_when_basic_route_is_viable() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 21, 21, 22, 31, 35, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn one_fan_capped_claim_peng_uses_dragon_pair_for_speed_over_five_pairs() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(1);
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 22, 31, 35, 35];

    assert!(!should_lock_seven_pairs_plan(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn dealer_claim_peng_preserves_six_pairs_seven_pairs_plan() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 31, 35, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn dealer_claim_peng_preserves_four_pairs_when_basic_hand_is_missing_suit() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    table.claim_window = Some(AiClaimView {
        tile: 11,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 13, 14, 31, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}
