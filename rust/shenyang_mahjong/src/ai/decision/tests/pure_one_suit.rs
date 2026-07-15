use super::*;

#[test]
fn capped_basic_foundation_disables_redundant_closed_pure_one_suit_plan() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(2);
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 11, 21, 35, 35, 35];

    assert!(has_basic_normal_route_foundation(
        &hand,
        &[],
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_eq!(estimated_visible_bonus_fan(&hand, &[]), 1);
    assert!(capped_basic_route_foundation_visible_fan_reaches_cap(
        &hand,
        &[],
        &table,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert!(pure_one_suit_plan_score(&hand, &[]) > 0.0);
    assert_eq!(
        pure_one_suit_plan_score_for_context(&hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC),
        0.0
    );
}

#[test]
fn half_capped_basic_foundation_disables_closed_pure_one_suit_chase() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(4);
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 11, 21, 35, 35, 35, 35];

    assert!(has_basic_normal_route_foundation(
        &hand,
        &[],
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_eq!(estimated_visible_bonus_fan(&hand, &[]), 2);
    assert!(pure_one_suit_plan_score(&hand, &[]) > 0.0);
    assert_eq!(
        pure_one_suit_plan_score_for_context(&hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC),
        0.0
    );
}

#[test]
fn capped_basic_foundation_preserves_three_suits_over_pure_one_suit_chase() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(2);
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 11, 21, 31, 35, 35, 35];
    let after_discard = remove_n_tiles(&hand, 11, 1);

    assert!(capped_basic_route_foundation_visible_fan_reaches_cap(
        &hand,
        &[],
        &table,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert!(
        three_suits_discard_bias(&after_discard, &[], &table, 0, 11, WIN_RULE_SHENYANG_BASIC)
            <= -80.0
    );
    assert!(!matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(11 | 21)
    ));
}

#[test]
fn capped_discard_does_not_chase_pure_one_suit_when_three_suits_remain() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(1);
    let hand = vec![1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 7, 8, 11, 21];

    assert!(!matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(11 | 21)
    ));
}

#[test]
fn capped_open_basic_route_disables_redundant_pure_one_suit_plan() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(2);
    let melds = vec![test_gang_meld(1)];
    let hand = vec![2, 3, 3, 4, 4, 5, 6, 7, 8, 9, 11, 21];

    assert!(has_open_meld(&melds));
    assert!(missing_suits(&hand, &melds).is_empty());
    assert!(has_terminal_or_honor_with_extra(&hand, &melds, None));
    assert!(has_triplet_or_dragon_pair(&hand, &melds));
    assert_eq!(estimated_visible_bonus_fan(&hand, &melds), 1);
    assert!(pure_one_suit_plan_score(&hand, &melds) > 0.0);
    assert_eq!(
        pure_one_suit_plan_score_for_context(&hand, &melds, &table, 0, WIN_RULE_SHENYANG_BASIC),
        0.0
    );
}

#[test]
fn capped_pure_one_suit_route_can_discard_last_honor_when_suits_are_missing() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(1);
    let hand = vec![2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 8, 8, 12, 31];

    assert!(
        pure_one_suit_plan_score_for_context(
            &remove_n_tiles(&hand, 31, 1),
            &[],
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC,
        ) > 0.0
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(31)
    );
}

#[test]
fn dealer_can_chase_overwhelming_pure_one_suit_shape() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    let hand = vec![1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 11, 35];

    assert!(matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(11 | 35)
    ));
}

#[test]
fn dealer_can_start_overwhelming_pure_one_suit_by_clearing_third_blocker() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    let hand = vec![1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 7, 11, 12, 13];

    assert!(matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(11 | 12 | 13)
    ));
}

#[test]
fn dealer_discard_does_not_chase_early_pure_one_suit_by_breaking_second_suit() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 11, 12, 31, 35];

    assert!(!matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(11 | 12)
    ));
}

#[test]
fn dealer_does_not_start_pure_one_suit_plan_at_eight_main_suit_tiles() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 11, 12, 21, 22, 31, 35];

    assert!(!matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(11 | 12 | 21 | 22)
    ));
}

#[test]
fn discard_can_pursue_pure_one_suit_when_shape_is_strong() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 7, 8, 9, 11];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(11)
    );
}

#[test]
fn discard_clears_honor_before_off_suit_singleton_for_pure_one_suit_plan() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 11, 31, 35, 36];

    assert!(matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(31 | 35 | 36)
    ));
}

#[test]
fn discard_clears_honor_when_early_pure_one_suit_plan_is_available() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 11, 12, 31, 35];

    assert!(matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(31 | 35)
    ));
}

#[test]
fn discard_clears_last_honor_for_pure_one_suit_without_terminal_need() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 8, 8, 12, 31];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(31)
    );
}

#[test]
fn discard_starts_pure_one_suit_plan_at_eight_main_suit_tiles() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 11, 12, 21, 22, 31, 35];

    assert!(matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(11 | 12 | 21 | 22 | 31 | 35)
    ));
}

#[test]
fn pure_one_suit_plan_abandons_exhausted_main_suit() {
    let hand = vec![1, 1, 2, 2, 3, 3, 4, 4, 11, 12, 21, 22, 31, 35];
    let live_table = table_with_discards(1, Vec::new());

    assert!(pure_one_suit_plan_score(&hand, &[]) > 0.0);
    assert!(
        pure_one_suit_plan_score_for_context(&hand, &[], &live_table, 0, WIN_RULE_SHENYANG_BASIC,)
            > 0.0
    );
    assert!(
        pure_one_suit_discard_bias(&hand, 11, &[], &live_table, 0, WIN_RULE_SHENYANG_BASIC,) > 0.0
    );

    let exhausted_main_suit = (1..=9)
        .flat_map(|tile| {
            let own_count = hand.iter().filter(|item| **item == tile).count();
            std::iter::repeat_n(tile, 4 - own_count)
        })
        .collect::<Vec<_>>();
    let exhausted_table = table_with_discards(1, exhausted_main_suit);

    assert_eq!(live_tile_count_for_suit(&hand, &exhausted_table, 0), 0);
    assert!(pure_one_suit_plan_score(&hand, &[]) > 0.0);
    assert_eq!(
        pure_one_suit_plan_score_for_context(
            &hand,
            &[],
            &exhausted_table,
            0,
            WIN_RULE_SHENYANG_BASIC,
        ),
        0.0
    );
    assert_eq!(
        pure_one_suit_discard_bias(&hand, 11, &[], &exhausted_table, 0, WIN_RULE_SHENYANG_BASIC,),
        0.0
    );
}

#[test]
fn pure_one_suit_plan_requires_enough_live_tiles_for_blockers() {
    let hand = vec![1, 1, 2, 2, 3, 3, 4, 4, 11, 12, 21, 22, 31, 35];
    let all_other_main_suit_tiles = (1..=9)
        .flat_map(|tile| {
            let own_count = hand.iter().filter(|item| **item == tile).count();
            std::iter::repeat_n(tile, 4 - own_count)
        })
        .collect::<Vec<_>>();
    let (_, _, blockers) = pure_one_suit_shape(&hand, &[]).expect("pure shape");

    assert_eq!(blockers, 6);
    let insufficient_table = table_with_discards(
        1,
        all_other_main_suit_tiles[..all_other_main_suit_tiles.len() - 5].to_vec(),
    );
    assert_eq!(live_tile_count_for_suit(&hand, &insufficient_table, 0), 5);
    assert_eq!(
        pure_one_suit_plan_score_for_context(
            &hand,
            &[],
            &insufficient_table,
            0,
            WIN_RULE_SHENYANG_BASIC,
        ),
        0.0
    );

    let sufficient_table = table_with_discards(
        1,
        all_other_main_suit_tiles[..all_other_main_suit_tiles.len() - blockers].to_vec(),
    );
    assert_eq!(
        live_tile_count_for_suit(&hand, &sufficient_table, 0),
        blockers as i32
    );
    assert!(
        pure_one_suit_plan_score_for_context(
            &hand,
            &[],
            &sufficient_table,
            0,
            WIN_RULE_SHENYANG_BASIC,
        ) > 0.0
    );
}

#[test]
fn pure_one_suit_plan_requires_enough_wall_tiles_for_blockers() {
    let hand = vec![1, 1, 2, 2, 3, 3, 4, 4, 11, 12, 21, 31, 35];
    let (_, _, blockers) = pure_one_suit_shape(&hand, &[]).expect("pure shape");
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = blockers;

    assert_eq!(blockers, 5);
    assert!(pure_one_suit_plan_score(&hand, &[]) > 0.0);
    assert_eq!(
        pure_one_suit_plan_score_for_context(&hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC),
        0.0
    );

    table.wall_count = blockers + 1;
    assert!(
        pure_one_suit_plan_score_for_context(&hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC) > 0.0
    );
}

#[test]
fn pure_one_suit_plan_only_counts_visible_main_suit_claim_opportunity() {
    let hand = vec![1, 1, 2, 2, 3, 3, 4, 4, 11, 12, 21, 31, 35];
    let mut forged_table = table_with_discards(1, Vec::new());
    forged_table.wall_count = 5;
    forged_table.claim_window = Some(AiClaimView {
        tile: 4,
        from_position: 1,
        eligible_positions: vec![0],
    });

    assert_eq!(
        pure_one_suit_plan_score_for_context(&hand, &[], &forged_table, 0, WIN_RULE_SHENYANG_BASIC,),
        0.0
    );

    let mut valid_table = forged_table.clone();
    valid_table.seats.get_mut(&1).unwrap().discards.push(4);
    assert!(
        pure_one_suit_plan_score_for_context(&hand, &[], &valid_table, 0, WIN_RULE_SHENYANG_BASIC,)
            > 0.0
    );
}

#[test]
fn non_dealer_relaxed_pure_one_suit_plan_can_break_three_suits() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 1;
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 11, 12, 13, 21];
    let after_discard = remove_n_tiles(&hand, 21, 1);

    assert!(
        pure_one_suit_plan_score_for_context(&after_discard, &[], &table, 0, WIN_RULE_RELAXED,)
            > 0.0
    );
    assert_eq!(
        three_suits_discard_bias(&after_discard, &[], &table, 0, 21, WIN_RULE_RELAXED),
        0.0
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(21)
    );
}

#[test]
fn pure_one_suit_plan_ignores_invalid_hand_blockers() {
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 91, 92, 93, 94, 95, 96, 97];

    assert_eq!(pure_one_suit_shape(&hand, &[]), Some((0, 8, 0)));
    assert!(pure_one_suit_plan_score(&hand, &[]) > 0.0);
}

#[test]
fn pure_one_suit_plan_ignores_malformed_off_suit_meld() {
    let hand = vec![1, 1, 2, 2, 3, 3, 4, 5, 6, 7, 8, 9, 9];
    let malformed_off_suit = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![11, 11],
        from_position: Some(1),
    };
    let valid_off_suit = test_peng_meld(11);

    assert!(pure_one_suit_plan_score(&hand, &[]) > 0.0);
    assert_eq!(
        pure_one_suit_plan_score(&hand, &[malformed_off_suit]),
        pure_one_suit_plan_score(&hand, &[])
    );
    assert_eq!(pure_one_suit_plan_score(&hand, &[valid_off_suit]), 0.0);
}

#[test]
fn pure_one_suit_route_can_discard_last_main_suit_terminal() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 8, 8];
    let after_discard = remove_n_tiles(&hand, 1, 1);

    assert!(has_terminal_or_honor_with_extra(
        &after_discard,
        &[],
        Some(1)
    ));
    assert!(!has_terminal_or_honor_with_extra(&after_discard, &[], None));
    assert!(
        pure_one_suit_plan_score_for_context(
            &after_discard,
            &[],
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC,
        ) > 0.0
    );
    assert!(!violates_basic_terminal_or_honor_discard(
        &after_discard,
        &[],
        &table,
        0,
        1,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_eq!(
        terminal_or_honor_discard_bias(&after_discard, &[], &table, 0, 1, WIN_RULE_SHENYANG_BASIC),
        0.0
    );
}

#[test]
fn pure_one_suit_rule_progress_does_not_require_opening() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 1, 2, 2, 3, 3, 4, 5, 6, 7, 8, 9];
    let pure_score =
        pure_one_suit_plan_score_for_context(&hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC);

    assert!(pure_score > 0.0);
    assert!(!has_open_meld(&[]));
    assert_eq!(
        shenyang_rule_progress_score(&hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC),
        pure_score
    );
}

#[test]
fn threatening_dealer_disables_marginal_closed_pure_one_suit_plan() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&1).unwrap().hand_count = 7;
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(23), test_peng_meld(24)];
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 11, 12, 21, 22, 31, 35];

    table.dealer_position = 3;
    assert!(!dealer_opponent_has_major_threat(
        &table,
        0,
        WIN_RULE_RELAXED
    ));
    assert_eq!(pure_one_suit_plan_score(&hand, &[]), 10.0);
    assert_eq!(
        pure_one_suit_plan_score_for_context(&hand, &[], &table, 0, WIN_RULE_RELAXED),
        10.0
    );
    assert!(pure_one_suit_discard_bias(&hand, 11, &[], &table, 0, WIN_RULE_RELAXED) > 0.0);

    table.dealer_position = 1;
    assert!(dealer_opponent_has_major_threat(
        &table,
        0,
        WIN_RULE_RELAXED
    ));
    assert_eq!(
        pure_one_suit_plan_score_for_context(&hand, &[], &table, 0, WIN_RULE_RELAXED),
        0.0
    );
    assert_eq!(
        pure_one_suit_discard_bias(&hand, 11, &[], &table, 0, WIN_RULE_RELAXED),
        0.0
    );
}

#[test]
fn threatening_dealer_preserves_committed_pure_one_suit_plans() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 1;
    table.seats.get_mut(&1).unwrap().hand_count = 7;
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(23), test_peng_meld(24)];
    let open_melds = vec![test_peng_meld(1)];
    let open_hand = vec![2, 3, 4, 5, 6, 7, 8, 11, 12, 21];
    let overwhelming_hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 9, 11, 31];

    assert!(dealer_opponent_has_major_threat(
        &table,
        0,
        WIN_RULE_RELAXED
    ));
    assert_eq!(pure_one_suit_plan_score(&open_hand, &open_melds), 23.0);
    assert_eq!(
        pure_one_suit_plan_score_for_context(&open_hand, &open_melds, &table, 0, WIN_RULE_RELAXED,),
        23.0
    );
    assert_eq!(pure_one_suit_plan_score(&overwhelming_hand, &[]), 28.0);
    assert_eq!(
        pure_one_suit_plan_score_for_context(&overwhelming_hand, &[], &table, 0, WIN_RULE_RELAXED,),
        28.0
    );
}

#[test]
fn closed_dealer_threat_only_suppresses_basic_pure_one_suit_plan() {
    let mut table = table_with_discards(1, vec![31, 32, 33, 34, 35, 36]);
    table.dealer_position = 1;
    table.wall_count = LATE_PRESSURE_WALL_COUNT;
    table.seats.get_mut(&1).unwrap().hand_count = 13;
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 11, 12, 21, 22, 31, 35];

    assert!(dealer_opponent_has_major_threat(
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert!(!dealer_opponent_has_major_threat(
        &table,
        0,
        WIN_RULE_RELAXED
    ));
    assert_eq!(
        pure_one_suit_plan_score_for_context(&hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC),
        0.0
    );
    assert_eq!(
        pure_one_suit_plan_score_for_context(&hand, &[], &table, 0, WIN_RULE_RELAXED),
        pure_one_suit_plan_score(&hand, &[])
    );
}

#[test]
fn seven_pairs_wait_discard_avoids_pure_one_suit_threat_tile() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 32;
    table.seats.get_mut(&1).unwrap().melds = vec![test_chi_meld(2), test_peng_meld(7)];
    let hand = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 18, 21, 21, 22, 22];

    assert_eq!(pair_count(&hand), 6);
    assert!(
        pure_one_suit_threat_discard_bias(&table, 0, 5, 1)
            < pure_one_suit_threat_discard_bias(&table, 0, 18, 1)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(18)
    );
}
