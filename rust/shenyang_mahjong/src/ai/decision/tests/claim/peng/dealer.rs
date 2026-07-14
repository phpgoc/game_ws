use super::*;

#[test]
fn dealer_claim_chi_waits_until_half_round_for_basic_ready() {
    let mut table = table_with_discards(3, Vec::new());
    table.dealer_position = 0;
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

    table.wall_count = LATE_PRESSURE_WALL_COUNT;
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Chi {
            consume_tiles: vec![1, 2]
        })
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
fn dealer_claim_peng_passes_open_pure_defense_hand_late() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    table.wall_count = 40;
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
    table.claim_window = Some(AiClaimView {
        tile: 2,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let melds = table.seats.get(&0).unwrap().melds.as_slice();
    let hand = vec![2, 2, 5, 8, 12, 14, 17, 21, 24, 27];

    assert!(has_door_opening_meld(melds, &table));
    assert_eq!(
        ready_tile_score(&hand, melds, &table, 0, WIN_RULE_SHENYANG_BASIC),
        0.0
    );
    assert_eq!(
        one_step_wait_potential(&hand, melds, &table, 0, WIN_RULE_SHENYANG_BASIC),
        0.0
    );
    assert!(hand_power(&hand) < 18.0);
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
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
