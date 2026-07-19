use super::*;

#[test]
fn capped_non_dealer_prefers_wider_wait_over_single_wait_fan() {
    let mut table = table_with_discards(1, Vec::new());
    table.score_cap = Some(2);
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
    let hand = vec![2, 2, 4, 5, 7, 11, 12, 13, 21, 22, 23];

    assert_eq!(choose_discard_from_view(&hand, &table, 0), Some(7));
}

#[test]
fn discard_candidates_ignore_invalid_tiles() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    let hand = vec![1, 1, 4, 7, 9, 12, 14, 14, 17, 21, 23, 25, 31, 99];

    let choice = choose_discard_from_view(&hand, &table, 0);

    assert!(choice.is_some_and(is_valid_tile));
}

#[test]
fn hand_power_ignores_invalid_tiles() {
    let base_hand = vec![1, 2, 3, 11, 12, 13];
    let hand_with_invalid_triplet = vec![1, 2, 3, 11, 12, 13, 99, 99, 99];

    assert!(!is_valid_tile(99));
    assert_eq!(hand_power(&[99, 99, 99]), 0.0);
    assert!((hand_power(&hand_with_invalid_triplet) - hand_power(&base_hand)).abs() < 0.0001);
}

#[test]
fn hand_power_is_stable_across_repeated_evaluations() {
    let hand = vec![1, 2, 3, 4, 5, 7, 9, 11, 13, 15, 17, 21, 31, 35];
    let expected = hand_power(&hand).to_bits();

    for _ in 0..2_048 {
        assert_eq!(hand_power(&hand).to_bits(), expected);
    }
}

#[test]
fn opponent_pressure_is_stable_across_rebuilt_tables() {
    let score = || {
        let mut table = table_with_discards(1, vec![1]);
        table.wall_count = 25;
        table.seats.insert(
            2,
            AiSeatView {
                position: 2,
                hand_count: 8,
                discards: Vec::new(),
                melds: vec![test_peng_meld(1), test_peng_meld(11)],
            },
        );
        table.seats.insert(
            3,
            AiSeatView {
                position: 3,
                hand_count: 10,
                discards: Vec::new(),
                melds: Vec::new(),
            },
        );
        estimate_pressure_for_tile(&table, 0, 1).to_bits()
    };
    let expected = score();

    for _ in 0..2_048 {
        assert_eq!(score(), expected);
    }
}

#[test]
fn hand_progress_ignores_invalid_melds_but_counts_valid_melds() {
    let table = table_with_discards(1, Vec::new());
    let invalid_melds = vec![
        WsShenyangMahjongMeld {
            kind: ShenyangMahjongMeldKind::CHI,
            tiles: vec![1, 1, 1],
            from_position: Some(1),
        },
        WsShenyangMahjongMeld {
            kind: ShenyangMahjongMeldKind::PENG,
            tiles: vec![11, 11],
            from_position: Some(1),
        },
    ];
    let valid_melds = vec![test_chi_meld(1), test_peng_meld(11)];
    let base = hand_progress_score(&[], &[], &table, 0);

    assert_eq!(hand_progress_score(&[], &invalid_melds, &table, 0), base);
    assert!(hand_progress_score(&[], &valid_melds, &table, 0) > base);

    let base_after_discard = hand_progress_score_after_discard(&[], &[], &table, 0, 5);
    assert_eq!(
        hand_progress_score_after_discard(&[], &invalid_melds, &table, 0, 5,),
        base_after_discard
    );
    assert!(
        hand_progress_score_after_discard(&[], &valid_melds, &table, 0, 5,) > base_after_discard
    );
}

#[test]
fn late_broken_basic_discard_follows_public_tile_for_weak_recoverable_hand() {
    let mut table = table_with_discards(1, vec![31]);
    table.wall_count = 40;
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 11, 14, 19, 21, 31, 32, 33];

    assert_eq!(
        unrecoverable_normal_hand_requirement_count(&hand, &[], &table, 0),
        0
    );
    assert!(hand_power(&hand) >= 16.0);
    assert!(hand_power(&hand) < 18.0);
    assert!(!should_lock_seven_pairs_plan(&hand, &[], &table, 0));
    assert_eq!(
        pure_one_suit_plan_score_for_context(&hand, &[], &table, 0),
        0.0
    );
    assert_eq!(piao_plan_score_for_context(&hand, &[], &table, 0), 0.0);
    assert_eq!(best_ready_score_after_discard(&hand, &[], &table, 0), 0.0);
    assert_eq!(
        best_one_step_wait_potential_after_discard(&hand, &[], &table, 0),
        0.0
    );
    assert!(should_use_broken_hand_public_defense_discard(
        &hand,
        &[],
        &table,
        0
    ));
    assert_eq!(choose_discard_from_view(&hand, &table, 0), Some(31));
}

#[test]
fn late_ready_discard_breaks_wait_for_public_safe_tile() {
    let mut table = table_with_discards(1, vec![5]);
    table.wall_count = 16;
    table.seats.get_mut(&0).unwrap().melds = vec![test_chi_meld(11)];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();
    let hand = vec![1, 2, 3, 4, 5, 6, 21, 22, 32, 35, 35];

    assert_eq!(
        ready_live_tile_count_after_discard(&remove_n_tiles(&hand, 5, 1), melds, &table, 0, 5,),
        0
    );
    assert!(
        ready_live_tile_count_after_discard(&remove_n_tiles(&hand, 32, 1), melds, &table, 0, 32,)
            > 0
    );
    assert_eq!(choose_discard_from_view(&hand, &table, 0), Some(5));
}

#[test]
fn late_unready_discard_uses_defense_before_hand_progress() {
    let mut table = table_with_discards(1, vec![14]);
    table.wall_count = 16;
    let hand = vec![1, 1, 4, 7, 9, 12, 14, 14, 17, 21, 23, 25, 31, 35];

    assert_eq!(choose_discard_from_view(&hand, &table, 0), Some(14));
}

#[test]
fn illegal_near_ready_shape_uses_defensive_opening() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 40;
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 31, 31, 35];

    assert_eq!(ready_tile_score(&hand, &[], &table, 0), 0.0);
    assert_eq!(one_step_wait_potential(&hand, &[], &table, 0), 0.0);
    assert!(should_open_broken_closed_hand_for_defense(
        &hand,
        &[],
        &table,
        0
    ));
}

#[test]
fn round_thresholds_match_ai_phase_boundaries() {
    let mut table = table_with_discards(1, Vec::new());

    table.wall_count = FINAL_DEFENSE_WALL_COUNT + 1;
    assert!(!is_late_defense_round(&table));
    table.wall_count = FINAL_DEFENSE_WALL_COUNT;
    assert!(is_late_defense_round(&table));

    table.wall_count = LATE_PRESSURE_WALL_COUNT + 1;
    assert!(!is_late_round(&table));
    table.wall_count = LATE_PRESSURE_WALL_COUNT;
    assert!(is_late_round(&table));

    table.wall_count = MID_BROKEN_HAND_WALL_COUNT + 1;
    assert!(!is_mid_broken_hand_defense_round(&table));
    assert!(!is_mid_opening_round(&table));
    table.wall_count = MID_BROKEN_HAND_WALL_COUNT;
    assert!(is_mid_broken_hand_defense_round(&table));
    assert!(is_mid_opening_round(&table));

    table.wall_count = MID_ROUND_WALL_COUNT + 1;
    assert!(!is_mid_round(&table));
    table.wall_count = MID_ROUND_WALL_COUNT;
    assert!(is_mid_round(&table));
}

#[test]
fn tile_pressure_ignores_invalid_melds_but_counts_valid_melds() {
    let mut table = table_with_discards(1, Vec::new());
    let base = estimate_pressure_for_tile(&table, 0, 5);
    table.seats.get_mut(&1).unwrap().melds = vec![
        WsShenyangMahjongMeld {
            kind: ShenyangMahjongMeldKind::CHI,
            tiles: vec![1, 1, 1],
            from_position: Some(0),
        },
        WsShenyangMahjongMeld {
            kind: ShenyangMahjongMeldKind::PENG,
            tiles: vec![11, 11],
            from_position: Some(0),
        },
    ];

    assert_eq!(estimate_pressure_for_tile(&table, 0, 5), base);

    table.seats.get_mut(&1).unwrap().melds = vec![test_chi_meld(1), test_peng_meld(11)];

    assert!((estimate_pressure_for_tile(&table, 0, 5) - (base - 0.7)).abs() < 0.0001);
}

#[test]
fn unique_tiles_ignores_invalid_tiles() {
    assert_eq!(unique_tiles(&[99, 1, 1, 37, 0]), vec![1, 37]);
}

#[test]
fn audit_non_dealer_discard_keeps_a_legal_ready_route() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 3;
    table.wall_count = 70;
    let mut random_state = 0x9e37_79b9_7f4a_7c15u64;

    for _ in 0..400 {
        let mut wall = SHENYANG_MAHJONG_TILE_KINDS
            .into_iter()
            .flat_map(|tile| [tile; 4])
            .collect::<Vec<_>>();
        for index in (1..wall.len()).rev() {
            random_state = random_state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            wall.swap(index, random_state as usize % (index + 1));
        }
        let mut hand = wall.into_iter().take(14).collect::<Vec<_>>();
        sort_tiles(&mut hand);
        if is_complete_win_for_table(&hand, &[], &table) {
            continue;
        }
        let ready_discards = unique_tiles(&hand)
            .into_iter()
            .filter(|tile| {
                let after_discard = remove_n_tiles(&hand, *tile, 1);
                ready_tile_score_after_discard(&after_discard, &[], &table, 0, *tile) > 0.0
            })
            .collect::<Vec<_>>();
        if ready_discards.is_empty() {
            continue;
        }

        let chosen = choose_discard_from_view(&hand, &table, 0)
            .expect("a valid fourteen-tile hand should choose a discard");
        let after_chosen = remove_n_tiles(&hand, chosen, 1);
        assert!(
            ready_tile_score_after_discard(&after_chosen, &[], &table, 0, chosen) > 0.0,
            "hand {hand:?} had ready discards {ready_discards:?} but chose {chosen}",
        );
    }
}
