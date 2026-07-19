use super::*;

#[test]
fn capped_four_piao_melds_prefers_wider_wait_over_honor_shape() {
    let mut table = table_with_discards(1, vec![31]);
    table.score_cap = Some(16);
    table.seats.get_mut(&0).unwrap().melds = vec![
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(21),
        test_peng_meld(32),
    ];
    let hand = vec![5, 31];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();

    assert!(
        piao_single_wait_tile_score(5, &[5], melds, &table, 0)
            > piao_single_wait_tile_score(31, &[31], melds, &table, 0)
    );
    assert_eq!(choose_discard_from_view(&hand, &table, 0), Some(31));
}

#[test]
fn dealer_four_piao_melds_prefers_live_middle_over_low_live_wind_wait() {
    let mut table = table_with_discards(1, vec![31, 31]);
    table.dealer_position = 0;
    table.seats.get_mut(&0).unwrap().melds = vec![
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(21),
        test_peng_meld(32),
    ];
    let hand = vec![5, 31];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();

    assert!(
        piao_single_wait_tile_score(5, &[5], melds, &table, 0)
            > piao_single_wait_tile_score(31, &[31], melds, &table, 0)
    );
    assert_eq!(choose_discard_from_view(&hand, &table, 0), Some(31));
}

#[test]
fn dealer_piao_single_wait_still_prefers_wider_middle_wait() {
    let mut table = table_with_discards(1, vec![31]);
    table.dealer_position = 0;
    table.wall_count = 32;
    table.seats.get_mut(&0).unwrap().melds = vec![
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(21),
        test_peng_meld(35),
    ];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();

    assert!(
        piao_single_wait_tile_score(5, &[5], melds, &table, 0)
            > piao_single_wait_tile_score(31, &[31], melds, &table, 0)
    );
}

#[test]
fn dealer_piao_single_wait_keeps_wider_wait_over_safer_discard() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    table.wall_count = 32;
    table.seats.get_mut(&0).unwrap().melds = vec![
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(21),
        test_peng_meld(32),
    ];
    let opponent = table.seats.get_mut(&1).unwrap();
    opponent.hand_count = 2;
    opponent.melds = vec![
        test_peng_meld(2),
        test_peng_meld(12),
        test_peng_meld(22),
        test_peng_meld(23),
    ];
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 13,
            discards: vec![31],
            melds: Vec::new(),
        },
    );
    let hand = vec![5, 31];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();
    let middle_wait_hand = vec![5];
    let wind_wait_hand = vec![31];
    let middle_wait_live = remaining_tile_count_with_melds_after_discards(
        &middle_wait_hand,
        melds,
        &table,
        0,
        5,
        &[31],
    );
    let wind_wait_live =
        remaining_tile_count_with_melds_after_discards(&wind_wait_hand, melds, &table, 0, 31, &[5]);

    assert!(middle_wait_live > wind_wait_live);
    assert!(opponent_threat_discard_bias(&table, 0, 31, 1) < 0.0);
    assert_eq!(choose_discard_from_view(&hand, &table, 0), Some(31));

    table.dealer_position = 3;
    table.score_cap = Some(64);
    assert_eq!(choose_discard_from_view(&hand, &table, 0), Some(5));

    table.score_cap = Some(50);
    assert_eq!(choose_discard_from_view(&hand, &table, 0), Some(31));
}

#[test]
fn discard_after_four_piao_melds_keeps_live_single_wait() {
    let mut table = table_with_discards(1, vec![36, 36, 36]);
    table.seats.get_mut(&0).unwrap().melds = vec![
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(21),
        test_peng_meld(31),
    ];
    let hand = vec![36, 37];

    assert_eq!(choose_discard_from_view(&hand, &table, 0), Some(36));
}

#[test]
fn discard_after_four_piao_melds_rejects_dead_exposed_wind_wait() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(21),
        test_peng_meld(31),
    ];
    let hand = vec![5, 31];

    assert_eq!(remaining_tile_count(&[31], &table, 0, 31), 0);
    assert_eq!(choose_discard_from_view(&hand, &table, 0), Some(31));
}

#[test]
fn late_open_hand_avoids_live_tile_against_four_piao_melds() {
    let mut seats = HashMap::new();
    seats.insert(
        0,
        AiSeatView {
            position: 0,
            hand_count: 1,
            discards: vec![31, 33, 19, 16, 1, 31, 21, 8, 12, 32, 3, 4, 2, 15, 22, 4],
            melds: vec![
                test_peng_meld(37),
                test_peng_meld(5),
                test_peng_meld(6),
                test_peng_meld(25),
            ],
        },
    );
    seats.insert(
        1,
        AiSeatView {
            position: 1,
            hand_count: 11,
            discards: vec![21, 4, 15, 35, 11, 12, 16, 34, 33, 33, 35, 35],
            melds: vec![test_peng_meld(19)],
        },
    );
    seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 13,
            discards: vec![34, 1, 22, 33, 12, 23, 5, 3, 25, 28, 1, 29],
            melds: Vec::new(),
        },
    );
    seats.insert(
        3,
        AiSeatView {
            position: 3,
            hand_count: 8,
            discards: vec![34, 32, 22, 8, 35, 16, 11, 12, 17, 3, 28, 28],
            melds: vec![test_peng_meld(7), test_peng_meld(26)],
        },
    );
    let table = AiPublicTable {
        current_position: 1,
        dealer_position: 0,
        wall_count: 31,
        score_cap: Some(16),
        allow_first_chi: true,
        ting_fan_enabled: false,
        ting_positions: Default::default(),
        current_self_draw_bonus_fan: 0,
        claim_is_rob_gang: false,
        claim_has_hu_response: false,
        claim_window: None,
        seats,
    };
    let hand = vec![7, 8, 9, 9, 9, 13, 22, 23, 24, 36, 36];

    assert_ne!(choose_discard_from_view(&hand, &table, 1), Some(13));
}

#[test]
fn piao_single_wait_discard_avoids_pure_one_suit_threat_tile() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 32;
    table.seats.get_mut(&0).unwrap().melds = vec![
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(21),
        test_peng_meld(32),
    ];
    table.seats.get_mut(&1).unwrap().melds = vec![test_chi_meld(2), test_peng_meld(7)];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();
    let hand = vec![5, 31];

    assert!(
        piao_single_wait_tile_score(31, &[31], melds, &table, 0)
            > piao_single_wait_tile_score(5, &[5], melds, &table, 0)
    );
    assert!(
        pure_one_suit_threat_discard_bias(&table, 0, 5, 1)
            < pure_one_suit_threat_discard_bias(&table, 0, 31, 1)
    );
    assert_eq!(choose_discard_from_view(&hand, &table, 0), Some(31));
}

#[test]
fn piao_single_wait_score_rejects_wait_that_cannot_complete_requirements() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![
        test_peng_meld(2),
        test_peng_meld(3),
        test_peng_meld(12),
        test_peng_meld(13),
    ];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();

    assert!(piao_needs_terminal_or_honor_from_melds(melds));
    assert_eq!(piao_missing_suits_from_melds(melds), vec![2]);
    assert_eq!(
        piao_single_wait_tile_score(15, &[15], melds, &table, 0),
        -240.0
    );
    assert!(piao_single_wait_tile_score(21, &[21], melds, &table, 0) > 0.0);
}

#[test]
fn threatening_dealer_increases_piao_live_wait_priority() {
    let mut table = table_with_discards(1, vec![31]);
    table.wall_count = 32;
    table.seats.get_mut(&0).unwrap().melds = vec![
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(21),
        test_peng_meld(32),
    ];
    let dealer = table.seats.get_mut(&1).unwrap();
    dealer.hand_count = 4;
    dealer.melds = vec![test_peng_meld(3), test_peng_meld(14), test_peng_meld(25)];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();

    table.dealer_position = 3;
    let regular_live_advantage = piao_single_wait_tile_score(5, &[5], melds, &table, 0)
        - piao_single_wait_tile_score(31, &[31], melds, &table, 0);
    assert!(!dealer_opponent_has_major_threat(&table, 0));

    table.dealer_position = 1;
    let threatened_live_advantage = piao_single_wait_tile_score(5, &[5], melds, &table, 0)
        - piao_single_wait_tile_score(31, &[31], melds, &table, 0);
    assert!(dealer_opponent_has_major_threat(&table, 0));
    assert!(threatened_live_advantage > regular_live_advantage);
}
