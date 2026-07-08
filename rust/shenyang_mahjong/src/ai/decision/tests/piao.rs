use super::*;

#[test]
fn piao_plan_ignores_malformed_chi_meld() {
    let hand = vec![1, 1, 2, 3, 4, 5, 6, 11, 11, 21, 21, 35, 35];
    let malformed_chi = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::CHI,
        tiles: vec![7, 7, 8],
        from_position: Some(1),
    };
    let valid_chi = test_chi_meld(7);

    assert!(piao_plan_score(&hand, &[]) > 0.0);
    assert_eq!(
        piao_plan_score(&hand, &[malformed_chi.clone()]),
        piao_plan_score(&hand, &[])
    );
    assert_eq!(piao_plan_score(&hand, &[valid_chi]), 0.0);
    assert_eq!(
        piao_threat_level(&[
            malformed_chi,
            test_peng_meld(1),
            test_peng_meld(11),
            test_peng_meld(21),
        ]),
        3
    );
}

#[test]
fn uncapped_room_keeps_piao_plan_biases() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 13, 21, 21, 22, 31, 32];

    assert!(piao_plan_score_for_context(&hand, &[], &table, 0) >= 20.0);
    assert!(piao_discard_bias(&hand, 1, &[], &table, 0, WIN_RULE_SHENYANG_BASIC) < 0.0);
    assert!(early_piao_candidate_discard_bias(&hand, 1, &[], &table, 0) < 0.0);
}

#[test]
fn dealer_ignores_marginal_piao_discard_bias_for_speed() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 13, 21, 21, 22, 31, 32];

    assert!(piao_plan_score(&hand, &[]) >= 20.0);
    assert!(piao_plan_score_for_context(&hand, &[], &table, 0) < 20.0);
    assert_eq!(
        piao_discard_bias(&hand, 1, &[], &table, 0, WIN_RULE_SHENYANG_BASIC),
        0.0
    );
    assert_eq!(
        early_piao_candidate_discard_bias(&hand, 1, &[], &table, 0),
        0.0
    );
}

#[test]
fn one_fan_capped_room_disables_piao_plan_biases() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(1);
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 13, 21, 21, 22, 31, 32];

    assert!(piao_plan_score(&hand, &[]) >= 20.0);
    assert_eq!(piao_plan_score_for_context(&hand, &[], &table, 0), 0.0);
    assert_eq!(
        piao_discard_bias(&hand, 1, &[], &table, 0, WIN_RULE_SHENYANG_BASIC),
        0.0
    );
    assert_eq!(
        early_piao_candidate_discard_bias(&hand, 1, &[], &table, 0),
        0.0
    );
}

#[test]
fn piao_plan_counts_open_triplet_with_two_pairs_as_route() {
    let hand = vec![11, 11, 12, 13, 14, 21, 21, 23, 24, 31];
    let melds = vec![test_peng_meld(1)];

    assert!(piao_plan_score(&hand, &melds) >= 20.0);
}

#[test]
fn piao_context_requires_terminal_or_honor() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![2, 2, 3, 3, 12, 12, 13, 13, 22, 22, 23, 23, 24];

    assert!(piao_plan_score(&hand, &[]) >= 20.0);
    assert_eq!(piao_plan_score_for_context(&hand, &[], &table, 0), 0.0);
}

#[test]
fn piao_context_requires_three_suits() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 2, 2, 3, 3, 4, 4, 11, 11, 12, 13, 31];

    assert!(piao_plan_score(&hand, &[]) >= 20.0);
    assert_eq!(piao_plan_score_for_context(&hand, &[], &table, 0), 0.0);
}

#[test]
fn piao_chi_preservation_uses_dealer_and_cap_context() {
    let table = table_with_discards(3, Vec::new());
    let hand = vec![1, 1, 2, 2, 3, 3, 4, 4, 5, 6, 11, 12, 21];

    assert!(should_preserve_piao_plan_for_chi(&hand, &[], &table, 0));

    let mut dealer_table = table.clone();
    dealer_table.dealer_position = 0;
    assert!(!should_preserve_piao_plan_for_chi(
        &hand,
        &[],
        &dealer_table,
        0
    ));

    let mut capped_table = table.clone();
    capped_table.max_fan = Some(1);
    assert!(!should_preserve_piao_plan_for_chi(
        &hand,
        &[],
        &capped_table,
        0
    ));
}

#[test]
fn discard_after_four_piao_melds_keeps_live_single_wait() {
    let mut table = table_with_discards(1, vec![36, 36, 36]);
    table.seats.get_mut(&0).unwrap().melds = vec![
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(21),
        test_peng_meld(31),
    ];
    let hand = vec![36, 37];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(36)
    );
}

#[test]
fn discard_after_four_piao_melds_rejects_dead_exposed_wind_wait() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(21),
        test_peng_meld(31),
    ];
    let hand = vec![5, 31];

    assert_eq!(remaining_tile_count(&[31], &table, 0, 31), 0);
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(31)
    );
}

#[test]
fn dealer_four_piao_melds_prefers_live_middle_over_low_live_wind_wait() {
    let mut table = table_with_discards(1, vec![31, 31]);
    table.dealer_position = 0;
    table.seats.get_mut(&0).unwrap().melds = vec![
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(21),
        test_peng_meld(32),
    ];
    let hand = vec![5, 31];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();

    assert!(
        piao_single_wait_tile_score(5, &[5], melds, &table, 0, WIN_RULE_SHENYANG_BASIC)
            > piao_single_wait_tile_score(31, &[31], melds, &table, 0, WIN_RULE_SHENYANG_BASIC)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(31)
    );
}

#[test]
fn dealer_piao_single_wait_still_prefers_wider_middle_wait() {
    let mut table = table_with_discards(1, vec![31]);
    table.dealer_position = 0;
    table.wall_count = 32;
    table.seats.get_mut(&0).unwrap().melds = vec![
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(21),
        test_peng_meld(35),
    ];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();

    assert!(
        piao_single_wait_tile_score(5, &[5], melds, &table, 0, WIN_RULE_SHENYANG_BASIC)
            > piao_single_wait_tile_score(31, &[31], melds, &table, 0, WIN_RULE_SHENYANG_BASIC)
    );
}

#[test]
fn capped_four_piao_melds_prefers_wider_wait_over_honor_shape() {
    let mut table = table_with_discards(1, vec![31]);
    table.max_fan = Some(4);
    table.seats.get_mut(&0).unwrap().melds = vec![
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(21),
        test_peng_meld(32),
    ];
    let hand = vec![5, 31];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();

    assert!(
        piao_single_wait_tile_score(5, &[5], melds, &table, 0, WIN_RULE_SHENYANG_BASIC)
            > piao_single_wait_tile_score(31, &[31], melds, &table, 0, WIN_RULE_SHENYANG_BASIC)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(31)
    );
}

#[test]
fn piao_single_wait_discard_avoids_pure_one_suit_threat_tile() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 32;
    table.seats.get_mut(&0).unwrap().melds = vec![
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(21),
        test_peng_meld(32),
    ];
    table.seats.get_mut(&1).unwrap().melds = vec![test_chi_meld(2), test_peng_meld(7)];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();
    let hand = vec![5, 31];

    assert!(
        piao_single_wait_tile_score(31, &[31], melds, &table, 0, WIN_RULE_SHENYANG_BASIC)
            > piao_single_wait_tile_score(5, &[5], melds, &table, 0, WIN_RULE_SHENYANG_BASIC)
    );
    assert!(
        pure_one_suit_threat_discard_bias(&table, 0, 5, 1)
            < pure_one_suit_threat_discard_bias(&table, 0, 31, 1)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(31)
    );
}

#[test]
fn discard_avoids_live_pair_against_piao_threat() {
    let mut table = table_with_discards(1, vec![31]);
    table.wall_count = 32;
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];
    let hand = vec![2, 3, 4, 5, 5, 6, 7, 11, 12, 13, 21, 22, 23, 31];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(31)
    );
}

#[test]
fn discard_follows_public_tile_over_live_pair_against_piao_threat() {
    let mut table = table_with_discards(1, vec![14]);
    table.wall_count = 32;
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];
    let hand = vec![2, 3, 4, 5, 5, 6, 7, 11, 12, 13, 14, 21, 22, 23];

    assert!(
        opponent_threat_discard_bias(&table, 0, 5, 2)
            < opponent_threat_discard_bias(&table, 0, 14, 1)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(14)
    );
}

#[test]
fn incomplete_sequence_bias_does_not_override_piao_pair_plan() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 4, 5, 11, 11, 12, 13, 21, 21, 22, 23, 35, 35];

    assert_eq!(
        incomplete_sequence_discard_bias(&hand, 1, &[], &table, 0, WIN_RULE_SHENYANG_BASIC),
        0.0
    );
    assert!(!matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(11 | 21 | 35)
    ));
}

#[test]
fn discard_three_pair_piao_candidate_still_prefers_wind_before_single_dragon() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 4, 5, 11, 11, 12, 13, 21, 21, 22, 23, 31, 35];

    assert_eq!(pair_count(&hand), 3);
    assert!(is_closed_early_piao_candidate(&hand, &[], &table, 0));
    assert!(piao_plan_score_for_context(&hand, &[], &table, 0) > 0.0);
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(31)
    );
}

#[test]
fn piao_plan_scores_three_pair_three_suit_candidate() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 4, 5, 11, 11, 12, 13, 21, 21, 22, 23, 31, 35];

    assert_eq!(pair_count(&hand), 3);
    assert_eq!(piao_plan_score_for_context(&hand, &[], &table, 0), 15.0);
}

#[test]
fn piao_plan_rejects_three_pair_candidate_missing_suit() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 4, 5, 11, 11, 12, 13, 14, 14, 16, 17, 31, 35];

    assert_eq!(pair_count(&hand), 3);
    assert_eq!(piao_plan_score_for_context(&hand, &[], &table, 0), 0.0);
}

#[test]
fn dealer_discounts_three_pair_piao_candidate_for_speed() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    let hand = vec![1, 1, 4, 5, 11, 11, 12, 13, 21, 21, 22, 23, 31, 35];

    assert_eq!(piao_plan_score_for_context(&hand, &[], &table, 0), 5.25);
}

#[test]
fn discard_four_pair_piao_candidate_clears_single_dragon_before_wind() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 4, 4, 11, 11, 12, 13, 21, 21, 22, 23, 31, 35];

    assert_eq!(pair_count(&hand), 4);
    assert!(piao_plan_score_for_context(&hand, &[], &table, 0) > 0.0);
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(35)
    );
}

#[test]
fn discard_preserves_four_pair_piao_candidate_over_public_pair_tile() {
    let mut table = table_with_discards(1, vec![11, 11]);
    table.wall_count = 36;
    let hand = vec![1, 1, 4, 5, 11, 11, 12, 13, 21, 21, 22, 23, 31, 31];

    assert!(!matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(1 | 11 | 21 | 31)
    ));
}

#[test]
fn discard_preserves_open_piao_pairs_over_public_pair_tile() {
    let mut table = table_with_discards(1, vec![11, 11]);
    table.wall_count = 36;
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    let hand = vec![11, 11, 12, 21, 21, 22, 23, 24, 31, 35, 36];

    assert!(!matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(11 | 21)
    ));
}

#[test]
fn piao_discard_bias_locks_pairs_after_two_triplet_groups() {
    let table = table_with_discards(1, Vec::new());
    let one_group_melds = vec![test_peng_meld(1)];
    let one_group_hand = vec![11, 11, 21, 21, 22, 23, 31, 35, 36, 37];
    let two_group_melds = vec![test_peng_meld(1), test_peng_meld(11)];
    let two_group_hand = vec![21, 21, 22, 23, 31, 35, 36, 37];

    assert_eq!(
        piao_discard_bias(
            &one_group_hand,
            21,
            &one_group_melds,
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC
        ),
        -16.0
    );
    assert_eq!(
        piao_discard_bias(
            &two_group_hand,
            21,
            &two_group_melds,
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC
        ),
        -24.0
    );
}

#[test]
fn piao_discard_bias_protects_live_dragon_pair() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 4, 5, 11, 11, 12, 13, 21, 21, 22, 23, 35, 35];

    let middle_pair = piao_discard_bias(&hand, 11, &[], &table, 0, WIN_RULE_SHENYANG_BASIC);
    let dragon_pair = piao_discard_bias(&hand, 35, &[], &table, 0, WIN_RULE_SHENYANG_BASIC);

    assert_eq!(pair_count(&hand), 4);
    assert!(remaining_tile_count(&hand, &table, 0, 35) > 0);
    assert!(dragon_pair < middle_pair);
}

#[test]
fn piao_discard_bias_protects_live_pair_over_dead_pair() {
    let table = table_with_discards(1, vec![11, 11]);
    let hand = vec![1, 1, 4, 4, 11, 11, 12, 13, 21, 21, 22, 23, 31, 35];

    let dead_pair = piao_discard_bias(&hand, 11, &[], &table, 0, WIN_RULE_SHENYANG_BASIC);
    let live_pair = piao_discard_bias(&hand, 21, &[], &table, 0, WIN_RULE_SHENYANG_BASIC);

    assert_eq!(pair_count(&hand), 4);
    assert_eq!(remaining_tile_count(&hand, &table, 0, 11), 0);
    assert!(remaining_tile_count(&hand, &table, 0, 21) > 0);
    assert!(live_pair < dead_pair);
}

#[test]
fn discard_preserves_committed_piao_pair_over_public_pair_tile() {
    let mut table = table_with_discards(1, vec![21, 21]);
    table.wall_count = 36;
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1), test_peng_meld(11)];
    let hand = vec![21, 21, 22, 23, 24, 31, 35, 36];

    assert!(!matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(21)
    ));
}

#[test]
fn discard_preserves_only_terminal_or_honor_for_piao_plan_even_relaxed() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![2, 2, 5, 5, 8, 8, 12, 12, 15, 16, 18, 22, 24, 31];

    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(31)
    );
}

#[test]
fn discard_preserves_only_third_suit_for_piao_plan_even_relaxed() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![2, 2, 2, 5, 5, 8, 8, 12, 12, 12, 15, 15, 22, 31];

    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(22)
    );
}

#[test]
fn discard_preserves_three_pair_three_suit_piao_candidate() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 4, 5, 6, 11, 11, 12, 13, 14, 21, 21, 22, 23];

    assert!(!matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(1 | 11 | 21)
    ));
}

#[test]
fn discard_preserves_three_pair_piao_candidate_over_public_pair_tile() {
    let mut table = table_with_discards(1, vec![11, 11]);
    table.wall_count = 36;
    let hand = vec![1, 1, 4, 5, 6, 11, 11, 12, 13, 14, 21, 21, 22, 23];

    assert!(is_closed_early_piao_candidate(&hand, &[], &table, 0));
    let chosen = choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC);
    assert!(
        !matches!(chosen, Some(1 | 11 | 21)),
        "unexpected pair discard: {chosen:?}"
    );
}

#[test]
fn discard_preserves_only_terminal_or_honor_for_three_pair_piao_candidate_even_relaxed() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![2, 2, 5, 5, 8, 8, 12, 14, 15, 16, 22, 24, 26, 31];

    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(31)
    );
}

#[test]
fn discard_preserves_only_third_suit_for_three_pair_piao_candidate_even_relaxed() {
    let mut table = table_with_discards(1, vec![24]);
    table.wall_count = 36;
    let hand = vec![2, 2, 5, 5, 8, 8, 12, 14, 15, 16, 24, 31, 35, 37];

    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(24)
    );
}

#[test]
fn seven_pairs_wait_discard_avoids_piao_missing_suit_threat_tile() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 32;
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(11), test_peng_meld(21), test_peng_meld(35)];
    let hand = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 21, 21, 22, 22, 31];
    let wind_wait = remove_n_tiles(&hand, 5, 1);
    let middle_wait = remove_n_tiles(&hand, 31, 1);

    assert_eq!(pair_count(&hand), 6);
    assert!(
        seven_pairs_wait_tile_score(31, &wind_wait, &table, 0)
            > seven_pairs_wait_tile_score(5, &middle_wait, &table, 0)
    );
    assert!(
        opponent_threat_discard_bias(&table, 0, 5, 1)
            < opponent_threat_discard_bias(&table, 0, 31, 1)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(31)
    );
}

#[test]
fn late_open_hand_avoids_live_tile_against_four_piao_melds() {
    let mut seats = HashMap::new();
    seats.insert(
        0,
        AiSeatView {
            position: 0,
            hand_count: 1,
            discards: vec![31, 33, 19, 16, 1, 31, 21, 8, 12, 32, 3, 4, 2, 15, 22, 4],
            melds: vec![
                test_peng_meld(37),
                test_peng_meld(5),
                test_peng_meld(6),
                test_peng_meld(25),
            ],
        },
    );
    seats.insert(
        1,
        AiSeatView {
            position: 1,
            hand_count: 11,
            discards: vec![21, 4, 15, 35, 11, 12, 16, 34, 33, 33, 35, 35],
            melds: vec![test_peng_meld(19)],
        },
    );
    seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 13,
            discards: vec![34, 1, 22, 33, 12, 23, 5, 3, 25, 28, 1, 29],
            melds: Vec::new(),
        },
    );
    seats.insert(
        3,
        AiSeatView {
            position: 3,
            hand_count: 8,
            discards: vec![34, 32, 22, 8, 35, 16, 11, 12, 17, 3, 28, 28],
            melds: vec![test_peng_meld(7), test_peng_meld(26)],
        },
    );
    let table = AiPublicTable {
        current_position: 1,
        dealer_position: 0,
        wall_count: 31,
        max_fan: Some(4),
        claim_window: None,
        seats,
    };
    let hand = vec![7, 8, 9, 9, 9, 13, 22, 23, 24, 36, 36];

    assert_ne!(
        choose_discard_from_view(&hand, &table, 1, WIN_RULE_SHENYANG_BASIC),
        Some(13)
    );
}

#[test]
fn opponent_piao_threat_ignores_player_after_chi_meld() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&1).unwrap().melds = vec![
        test_chi_meld(2),
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(21),
    ];

    assert_eq!(piao_threat_level(&table.seats.get(&1).unwrap().melds), 0);
    assert_eq!(opponent_threat_discard_bias(&table, 0, 5, 2), 0.0);
}

#[test]
fn opponent_four_piao_threat_penalizes_live_pair_more_than_singleton() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().hand_count = 2;
    table.seats.get_mut(&1).unwrap().melds = vec![
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(21),
        test_peng_meld(31),
    ];

    assert!(
        opponent_threat_discard_bias(&table, 0, 5, 2)
            < opponent_threat_discard_bias(&table, 0, 6, 1)
    );
}

#[test]
fn opponent_four_piao_threat_ignores_impossible_two_missing_suits() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().hand_count = 2;
    table.seats.get_mut(&1).unwrap().melds = vec![
        test_peng_meld(2),
        test_peng_meld(3),
        test_peng_meld(4),
        test_peng_meld(5),
    ];

    assert_eq!(
        piao_missing_suits_from_melds(&table.seats.get(&1).unwrap().melds),
        vec![1, 2]
    );
    assert!(piao_threat_cannot_satisfy_three_suits(
        &table.seats.get(&1).unwrap().melds,
        table.seats.get(&1).unwrap().hand_count
    ));
    assert_eq!(opponent_threat_discard_bias(&table, 0, 31, 1), 0.0);

    table.seats.get_mut(&1).unwrap().melds = vec![
        test_peng_meld(2),
        test_peng_meld(3),
        test_peng_meld(12),
        test_peng_meld(13),
    ];
    assert_eq!(
        piao_missing_suits_from_melds(&table.seats.get(&1).unwrap().melds),
        vec![2]
    );
    assert!(!piao_threat_cannot_satisfy_three_suits(
        &table.seats.get(&1).unwrap().melds,
        table.seats.get(&1).unwrap().hand_count
    ));
    assert!(opponent_threat_discard_bias(&table, 0, 21, 1) < 0.0);
}

#[test]
fn opponent_four_piao_threat_penalizes_missing_suit_wait_tile() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().hand_count = 2;
    table.seats.get_mut(&1).unwrap().melds = vec![
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(12),
        test_peng_meld(31),
    ];

    assert_eq!(
        piao_missing_suits_from_melds(&table.seats.get(&1).unwrap().melds),
        vec![2]
    );
    assert!(
        opponent_threat_discard_bias(&table, 0, 25, 1)
            < opponent_threat_discard_bias(&table, 0, 15, 1)
    );
}

#[test]
fn piao_threat_penalizes_live_wind_pair_more_than_terminal_singleton() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];

    assert!(
        opponent_threat_discard_bias(&table, 0, 31, 2)
            < opponent_threat_discard_bias(&table, 0, 9, 1)
    );
}

#[test]
fn piao_threat_needing_yaojiu_penalizes_live_terminal_over_middle() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 32;
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(2), test_peng_meld(12), test_peng_meld(22)];

    assert!(piao_needs_terminal_or_honor_from_melds(
        &table.seats.get(&1).unwrap().melds
    ));
    assert!(
        opponent_threat_discard_bias(&table, 0, 9, 1)
            < opponent_threat_discard_bias(&table, 0, 5, 1)
    );
    assert!(
        opponent_threat_discard_bias(&table, 0, 31, 1)
            < opponent_threat_discard_bias(&table, 0, 5, 1)
    );
}

#[test]
fn piao_threat_discounts_exposed_meld_tiles() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_peng_meld(6)],
        },
    );

    assert!(
        opponent_threat_discard_bias(&table, 0, 6, 1)
            > opponent_threat_discard_bias(&table, 0, 5, 1)
    );
}
