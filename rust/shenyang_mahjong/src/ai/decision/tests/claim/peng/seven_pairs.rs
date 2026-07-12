use super::*;

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
fn required_peng_gain_ignores_malformed_meld_for_four_pair_protection() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 22, 31, 35, 36];
    let malformed_meld = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::CHI,
        tiles: vec![3, 3, 4],
        from_position: Some(1),
    };
    let base = required_peng_gain(&hand, &[], &table, 0, WIN_RULE_RELAXED, 31);

    assert_eq!(pair_count(&hand), 4);
    assert_eq!(valid_meld_count(&[malformed_meld.clone()]), 0);
    assert_eq!(
        required_peng_gain(&hand, &[malformed_meld], &table, 0, WIN_RULE_RELAXED, 31,),
        base
    );
    assert_eq!(
        required_peng_gain(&hand, &[test_chi_meld(3)], &table, 0, WIN_RULE_RELAXED, 31,),
        base - 8.0
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
fn claim_peng_takes_dragon_when_five_pairs_are_live() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 22, 31, 35, 35];

    assert_eq!(pair_count(&hand), 5);
    assert!(should_lock_seven_pairs_plan(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert!(remaining_tile_count(&hand, &table, 0, 2) > 0);
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_takes_dragon_from_live_five_pairs_with_malformed_meld() {
    let mut table = table_with_discards(1, Vec::new());
    let malformed_meld = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![31, 31],
        from_position: Some(2),
    };
    table.seats.get_mut(&0).unwrap().melds = vec![malformed_meld.clone()];
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 22, 31, 35, 35];

    assert_eq!(valid_meld_count(&[malformed_meld.clone()]), 0);
    assert!(should_lock_seven_pairs_plan(
        &hand,
        &[malformed_meld.clone()],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC,
    ));
    assert!(should_claim_dragon_peng_over_live_five_pairs(
        &hand,
        &[malformed_meld],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC,
        35,
        1,
    ));
    assert!(!should_claim_dragon_peng_over_live_five_pairs(
        &hand,
        &[test_peng_meld(31)],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC,
        35,
        1,
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_preserves_five_pairs_when_pair_is_dead() {
    let mut table = table_with_discards(1, vec![2, 2]);
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 22, 31, 35, 35];

    assert_eq!(pair_count(&hand), 5);
    assert!(missing_suits(&hand, &[]).is_empty());
    assert_eq!(remaining_tile_count(&hand, &table, 0, 2), 0);
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn two_fan_capped_claim_peng_uses_dragon_pair_for_speed_over_five_pairs() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(2);
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 22, 31, 35, 35];

    assert!(should_lock_seven_pairs_plan(
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
