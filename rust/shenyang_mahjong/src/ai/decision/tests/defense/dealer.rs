use super::*;

#[test]
fn dealer_identity_increases_existing_opponent_threats() {
    let mut piao_table = table_with_discards(1, Vec::new());
    piao_table.wall_count = 32;
    piao_table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];
    let piao_dealer_bias = opponent_threat_discard_bias(&piao_table, 0, 35, 1);
    piao_table.dealer_position = 3;
    let piao_non_dealer_bias = opponent_threat_discard_bias(&piao_table, 0, 35, 1);
    assert!(piao_dealer_bias < piao_non_dealer_bias);

    let mut pure_table = table_with_discards(1, Vec::new());
    pure_table.wall_count = 32;
    pure_table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(1), test_peng_meld(2)];
    let pure_dealer_bias = pure_one_suit_threat_discard_bias(&pure_table, 0, 5, 1);
    pure_table.dealer_position = 3;
    let pure_non_dealer_bias = pure_one_suit_threat_discard_bias(&pure_table, 0, 5, 1);
    assert!(pure_dealer_bias < pure_non_dealer_bias);

    let mut closed_table = table_with_discards(1, vec![11, 14, 19, 31, 35, 36]);
    closed_table.wall_count = FINAL_DEFENSE_WALL_COUNT;
    closed_table.seats.get_mut(&1).unwrap().hand_count = 13;
    let closed_dealer_bias = closed_opponent_threat_discard_bias(&closed_table, 0, 5, 1);
    closed_table.dealer_position = 3;
    let closed_non_dealer_bias = closed_opponent_threat_discard_bias(&closed_table, 0, 5, 1);
    assert!(closed_dealer_bias < closed_non_dealer_bias);
}

#[test]
fn major_dealer_threat_disables_single_wait_fan_bias() {
    let mut table = table_with_discards(1, Vec::new());
    let melds = vec![test_peng_meld(31)];
    let win_hand = vec![2, 2, 5, 6, 7, 11, 12, 13, 21, 22, 23];

    let baseline = fan_wait_bias(&win_hand, &melds, &table, 0, 6, 4, &[]);
    assert!(baseline > 0.0);

    let dealer = table.seats.get_mut(&1).unwrap();
    dealer.hand_count = 4;
    dealer.melds = vec![test_peng_meld(3), test_peng_meld(14), test_peng_meld(25)];
    assert!(dealer_opponent_has_major_threat(&table, 0));
    assert_eq!(fan_wait_bias(&win_hand, &melds, &table, 0, 6, 4, &[],), 0.0);

    table.dealer_position = 3;
    assert!(!dealer_opponent_has_major_threat(&table, 0));
    assert_eq!(
        fan_wait_bias(&win_hand, &melds, &table, 0, 6, 4, &[],),
        baseline
    );
}

#[test]
fn seven_pairs_prefers_live_middle_wait_against_major_dealer_threat() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&1).unwrap().hand_count = 7;
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(23), test_peng_meld(24)];
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_chi_meld(1)],
        },
    );
    let hand = vec![1, 2, 2, 3, 3, 11, 11, 12, 12, 15, 16, 16, 26, 26];
    let terminal_wait = remove_n_tiles(&hand, 15, 1);
    let middle_wait = remove_n_tiles(&hand, 1, 1);

    table.dealer_position = 2;
    assert!(!dealer_opponent_has_major_threat(&table, 0));
    assert!(
        seven_pairs_wait_tile_score(1, &terminal_wait, &table, 0)
            > seven_pairs_wait_tile_score(15, &middle_wait, &table, 0)
    );
    assert!(
        seven_pairs_wait_discard_bias(&hand, 15, &[], &table, 0)
            > seven_pairs_wait_discard_bias(&hand, 1, &[], &table, 0)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(15)
    );

    table.dealer_position = 1;
    assert!(dealer_opponent_has_major_threat(&table, 0));
    assert!(
        seven_pairs_wait_tile_score(15, &middle_wait, &table, 0)
            > seven_pairs_wait_tile_score(1, &terminal_wait, &table, 0)
    );
    assert!(
        seven_pairs_wait_discard_bias(&hand, 1, &[], &table, 0)
            > seven_pairs_wait_discard_bias(&hand, 15, &[], &table, 0)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(1)
    );
}
