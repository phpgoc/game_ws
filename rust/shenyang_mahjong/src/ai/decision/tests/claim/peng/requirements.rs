use super::*;

#[test]
fn claim_peng_allows_dragon_when_missing_suit_can_still_be_recovered() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 31, 35, 35, 36];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_opens_missing_suit_hand_without_a_legal_near_ready_route() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 52;
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 3, 4, 5, 6, 11, 12, 13, 14, 15, 31];

    assert_eq!(
        one_step_wait_potential(&hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC),
        0.0
    );
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_passes_incomplete_virtual_hand() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();

    assert_eq!(
        choose_claim_from_view(&[35, 35], &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_source_position_is_not_eligible() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 0,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 35, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        None
    );
}
