use super::*;

#[test]
fn declared_ting_opponent_penalizes_live_tiles_by_safety_order() {
    let mut table = table_with_discards(1, Vec::new());

    assert_eq!(ting_opponent_threat_discard_bias(&table, 0, 5, 1), 0.0);

    table.ting_positions.insert(1);

    let honor = ting_opponent_threat_discard_bias(&table, 0, 31, 1);
    let terminal = ting_opponent_threat_discard_bias(&table, 0, 1, 1);
    let middle = ting_opponent_threat_discard_bias(&table, 0, 5, 1);
    assert!(middle < terminal);
    assert!(terminal < honor);
}

#[test]
fn declared_ting_threat_keeps_publicly_discarded_and_accounted_tiles_safe() {
    let mut table = table_with_discards(1, vec![5]);
    table.ting_positions.insert(1);

    assert_eq!(ting_opponent_threat_discard_bias(&table, 0, 5, 1), 0.0);

    table.seats.get_mut(&1).unwrap().discards.clear();
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(5)];

    assert_eq!(ting_opponent_threat_discard_bias(&table, 0, 5, 1), 0.0);
}
