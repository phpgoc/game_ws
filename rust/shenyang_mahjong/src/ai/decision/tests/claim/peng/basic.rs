use super::*;

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
