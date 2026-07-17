use super::*;

#[test]
fn basic_heng_complete_decomposition_ignores_malformed_meld_count() {
    let malformed_meld = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::CHI,
        tiles: vec![11, 11, 11],
        from_position: Some(1),
    };
    let hand = vec![1, 2, 2, 3, 3, 3, 4, 4, 5, 26, 26];

    assert!(!is_valid_meld(&malformed_meld));
    assert!(has_triplet_or_dragon_pair(&hand, &[]));
    assert!(has_triplet_or_dragon_pair(&hand, &[malformed_meld]));
    assert!(!has_triplet_or_dragon_pair(&hand, &[test_chi_meld(11)]));
}

#[test]
fn basic_heng_filter_ignores_chi_tile_plus_hand_pair() {
    let table = table_with_discards(1, Vec::new());
    let melds = vec![test_chi_meld(1)];
    let hand_after_discard = vec![1, 1, 11, 12, 13, 21, 22, 23, 31, 35];

    assert!(violates_basic_heng_discard(
        &hand_after_discard,
        &melds,
        &table,
        0,
        35
    ));
}

#[test]
fn legacy_rule_number_cannot_disable_shenyang_discard_guards() {
    let table = table_with_discards(1, Vec::new());
    let melds = vec![test_chi_meld(11)];
    let hand_after_discard = vec![1, 1, 21, 22, 23, 24, 25, 26, 31, 35];

    assert!(violates_basic_heng_discard(
        &hand_after_discard,
        &melds,
        &table,
        0,
        35,
    ));
    assert!(basic_heng_seed_discard_bias(&hand_after_discard, 35, &melds) < 0.0);

    let no_terminal_after = vec![2, 3, 4, 12, 13, 14, 22, 23, 24, 5];
    assert!(violates_basic_terminal_or_honor_discard(
        &no_terminal_after,
        &[],
        &table,
        0,
        35,
    ));

    let two_suits_after = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 14, 15, 31, 35];
    assert!(violates_basic_three_suits_discard(
        &two_suits_after,
        &[],
        &table,
        0,
        21,
    ));
}

#[test]
fn basic_heng_filter_ignores_invalid_hand_triplet() {
    let hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 35, 99, 99, 99];

    assert!(!is_valid_tile(99));
    assert!(!has_triplet_like_group(&hand, &[]));
    assert!(!has_triplet_or_dragon_pair(&hand, &[]));
    assert!(basic_heng_seed_discard_bias(&hand, 35, &[]) < 0.0);
}

#[test]
fn basic_heng_filter_ignores_short_gang_meld() {
    let melds = vec![WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::GANG,
        tiles: vec![1, 1, 1],
        from_position: Some(1),
    }];
    let hand = vec![11, 12, 13, 21, 22, 23, 31, 35];

    assert!(!is_triplet_like_meld(&melds[0]));
    assert!(!has_open_meld(&melds));
    assert!(!has_triplet_or_dragon_pair(&hand, &melds));
    assert_eq!(piao_threat_level(&melds), 0);
}

#[test]
fn basic_heng_filter_ignores_short_triplet_like_meld() {
    let melds = vec![WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![1, 1],
        from_position: Some(1),
    }];
    let hand = vec![11, 12, 13, 21, 22, 23, 31, 35];

    assert!(!is_triplet_like_meld(&melds[0]));
    assert!(!has_open_meld(&melds));
    assert!(!has_peng_meld(&melds, 1));
    assert!(!has_triplet_or_dragon_pair(&hand, &melds));
    assert_eq!(piao_threat_level(&melds), 0);
}

#[test]
fn basic_heng_heuristic_uses_complete_decomposition_for_fake_triplet() {
    let melds = vec![test_chi_meld(11)];
    let hand = vec![1, 2, 2, 3, 3, 3, 4, 4, 5, 26, 26];

    assert!(is_complete_win(&hand, melds.len()));
    assert!(hand.iter().filter(|tile| **tile == 3).count() >= 3);
    assert!(!has_triplet_in_standard_decomposition(&hand));
    assert!(!has_triplet_or_dragon_pair(&hand, &melds));
}

#[test]
fn basic_heng_recovery_requires_enough_wall_tiles() {
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 14, 15, 21, 22];
    let mut discards = SHENYANG_MAHJONG_TILE_KINDS
        .into_iter()
        .flat_map(|tile| {
            let visible = if tile == 35 {
                2
            } else if is_dragon(tile) {
                3
            } else {
                2
            };
            std::iter::repeat_n(tile, visible)
        })
        .collect::<Vec<_>>();
    sort_tiles(&mut discards);
    let mut table = table_with_discards(1, discards);
    table.wall_count = 1;

    assert!(!has_triplet_or_dragon_pair(&hand, &[]));
    assert_eq!(remaining_tile_count(&hand, &table, 0, 35), 2);
    assert!(!can_recover_basic_heng(&hand, &[], &table, 0));
    assert!(!can_recover_basic_heng_after_discard(
        &hand,
        &[],
        &table,
        0,
        31,
    ));

    let mut seeded_hand = hand.clone();
    *seeded_hand.last_mut().unwrap() = 35;
    assert_eq!(remaining_tile_count(&seeded_hand, &table, 0, 35), 1);
    assert!(can_recover_basic_heng(&seeded_hand, &[], &table, 0));
    assert!(can_recover_basic_heng_after_discard(
        &seeded_hand,
        &[],
        &table,
        0,
        31,
    ));

    table.wall_count = 2;
    assert!(can_recover_basic_heng(&hand, &[], &table, 0));
    assert!(can_recover_basic_heng_after_discard(
        &hand,
        &[],
        &table,
        0,
        31,
    ));
}

#[test]
fn broken_closed_defense_opens_mid_severely_broken_hand() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 52;
    let hand = vec![2, 5, 8, 11, 14, 17, 19, 31, 31, 33, 35, 36, 37];

    assert!(should_open_broken_closed_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert!(should_open_broken_closed_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        0
    ));
}

#[test]
fn broken_closed_defense_opens_mid_when_heng_is_unrecoverable() {
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 14, 15, 21, 22];
    let mut table = table_with_discards(1, dead_basic_heng_discards(&hand));
    table.wall_count = 52;

    assert!(hand_power(&hand) >= 14.0);
    assert_eq!(
        ready_tile_score(&hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC),
        0.0
    );
    assert_eq!(
        one_step_wait_potential(&hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC),
        0.0
    );
    assert_eq!(
        unrecoverable_basic_rule_requirement_count(&hand, &[], &table, 0),
        1
    );
    assert!(should_open_broken_closed_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
}

#[test]
fn broken_closed_defense_rejects_illegal_near_ready_shape() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 40;
    let hand = vec![2, 2, 3, 4, 5, 11, 12, 13, 21, 22, 23, 31, 35];

    assert_eq!(
        ready_tile_score(&hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC),
        0.0
    );
    assert_eq!(
        one_step_wait_potential(&hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC),
        0.0
    );
    assert!(should_open_broken_closed_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
}

#[test]
fn broken_closed_defense_waits_mid_recoverable_no_terminal_hand() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 52;
    let hand = vec![2, 2, 2, 5, 5, 6, 7, 12, 13, 14, 22, 23, 24];

    assert_eq!(
        unrecoverable_basic_rule_requirement_count(&hand, &[], &table, 0),
        0
    );
    assert!(!should_open_broken_closed_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
}

#[test]
fn broken_closed_defense_waits_mid_when_basic_requirements_are_intact() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 52;
    let hand = vec![1, 1, 1, 2, 3, 4, 11, 12, 13, 21, 22, 23, 35];

    assert!(!should_open_broken_closed_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
}

#[test]
fn dealer_prefers_wider_wait_over_single_wait_fan() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
    let hand = vec![2, 2, 4, 5, 7, 11, 12, 13, 21, 22, 23];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(7)
    );
}

#[test]
fn missing_suits_tracks_three_suits_need() {
    let hand = vec![1, 2, 3, 11, 18, 19, 21, 22, 23, 24, 25, 26, 35, 36];

    assert!(missing_suits(&hand, &[]).is_empty());
    assert_eq!(missing_suits(&hand[0..6], &[]), vec![2]);
}

#[test]
fn near_capped_non_dealer_prefers_wider_wait_over_single_wait_fan() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(4);
    table.seats.get_mut(&0).unwrap().melds = vec![test_gang_meld(35)];
    let hand = vec![2, 2, 4, 5, 7, 11, 12, 13, 21, 22, 23];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(7)
    );
}

#[test]
fn non_dealer_avoids_nearly_dead_single_wait_before_late_round() {
    let mut table = table_with_discards(1, vec![6, 6, 6]);
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
    let hand = vec![2, 2, 4, 5, 7, 11, 12, 13, 21, 22, 23];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(7)
    );
}

#[test]
fn non_dealer_can_choose_edge_wait_for_extra_fan() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(11), test_chi_meld(21)];
    let hand = vec![1, 1, 2, 4, 4, 6, 7, 8];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();
    let edge_wait = remove_n_tiles(&hand, 1, 1);
    let closed_middle_wait = remove_n_tiles(&hand, 4, 1);

    assert!(
        ready_tile_score_after_discard(&edge_wait, melds, &table, 0, WIN_RULE_SHENYANG_BASIC, 1,)
            > ready_tile_score_after_discard(
                &closed_middle_wait,
                melds,
                &table,
                0,
                WIN_RULE_SHENYANG_BASIC,
                4,
            )
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(1)
    );
}

#[test]
fn non_dealer_can_choose_single_wait_for_extra_fan_before_late_round() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
    let hand = vec![2, 2, 4, 5, 7, 11, 12, 13, 21, 22, 23];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(4)
    );
}

#[test]
fn non_dealer_prefers_wider_wait_against_threatening_dealer() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
    let dealer = table.seats.get_mut(&1).unwrap();
    dealer.hand_count = 4;
    dealer.melds = vec![test_peng_meld(3), test_peng_meld(14), test_peng_meld(25)];
    let hand = vec![2, 2, 4, 5, 7, 11, 12, 13, 21, 22, 23];

    assert!(dealer_opponent_has_major_threat(&table, 0));
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(7)
    );

    table.dealer_position = 3;
    assert!(!dealer_opponent_has_major_threat(&table, 0));
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(4)
    );
}

#[test]
fn open_meld_filter_ignores_malformed_melds() {
    let malformed_peng = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![1, 1],
        from_position: Some(1),
    };
    let malformed_chi = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::CHI,
        tiles: vec![1, 1, 1],
        from_position: Some(1),
    };
    let invalid_tile_peng = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![99, 99, 99],
        from_position: Some(1),
    };

    assert!(!has_open_meld(&[malformed_peng]));
    assert!(!has_open_meld(&[malformed_chi]));
    assert!(!has_open_meld(&[invalid_tile_peng.clone()]));
    assert!(!is_triplet_like_meld(&invalid_tile_peng));
    assert_eq!(piao_threat_level(&[invalid_tile_peng]), 0);
    assert!(has_open_meld(&[test_chi_meld(1)]));
}

#[test]
fn recoverable_basic_heng_counts_live_dragon_pair_without_hand_seed() {
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 14, 15, 21, 22];
    let mut discards = SHENYANG_MAHJONG_TILE_KINDS
        .into_iter()
        .flat_map(|tile| {
            let visible = if tile == 35 {
                2
            } else if is_dragon(tile) {
                3
            } else {
                2
            };
            std::iter::repeat_n(tile, visible)
        })
        .collect::<Vec<_>>();
    sort_tiles(&mut discards);
    let table = table_with_discards(1, discards);

    assert!(!has_triplet_or_dragon_pair(&hand, &[]));
    assert_eq!(remaining_tile_count(&hand, &table, 0, 35), 2);
    assert!(can_recover_basic_heng(&hand, &[], &table, 0));
    assert_eq!(
        unrecoverable_basic_rule_requirement_count(&hand, &[], &table, 0),
        0
    );
}

#[test]
fn recoverable_basic_heng_discounts_projected_meld_tiles() {
    let hand = vec![1, 2, 3, 4, 5, 11, 12, 13, 14, 15, 21, 22, 23];
    let mut table = table_with_discards(1, dead_basic_heng_discards(&hand));
    if let Some(index) = table
        .seats
        .get(&1)
        .unwrap()
        .discards
        .iter()
        .position(|tile| *tile == 5)
    {
        table.seats.get_mut(&1).unwrap().discards.remove(index);
    }
    let projected_melds = vec![test_chi_meld(5)];

    assert!(!has_triplet_or_dragon_pair(&hand, &projected_melds));
    assert_eq!(
        remaining_tile_count_with_melds_after_discards(&hand, &projected_melds, &table, 0, 5, &[]),
        1
    );
    assert!(!can_recover_basic_heng(&hand, &projected_melds, &table, 0));
}

#[test]
fn closed_defense_requires_terminal_or_honor() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 40;
    let hand = vec![2, 2, 2, 5, 5, 5, 12, 12, 12, 14, 17, 23, 27];

    assert_eq!(
        ready_tile_score(&hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC),
        0.0
    );
    assert_eq!(
        one_step_wait_potential(&hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC),
        0.0
    );
    assert!(hand_power(&hand) >= 18.0);
    assert!(should_open_broken_closed_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
}

#[test]
fn route_requirement_scans_ignore_invalid_hand_tiles() {
    let hand = vec![2, 3, 4, 5, 6, 7, 12, 13, 14, 15, 16, 17, 18, 99];

    assert!(!is_honor(99));
    assert_eq!(terminal_or_honor_count(&hand, &[]), 0);
    assert!(!has_terminal_or_honor_with_extra(&hand, &[], None));
}

#[test]
fn route_requirement_scans_ignore_malformed_meld_tiles() {
    let malformed_terminal = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![1, 1],
        from_position: Some(1),
    };
    let malformed_third_suit = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![21, 21],
        from_position: Some(1),
    };
    let hand = vec![2, 3, 4, 5, 6, 7, 12, 13, 14, 15, 16, 17, 18, 18];

    assert!(!has_terminal_or_honor_with_extra(
        &hand,
        &[malformed_terminal.clone()],
        None
    ));
    assert_eq!(terminal_or_honor_count(&hand, &[malformed_terminal]), 0);
    assert_eq!(
        missing_suits(&hand, &[malformed_third_suit.clone()]),
        vec![2]
    );
    assert_eq!(
        suited_tile_count_for_suit(&hand, &[malformed_third_suit], 2),
        0
    );
}

#[test]
fn shenyang_rule_progress_penalizes_unrecoverable_missing_heng_more() {
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 14, 15, 21, 22];
    let live_table = table_with_discards(1, Vec::new());
    let dead_table = table_with_discards(1, dead_basic_heng_discards(&hand));

    assert!(can_recover_basic_heng(&hand, &[], &live_table, 0));
    assert!(!can_recover_basic_heng(&hand, &[], &dead_table, 0));

    let live_score = shenyang_rule_progress_score(&hand, &[], &live_table, 0);
    let dead_score = shenyang_rule_progress_score(&hand, &[], &dead_table, 0);

    assert!(
        dead_score < live_score - 8.0,
        "unrecoverable missing heng should push ordinary hands toward defense"
    );
}

#[test]
fn unrecoverable_basic_rule_counts_dead_heng_requirement() {
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 14, 15, 21, 22];
    let table = table_with_discards(1, dead_basic_heng_discards(&hand));

    assert!(missing_suits(&hand, &[]).is_empty());
    assert!(has_terminal_or_honor_with_extra(&hand, &[], None));
    assert!(!has_triplet_or_dragon_pair(&hand, &[]));
    assert!(!can_recover_basic_heng(&hand, &[], &table, 0));
    assert_eq!(
        unrecoverable_basic_rule_requirement_count(&hand, &[], &table, 0),
        1
    );
}

#[test]
fn visible_tile_counts_ignore_malformed_meld_tiles() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&1).unwrap().melds = vec![
        WsShenyangMahjongMeld {
            kind: ShenyangMahjongMeldKind::PENG,
            tiles: vec![14, 14],
            from_position: Some(0),
        },
        WsShenyangMahjongMeld {
            kind: ShenyangMahjongMeldKind::PENG,
            tiles: vec![99, 99, 99],
            from_position: Some(0),
        },
        test_peng_meld(14),
    ];

    assert_eq!(visible_tile_count(&table, 14), 3);
    assert_eq!(exposed_meld_tile_count(&table, 14), 3);
    assert_eq!(open_meld_tile_count(&table, 14), 3);
    assert_eq!(remaining_tile_count(&[14], &table, 0, 14), 0);
}
