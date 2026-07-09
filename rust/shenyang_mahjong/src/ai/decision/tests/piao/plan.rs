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
fn capped_open_basic_route_disables_redundant_piao_plan() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(2);
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(35)];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();
    let hand = vec![1, 1, 2, 3, 11, 11, 12, 13, 21, 21];

    assert!(has_open_meld(melds));
    assert_eq!(estimated_visible_bonus_fan(&hand, melds), 1);
    assert!(piao_plan_score(&hand, melds) >= 20.0);
    assert_eq!(piao_plan_score_for_context(&hand, melds, &table, 0), 0.0);
    assert_eq!(
        piao_discard_bias(&hand, 1, melds, &table, 0, WIN_RULE_SHENYANG_BASIC),
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
