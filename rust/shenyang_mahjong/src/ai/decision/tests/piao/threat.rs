use super::*;

#[test]
fn opponent_piao_threat_ignores_player_after_chi_meld() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&1).unwrap().melds = vec![
        test_chi_meld(2),
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(21),
    ];

    assert_eq!(piao_threat_level(&table.seats.get(&1).unwrap().melds), 0);
    assert_eq!(opponent_threat_discard_bias(&table, 0, 5, 2), 0.0);
}

#[test]
fn opponent_four_piao_threat_penalizes_live_pair_more_than_singleton() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().hand_count = 2;
    table.seats.get_mut(&1).unwrap().melds = vec![
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(21),
        test_peng_meld(31),
    ];

    assert!(
        opponent_threat_discard_bias(&table, 0, 5, 2)
            < opponent_threat_discard_bias(&table, 0, 6, 1)
    );
}

#[test]
fn piao_threat_penalizes_own_pair_more_than_own_triplet() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];

    assert!(piao_threat_pair_penalty(31, 2) > piao_threat_pair_penalty(31, 3));
    assert!(piao_threat_pair_penalty(5, 2) > piao_threat_pair_penalty(5, 3));
    assert!(
        opponent_threat_discard_bias(&table, 0, 31, 2)
            < opponent_threat_discard_bias(&table, 0, 31, 3)
    );
}

#[test]
fn opponent_four_piao_threat_ignores_impossible_two_missing_suits() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().hand_count = 2;
    table.seats.get_mut(&1).unwrap().melds = vec![
        test_peng_meld(2),
        test_peng_meld(3),
        test_peng_meld(4),
        test_peng_meld(5),
    ];

    assert_eq!(
        piao_missing_suits_from_melds(&table.seats.get(&1).unwrap().melds),
        vec![1, 2]
    );
    assert!(piao_threat_cannot_satisfy_three_suits(
        &table.seats.get(&1).unwrap().melds,
        table.seats.get(&1).unwrap().hand_count
    ));
    assert_eq!(opponent_threat_discard_bias(&table, 0, 31, 1), 0.0);

    table.seats.get_mut(&1).unwrap().melds = vec![
        test_peng_meld(2),
        test_peng_meld(3),
        test_peng_meld(12),
        test_peng_meld(13),
    ];
    assert_eq!(
        piao_missing_suits_from_melds(&table.seats.get(&1).unwrap().melds),
        vec![2]
    );
    assert!(!piao_threat_cannot_satisfy_three_suits(
        &table.seats.get(&1).unwrap().melds,
        table.seats.get(&1).unwrap().hand_count
    ));
    assert!(opponent_threat_discard_bias(&table, 0, 21, 1) < 0.0);
}

#[test]
fn opponent_three_honor_piao_threat_ignores_impossible_three_missing_suits() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 32;
    table.seats.get_mut(&1).unwrap().hand_count = 5;
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(31), test_peng_meld(35), test_peng_meld(36)];

    assert_eq!(
        piao_missing_suits_from_melds(&table.seats.get(&1).unwrap().melds),
        vec![0, 1, 2]
    );
    assert!(piao_threat_cannot_satisfy_three_suits(
        &table.seats.get(&1).unwrap().melds,
        table.seats.get(&1).unwrap().hand_count
    ));
    assert!(!piao_threat_needs_suit(table.seats.get(&1).unwrap(), 0));
    assert_eq!(opponent_threat_discard_bias(&table, 0, 1, 1), 0.0);
}

#[test]
fn opponent_three_piao_threat_can_still_cover_two_missing_suits() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 32;
    table.seats.get_mut(&1).unwrap().hand_count = 5;
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(2), test_peng_meld(3), test_peng_meld(4)];

    assert_eq!(
        piao_missing_suits_from_melds(&table.seats.get(&1).unwrap().melds),
        vec![1, 2]
    );
    assert!(!piao_threat_cannot_satisfy_three_suits(
        &table.seats.get(&1).unwrap().melds,
        table.seats.get(&1).unwrap().hand_count
    ));
    assert!(piao_threat_needs_suit(table.seats.get(&1).unwrap(), 1));
    assert!(opponent_threat_discard_bias(&table, 0, 11, 1) < 0.0);
}

#[test]
fn opponent_four_piao_threat_penalizes_missing_suit_wait_tile() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().hand_count = 2;
    table.seats.get_mut(&1).unwrap().melds = vec![
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(12),
        test_peng_meld(31),
    ];

    assert_eq!(
        piao_missing_suits_from_melds(&table.seats.get(&1).unwrap().melds),
        vec![2]
    );
    assert!(
        opponent_threat_discard_bias(&table, 0, 25, 1)
            < opponent_threat_discard_bias(&table, 0, 15, 1)
    );
    assert_eq!(opponent_threat_discard_bias(&table, 0, 15, 1), 0.0);
}

#[test]
fn opponent_four_piao_threat_needing_yaojiu_only_penalizes_missing_suit_terminal_wait() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().hand_count = 2;
    table.seats.get_mut(&1).unwrap().melds = vec![
        test_peng_meld(2),
        test_peng_meld(3),
        test_peng_meld(12),
        test_peng_meld(13),
    ];

    assert_eq!(
        piao_missing_suits_from_melds(&table.seats.get(&1).unwrap().melds),
        vec![2]
    );
    assert!(piao_needs_terminal_or_honor_from_melds(
        &table.seats.get(&1).unwrap().melds
    ));
    assert!(
        opponent_threat_discard_bias(&table, 0, 21, 1)
            < opponent_threat_discard_bias(&table, 0, 25, 1)
    );
    assert!(
        opponent_threat_discard_bias(&table, 0, 25, 1)
            >= opponent_threat_discard_bias(&table, 0, 15, 1)
    );
    assert!(
        opponent_threat_discard_bias(&table, 0, 21, 1)
            < opponent_threat_discard_bias(&table, 0, 31, 1)
    );
    assert!(
        opponent_threat_discard_bias(&table, 0, 31, 1)
            >= opponent_threat_discard_bias(&table, 0, 25, 1)
    );
}

#[test]
fn piao_threat_penalizes_live_wind_pair_more_than_terminal_singleton() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];

    assert!(
        opponent_threat_discard_bias(&table, 0, 31, 2)
            < opponent_threat_discard_bias(&table, 0, 9, 1)
    );
}

#[test]
fn piao_threat_needing_yaojiu_penalizes_live_terminal_over_middle() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 32;
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(2), test_peng_meld(12), test_peng_meld(22)];

    assert!(piao_needs_terminal_or_honor_from_melds(
        &table.seats.get(&1).unwrap().melds
    ));
    assert!(
        opponent_threat_discard_bias(&table, 0, 9, 1)
            < opponent_threat_discard_bias(&table, 0, 5, 1)
    );
    assert!(
        opponent_threat_discard_bias(&table, 0, 31, 1)
            < opponent_threat_discard_bias(&table, 0, 5, 1)
    );
}

#[test]
fn piao_threat_discounts_exposed_meld_tiles() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];
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
        opponent_threat_discard_bias(&table, 0, 6, 1)
            > opponent_threat_discard_bias(&table, 0, 5, 1)
    );
}

#[test]
fn piao_threat_discounts_public_discards_from_other_seats() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: vec![5, 5],
            melds: Vec::new(),
        },
    );

    assert_eq!(public_discard_count(&table, 5), 2);
    assert_eq!(public_discard_count(&table, 6), 0);
    assert!(
        opponent_threat_discard_bias(&table, 0, 5, 1)
            > opponent_threat_discard_bias(&table, 0, 6, 1)
    );
}

#[test]
fn piao_threat_values_repeated_discards_from_threat_seat() {
    let mut table = table_with_discards(1, vec![5]);
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];

    let single_discard_bias = opponent_threat_discard_bias(&table, 0, 5, 1);

    table.seats.get_mut(&1).unwrap().discards = vec![5, 5];
    let repeated_discard_bias = opponent_threat_discard_bias(&table, 0, 5, 1);

    assert!(single_discard_bias > 0.0);
    assert!(repeated_discard_bias > single_discard_bias);
}

#[test]
fn piao_threat_ignores_tile_fully_accounted_by_public_and_own_tiles() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().hand_count = 2;
    table.seats.get_mut(&1).unwrap().melds = vec![
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(21),
        test_peng_meld(31),
    ];
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: vec![5, 5],
            melds: Vec::new(),
        },
    );

    assert!(piao_threat_tile_fully_accounted(&table, 5, 2));
    assert_eq!(opponent_threat_discard_bias(&table, 0, 5, 2), 0.0);
    assert!(opponent_threat_discard_bias(&table, 0, 6, 1) < 0.0);
}

#[test]
fn piao_threat_exposure_reaches_zero_when_all_copies_are_public() {
    let table = table_with_discards(1, vec![5, 5, 5, 5]);

    assert_eq!(piao_threat_exposure_scale(&table, 5), 0.0);
}
