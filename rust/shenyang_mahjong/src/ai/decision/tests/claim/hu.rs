use super::*;

#[test]
fn claim_tile_is_visible_only_when_source_latest_discard_matches() {
    let mut table = table_with_discards(1, vec![16, 17]);
    table.claim_window = Some(AiClaimView {
        tile: 16,
        from_position: 1,
        eligible_positions: vec![0],
    });

    assert!(!claim_tile_already_visible(&table, 16));

    table.seats.get_mut(&1).unwrap().discards.push(16);
    assert!(claim_tile_already_visible(&table, 16));
}

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
        choose_claim_from_view(&hand, &claim, &table, 0),
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
        choose_claim_from_view(&hand, &claim, &table, 0),
        Some(AiClaimChoice::Hu)
    );
}

#[test]
fn claim_hu_accepts_fully_closed_piao_with_dragon_pair() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 1, 11, 11, 11, 21, 21, 21, 31, 31, 31, 35];

    assert!(!has_door_opening_meld(&[], &table));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0),
        Some(AiClaimChoice::Hu)
    );
}

#[test]
fn claim_hu_rejects_fully_closed_piao_with_ordinary_pair() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 1, 11, 11, 11, 21, 21, 21, 31, 31, 31, 5];

    assert!(!has_door_opening_meld(&[], &table));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_hu_allows_closed_sequence_dragon_pair_win_when_first_chi_disabled() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0),
        Some(AiClaimChoice::Pass)
    );

    table.allow_first_chi = false;
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0),
        Some(AiClaimChoice::Hu)
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
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    let hand = vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0),
        Some(AiClaimChoice::Hu)
    );
}

#[test]
fn claim_hu_counts_chi_as_opening_meld() {
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
        choose_claim_from_view(&hand, &claim, &table, 0),
        Some(AiClaimChoice::Hu)
    );
}

#[test]
fn claim_hu_does_not_double_count_visible_tile_to_create_capped_wait() {
    let mut table = table_with_discards(1, vec![16]);
    table.score_cap = Some(4);
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

    assert!(is_complete_win_for_table(&current_win, melds, &table));
    assert_eq!(
        estimated_fan_with_known_unavailable_wait(
            &current_win,
            melds,
            16,
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
        estimated_fan_with_known_unavailable_wait(
            &capped_wait_win,
            melds,
            13,
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
        16,
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0),
        Some(AiClaimChoice::Hu)
    );
}

#[test]
fn claim_hu_passes_when_unowned_tile_has_five_visible_copies() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 3,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(21)];
    let hand = vec![1, 2, 11, 12, 13, 31, 31, 31, 35, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0),
        Some(AiClaimChoice::Hu)
    );

    table.seats.get_mut(&1).unwrap().discards = vec![3, 3, 3, 3, 3];
    assert_eq!(visible_tile_count(&table, 3), 5);
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0),
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
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    let hand = vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0),
        Some(AiClaimChoice::Hu)
    );
}

#[test]
fn claim_hu_takes_when_current_fan_exceeds_half_cap() {
    let mut table = table_with_discards(1, Vec::new());
    table.score_cap = Some(64);
    table.seats.get_mut(&0).unwrap().melds = vec![test_gang_meld(35)];
    table.seats.get_mut(&0).unwrap().discards = vec![36];
    table.claim_window = Some(AiClaimView {
        tile: 36,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 36];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();
    let mut current_win = hand.clone();
    current_win.push(36);
    sort_tiles(&mut current_win);
    let current_known_unavailable = known_unavailable_tiles_for_claimed_win(&table, 0, 36);

    assert_eq!(
        estimated_fan_with_known_unavailable_wait(
            &current_win,
            melds,
            36,
            &current_known_unavailable,
        ),
        4
    );
    assert!(!should_pass_hu_for_capped_live_wait(
        &hand,
        &current_win,
        melds,
        &table,
        0,
        36,
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0),
        Some(AiClaimChoice::Hu)
    );
}

#[test]
fn claim_hu_takes_five_fan_at_fifty_point_cap() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 3;
    table.score_cap = Some(50);
    table.seats.get_mut(&0).unwrap().melds = vec![test_gang_meld(1), test_gang_meld(35)];
    table.seats.get_mut(&1).unwrap().melds = vec![WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![9, 9, 9],
        from_position: Some(2),
    }];
    table.claim_window = Some(AiClaimView {
        tile: 36,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![11, 12, 13, 21, 22, 23, 36];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();
    let mut current_win = hand.clone();
    current_win.push(36);
    sort_tiles(&mut current_win);
    let current_known_unavailable = known_unavailable_tiles_for_claimed_win(&table, 0, 36);

    assert_eq!(
        estimated_fan_with_known_unavailable_wait(
            &current_win,
            melds,
            36,
            &current_known_unavailable,
        ),
        5
    );
    assert!(shenyang_fan_score_exceeds_half_cap(5, 50));
    assert!(!should_pass_hu_for_capped_live_wait(
        &hand,
        &current_win,
        melds,
        &table,
        0,
        36,
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0),
        Some(AiClaimChoice::Hu)
    );
}

#[test]
fn claim_hu_takes_when_payer_modifier_reaches_cap() {
    let mut dealer_payer_table = table_with_discards(1, Vec::new());
    dealer_payer_table.score_cap = Some(4);
    dealer_payer_table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    dealer_payer_table.seats.get_mut(&1).unwrap().melds = vec![WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![9, 9, 9],
        from_position: Some(2),
    }];
    dealer_payer_table.claim_window = Some(AiClaimView {
        tile: 16,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let hand = vec![13, 14, 15, 15, 16, 16, 17, 28, 28, 28];
    let mut win_hand = hand.clone();
    win_hand.push(16);
    sort_tiles(&mut win_hand);
    let melds = dealer_payer_table.seats.get(&0).unwrap().melds.as_slice();

    assert!(!dealer_opponent_has_major_threat(&dealer_payer_table, 0));
    assert!(!should_pass_hu_for_capped_live_wait(
        &hand,
        &win_hand,
        melds,
        &dealer_payer_table,
        0,
        16,
    ));
    assert_eq!(
        choose_claim_from_view(
            &hand,
            dealer_payer_table.claim_window.as_ref().unwrap(),
            &dealer_payer_table,
            0,
        ),
        Some(AiClaimChoice::Hu)
    );

    let mut closed_payer_table = dealer_payer_table.clone();
    closed_payer_table.dealer_position = 3;
    closed_payer_table.score_cap = Some(8);
    closed_payer_table.seats.get_mut(&1).unwrap().melds.clear();
    for position in 2..=3 {
        closed_payer_table.seats.insert(
            position,
            AiSeatView {
                position,
                hand_count: 13,
                discards: Vec::new(),
                melds: Vec::new(),
            },
        );
    }
    assert!(!should_pass_hu_for_capped_live_wait(
        &hand,
        &win_hand,
        melds,
        &closed_payer_table,
        0,
        16,
    ));
    assert_eq!(
        choose_claim_from_view(
            &hand,
            closed_payer_table.claim_window.as_ref().unwrap(),
            &closed_payer_table,
            0,
        ),
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
        estimated_fan_with_known_unavailable_wait(&win_hand, &[], 1, &public_unavailable,),
        5
    );
    assert_eq!(
        estimated_fan_with_known_unavailable_wait(&win_hand, &[], 1, &claimed_unavailable,),
        6
    );
}

#[test]
fn dealer_claim_hu_takes_one_fan_short_instead_of_chasing_cap() {
    let mut table = table_with_discards(1, vec![16]);
    table.dealer_position = 0;
    table.score_cap = Some(4);
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
        16,
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0),
        Some(AiClaimChoice::Hu)
    );
}

#[test]
fn dealer_self_draw_hu_takes_one_fan_short_instead_of_chasing_cap() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    table.score_cap = Some(4);
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    let win_hand = vec![13, 14, 15, 15, 16, 16, 16, 17, 28, 28, 28];

    assert!(!should_pass_self_draw_hu_from_view(
        &win_hand, &table, 0, 16,
    ));
}

#[test]
fn final_claim_hu_takes_one_fan_short_without_full_wall_cycle() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 3;
    table.score_cap = Some(4);
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
        16,
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0),
        Some(AiClaimChoice::Hu)
    );
}

#[test]
fn final_self_draw_hu_takes_one_fan_short_without_full_wall_cycle() {
    let mut table = table_with_discards(1, vec![16]);
    table.wall_count = 3;
    table.score_cap = Some(4);
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    let win_hand = vec![13, 14, 15, 15, 16, 16, 16, 17, 28, 28, 28];

    assert!(!should_pass_self_draw_hu_from_view(
        &win_hand, &table, 0, 16,
    ));
}

#[test]
fn hu_takes_legal_hand_against_long_closed_dealer() {
    let mut table = table_with_discards(1, vec![31, 32, 33, 34, 35, 16]);
    table.wall_count = FINAL_DEFENSE_WALL_COUNT;
    table.score_cap = Some(4);
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    table.seats.get_mut(&0).unwrap().discards = vec![16];
    table.seats.get_mut(&1).unwrap().hand_count = 13;
    table.claim_window = Some(AiClaimView {
        tile: 16,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![13, 14, 15, 15, 16, 16, 17, 28, 28, 28];
    let mut win_hand = hand.clone();
    win_hand.push(16);
    sort_tiles(&mut win_hand);
    let melds = table.seats.get(&0).unwrap().melds.as_slice();

    assert!(dealer_opponent_has_major_threat(&table, 0));
    let mut non_dealer_threat_table = table.clone();
    non_dealer_threat_table.dealer_position = 3;
    assert!(!dealer_opponent_has_major_threat(
        &non_dealer_threat_table,
        0
    ));
    assert!(!should_pass_hu_for_capped_live_wait(
        &hand, &win_hand, melds, &table, 0, 16,
    ));

    let mut self_draw_table = table.clone();
    self_draw_table.claim_window = None;
    self_draw_table.seats.get_mut(&1).unwrap().discards = vec![31, 32, 33, 34, 35, 36];
    assert!(dealer_opponent_has_major_threat(&self_draw_table, 0));
    assert!(!should_pass_self_draw_hu_from_view(
        &win_hand,
        &self_draw_table,
        0,
        16,
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0),
        Some(AiClaimChoice::Hu)
    );
}

#[test]
fn hu_takes_one_fan_short_against_threatening_dealer() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = FINAL_DEFENSE_WALL_COUNT;
    table.score_cap = Some(4);
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(9)];
    table.seats.get_mut(&0).unwrap().discards = vec![16];
    let dealer = table.seats.get_mut(&1).unwrap();
    dealer.hand_count = 4;
    dealer.melds = vec![test_peng_meld(2), test_peng_meld(12), test_peng_meld(22)];
    table.claim_window = Some(AiClaimView {
        tile: 16,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![13, 14, 15, 15, 16, 16, 17, 28, 28, 28];
    let mut win_hand = hand.clone();
    win_hand.push(16);
    sort_tiles(&mut win_hand);
    let melds = table.seats.get(&0).unwrap().melds.as_slice();

    assert!(dealer_opponent_has_major_threat(&table, 0));
    let mut non_dealer_threat_table = table.clone();
    non_dealer_threat_table.dealer_position = 3;
    assert!(!dealer_opponent_has_major_threat(
        &non_dealer_threat_table,
        0
    ));
    assert!(should_pass_hu_for_capped_live_wait(
        &hand,
        &win_hand,
        melds,
        &non_dealer_threat_table,
        0,
        16,
    ));
    assert!(should_pass_self_draw_hu_from_view(
        &win_hand,
        &non_dealer_threat_table,
        0,
        16,
    ));
    assert!(!should_pass_hu_for_capped_live_wait(
        &hand, &win_hand, melds, &table, 0, 16,
    ));
    assert!(!should_pass_self_draw_hu_from_view(
        &win_hand, &table, 0, 16,
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0),
        Some(AiClaimChoice::Hu)
    );
}

#[test]
fn late_claim_hu_can_pass_one_fan_short_when_capped_wait_is_live() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 3;
    table.wall_count = FINAL_DEFENSE_WALL_COUNT;
    table.score_cap = Some(4);
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    table.seats.get_mut(&0).unwrap().discards = vec![16];
    table.seats.get_mut(&1).unwrap().melds = vec![WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![9, 9, 9],
        from_position: Some(2),
    }];
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
        16,
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn rob_gang_hu_takes_when_its_bonus_fan_reaches_cap() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 3;
    table.score_cap = Some(4);
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(9)];
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(4)];
    table.claim_window = Some(AiClaimView {
        tile: 4,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 3, 11, 12, 13, 21, 22, 23, 35, 35];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();
    let mut current_win = hand.clone();
    current_win.push(4);
    sort_tiles(&mut current_win);

    assert!(claim_known_tile_counts_are_possible(
        &hand, melds, &claim, &table,
    ));
    let current_known_unavailable = known_unavailable_tiles_for_claimed_win(&table, 0, 4);
    assert_eq!(
        estimated_fan_with_known_unavailable_wait(
            &current_win,
            melds,
            4,
            &current_known_unavailable,
        ),
        1
    );
    let pass_known_unavailable =
        known_unavailable_tiles_with_simulated_discards(&table, 0, melds, &[4]);
    let mut projected_win = hand.clone();
    projected_win.push(1);
    sort_tiles(&mut projected_win);
    assert_eq!(
        estimated_fan_with_known_unavailable_wait(
            &projected_win,
            melds,
            1,
            &pass_known_unavailable,
        ),
        2
    );
    assert_eq!(
        remaining_tile_count_with_melds_after_discards(&hand, melds, &table, 0, 1, &[4]),
        4
    );
    assert!(should_pass_hu_for_capped_live_wait(
        &hand,
        &current_win,
        melds,
        &table,
        0,
        4,
    ));

    table.claim_is_rob_gang = true;
    assert!(!should_pass_hu_for_capped_live_wait(
        &hand,
        &current_win,
        melds,
        &table,
        0,
        4,
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0),
        Some(AiClaimChoice::Hu)
    );
}

#[test]
fn claim_hu_does_not_reuse_current_payer_for_projected_capped_wait() {
    let mut table = table_with_discards(1, vec![16]);
    table.score_cap = Some(8);
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    table.seats.get_mut(&0).unwrap().discards = vec![16];
    table.seats.get_mut(&1).unwrap().melds = vec![WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![9, 9, 9],
        from_position: Some(2),
    }];
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![WsShenyangMahjongMeld {
                kind: ShenyangMahjongMeldKind::PENG,
                tiles: vec![19, 19, 19],
                from_position: Some(3),
            }],
        },
    );
    table.seats.insert(
        3,
        AiSeatView {
            position: 3,
            hand_count: 13,
            discards: Vec::new(),
            melds: Vec::new(),
        },
    );
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
    let mut projected_payment_fans = payment_fans_for_table(2, &table, 0, None);
    projected_payment_fans.sort_unstable();

    assert!(!dealer_opponent_has_major_threat(&table, 0));
    assert_eq!(payment_fans_for_table(1, &table, 0, Some(1)), vec![2]);
    assert_eq!(payment_fans_for_table(2, &table, 0, Some(1)), vec![3]);
    assert_eq!(projected_payment_fans, vec![2, 3, 3]);
    assert!(!should_pass_hu_for_capped_live_wait(
        &hand,
        &current_win,
        melds,
        &table,
        0,
        16,
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0),
        Some(AiClaimChoice::Hu)
    );
}

#[test]
fn claim_hu_counts_closed_payment_fan_for_next_capped_wait() {
    let mut table = table_with_discards(1, vec![16]);
    table.dealer_position = 3;
    table.score_cap = Some(16);
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    table.seats.get_mut(&0).unwrap().discards = vec![16];
    for position in 2..=3 {
        table.seats.insert(
            position,
            AiSeatView {
                position,
                hand_count: 13,
                discards: Vec::new(),
                melds: Vec::new(),
            },
        );
    }
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
    let current_known_unavailable = known_unavailable_tiles_for_claimed_win(&table, 0, 16);
    let pass_simulated_discards = Vec::new();
    let pass_known_unavailable =
        known_unavailable_tiles_with_simulated_discards(&table, 0, melds, &pass_simulated_discards);
    let mut projected_win = hand.clone();
    projected_win.push(13);
    sort_tiles(&mut projected_win);

    assert!(!dealer_opponent_has_major_threat(&table, 0));
    assert!(claim_tile_already_visible(&table, 16));
    assert_eq!(
        estimated_fan_with_known_unavailable_wait(
            &current_win,
            melds,
            16,
            &current_known_unavailable,
        ),
        1
    );
    assert_eq!(
        estimated_fan_with_known_unavailable_wait(
            &projected_win,
            melds,
            13,
            &pass_known_unavailable,
        ),
        2
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
    assert!(
        capped_hu_chase_wall_hit_probability(&table, 0, 3)
            >= CAPPED_HU_CHASE_MIN_WALL_HIT_PROBABILITY
    );
    assert_eq!(payment_fans_for_table(1, &table, 0, Some(1)), vec![3]);
    assert_eq!(payment_fans_for_table(2, &table, 0, Some(1)), vec![4]);
    assert!(should_pass_hu_for_capped_live_wait(
        &hand,
        &current_win,
        melds,
        &table,
        0,
        16,
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn confirmed_multi_hu_uses_single_closed_payer_fan() {
    let mut table = table_with_discards(1, vec![16]);
    table.dealer_position = 3;
    for position in 2..=3 {
        table.seats.insert(
            position,
            AiSeatView {
                position,
                hand_count: 13,
                discards: Vec::new(),
                melds: Vec::new(),
            },
        );
    }

    assert_eq!(payment_fans_for_table(1, &table, 0, Some(1)), vec![3]);

    table.claim_has_hu_response = true;
    assert_eq!(payment_fans_for_table(1, &table, 0, Some(1)), vec![2]);
}

#[test]
fn late_claim_hu_takes_when_capped_wait_is_unlikely_to_reach_wall() {
    let mut table = table_with_discards(1, vec![16]);
    table.wall_count = 4;
    table.score_cap = Some(4);
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    table.seats.get_mut(&0).unwrap().discards = vec![16];
    table.claim_window = Some(AiClaimView {
        tile: 16,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![13, 14, 15, 15, 16, 16, 17, 28, 28, 28];
    let mut win_hand = hand.clone();
    win_hand.push(16);
    sort_tiles(&mut win_hand);
    let melds = table.seats.get(&0).unwrap().melds.as_slice();

    assert!(
        capped_hu_chase_wall_hit_probability(&table, 0, 3)
            < CAPPED_HU_CHASE_MIN_WALL_HIT_PROBABILITY
    );
    assert!(!should_pass_hu_for_capped_live_wait(
        &hand, &win_hand, melds, &table, 0, 16,
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0),
        Some(AiClaimChoice::Hu)
    );
}

#[test]
fn late_self_draw_hu_takes_when_capped_wait_is_unlikely_to_reach_wall() {
    let mut table = table_with_discards(1, vec![16]);
    table.wall_count = 4;
    table.score_cap = Some(4);
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    let win_hand = vec![13, 14, 15, 15, 16, 16, 16, 17, 28, 28, 28];

    assert!(is_late_defense_round(&table));
    assert!(
        capped_hu_chase_wall_hit_probability(&table, 0, 3)
            < CAPPED_HU_CHASE_MIN_WALL_HIT_PROBABILITY
    );
    assert!(!should_pass_self_draw_hu_from_view(
        &win_hand, &table, 0, 16,
    ));
}

#[test]
fn self_draw_hu_takes_when_dealer_payment_reaches_cap() {
    let mut table = table_with_discards(1, vec![16]);
    table.score_cap = Some(4);
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
        estimated_fan_with_known_unavailable_wait(&win_hand, melds, 16, &[],),
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
        estimated_fan_with_known_unavailable_wait(
            &capped_wait_win,
            melds,
            13,
            &pass_known_unavailable,
        ),
        2
    );
    assert!(!should_pass_self_draw_hu_from_view(
        &win_hand, &table, 0, 16,
    ));
}

#[test]
fn self_draw_hu_counts_each_payment_fan_for_next_capped_wait() {
    let mut table = table_with_discards(1, vec![16]);
    table.dealer_position = 3;
    table.score_cap = Some(8);
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    table.seats.get_mut(&1).unwrap().hand_count = 13;
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 13,
            discards: Vec::new(),
            melds: Vec::new(),
        },
    );
    table.seats.insert(
        3,
        AiSeatView {
            position: 3,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_peng_meld(9)],
        },
    );
    let win_hand = vec![13, 14, 15, 15, 16, 16, 16, 17, 28, 28, 28];
    let mut current_payment_fans = payment_fans_for_table(1, &table, 0, None);
    let mut projected_payment_fans = payment_fans_for_table(2, &table, 0, None);
    current_payment_fans.sort_unstable();
    projected_payment_fans.sort_unstable();

    assert!(!dealer_opponent_has_major_threat(&table, 0));
    assert_eq!(current_payment_fans, vec![2, 2, 2]);
    assert_eq!(projected_payment_fans, vec![3, 3, 3]);
    assert!(should_pass_self_draw_hu_from_view(&win_hand, &table, 0, 16,));

    table.current_self_draw_bonus_fan = 1;
    assert!(!should_pass_self_draw_hu_from_view(
        &win_hand, &table, 0, 16,
    ));
}

#[test]
fn self_draw_hu_takes_when_current_fan_exceeds_half_cap() {
    let mut table = table_with_discards(1, vec![16]);
    table.score_cap = Some(64);
    table.seats.get_mut(&0).unwrap().melds = vec![test_concealed_gang_meld(35)];
    let win_hand = vec![13, 14, 15, 15, 16, 16, 16, 17, 28, 28, 28];

    assert!(!should_pass_self_draw_hu_from_view(
        &win_hand, &table, 0, 16,
    ));
}
