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
