use super::*;

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
