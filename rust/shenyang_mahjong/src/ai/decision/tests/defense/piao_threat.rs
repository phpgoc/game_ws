use super::*;

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
