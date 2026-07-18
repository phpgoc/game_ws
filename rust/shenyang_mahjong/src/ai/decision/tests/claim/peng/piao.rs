use super::*;

#[test]
fn claim_peng_passes_raw_piao_shape_without_terminal_or_honor() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(2)];
    table.claim_window = Some(AiClaimView {
        tile: 13,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![12, 13, 13, 14, 15, 22, 22, 23, 25, 25];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();

    assert!(piao_plan_score(&hand, melds) >= 32.0);
    assert_eq!(piao_plan_score_for_context(&hand, melds, &table, 0), 0.0);
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_pursues_piao_plan_after_open_triplet() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![1, 1, 1],
        from_position: Some(2),
    }];
    table.claim_window = Some(AiClaimView {
        tile: 21,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![11, 11, 21, 21, 31, 31, 35, 35, 36, 37];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_opens_fully_closed_piao_shape_to_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 1, 5, 9, 11, 11, 11, 21, 21, 21, 35, 35];

    assert!(!has_door_opening_meld(&[], &table));
    assert_eq!(ready_tile_score(&hand, &[], &table, 0), 0.0);
    assert!(should_claim_peng_for_closed_early_piao_candidate(
        &hand,
        &[],
        &table,
        0,
        35,
        1,
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_takes_four_pair_three_suit_piao_start() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 21,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 13, 21, 21, 22, 31, 32];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_takes_fourth_piao_meld_to_set_up_shou_ba_yi() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds =
        vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![35, 35, 36, 37];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_takes_three_pair_three_suit_piao_start() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 4, 5, 6, 11, 11, 12, 13, 21, 21, 22, 23];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn closed_early_piao_peng_passes_against_threatening_dealer() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    table.seats.get_mut(&1).unwrap().hand_count = 7;
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(23), test_peng_meld(24)];
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 4, 5, 5, 6, 11, 12, 13, 21, 21, 35, 35];

    table.dealer_position = 3;
    assert!(!dealer_opponent_has_major_threat(&table, 0));
    assert!(should_claim_peng_for_closed_early_piao_candidate(
        &hand,
        &[],
        &table,
        0,
        5,
        1,
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0),
        Some(AiClaimChoice::Peng)
    );

    table.dealer_position = 1;
    assert!(dealer_opponent_has_major_threat(&table, 0));
    assert!(!should_claim_peng_for_closed_early_piao_candidate(
        &hand,
        &[],
        &table,
        0,
        5,
        1,
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn dealer_ready_piao_passes_fourth_plain_peng_for_speed() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    table.seats.get_mut(&0).unwrap().melds =
        vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![5, 5, 6, 8];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();
    let current_ready_score = ready_tile_score(&hand, melds, &table, 0);

    assert!(current_ready_score > 0.0);
    assert!(!should_claim_ready_piao_peng_for_shou_ba_yi(
        &hand,
        melds,
        &table,
        0,
        5,
        1,
        current_ready_score
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn ready_piao_claim_peng_takes_fourth_plain_meld_for_shou_ba_yi() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds =
        vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![5, 5, 6, 8];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();
    let current_ready_score = ready_tile_score(&hand, melds, &table, 0);

    assert!(current_ready_score > 0.0);
    assert!(!is_complete_win_with_melds(&[5, 5, 5, 6, 8], melds));
    assert!(should_claim_ready_piao_peng_for_shou_ba_yi(
        &hand,
        melds,
        &table,
        0,
        5,
        1,
        current_ready_score
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn ready_piao_passes_shou_ba_yi_peng_against_threatening_dealer() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds =
        vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    table.seats.get_mut(&1).unwrap().hand_count = 7;
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(23), test_peng_meld(24)];
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![5, 5, 6, 8];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();

    table.dealer_position = 3;
    let current_ready_score = ready_tile_score(&hand, melds, &table, 0);
    assert!(!dealer_opponent_has_major_threat(&table, 0));
    assert!(should_claim_ready_piao_peng_for_shou_ba_yi(
        &hand,
        melds,
        &table,
        0,
        5,
        1,
        current_ready_score,
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0),
        Some(AiClaimChoice::Peng)
    );

    table.dealer_position = 1;
    let threatened_ready_score = ready_tile_score(&hand, melds, &table, 0);
    assert!(dealer_opponent_has_major_threat(&table, 0));
    assert!(!should_claim_ready_piao_peng_for_shou_ba_yi(
        &hand,
        melds,
        &table,
        0,
        5,
        1,
        threatened_ready_score,
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_takes_closed_early_piao_candidate_over_sequence_shape() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 4, 5, 5, 6, 11, 12, 13, 21, 21, 35, 35];

    assert!(tile_is_middle_of_sequence(&hand, 5));
    assert!(should_claim_peng_for_closed_early_piao_candidate(
        &hand,
        &[],
        &table,
        0,
        5,
        1
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn closed_piao_peng_ignores_malformed_meld() {
    let mut table = table_with_discards(1, Vec::new());
    let malformed_meld = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![31, 31],
        from_position: Some(2),
    };
    table.seats.get_mut(&0).unwrap().melds = vec![malformed_meld.clone()];
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 4, 5, 5, 6, 11, 12, 13, 21, 21, 35, 35];

    assert_eq!(valid_meld_count(&[malformed_meld.clone()]), 0);
    assert!(should_claim_peng_for_closed_early_piao_candidate(
        &hand,
        &[malformed_meld],
        &table,
        0,
        5,
        1
    ));
    assert!(!should_claim_peng_for_closed_early_piao_candidate(
        &hand,
        &[test_peng_meld(31)],
        &table,
        0,
        5,
        1
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0),
        Some(AiClaimChoice::Peng)
    );
}
