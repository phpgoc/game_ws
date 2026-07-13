use super::*;

#[test]
fn claim_hu_accepts_open_meld_remainder() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    table.seats.get_mut(&0).unwrap().melds = vec![
        share_type_public::games::shenyang_mahjong::WsShenyangMahjongMeld {
            kind: share_type_public::games::shenyang_mahjong::ShenyangMahjongMeldKind::PENG,
            tiles: vec![1, 1, 1],
            from_position: Some(2),
        },
    ];
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Hu)
    );
}

#[test]
fn claim_hu_accepts_seven_pairs() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Hu)
    );
}

#[test]
fn claimed_fourth_copy_keeps_seven_pairs_single_wait_fan() {
    let mut table = table_with_discards(1, vec![1]);
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let hand = vec![1, 1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22];
    let mut win_hand = hand.clone();
    win_hand.push(1);
    sort_tiles(&mut win_hand);
    let public_unavailable = known_unavailable_tiles_with_simulated_discards(&table, 0, &[], &[]);
    let claimed_unavailable = known_unavailable_tiles_for_claimed_win(&table, 0, 1);

    assert_eq!(
        public_unavailable.iter().filter(|tile| **tile == 1).count(),
        1
    );
    assert_eq!(
        claimed_unavailable
            .iter()
            .filter(|tile| **tile == 1)
            .count(),
        0
    );
    assert_eq!(
        estimated_fan_with_known_unavailable_wait_and_open_rule(
            &win_hand,
            &[],
            1,
            WIN_RULE_SHENYANG_BASIC,
            table.chi_opens_door,
            &public_unavailable,
        ),
        5
    );
    assert_eq!(
        estimated_fan_with_known_unavailable_wait_and_open_rule(
            &win_hand,
            &[],
            1,
            WIN_RULE_SHENYANG_BASIC,
            table.chi_opens_door,
            &claimed_unavailable,
        ),
        6
    );
}

#[test]
fn claim_hu_beats_other_claims() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Hu)
    );
}

#[test]
fn claim_hu_does_not_double_count_visible_tile_to_create_capped_wait() {
    let mut table = table_with_discards(1, vec![16]);
    table.max_fan = Some(2);
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    table.claim_window = Some(AiClaimView {
        tile: 16,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![13, 14, 15, 15, 16, 16, 17, 28, 28, 28];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();
    let mut current_win = hand.clone();
    current_win.push(16);
    sort_tiles(&mut current_win);
    let mut capped_wait_win = hand.clone();
    capped_wait_win.push(13);
    sort_tiles(&mut capped_wait_win);
    let current_known_unavailable =
        known_unavailable_tiles_with_simulated_discards(&table, 0, melds, &[]);
    let pass_simulated_discards = [];
    let pass_known_unavailable =
        known_unavailable_tiles_with_simulated_discards(&table, 0, melds, &pass_simulated_discards);

    assert!(is_complete_win_for_table(
        &current_win,
        melds,
        &table,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_eq!(
        estimated_fan_with_known_unavailable_wait_and_open_rule(
            &current_win,
            melds,
            16,
            WIN_RULE_SHENYANG_BASIC,
            table.chi_opens_door,
            &current_known_unavailable,
        ),
        1
    );
    assert_eq!(
        remaining_tile_count_with_melds_after_discards(
            &hand,
            melds,
            &table,
            0,
            13,
            &pass_simulated_discards,
        ),
        3
    );
    assert_eq!(
        estimated_fan_with_known_unavailable_wait_and_open_rule(
            &capped_wait_win,
            melds,
            13,
            WIN_RULE_SHENYANG_BASIC,
            table.chi_opens_door,
            &pass_known_unavailable,
        ),
        1
    );
    assert!(!should_pass_hu_for_capped_live_wait(
        &hand,
        &current_win,
        melds,
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC,
        16,
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Hu)
    );
}

#[test]
fn self_draw_hu_can_pass_one_fan_short_when_capped_wait_is_live() {
    let mut table = table_with_discards(1, vec![16]);
    table.max_fan = Some(2);
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    let win_hand = vec![13, 14, 15, 15, 16, 16, 16, 17, 28, 28, 28];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();
    let mut capped_wait_win = remove_n_tiles(&win_hand, 16, 1);
    capped_wait_win.push(13);
    sort_tiles(&mut capped_wait_win);
    let pass_simulated_discards = [16];
    let pass_known_unavailable =
        known_unavailable_tiles_with_simulated_discards(&table, 0, melds, &pass_simulated_discards);

    assert_eq!(
        estimated_fan_with_known_unavailable_wait_and_open_rule(
            &win_hand,
            melds,
            16,
            WIN_RULE_SHENYANG_BASIC,
            table.chi_opens_door,
            &[],
        ),
        1
    );
    assert_eq!(
        remaining_tile_count_with_melds_after_discards(
            &remove_n_tiles(&win_hand, 16, 1),
            melds,
            &table,
            0,
            13,
            &pass_simulated_discards,
        ),
        3
    );
    assert_eq!(
        estimated_fan_with_known_unavailable_wait_and_open_rule(
            &capped_wait_win,
            melds,
            13,
            WIN_RULE_SHENYANG_BASIC,
            table.chi_opens_door,
            &pass_known_unavailable,
        ),
        2
    );
    assert!(should_pass_self_draw_hu_from_view(
        &win_hand,
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC,
        16,
    ));
}

#[test]
fn late_claim_hu_can_pass_one_fan_short_when_capped_wait_is_live() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = FINAL_DEFENSE_WALL_COUNT;
    table.max_fan = Some(2);
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    table.seats.get_mut(&0).unwrap().discards = vec![16];
    table.claim_window = Some(AiClaimView {
        tile: 16,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![13, 14, 15, 15, 16, 16, 17, 28, 28, 28];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();
    let mut current_win = hand.clone();
    current_win.push(16);
    sort_tiles(&mut current_win);

    assert!(is_late_defense_round(&table));
    assert!(should_pass_hu_for_capped_live_wait(
        &hand,
        &current_win,
        melds,
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC,
        16,
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn late_self_draw_hu_can_pass_one_fan_short_when_capped_wait_is_live() {
    let mut table = table_with_discards(1, vec![16]);
    table.wall_count = 4;
    table.max_fan = Some(2);
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    let win_hand = vec![13, 14, 15, 15, 16, 16, 16, 17, 28, 28, 28];

    assert!(is_late_defense_round(&table));
    assert!(should_pass_self_draw_hu_from_view(
        &win_hand,
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC,
        16,
    ));
}

#[test]
fn final_claim_hu_takes_one_fan_short_without_full_wall_cycle() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 3;
    table.max_fan = Some(2);
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    table.seats.get_mut(&0).unwrap().discards = vec![16];
    table.claim_window = Some(AiClaimView {
        tile: 16,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![13, 14, 15, 15, 16, 16, 17, 28, 28, 28];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();
    let mut current_win = hand.clone();
    current_win.push(16);
    sort_tiles(&mut current_win);

    assert!(!should_pass_hu_for_capped_live_wait(
        &hand,
        &current_win,
        melds,
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC,
        16,
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Hu)
    );
}

#[test]
fn final_self_draw_hu_takes_one_fan_short_without_full_wall_cycle() {
    let mut table = table_with_discards(1, vec![16]);
    table.wall_count = 3;
    table.max_fan = Some(2);
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    let win_hand = vec![13, 14, 15, 15, 16, 16, 16, 17, 28, 28, 28];

    assert!(!should_pass_self_draw_hu_from_view(
        &win_hand,
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC,
        16,
    ));
}

#[test]
fn claim_hu_takes_when_current_fan_exceeds_half_cap() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(6);
    table.seats.get_mut(&0).unwrap().melds = vec![test_concealed_gang_meld(35)];
    table.seats.get_mut(&0).unwrap().discards = vec![16];
    table.claim_window = Some(AiClaimView {
        tile: 16,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![13, 14, 15, 15, 16, 16, 17, 28, 28, 28];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();
    let mut current_win = hand.clone();
    current_win.push(16);
    sort_tiles(&mut current_win);
    let mut capped_wait_win = hand.clone();
    capped_wait_win.push(13);
    sort_tiles(&mut capped_wait_win);
    let pass_simulated_discards = [16];
    let current_known_unavailable = known_unavailable_tiles_for_claimed_win(&table, 0, 16);
    let pass_known_unavailable =
        known_unavailable_tiles_with_simulated_discards(&table, 0, melds, &pass_simulated_discards);

    assert_eq!(
        estimated_fan_with_known_unavailable_wait_and_open_rule(
            &current_win,
            melds,
            16,
            WIN_RULE_RELAXED,
            table.chi_opens_door,
            &current_known_unavailable,
        ),
        5
    );
    assert_eq!(
        remaining_tile_count_with_melds_after_discards(
            &hand,
            melds,
            &table,
            0,
            13,
            &pass_simulated_discards,
        ),
        3
    );
    assert_eq!(
        estimated_fan_with_known_unavailable_wait_and_open_rule(
            &capped_wait_win,
            melds,
            13,
            WIN_RULE_RELAXED,
            table.chi_opens_door,
            &pass_known_unavailable,
        ),
        6
    );
    assert!(!should_pass_hu_for_capped_live_wait(
        &hand,
        &current_win,
        melds,
        &table,
        0,
        WIN_RULE_RELAXED,
        16,
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Hu)
    );
}

#[test]
fn self_draw_hu_takes_when_current_fan_exceeds_half_cap() {
    let mut table = table_with_discards(1, vec![16]);
    table.max_fan = Some(6);
    table.seats.get_mut(&0).unwrap().melds = vec![test_concealed_gang_meld(35)];
    let win_hand = vec![13, 14, 15, 15, 16, 16, 16, 17, 28, 28, 28];

    assert!(!should_pass_self_draw_hu_from_view(
        &win_hand,
        &table,
        0,
        WIN_RULE_RELAXED,
        16,
    ));
}

#[test]
fn dealer_claim_hu_takes_one_fan_short_instead_of_chasing_cap() {
    let mut table = table_with_discards(1, vec![16]);
    table.dealer_position = 0;
    table.max_fan = Some(2);
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    table.claim_window = Some(AiClaimView {
        tile: 16,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![13, 14, 15, 15, 16, 16, 17, 28, 28, 28];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();
    let mut current_win = hand.clone();
    current_win.push(16);
    sort_tiles(&mut current_win);

    assert!(!should_pass_hu_for_capped_live_wait(
        &hand,
        &current_win,
        melds,
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC,
        16,
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Hu)
    );
}

#[test]
fn dealer_self_draw_hu_takes_one_fan_short_instead_of_chasing_cap() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    table.max_fan = Some(2);
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    let win_hand = vec![13, 14, 15, 15, 16, 16, 16, 17, 28, 28, 28];

    assert!(!should_pass_self_draw_hu_from_view(
        &win_hand,
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC,
        16,
    ));
}

#[test]
fn claim_hu_respects_whether_chi_opens_door() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    table.seats.get_mut(&0).unwrap().melds = vec![test_chi_meld(1)];
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Hu)
    );

    table.chi_opens_door = false;
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_hu_still_wins_during_final_defense() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Hu)
    );
}
