use super::*;

#[test]
fn discard_four_pair_piao_candidate_clears_single_dragon_before_wind() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 4, 4, 11, 11, 12, 13, 21, 21, 22, 23, 31, 35];

    assert_eq!(pair_count(&hand), 4);
    assert!(piao_plan_score_for_context(&hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC) > 0.0);
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(35)
    );
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
fn discard_preserves_only_terminal_or_honor_for_piao_plan() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![2, 2, 5, 5, 8, 8, 12, 12, 15, 16, 18, 22, 24, 31];

    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(31)
    );
}

#[test]
fn discard_preserves_only_terminal_or_honor_for_three_pair_piao_candidate() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![2, 2, 5, 5, 8, 8, 12, 14, 15, 16, 22, 24, 26, 31];

    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(31)
    );
}

#[test]
fn discard_preserves_only_third_suit_for_piao_plan() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![2, 2, 2, 5, 5, 8, 8, 12, 12, 12, 15, 15, 22, 31];

    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(22)
    );
}

#[test]
fn discard_preserves_only_third_suit_for_three_pair_piao_candidate() {
    let mut table = table_with_discards(1, vec![24]);
    table.wall_count = 36;
    let hand = vec![2, 2, 5, 5, 8, 8, 12, 14, 15, 16, 24, 31, 35, 37];

    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(24)
    );
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
fn discard_preserves_three_pair_piao_candidate_over_public_pair_tile() {
    let mut table = table_with_discards(1, vec![11, 11]);
    table.wall_count = 36;
    let hand = vec![1, 1, 4, 5, 6, 11, 11, 12, 13, 14, 21, 21, 22, 23];

    assert!(is_closed_early_piao_candidate(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    let chosen = choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC);
    assert!(
        !matches!(chosen, Some(1 | 11 | 21)),
        "unexpected pair discard: {chosen:?}"
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
fn discard_three_pair_piao_candidate_still_prefers_wind_before_single_dragon() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 4, 5, 11, 11, 12, 13, 21, 21, 22, 23, 31, 35];

    assert_eq!(pair_count(&hand), 3);
    assert!(is_closed_early_piao_candidate(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert!(piao_plan_score_for_context(&hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC) > 0.0);
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(31)
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
    assert_eq!(piao_discard_bias(&hand, 35, &[], &table, 0, 0), dragon_pair);

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
