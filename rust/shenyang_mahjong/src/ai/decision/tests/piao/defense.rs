use super::*;

#[test]
fn discard_avoids_live_pair_against_piao_threat() {
    let mut table = table_with_discards(1, vec![31]);
    table.wall_count = 32;
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];
    let hand = vec![2, 3, 4, 5, 5, 6, 7, 11, 12, 13, 21, 22, 23, 31];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(31)
    );
}

#[test]
fn discard_follows_public_tile_over_live_pair_against_piao_threat() {
    let mut table = table_with_discards(1, vec![14]);
    table.wall_count = 32;
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];
    let hand = vec![2, 3, 4, 5, 5, 6, 7, 11, 12, 13, 14, 21, 22, 23];

    assert!(
        opponent_threat_discard_bias(&table, 0, 5, 2)
            < opponent_threat_discard_bias(&table, 0, 14, 1)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(14)
    );
}

#[test]
fn seven_pairs_wait_discard_avoids_piao_missing_suit_threat_tile() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 32;
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(13), test_peng_meld(23), test_peng_meld(35)];
    let hand = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 21, 21, 22, 22, 31];
    let wind_wait = remove_n_tiles(&hand, 5, 1);
    let middle_wait = remove_n_tiles(&hand, 31, 1);

    assert_eq!(pair_count(&hand), 6);
    assert!(
        seven_pairs_wait_tile_score(31, &wind_wait, &table, 0)
            > seven_pairs_wait_tile_score(5, &middle_wait, &table, 0)
    );
    assert!(
        opponent_threat_discard_bias(&table, 0, 5, 1)
            < opponent_threat_discard_bias(&table, 0, 31, 1)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(31)
    );
}
