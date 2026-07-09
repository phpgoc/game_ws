use super::*;

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
        pure_one_suit_plan_score_for_context(&hand, &melds, &table, 0),
        0.0
    );
}

#[test]
fn capped_pure_one_suit_route_can_discard_last_honor_when_suits_are_missing() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(1);
    let hand = vec![2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 8, 8, 12, 31];

    assert!(
        pure_one_suit_plan_score_for_context(&remove_n_tiles(&hand, 31, 1), &[], &table, 0) > 0.0
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(31)
    );
}

#[test]
fn pure_one_suit_rule_progress_does_not_require_opening() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 1, 2, 2, 3, 3, 4, 5, 6, 7, 8, 9];
    let pure_score = pure_one_suit_plan_score_for_context(&hand, &[], &table, 0);

    assert!(pure_score > 0.0);
    assert!(!has_open_meld(&[]));
    assert_eq!(
        shenyang_rule_progress_score(&hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC),
        pure_score
    );
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
fn discard_can_pursue_pure_one_suit_when_shape_is_strong() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 7, 8, 9, 11];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(11)
    );
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
fn discard_clears_honor_before_off_suit_singleton_for_pure_one_suit_plan() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 11, 31, 35, 36];

    assert!(matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(31 | 35 | 36)
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
fn non_dealer_relaxed_pure_one_suit_plan_can_break_three_suits() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 1;
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 11, 12, 13, 21];
    let after_discard = remove_n_tiles(&hand, 21, 1);

    assert!(pure_one_suit_plan_score_for_context(&after_discard, &[], &table, 0) > 0.0);
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

#[test]
fn discard_starts_pure_one_suit_plan_at_eight_main_suit_tiles() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 11, 12, 21, 22, 31, 35];

    assert!(matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(11 | 12 | 21 | 22 | 31 | 35)
    ));
}
