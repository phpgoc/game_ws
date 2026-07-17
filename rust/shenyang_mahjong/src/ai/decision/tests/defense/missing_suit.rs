use super::*;

#[test]
fn late_defense_avoids_cold_honor_against_closed_opponent() {
    let mut table = table_with_discards(1, vec![11, 14, 19]);
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().hand_count = 13;
    let hand = vec![2, 3, 4, 5, 6, 7, 8, 12, 13, 15, 16, 17, 18, 31];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(12)
    );
}

#[test]
fn late_defense_bias_keeps_public_honor_above_four_public_middle_tiles() {
    let mut table = table_with_discards(1, vec![5, 5, 5, 5, 31]);
    table.wall_count = 16;

    assert!(late_defense_discard_bias(&table, 0, 31) > late_defense_discard_bias(&table, 0, 5));
}

#[test]
fn late_defense_candidates_avoid_piao_needed_suit_over_missing_suit_read() {
    let mut table = table_with_discards(1, vec![1, 4, 9]);
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(11), test_peng_meld(21), test_peng_meld(31)];
    let hand = vec![5, 12];

    assert_eq!(
        choose_late_defense_discard_from_candidates(&hand, &table, 0, vec![5, 12]),
        Some(12)
    );
}

#[test]
fn late_defense_chi_only_closed_opponent_blocks_missing_suit_read_when_chi_does_not_open_door() {
    let mut table = table_with_discards(1, vec![1, 4, 9]);
    table.wall_count = 16;
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: vec![1, 4, 9],
            melds: Vec::new(),
        },
    );
    table.seats.insert(
        3,
        AiSeatView {
            position: 3,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_chi_meld(11)],
        },
    );

    assert!(opponent_missing_suit_safety_bias(&table, 0, 5) > 0.0);
}

#[test]
fn late_defense_closed_opponent_blocks_other_missing_suit_reads() {
    let mut table = table_with_discards(1, vec![1, 4, 9]);
    table.wall_count = 16;
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: vec![1, 4, 9],
            melds: Vec::new(),
        },
    );
    let mut closed_threat_table = table.clone();
    closed_threat_table.seats.insert(
        3,
        AiSeatView {
            position: 3,
            hand_count: 13,
            discards: Vec::new(),
            melds: Vec::new(),
        },
    );

    assert!(opponent_missing_suit_safety_bias(&table, 0, 5) > 0.0);
    assert_eq!(
        opponent_missing_suit_safety_bias(&closed_threat_table, 0, 5),
        0.0
    );
}

#[test]
fn late_defense_concealed_gang_opponent_blocks_other_missing_suit_reads() {
    let mut table = table_with_discards(1, vec![1, 4, 9]);
    table.wall_count = 16;
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: vec![1, 4, 9],
            melds: Vec::new(),
        },
    );

    let mut short_closed_table = table.clone();
    short_closed_table.seats.insert(
        3,
        AiSeatView {
            position: 3,
            hand_count: 9,
            discards: Vec::new(),
            melds: Vec::new(),
        },
    );

    let mut concealed_gang_table = short_closed_table.clone();
    concealed_gang_table.seats.get_mut(&3).unwrap().melds = vec![test_concealed_gang_meld(9)];

    assert!(opponent_missing_suit_safety_bias(&short_closed_table, 0, 5) > 0.0);
    assert_eq!(
        opponent_missing_suit_safety_bias(&concealed_gang_table, 0, 5),
        0.0
    );
}

#[test]
fn late_defense_discards_fully_accounted_tile_before_live_wind() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().melds = vec![test_chi_meld(4)];
    let hand = vec![2, 4, 6, 6, 6, 8, 12, 14, 16, 18, 22, 24, 31, 35];

    assert_eq!(public_discard_count(&table, 6), 0);
    assert_eq!(exposed_meld_tile_count(&table, 6), 1);
    assert_eq!(
        choose_late_defense_discard_from_candidates(&hand, &table, 0, vec![6, 31]),
        Some(6)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(6)
    );
}

#[test]
fn late_defense_discards_live_middle_before_breaking_terminal_pair() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    let hand = vec![2, 4, 5, 6, 8, 9, 9, 12, 14, 16, 18, 22, 24, 26];

    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(9)
    );
}

#[test]
fn late_defense_discards_three_exposed_meld_tile_before_live_wind() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_peng_meld(6)],
        },
    );
    let hand = vec![2, 4, 6, 8, 12, 14, 16, 18, 22, 24, 26, 28, 31, 35];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(6)
    );
}

#[test]
fn late_defense_does_not_mark_exposed_suit_as_missing() {
    let mut table = table_with_discards(1, vec![11, 14, 19]);
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().melds = vec![WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![12, 12, 12],
        from_position: Some(0),
    }];

    assert_eq!(opponent_missing_suit_safety_bias(&table, 0, 12), 0.0);
}

#[test]
fn late_defense_does_not_mark_piao_needed_suit_as_missing() {
    let mut table = table_with_discards(1, vec![1, 4, 9]);
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(11), test_peng_meld(21), test_peng_meld(31)];

    assert_eq!(
        piao_missing_suits_from_melds(&table.seats.get(&1).unwrap().melds),
        vec![0]
    );
    assert_eq!(opponent_missing_suit_safety_bias(&table, 0, 5), 0.0);
}

#[test]
fn late_defense_final_piao_wait_only_blocks_missing_suit_tiles_that_can_complete_requirements() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().hand_count = 2;
    table.seats.get_mut(&1).unwrap().melds = vec![
        test_peng_meld(2),
        test_peng_meld(3),
        test_peng_meld(12),
        test_peng_meld(13),
    ];
    for position in [2, 3] {
        table.seats.insert(
            position,
            AiSeatView {
                position,
                hand_count: 10,
                discards: vec![22, 25, 28],
                melds: Vec::new(),
            },
        );
    }

    assert_eq!(
        piao_missing_suits_from_melds(&table.seats.get(&1).unwrap().melds),
        vec![2]
    );
    assert!(piao_needs_terminal_or_honor_from_melds(
        &table.seats.get(&1).unwrap().melds
    ));
    assert_eq!(opponent_missing_suit_safety_bias(&table, 0, 21), 0.0);
    assert!(opponent_missing_suit_safety_bias(&table, 0, 25) > 0.0);
}

#[test]
fn late_defense_follows_public_tile_before_live_missing_suit_read() {
    let missing_suit_discards = vec![11, 13, 14, 19, 11, 13, 14, 19, 11, 13];
    let mut table = table_with_discards(1, {
        let mut discards = missing_suit_discards.clone();
        discards.push(5);
        discards
    });
    table.wall_count = 16;
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: missing_suit_discards.clone(),
            melds: Vec::new(),
        },
    );
    table.seats.insert(
        3,
        AiSeatView {
            position: 3,
            hand_count: 10,
            discards: missing_suit_discards,
            melds: Vec::new(),
        },
    );
    let hand = vec![2, 5, 7, 9, 12, 16, 18, 21, 23, 25, 27, 31, 33, 35];

    assert!(
        late_defense_tile_safety_score(&table, 0, 12, 1)
            > late_defense_tile_safety_score(&table, 0, 5, 1)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(5)
    );
}

#[test]
fn late_defense_missing_suit_read_can_beat_live_wind() {
    let mut table = table_with_discards(1, vec![11, 14, 19]);
    table.wall_count = 16;
    let hand = vec![2, 3, 4, 5, 6, 7, 8, 12, 13, 15, 16, 17, 18, 31];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(12)
    );
}

#[test]
fn late_defense_piao_needed_suit_blocks_other_missing_suit_reads() {
    let mut table = table_with_discards(1, vec![1, 4, 9]);
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(11), test_peng_meld(21), test_peng_meld(31)];
    for position in [2, 3] {
        table.seats.insert(
            position,
            AiSeatView {
                position,
                hand_count: 10,
                discards: vec![1, 4, 9],
                melds: Vec::new(),
            },
        );
    }
    let mut no_piao_table = table.clone();
    no_piao_table.seats.get_mut(&1).unwrap().melds.clear();

    assert!(opponent_missing_suit_safety_bias(&no_piao_table, 0, 5) > 0.0);
    assert_eq!(opponent_missing_suit_safety_bias(&table, 0, 5), 0.0);
}

#[test]
fn late_defense_prefers_live_middle_before_breaking_terminal_pair() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;

    assert!(
        late_defense_tile_safety_score(&table, 0, 5, 1)
            > late_defense_tile_safety_score(&table, 0, 9, 2)
    );
}

#[test]
fn late_defense_prefers_live_wind_then_terminal_then_middle() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;

    assert!(
        late_defense_tile_safety_score(&table, 0, 31, 1)
            > late_defense_tile_safety_score(&table, 0, 9, 1)
    );
    assert!(
        late_defense_tile_safety_score(&table, 0, 9, 1)
            > late_defense_tile_safety_score(&table, 0, 5, 1)
    );
}

#[test]
fn late_defense_prefers_lone_wind_before_breaking_wind_pair() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    let hand = vec![1, 2, 4, 6, 8, 11, 13, 15, 17, 21, 23, 31, 31, 32];

    assert!(
        late_defense_tile_safety_score(&table, 0, 32, 1)
            > late_defense_tile_safety_score(&table, 0, 31, 2)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(32)
    );
}

#[test]
fn late_defense_prefers_opponent_missing_suit_tile() {
    let mut table = table_with_discards(1, vec![11, 14, 19]);
    table.wall_count = 16;
    let hand = vec![2, 3, 4, 5, 6, 7, 8, 12, 13, 15, 16, 17, 18, 22];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(12)
    );
}

#[test]
fn late_defense_prefers_own_previous_middle_discard_over_other_public_middle() {
    let mut table = table_with_discards(1, vec![5]);
    table.wall_count = 16;
    table.seats.get_mut(&0).unwrap().discards = vec![8];
    let hand = vec![2, 3, 5, 7, 8, 12, 14, 16, 18, 21, 23, 25, 31, 35];

    assert!(
        late_defense_tile_safety_score(&table, 0, 8, 1)
            > late_defense_tile_safety_score(&table, 0, 5, 1)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(8)
    );
}

#[test]
fn late_defense_prefers_public_honor_over_multiple_public_suited_tile() {
    let mut table = table_with_discards(1, vec![5, 5, 31]);
    table.wall_count = 16;

    assert!(
        late_defense_tile_safety_score(&table, 0, 31, 1)
            > late_defense_tile_safety_score(&table, 0, 5, 1)
    );
}

#[test]
fn late_defense_prefers_public_middle_tile_over_live_wind() {
    let mut table = table_with_discards(1, vec![5]);
    table.wall_count = 16;

    assert!(
        late_defense_tile_safety_score(&table, 0, 5, 1)
            > late_defense_tile_safety_score(&table, 0, 31, 1)
    );
}

#[test]
fn late_defense_prefers_public_middle_tile_over_public_terminal() {
    let mut table = table_with_discards(1, vec![5, 9]);
    table.wall_count = 16;

    assert!(
        late_defense_tile_safety_score(&table, 0, 5, 1)
            > late_defense_tile_safety_score(&table, 0, 9, 1)
    );
}

#[test]
fn late_defense_prefers_public_tile_seen_from_multiple_seats() {
    let mut table = table_with_discards(1, vec![5, 5]);
    table.wall_count = 16;
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: vec![6],
            melds: Vec::new(),
        },
    );
    table.seats.insert(
        3,
        AiSeatView {
            position: 3,
            hand_count: 10,
            discards: vec![6],
            melds: Vec::new(),
        },
    );

    assert_eq!(
        public_discard_count(&table, 5),
        public_discard_count(&table, 6)
    );
    assert!(public_discard_seat_count(&table, 6) > public_discard_seat_count(&table, 5));
    assert!(late_defense_discard_bias(&table, 0, 6) > late_defense_discard_bias(&table, 0, 5));
}

#[test]
fn late_defense_ten_tile_closed_threat_blocks_other_missing_suit_reads() {
    let mut table = table_with_discards(1, vec![1, 4, 9]);
    table.wall_count = 16;
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: vec![1, 4, 9],
            melds: Vec::new(),
        },
    );

    let mut short_closed_table = table.clone();
    short_closed_table.seats.insert(
        3,
        AiSeatView {
            position: 3,
            hand_count: 9,
            discards: Vec::new(),
            melds: Vec::new(),
        },
    );

    let mut closed_threat_table = short_closed_table.clone();
    closed_threat_table.seats.get_mut(&3).unwrap().hand_count = 10;

    assert!(opponent_missing_suit_safety_bias(&short_closed_table, 0, 5) > 0.0);
    assert_eq!(
        opponent_missing_suit_safety_bias(&closed_threat_table, 0, 5),
        0.0
    );
}

#[test]
fn late_defense_treats_terminal_only_suit_discards_as_weak_missing_suit_read() {
    let mut terminal_only = table_with_discards(1, vec![1, 9, 1]);
    terminal_only.wall_count = 16;
    let mut with_middle = table_with_discards(1, vec![1, 5, 9]);
    with_middle.wall_count = 16;

    assert_eq!(opponent_missing_suit_safety_bias(&terminal_only, 0, 5), 2.0);
    assert!(
        opponent_missing_suit_safety_bias(&with_middle, 0, 5)
            > opponent_missing_suit_safety_bias(&terminal_only, 0, 5)
    );
}

#[test]
fn late_defense_values_fully_accounted_pair_over_live_wind() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().melds = vec![test_chi_meld(4)];
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_chi_meld(6)],
        },
    );

    assert_eq!(public_discard_count(&table, 6), 0);
    assert_eq!(exposed_meld_tile_count(&table, 6), 2);
    assert!(late_defense_tile_fully_accounted(&table, 6, 2));
    assert!(
        late_defense_tile_safety_score(&table, 0, 6, 2)
            > late_defense_tile_safety_score(&table, 0, 31, 1)
    );
}

#[test]
fn late_defense_values_three_exposed_meld_tiles_over_live_wind() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_peng_meld(6)],
        },
    );

    assert!(
        late_defense_tile_safety_score(&table, 0, 6, 1)
            > late_defense_tile_safety_score(&table, 0, 31, 1)
    );
}

#[test]
fn late_defense_values_two_exposed_meld_tiles_over_live_wind() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().melds = vec![test_chi_meld(4)];
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_chi_meld(6)],
        },
    );
    table.seats.insert(
        3,
        AiSeatView {
            position: 3,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_chi_meld(11)],
        },
    );

    assert_eq!(exposed_meld_tile_count(&table, 6), 2);
    assert!(
        late_defense_tile_safety_score(&table, 0, 6, 1)
            > late_defense_tile_safety_score(&table, 0, 31, 1)
    );
}

#[test]
fn late_discard_follows_safe_tile_over_hand_efficiency() {
    let mut table = table_with_discards(1, vec![5]);
    table.wall_count = 16;
    let hand = vec![3, 4, 5, 5, 6, 7, 11, 12, 13, 21, 22, 23, 31, 35];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(5)
    );
}

#[test]
fn mid_round_discard_follows_multiple_public_terminal_over_live_wind() {
    let mut table = table_with_discards(1, vec![9, 9]);
    table.wall_count = 36;
    let hand = vec![1, 2, 4, 6, 8, 9, 11, 12, 14, 16, 21, 23, 25, 31];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(9)
    );
}

#[test]
fn mid_round_public_discard_prefers_own_previous_middle_over_other_public_middle() {
    let mut table = table_with_discards(1, vec![5]);
    table.wall_count = 36;
    table.seats.get_mut(&0).unwrap().discards = vec![8];
    let hand = vec![2, 3, 5, 7, 8, 12, 14, 16, 18, 21, 23, 25, 31, 35];

    assert!(
        mid_round_public_discard_bias(&table, 0, 8) > mid_round_public_discard_bias(&table, 0, 5)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(8)
    );
}

#[test]
fn mid_round_public_discard_prefers_tile_seen_from_multiple_seats() {
    let mut table = table_with_discards(1, vec![5, 5]);
    table.wall_count = 36;
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: vec![6],
            melds: Vec::new(),
        },
    );
    table.seats.insert(
        3,
        AiSeatView {
            position: 3,
            hand_count: 10,
            discards: vec![6],
            melds: Vec::new(),
        },
    );

    assert_eq!(
        public_discard_count(&table, 5),
        public_discard_count(&table, 6)
    );
    assert!(public_discard_seat_count(&table, 6) > public_discard_seat_count(&table, 5));
    assert!(
        mid_round_public_discard_bias(&table, 0, 6) > mid_round_public_discard_bias(&table, 0, 5)
    );
}
