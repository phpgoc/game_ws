use super::*;

#[test]
fn closed_opponent_threat_does_not_penalize_public_safe_tile() {
    let mut table = table_with_discards(1, vec![31]);
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().hand_count = 13;

    assert_eq!(closed_opponent_threat_discard_bias(&table, 0, 31, 1), 0.0);
    assert!(closed_opponent_threat_discard_bias(&table, 0, 32, 1) < 0.0);
}

#[test]
fn closed_opponent_threat_discounts_exposed_meld_tiles() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().hand_count = 13;
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_peng_meld(9)],
        },
    );

    let exposed_terminal_bias = closed_opponent_threat_discard_bias(&table, 0, 9, 1);
    let cold_honor_bias = closed_opponent_threat_discard_bias(&table, 0, 31, 1);

    assert!(exposed_terminal_bias < 0.0);
    assert!(exposed_terminal_bias > cold_honor_bias);
}

#[test]
fn closed_opponent_threat_discounts_suit_after_shedding_it() {
    let mut table = table_with_discards(1, vec![11, 14, 19]);
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().hand_count = 13;

    let shed_suit_bias = closed_opponent_threat_discard_bias(&table, 0, 12, 1);
    let untouched_suit_bias = closed_opponent_threat_discard_bias(&table, 0, 5, 1);

    assert!(shed_suit_bias < 0.0);
    assert!(shed_suit_bias > untouched_suit_bias);
}

#[test]
fn closed_opponent_threat_lightly_discounts_suit_after_one_shed() {
    let mut neutral = table_with_discards(1, Vec::new());
    neutral.wall_count = 16;
    neutral.seats.get_mut(&1).unwrap().hand_count = 13;

    let mut one_shed = table_with_discards(1, vec![11]);
    one_shed.wall_count = 16;
    one_shed.seats.get_mut(&1).unwrap().hand_count = 13;

    let neutral_bias = closed_opponent_threat_discard_bias(&neutral, 0, 12, 1);
    let one_shed_bias = closed_opponent_threat_discard_bias(&one_shed, 0, 12, 1);

    assert!(one_shed_bias < 0.0);
    assert!(one_shed_bias > neutral_bias);
}

#[test]
fn closed_opponent_threat_grows_for_unshed_suit_after_off_suit_discards() {
    let mut neutral = table_with_discards(1, Vec::new());
    neutral.wall_count = 16;
    neutral.seats.get_mut(&1).unwrap().hand_count = 13;

    let mut committed = table_with_discards(1, vec![11, 14, 19, 31]);
    committed.wall_count = 16;
    committed.seats.get_mut(&1).unwrap().hand_count = 13;

    assert!(
        closed_opponent_threat_discard_bias(&committed, 0, 5, 1)
            < closed_opponent_threat_discard_bias(&neutral, 0, 5, 1)
    );
    assert!(
        closed_opponent_threat_discard_bias(&committed, 0, 12, 1)
            > closed_opponent_threat_discard_bias(&neutral, 0, 12, 1)
    );
}

#[test]
fn closed_opponent_threat_ignores_fully_exposed_tile() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().hand_count = 13;
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_gang_meld(9)],
        },
    );

    assert_eq!(closed_opponent_threat_discard_bias(&table, 0, 9, 1), 0.0);
}

#[test]
fn closed_opponent_threat_counts_ai_controlled_table_seat() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().hand_count = 13;

    assert!(closed_opponent_threat_discard_bias(&table, 0, 32, 1) < 0.0);
}

#[test]
fn closed_opponent_threat_counts_concealed_gang_as_closed() {
    let mut concealed = table_with_discards(1, Vec::new());
    concealed.wall_count = 16;
    concealed.seats.get_mut(&1).unwrap().melds = vec![test_concealed_gang_meld(9)];

    let mut open = table_with_discards(1, Vec::new());
    open.wall_count = 16;
    open.seats.get_mut(&1).unwrap().melds = vec![test_gang_meld(9)];

    assert!(closed_opponent_threat_discard_bias(&concealed, 0, 32, 1) < 0.0);
    assert_eq!(closed_opponent_threat_discard_bias(&open, 0, 32, 1), 0.0);
}

#[test]
fn closed_opponent_threat_counts_short_hand_after_concealed_gang() {
    let mut short_closed = table_with_discards(1, Vec::new());
    short_closed.wall_count = 16;
    short_closed.seats.get_mut(&1).unwrap().hand_count = 9;

    let mut short_concealed_gang = short_closed.clone();
    short_concealed_gang.seats.get_mut(&1).unwrap().melds = vec![test_concealed_gang_meld(9)];

    let mut longer_concealed_gang = short_concealed_gang.clone();
    longer_concealed_gang.seats.get_mut(&1).unwrap().hand_count = 10;

    assert_eq!(
        closed_opponent_threat_discard_bias(&short_closed, 0, 32, 1),
        0.0
    );
    assert!(
        closed_opponent_threat_discard_bias(&short_concealed_gang, 0, 32, 1)
            < closed_opponent_threat_discard_bias(&longer_concealed_gang, 0, 32, 1)
    );
}

#[test]
fn closed_opponent_threat_grows_after_concealed_gang() {
    let mut closed = table_with_discards(1, Vec::new());
    closed.wall_count = 16;
    closed.seats.get_mut(&1).unwrap().hand_count = 13;

    let mut concealed_gang = closed.clone();
    let seat = concealed_gang.seats.get_mut(&1).unwrap();
    seat.hand_count = 10;
    seat.melds = vec![test_concealed_gang_meld(9)];

    assert!(
        closed_opponent_threat_discard_bias(&concealed_gang, 0, 32, 1)
            < closed_opponent_threat_discard_bias(&closed, 0, 32, 1)
    );
}

#[test]
fn closed_opponent_threat_penalizes_cold_pair_more_than_singleton() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().hand_count = 13;

    assert!(
        closed_opponent_threat_discard_bias(&table, 0, 9, 2)
            < closed_opponent_threat_discard_bias(&table, 0, 19, 1)
    );
}

#[test]
fn closed_opponent_threat_starts_before_final_defense() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 37;
    table.seats.get_mut(&1).unwrap().hand_count = 13;
    let mid_round_bias = closed_opponent_threat_discard_bias(&table, 0, 32, 1);
    table.wall_count = 16;
    let late_defense_bias = closed_opponent_threat_discard_bias(&table, 0, 32, 1);

    assert!(mid_round_bias < 0.0);
    assert!(mid_round_bias > late_defense_bias);
}

#[test]
fn late_defense_can_follow_exposed_terminal_over_live_wind() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().hand_count = 13;
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_peng_meld(9)],
        },
    );
    let hand = vec![2, 4, 6, 8, 9, 12, 14, 16, 18, 22, 24, 26, 28, 31];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(9)
    );
}

#[test]
fn late_defense_avoids_breaking_cold_terminal_pair_against_closed_opponent() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().hand_count = 13;
    let hand = vec![2, 4, 6, 8, 9, 9, 12, 14, 16, 18, 19, 22, 24, 26];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(19)
    );
}

#[test]
fn mid_round_non_dealer_piao_single_wait_can_chase_wind_fan() {
    let mut table = table_with_discards(1, vec![31]);
    table.wall_count = 32;
    table.seats.get_mut(&0).unwrap().melds = vec![
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(21),
        test_peng_meld(35),
    ];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();

    assert_eq!(remaining_tile_count(&[31], &table, 0, 31), 2);
    assert_eq!(remaining_tile_count(&[5], &table, 0, 5), 3);
    assert!(
        piao_single_wait_tile_score(31, &[31], melds, &table, 0, WIN_RULE_SHENYANG_BASIC)
            > piao_single_wait_tile_score(5, &[5], melds, &table, 0, WIN_RULE_SHENYANG_BASIC)
    );
}

#[test]
fn late_defense_breaks_locked_five_pairs_for_only_public_tile() {
    let mut table = table_with_discards(1, vec![1]);
    table.wall_count = 20;
    let hand = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 14, 21, 21, 31, 35];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(1)
    );
}

#[test]
fn late_defense_preserves_locked_five_pairs_without_public_tile() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 20;
    let hand = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 14, 21, 21, 31, 35];

    assert!(!matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(1 | 2 | 11 | 12 | 21)
    ));
}

#[test]
fn late_defense_locked_five_pairs_follows_public_singleton_without_breaking_pairs() {
    let mut table = table_with_discards(1, vec![5]);
    table.wall_count = 20;
    let hand = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 14, 21, 21, 31, 35];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(5)
    );
}

#[test]
fn mid_round_discard_follows_public_honor_over_live_dragon() {
    let mut table = table_with_discards(1, vec![31]);
    table.wall_count = 46;
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 36];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(31)
    );
}

#[test]
fn mid_round_discard_follows_public_dragon_over_multiple_public_terminal() {
    let mut table = table_with_discards(1, vec![9, 9, 35]);
    table.wall_count = 46;
    let hand = vec![1, 2, 3, 9, 11, 12, 14, 16, 18, 21, 22, 24, 26, 35];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(35)
    );
}

#[test]
fn mid_round_discard_follows_public_middle_before_late_round() {
    let mut table = table_with_discards(1, vec![14]);
    table.wall_count = 55;
    let hand = vec![1, 2, 3, 9, 11, 12, 14, 16, 18, 21, 22, 24, 26, 35];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(14)
    );
}

#[test]
fn mid_round_live_dragon_risk_grows_when_opponents_are_open() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 42;
    let base = mid_round_live_honor_risk_bias(&table, 0, 35, 1);
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(9)];
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_peng_meld(16)],
        },
    );

    assert!(mid_round_live_honor_risk_bias(&table, 0, 35, 1) < base);
}

#[test]
fn mid_round_live_dragon_risk_ignores_concealed_gang_opponent() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 42;
    let base = mid_round_live_honor_risk_bias(&table, 0, 35, 1);

    table.seats.get_mut(&1).unwrap().melds = vec![test_concealed_gang_meld(9)];
    assert_eq!(mid_round_live_honor_risk_bias(&table, 0, 35, 1), base);

    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(9)];
    assert!(mid_round_live_honor_risk_bias(&table, 0, 35, 1) < base);
}

#[test]
fn mid_round_open_dragon_meld_does_not_add_live_dragon_pressure() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 42;
    let base = mid_round_live_honor_risk_bias(&table, 0, 35, 1);

    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(35)];
    assert_eq!(open_opponent_live_dragon_risk(&table, 0, 35), 0.0);
    assert_eq!(mid_round_live_honor_risk_bias(&table, 0, 35, 1), base);

    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(9)];
    assert!(open_opponent_live_dragon_risk(&table, 0, 35) > 0.0);
    assert!(mid_round_live_honor_risk_bias(&table, 0, 35, 1) < base);
}

#[test]
fn mid_round_live_dragon_risk_discounts_exposed_meld_tiles() {
    let mut exposed_table = table_with_discards(1, Vec::new());
    exposed_table.wall_count = 42;
    exposed_table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(9)];
    exposed_table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_peng_meld(35)],
        },
    );

    let mut live_table = exposed_table.clone();
    live_table.seats.get_mut(&2).unwrap().melds = vec![test_peng_meld(16)];

    assert!(live_risk_exposure_scale(&exposed_table, 35) < 1.0);
    assert!(
        open_opponent_live_dragon_risk(&exposed_table, 0, 35)
            < open_opponent_live_dragon_risk(&live_table, 0, 35)
    );
}

#[test]
fn mid_round_open_honor_meld_tile_is_safer_than_live_dragon() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 42;
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(35)];

    let exposed_dragon_safety = mid_round_open_meld_safety_bias(&table, 35);
    let live_dragon_safety = mid_round_open_meld_safety_bias(&table, 36);
    assert!(exposed_dragon_safety > 0.0);
    assert_eq!(live_dragon_safety, 0.0);

    let exposed_dragon_score =
        exposed_dragon_safety + mid_round_live_honor_risk_bias(&table, 0, 35, 1);
    let live_dragon_score = live_dragon_safety + mid_round_live_honor_risk_bias(&table, 0, 36, 1);
    assert!(exposed_dragon_score > live_dragon_score);
}

#[test]
fn mid_round_discard_avoids_live_dragon_against_open_opponent() {
    let mut table = table_with_discards(1, vec![14]);
    table.wall_count = 42;
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(9)];
    let hand = vec![1, 2, 3, 11, 12, 13, 14, 16, 18, 21, 22, 23, 31, 35];

    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(35)
    );
}

#[test]
fn mid_round_discard_follows_public_middle_over_live_terminal() {
    let mut table = table_with_discards(1, vec![14]);
    table.wall_count = 37;
    let hand = vec![1, 2, 3, 9, 11, 12, 14, 16, 18, 21, 22, 24, 26, 31];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(14)
    );
}

#[test]
fn mid_round_discard_follows_public_middle_over_cold_wind_against_closed_opponent() {
    let mut table = table_with_discards(1, vec![14]);
    table.wall_count = 37;
    table.seats.get_mut(&1).unwrap().hand_count = 13;
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 14, 21, 22, 23, 31];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(14)
    );
}

#[test]
fn mid_round_live_suited_risk_grows_when_opponents_are_open() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 37;
    let hand = vec![1, 2, 3, 9, 11, 12, 14, 16, 18, 21, 22, 24, 26, 31];
    let base = mid_round_live_suited_risk_bias(&hand, &[], &table, 0, 9, 1, WIN_RULE_RELAXED);
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(16)];

    assert!(mid_round_live_suited_risk_bias(&hand, &[], &table, 0, 9, 1, WIN_RULE_RELAXED) < base);
}

#[test]
fn mid_round_live_suited_risk_ignores_concealed_gang_opponent() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 37;
    let hand = vec![1, 2, 3, 9, 11, 12, 14, 16, 18, 21, 22, 24, 26, 31];
    let base = mid_round_live_suited_risk_bias(&hand, &[], &table, 0, 9, 1, WIN_RULE_RELAXED);

    table.seats.get_mut(&1).unwrap().melds = vec![test_concealed_gang_meld(16)];
    assert_eq!(
        mid_round_live_suited_risk_bias(&hand, &[], &table, 0, 9, 1, WIN_RULE_RELAXED),
        base
    );

    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(16)];
    assert!(mid_round_live_suited_risk_bias(&hand, &[], &table, 0, 9, 1, WIN_RULE_RELAXED) < base);
}

#[test]
fn mid_round_open_meld_tile_is_safer_than_live_suited_tile() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 37;
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(14)];
    let hand = vec![1, 2, 3, 9, 11, 12, 14, 16, 18, 21, 22, 24, 26, 28];

    assert!(mid_round_open_meld_safety_bias(&table, 14) > 0.0);
    assert_eq!(
        open_opponent_live_suited_risk(&table, 0, 14),
        0.0,
        "an opponent who already opened this tile should not add live-tile pressure for it"
    );
    assert!(
        mid_round_open_meld_safety_bias(&table, 14) > mid_round_open_meld_safety_bias(&table, 9)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(14)
    );
}

#[test]
fn mid_round_live_suited_risk_discounts_exposed_meld_tiles() {
    let mut exposed_table = table_with_discards(1, Vec::new());
    exposed_table.wall_count = 37;
    exposed_table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(16)];
    exposed_table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_peng_meld(9)],
        },
    );
    let hand = vec![1, 2, 3, 9, 11, 12, 14, 16, 18, 21, 22, 24, 26, 31];

    let mut live_table = exposed_table.clone();
    live_table.seats.get_mut(&2).unwrap().melds = vec![test_peng_meld(35)];

    assert!(live_risk_exposure_scale(&exposed_table, 9) < 1.0);
    assert!(
        mid_round_live_suited_risk_bias(&hand, &[], &exposed_table, 0, 9, 1, WIN_RULE_RELAXED)
            > mid_round_live_suited_risk_bias(&hand, &[], &live_table, 0, 9, 1, WIN_RULE_RELAXED)
    );
}

#[test]
fn mid_round_values_two_open_meld_tiles_over_live_dragon() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 37;
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
    let hand = vec![1, 2, 3, 6, 9, 11, 12, 14, 16, 18, 21, 22, 24, 35];

    assert_eq!(open_meld_tile_count(&table, 6), 2);
    assert!(
        mid_round_open_meld_safety_bias(&table, 6)
            + mid_round_live_honor_risk_bias(&table, 0, 6, 1)
            > mid_round_open_meld_safety_bias(&table, 35)
                + mid_round_live_honor_risk_bias(&table, 0, 35, 1)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(6)
    );
}

#[test]
fn open_meld_tile_count_ignores_malformed_meld_tiles() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&1).unwrap().melds = vec![
        WsShenyangMahjongMeld {
            kind: ShenyangMahjongMeldKind::PENG,
            tiles: vec![14, 14],
            from_position: Some(0),
        },
        test_peng_meld(14),
    ];

    assert_eq!(open_meld_tile_count(&table, 14), 3);
}

#[test]
fn open_opponent_exists_ignores_tile_from_its_open_meld() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 37;
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(14)];

    assert!(!open_opponent_exists_for_tile(&table, 0, 14));
    assert!(open_opponent_exists_for_tile(&table, 0, 15));
}

#[test]
fn own_open_live_suited_pressure_ignores_opponent_open_meld_tile() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 37;
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(14)];
    let melds = vec![test_peng_meld(1), test_peng_meld(11)];

    assert_eq!(own_open_live_suited_pressure(&melds, &table, 0, 14), 0.0);
    assert!(own_open_live_suited_pressure(&melds, &table, 0, 15) > 0.0);
}

#[test]
fn mid_round_discard_avoids_live_terminal_against_open_opponent() {
    let mut table = table_with_discards(1, vec![14]);
    table.wall_count = 37;
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(16)];
    let hand = vec![1, 2, 3, 9, 11, 12, 14, 16, 18, 21, 22, 24, 26, 31];

    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(9)
    );
}

#[test]
fn mid_round_open_hand_does_not_chase_wait_fan_with_live_terminal_discard() {
    let mut seats = HashMap::new();
    seats.insert(
        0,
        AiSeatView {
            position: 0,
            hand_count: 1,
            discards: vec![31, 33, 16, 1, 31, 21, 8, 12, 32, 3, 4, 2, 15],
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
            hand_count: 10,
            discards: vec![21, 4, 15, 35, 37, 11, 12, 16, 5, 33, 33, 35],
            melds: vec![test_peng_meld(19)],
        },
    );
    seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 13,
            discards: vec![34, 1, 22, 33, 12, 23, 5, 3, 28, 1],
            melds: Vec::new(),
        },
    );
    seats.insert(
        3,
        AiSeatView {
            position: 3,
            hand_count: 8,
            discards: vec![34, 32, 22, 8, 35, 16, 11, 12, 25, 17, 3],
            melds: vec![test_peng_meld(7), test_peng_meld(26)],
        },
    );
    let table = AiPublicTable {
        current_position: 3,
        dealer_position: 0,
        wall_count: 37,
        max_fan: Some(4),
        claim_window: None,
        seats,
    };
    let hand = vec![9, 13, 14, 15, 24, 24, 28, 29];

    assert_ne!(
        choose_discard_from_view(&hand, &table, 3, WIN_RULE_SHENYANG_BASIC),
        Some(9)
    );
}

#[test]
fn late_defense_avoids_cold_honor_against_closed_opponent() {
    let mut table = table_with_discards(1, vec![11, 14, 19]);
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().hand_count = 13;
    let hand = vec![2, 3, 4, 5, 6, 7, 8, 12, 13, 15, 16, 17, 18, 31];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(12)
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
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(5)
    );
}

#[test]
fn late_defense_prefers_opponent_missing_suit_tile() {
    let mut table = table_with_discards(1, vec![11, 14, 19]);
    table.wall_count = 16;
    let hand = vec![2, 3, 4, 5, 6, 7, 8, 12, 13, 15, 16, 17, 18, 22];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(12)
    );
}

#[test]
fn late_defense_missing_suit_read_can_beat_live_wind() {
    let mut table = table_with_discards(1, vec![11, 14, 19]);
    table.wall_count = 16;
    let hand = vec![2, 3, 4, 5, 6, 7, 8, 12, 13, 15, 16, 17, 18, 31];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(12)
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
fn late_defense_bias_keeps_public_honor_above_four_public_middle_tiles() {
    let mut table = table_with_discards(1, vec![5, 5, 5, 5, 31]);
    table.wall_count = 16;

    assert!(late_defense_discard_bias(&table, 0, 31) > late_defense_discard_bias(&table, 0, 5));
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
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(8)
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
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(6)
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
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(32)
    );
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
fn late_defense_discards_live_middle_before_breaking_terminal_pair() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    let hand = vec![2, 4, 5, 6, 8, 9, 9, 12, 14, 16, 18, 22, 24, 26];

    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(9)
    );
}

#[test]
fn late_discard_follows_safe_tile_over_hand_efficiency() {
    let mut table = table_with_discards(1, vec![5]);
    table.wall_count = 16;
    let hand = vec![3, 4, 5, 5, 6, 7, 11, 12, 13, 21, 22, 23, 31, 35];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(5)
    );
}

#[test]
fn mid_round_discard_follows_multiple_public_terminal_over_live_wind() {
    let mut table = table_with_discards(1, vec![9, 9]);
    table.wall_count = 36;
    let hand = vec![1, 2, 4, 6, 8, 9, 11, 12, 14, 16, 21, 23, 25, 31];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
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
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
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

#[test]
fn mid_broken_basic_discard_follows_public_tile_before_hand_shape() {
    let mut table = table_with_discards(1, vec![5]);
    table.wall_count = 40;
    let hand = vec![2, 5, 8, 12, 14, 17, 22, 24, 27, 31, 32, 33, 35, 37];

    assert!(should_use_broken_hand_public_defense_discard(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(5)
    );
}

#[test]
fn mid_broken_basic_discard_follows_open_meld_tile_without_public_discards() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 40;
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(14)];
    let hand = vec![2, 5, 8, 12, 14, 17, 22, 24, 27, 31, 32, 33, 35, 37];

    assert_eq!(public_discard_count(&table, 14), 0);
    assert!(mid_round_open_meld_safety_bias(&table, 14) > 0.0);
    assert!(should_use_broken_hand_public_defense_discard(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(14)
    );
}

#[test]
fn mid_broken_discard_uses_opponent_missing_suit_read_without_public_tile() {
    let mut table = table_with_discards(1, vec![11, 13, 15, 18]);
    table.wall_count = 40;
    let hand = vec![2, 5, 8, 12, 14, 17, 22, 24, 27, 31, 32, 33, 35, 37];

    assert_eq!(public_discard_count(&table, 12), 0);
    assert_eq!(mid_round_open_meld_safety_bias(&table, 12), 0.0);
    assert!(mid_broken_opponent_missing_suit_safety_bias(&table, 0, 12) > 0.0);
    assert!(should_use_broken_hand_public_defense_discard(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_RELAXED
    ));
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(12)
    );
}

#[test]
fn mid_broken_relaxed_discard_follows_public_tile_before_hand_shape() {
    let mut table = table_with_discards(1, vec![5]);
    table.wall_count = 40;
    let hand = vec![2, 5, 8, 12, 14, 17, 22, 24, 27, 31, 32, 33, 35, 37];

    assert!(should_use_broken_hand_public_defense_discard(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_RELAXED
    ));
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(5)
    );
}

#[test]
fn mid_broken_public_defense_preserves_dragon_pair_over_public_singleton() {
    let mut table = table_with_discards(1, vec![5, 35]);
    table.wall_count = 40;
    let hand = vec![2, 5, 8, 12, 14, 17, 22, 24, 27, 31, 32, 33, 35, 35];

    assert!(should_use_broken_hand_public_defense_discard(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_RELAXED
    ));
    assert!(
        public_defense_tile_safety_score(&table, 0, 5, 1)
            > public_defense_tile_safety_score(&table, 0, 35, 2)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(5)
    );
}

#[test]
fn mid_broken_basic_public_defense_preserves_only_recoverable_heng_seed() {
    let hand = vec![1, 2, 5, 8, 11, 12, 14, 17, 21, 22, 24, 27, 29, 35];
    let mut discards = dead_basic_heng_discards(&hand);
    if let Some(index) = discards.iter().position(|tile| *tile == 35) {
        discards.remove(index);
    }
    let mut table = table_with_discards(1, discards);
    table.wall_count = 40;

    assert!(can_recover_basic_heng(&hand, &[], &table));
    let after_dragon = remove_n_tiles(&hand, 35, 1);
    assert!(!can_recover_basic_heng_after_discard(
        &after_dragon,
        &[],
        &table,
        35
    ));
    assert!(should_use_broken_hand_public_defense_discard(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(35)
    );
}

#[test]
fn mid_broken_public_defense_preserves_triplet_over_public_pair() {
    let mut table = table_with_discards(1, vec![5, 7]);
    table.wall_count = 40;
    let hand = vec![2, 5, 5, 5, 7, 7, 12, 14, 17, 22, 24, 27, 31, 33];

    assert!(
        public_defense_tile_safety_score(&table, 0, 7, 2)
            > public_defense_tile_safety_score(&table, 0, 5, 3)
    );
    assert_eq!(
        choose_public_defense_discard_from_candidates(
            &hand,
            &[],
            &table,
            0,
            WIN_RULE_RELAXED,
            vec![5, 7]
        ),
        Some(7)
    );
}

#[test]
fn dealer_mid_unrecoverable_basic_hand_uses_public_defense_discard() {
    let mut discards = dead_terminal_or_honor_discards();
    discards.push(5);
    let mut table = table_with_discards(1, discards);
    table.dealer_position = 0;
    table.wall_count = 52;
    let hand = vec![2, 3, 4, 5, 5, 6, 7, 12, 13, 14, 22, 23, 24, 25];

    assert_eq!(
        unrecoverable_basic_rule_requirement_count(&hand, &[], &table),
        1
    );
    assert!(should_use_broken_hand_public_defense_discard(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(5)
    );
}

#[test]
fn mid_broken_public_defense_preserves_locked_seven_pairs_route() {
    let mut table = table_with_discards(1, vec![11]);
    table.wall_count = 40;
    let hand = vec![1, 1, 2, 2, 3, 3, 4, 4, 11, 11, 12, 12, 21, 31];

    assert!(!should_use_broken_hand_public_defense_discard(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(11)
    );
}

#[test]
fn mid_round_non_dealer_can_choose_single_wait_for_extra_fan() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 30;
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
    let hand = vec![2, 2, 4, 5, 7, 11, 12, 13, 21, 22, 23];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(4)
    );
}

#[test]
fn late_defense_non_dealer_prefers_wider_wait_over_single_wait_fan() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 20;
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
    let hand = vec![2, 2, 4, 5, 7, 11, 12, 13, 21, 22, 23];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(7)
    );
}

#[test]
fn opponent_threat_starts_after_three_triplet_melds() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];

    assert!(opponent_threat_discard_bias(&table, 0, 5, 2) < -9.0);

    table.seats.get_mut(&1).unwrap().melds.pop();
    assert_eq!(opponent_threat_discard_bias(&table, 0, 5, 2), 0.0);
}

#[test]
fn pure_one_suit_threat_penalizes_live_main_suit_tile() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 32;
    table.seats.get_mut(&1).unwrap().melds = vec![test_chi_meld(11), test_peng_meld(14)];

    assert_eq!(
        pure_one_suit_threat_suit(table.seats.get(&1).unwrap()),
        Some((1, 2))
    );
    assert!(
        pure_one_suit_threat_discard_bias(&table, 0, 18, 1)
            < pure_one_suit_threat_discard_bias(&table, 0, 22, 1)
    );
    assert_eq!(
        pure_one_suit_threat_discard_bias(&table, 0, 14, 1),
        0.0,
        "a tile already exposed by that opponent should not be treated as live"
    );
}

#[test]
fn pure_one_suit_threat_uses_opponent_discards_as_route_evidence() {
    let mut base_table = table_with_discards(1, Vec::new());
    base_table.wall_count = 32;
    base_table.seats.get_mut(&1).unwrap().melds = vec![test_chi_meld(11), test_peng_meld(14)];

    let mut same_suit_discards = base_table.clone();
    same_suit_discards.seats.get_mut(&1).unwrap().discards = vec![15, 16];

    let mut off_suit_discards = base_table.clone();
    off_suit_discards.seats.get_mut(&1).unwrap().discards = vec![2, 22, 31, 35];

    let base_bias = pure_one_suit_threat_discard_bias(&base_table, 0, 18, 1);
    let same_suit_bias = pure_one_suit_threat_discard_bias(&same_suit_discards, 0, 18, 1);
    let off_suit_bias = pure_one_suit_threat_discard_bias(&off_suit_discards, 0, 18, 1);

    assert!(
        same_suit_bias > base_bias,
        "discarding the same suit should make the pure-one-suit route less credible"
    );
    assert!(
        off_suit_bias < base_bias,
        "clearing other suits should make the pure-one-suit route more credible"
    );
}

#[test]
fn pure_one_suit_threat_reads_single_meld_with_strong_off_suit_discards() {
    let mut table = table_with_discards(1, vec![2, 22, 31, 35]);
    table.wall_count = 32;
    table.seats.get_mut(&1).unwrap().melds = vec![test_chi_meld(11)];

    assert_eq!(
        pure_one_suit_threat_suit(table.seats.get(&1).unwrap()),
        Some((1, 1))
    );
    assert!(
        pure_one_suit_threat_discard_bias(&table, 0, 18, 1)
            < pure_one_suit_threat_discard_bias(&table, 0, 24, 1)
    );
}

#[test]
fn pure_one_suit_threat_reads_closed_discard_pattern() {
    let mut table = table_with_discards(1, vec![1, 2, 11, 12, 31]);
    table.wall_count = 32;

    assert_eq!(
        pure_one_suit_threat_suit(table.seats.get(&1).unwrap()),
        Some((2, 0))
    );
    assert!(
        pure_one_suit_threat_discard_bias(&table, 0, 24, 1)
            < pure_one_suit_threat_discard_bias(&table, 0, 14, 1)
    );
}

#[test]
fn pure_one_suit_threat_ignores_weak_single_meld_evidence() {
    let mut weak_table = table_with_discards(1, vec![2, 22, 31]);
    weak_table.wall_count = 32;
    weak_table.seats.get_mut(&1).unwrap().melds = vec![test_chi_meld(11)];

    assert_eq!(
        pure_one_suit_threat_suit(weak_table.seats.get(&1).unwrap()),
        None
    );

    let mut same_suit_table = table_with_discards(1, vec![2, 22, 31, 35, 15]);
    same_suit_table.wall_count = 32;
    same_suit_table.seats.get_mut(&1).unwrap().melds = vec![test_chi_meld(11)];

    assert_eq!(
        pure_one_suit_threat_suit(same_suit_table.seats.get(&1).unwrap()),
        None
    );
}

#[test]
fn pure_one_suit_threat_ignores_ambiguous_closed_discards() {
    let only_honors = table_with_discards(1, vec![31, 32, 33, 35, 36]);
    let one_suit_only = table_with_discards(1, vec![1, 2, 3, 4, 5]);

    assert_eq!(
        pure_one_suit_threat_suit(only_honors.seats.get(&1).unwrap()),
        None
    );
    assert_eq!(
        pure_one_suit_threat_suit(one_suit_only.seats.get(&1).unwrap()),
        None
    );
}

#[test]
fn mid_round_discard_avoids_pure_one_suit_threat_suit() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 32;
    table.seats.get_mut(&1).unwrap().melds = vec![test_chi_meld(11), test_peng_meld(14)];
    let hand = vec![2, 3, 4, 6, 7, 8, 12, 16, 18, 22, 24, 26, 31, 35];

    assert!(
        pure_one_suit_threat_discard_bias(&table, 0, 18, 1)
            < pure_one_suit_threat_discard_bias(&table, 0, 22, 1)
    );
    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(18)
    );
}

#[test]
fn late_defense_can_follow_exposed_middle_against_piao_threat() {
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
    let hand = vec![2, 3, 4, 5, 6, 7, 8, 12, 14, 16, 18, 22, 24, 26];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(6)
    );
}

#[test]
fn late_defense_avoids_breaking_wind_pair_against_piao_threat() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];
    let hand = vec![2, 4, 6, 8, 9, 12, 14, 16, 18, 22, 24, 26, 31, 31];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(9)
    );
}

#[test]
fn late_defense_avoids_piao_threat_missing_suit_wait_tiles() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(11), test_peng_meld(21), test_peng_meld(31)];
    let hand = vec![2, 3, 5, 6, 8, 12, 13, 15, 16, 18, 22, 23, 25, 28];

    assert!(!matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(2 | 3 | 5 | 6 | 8)
    ));
}

#[test]
fn opponent_threat_counts_ai_controlled_table_seat() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];

    assert!(opponent_threat_discard_bias(&table, 0, 5, 1) < 0.0);
}

#[test]
fn opponent_missing_suit_read_counts_ai_controlled_table_seat() {
    let mut table = table_with_discards(1, vec![11, 12, 13]);
    table.wall_count = 16;

    assert!(opponent_missing_suit_safety_bias(&table, 0, 14) > 0.0);
}
