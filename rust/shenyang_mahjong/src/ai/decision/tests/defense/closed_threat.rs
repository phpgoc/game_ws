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
fn closed_opponent_threat_discounts_partially_exposed_meld_tiles() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().hand_count = 13;
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_chi_meld(7)],
        },
    );

    let exposed_terminal_bias = closed_opponent_threat_discard_bias(&table, 0, 9, 1);
    let cold_honor_bias = closed_opponent_threat_discard_bias(&table, 0, 31, 1);

    assert!(exposed_terminal_bias < 0.0);
    assert!(exposed_terminal_bias > cold_honor_bias);
}

#[test]
fn closed_opponent_threat_ignores_tile_fully_accounted_by_exposed_and_own_tiles() {
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

    assert!(closed_threat_tile_fully_accounted(&table, 9, 1));
    assert_eq!(closed_opponent_threat_discard_bias(&table, 0, 9, 1), 0.0);
    assert!(closed_opponent_threat_discard_bias(&table, 0, 31, 1) < 0.0);
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
