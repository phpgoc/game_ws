use super::*;

#[test]
fn claim_gang_beats_peng_when_not_winning() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 35, 35, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Gang)
    );
}

#[test]
fn claim_gang_takes_dragon_gang_to_open_basic_hand_before_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 7, 11, 12, 14, 21, 35, 35, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Gang)
    );
}

#[test]
fn one_fan_capped_claim_gang_penges_dragon_for_speed_before_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(1);
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 7, 11, 12, 14, 21, 35, 35, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_gang_delays_open_piao_plain_gang_until_ready_and_pengs() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    table.claim_window = Some(AiClaimView {
        tile: 21,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![11, 11, 21, 21, 21, 31, 31, 35, 35, 36];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_gang_delays_open_plain_gang_when_not_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_chi_meld(1)];
    table.claim_window = Some(AiClaimView {
        tile: 9,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![4, 5, 6, 9, 9, 9, 11, 12, 14, 21];

    assert_ne!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Gang)
    );
}

#[test]
fn claim_gang_opens_closed_plain_basic_hand_before_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 3,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![3, 3, 3, 4, 5, 7, 8, 11, 12, 14, 21, 22, 31];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Gang)
    );
}

#[test]
fn claim_gang_penges_closed_early_piao_candidate() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 1, 4, 5, 6, 11, 11, 12, 13, 21, 21, 22];

    assert!(is_closed_early_piao_candidate(&hand, &[], &table, 0));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_gang_opens_broken_closed_hand_late_for_defense() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 2,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 2, 2, 4, 7, 12, 14, 17, 31, 32, 33, 34, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Gang)
    );
}

#[test]
fn dealer_claim_gang_opens_broken_closed_hand_for_speed() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 2,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 2, 2, 4, 7, 12, 14, 17, 31, 32, 33, 34, 35];

    assert!(should_open_broken_closed_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Gang)
    );
}

#[test]
fn claim_gang_passes_final_unready_broken_hand_for_defense() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.claim_window = Some(AiClaimView {
        tile: 2,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 2, 2, 4, 7, 12, 14, 17, 31, 32, 33, 34, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_gang_opens_broken_closed_hand_for_defense_in_relaxed_rule() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 31,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 5, 8, 12, 14, 17, 21, 24, 27, 31, 31, 31, 34];

    assert!(should_open_broken_closed_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_RELAXED
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Gang)
    );
}

#[test]
fn claim_gang_opens_mid_missing_suit_no_terminal_hand_for_defense() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 52;
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 3, 5, 5, 5, 7, 8, 12, 14, 15, 16, 17, 18];

    assert!(should_open_broken_closed_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Gang)
    );
}

#[test]
fn claim_gang_passes_when_it_breaks_locked_pure_one_suit_plan() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 11,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 11, 11, 11];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_gang_passes_dragon_when_pure_one_suit_plan_starts_at_eight_tiles() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 11, 21, 35, 35, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_gang_passes_closed_pure_one_suit_plan_before_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 11];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_gang_takes_ready_main_suit_pure_one_suit_when_not_capped() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 9];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Gang)
    );
}

#[test]
fn claim_gang_passes_ready_pure_one_suit_when_visible_fan_capped() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(4);
    table.seats.get_mut(&0).unwrap().melds = vec![test_chi_meld(2)];
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 1, 5, 6, 7, 8, 8, 9, 9];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_gang_passes_capped_closed_pure_one_suit_wait() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(4);
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 9];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_gang_passes_when_it_breaks_locked_seven_pairs_plan() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 31];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_gang_preserves_five_pairs_even_for_dragon_gang() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 22, 35, 35, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn three_fan_capped_claim_gang_penges_dragon_over_five_pairs() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(3);
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 22, 35, 35, 35];

    assert!(!should_lock_seven_pairs_plan(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert!(capped_basic_route_foundation_visible_fan_exceeds_half_cap(
        &hand,
        &[],
        &table,
        WIN_RULE_SHENYANG_BASIC
    ));
    let gang_hand = remove_n_tiles(&hand, 35, 3);
    let gang_melds = vec![claim_gang_meld(35, 1)];
    assert_eq!(estimated_visible_bonus_fan(&gang_hand, &gang_melds), 2);
    assert!(capped_open_basic_route_visible_fan_reaches_cap(
        &gang_hand,
        &gang_melds,
        &table
    ));
    let peng_hand = remove_n_tiles(&hand, 35, 2);
    let peng_melds = vec![claim_peng_meld(35, 1)];
    assert!(capped_open_basic_route_visible_fan_reaches_cap(
        &peng_hand,
        &peng_melds,
        &table
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_gang_skips_plain_gang_when_ready_fan_already_capped() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(3);
    table.seats.get_mut(&0).unwrap().melds = vec![test_gang_meld(35)];
    table.claim_window = Some(AiClaimView {
        tile: 9,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 9, 9, 9, 11, 12, 13, 21];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_gang_skips_ready_plain_gang_when_fan_exceeds_half_cap() {
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
    let hand = vec![1, 2, 3, 9, 9, 9, 11, 12, 13, 21];

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
fn claim_gang_takes_open_plain_gang_when_it_reaches_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_chi_meld(1)];
    table.claim_window = Some(AiClaimView {
        tile: 9,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![4, 5, 6, 9, 9, 9, 11, 12, 13, 21];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Gang)
    );
}

#[test]
fn capped_open_basic_route_delays_plain_gang_before_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(2);
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(35)];
    table.claim_window = Some(AiClaimView {
        tile: 9,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let melds = table.seats.get(&0).unwrap().melds.as_slice();
    let hand = vec![1, 4, 7, 9, 9, 9, 11, 14, 17, 21];

    assert!(capped_open_basic_route_visible_fan_reaches_cap(
        &hand, melds, &table
    ));
    assert_eq!(
        ready_tile_score(&hand, melds, &table, 0, WIN_RULE_SHENYANG_BASIC),
        0.0
    );
    assert!(!should_claim_gang_from_discard(
        &hand,
        melds,
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC,
        9,
        1
    ));
    assert_ne!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Gang)
    );
}

#[test]
fn claim_gang_penges_to_preserve_four_gui_yi_when_peng_stays_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    table.seats.get_mut(&0).unwrap().melds = vec![test_chi_meld(11)];
    table.claim_window = Some(AiClaimView {
        tile: 4,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 2, 2, 4, 4, 4, 5, 21, 21, 21];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn capped_claim_gang_does_not_peng_to_preserve_redundant_four_gui_yi() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    table.max_fan = Some(2);
    table.seats.get_mut(&0).unwrap().melds = vec![test_chi_meld(11)];
    table.claim_window = Some(AiClaimView {
        tile: 4,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 2, 2, 4, 4, 4, 5, 21, 21, 21];

    assert!(!ready_visible_fan_reaches_cap(
        &hand,
        table.seats.get(&0).unwrap().melds.as_slice(),
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Gang)
    );
}

#[test]
fn claim_gang_takes_late_ready_dragon_gang_when_it_keeps_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 36;
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 11, 12, 13, 21, 35, 35, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Gang)
    );
}

#[test]
fn claim_gang_passes_late_ready_hand_when_gang_breaks_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 36;
    table.claim_window = Some(AiClaimView {
        tile: 6,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 4, 6, 6, 6, 7, 8, 13, 14, 15, 23, 24, 25];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Pass)
    );
}
