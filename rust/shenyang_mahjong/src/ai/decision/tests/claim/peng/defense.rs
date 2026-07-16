use super::*;

#[test]
fn claim_gang_passes_when_terminal_or_honor_is_unrecoverable_for_basic() {
    let mut table = table_with_discards(1, dead_terminal_or_honor_discards());
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 4, 5, 5, 5, 12, 13, 14, 15, 16, 22, 23, 24];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_opens_broken_closed_hand_for_defense_in_relaxed_rule() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 31,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 5, 8, 12, 14, 17, 21, 24, 27, 31, 31, 33, 34];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_opens_broken_closed_hand_late_for_defense() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 2,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 2, 4, 7, 12, 14, 17, 31, 32, 33, 34, 35, 36];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_opens_late_broken_missing_suit_hand_even_for_dragon() {
    let dead_bamboo = (21..=29)
        .flat_map(|tile| std::iter::repeat_n(tile, 4))
        .collect::<Vec<_>>();
    let mut table = table_with_discards(1, dead_bamboo);
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 4, 6, 8, 11, 13, 16, 19, 31, 35, 35, 36];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_opens_late_broken_no_terminal_hand_for_defense() {
    let mut table = table_with_discards(1, dead_terminal_or_honor_discards());
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 4, 5, 5, 7, 12, 14, 16, 18, 22, 24, 26, 28];

    assert!(should_open_broken_closed_hand_for_defense(
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

#[test]
fn claim_peng_opens_mid_severely_broken_closed_hand_for_defense() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 52;
    table.claim_window = Some(AiClaimView {
        tile: 31,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 5, 8, 11, 14, 17, 19, 31, 31, 33, 35, 36, 37];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_opens_mid_unrecoverable_no_terminal_hand_for_defense() {
    let mut table = table_with_discards(1, dead_terminal_or_honor_discards());
    table.wall_count = 52;
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 3, 4, 5, 5, 6, 7, 12, 13, 14, 22, 23, 24];

    assert_eq!(
        unrecoverable_basic_rule_requirement_count(&hand, &[], &table, 0),
        1
    );
    assert!(should_open_broken_closed_hand_for_defense(
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

#[test]
fn claim_peng_passes_final_unready_broken_hand_for_defense() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.claim_window = Some(AiClaimView {
        tile: 2,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 2, 4, 7, 12, 14, 17, 31, 32, 33, 34, 35, 36];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_passes_open_basic_pure_defense_hand_to_avoid_more_exposure() {
    let mut table = table_with_discards(1, Vec::new());
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
fn claim_peng_passes_open_relaxed_pure_defense_hand_to_avoid_more_exposure() {
    let mut table = table_with_discards(1, Vec::new());
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

    assert_eq!(
        ready_tile_score(&hand, melds, &table, 0, WIN_RULE_RELAXED),
        0.0
    );
    assert_eq!(
        one_step_wait_potential(&hand, melds, &table, 0, WIN_RULE_RELAXED),
        0.0
    );
    assert!(hand_power(&hand) < 18.0);
    assert!(should_pass_peng_for_open_pure_defense(
        &hand,
        melds,
        &table,
        0,
        WIN_RULE_RELAXED,
        2
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_passes_open_severely_broken_hand_from_mid_defense() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = MID_BROKEN_HAND_WALL_COUNT;
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
    table.claim_window = Some(AiClaimView {
        tile: 2,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let melds = table.seats.get(&0).unwrap().melds.clone();
    let hand = vec![2, 2, 5, 8, 12, 14, 17, 21, 24, 27];

    assert!(is_mid_broken_hand_defense_round(&table));
    assert!(!is_late_round(&table));
    assert!(hand_power(&hand) < 14.0);
    assert!(should_pass_peng_for_open_pure_defense(
        &hand,
        &melds,
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC,
        2,
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );

    table.wall_count = MID_BROKEN_HAND_WALL_COUNT + 1;
    assert!(!should_pass_peng_for_open_pure_defense(
        &hand,
        &melds,
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC,
        2,
    ));
}

#[test]
fn claim_peng_passes_when_missing_suit_is_unrecoverable_even_for_dragon() {
    let dead_bamboo = (21..=29)
        .flat_map(|tile| std::iter::repeat_n(tile, 4))
        .collect::<Vec<_>>();
    let mut table = table_with_discards(1, dead_bamboo);
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 31, 35, 35, 36];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_passes_when_terminal_or_honor_is_unrecoverable_for_basic() {
    let mut table = table_with_discards(1, dead_terminal_or_honor_discards());
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 4, 5, 5, 12, 13, 14, 15, 16, 22, 23, 24, 25];

    assert!(claim_leaves_unrecoverable_terminal_or_honor(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC,
        ShenyangMahjongMeldKind::PENG,
        5,
        1
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}
