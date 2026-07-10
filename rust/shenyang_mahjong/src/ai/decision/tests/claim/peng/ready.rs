use super::*;

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
fn claim_ready_hand_passes_peng_when_fan_exceeds_half_cap() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(4);
    table.seats.get_mut(&0).unwrap().melds = vec![test_gang_meld(35)];
    table.claim_window = Some(AiClaimView {
        tile: 9,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let melds = table.seats.get(&0).unwrap().melds.as_slice();
    let hand = vec![1, 2, 3, 9, 9, 11, 12, 13, 21, 22];

    assert!(ready_tile_score(&hand, melds, &table, 0, WIN_RULE_SHENYANG_BASIC) > 0.0);
    assert!(ready_visible_fan_exceeds_half_cap(
        &hand,
        melds,
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
