use super::*;

#[test]
fn fan_wait_bias_stops_when_visible_fan_exceeds_half_cap() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 30;
    table.score_cap = Some(15);
    let melds = vec![test_gang_meld(35)];
    let win_hand = vec![2, 2, 5, 6, 7, 11, 12, 13, 21, 22, 23];

    let visible_fan = estimated_visible_fan_without_wait(&win_hand, &melds);
    assert_eq!(visible_fan, 3);
    assert!(shenyang_fan_score_exceeds_half_cap(
        visible_fan,
        table.score_cap.unwrap()
    ));

    assert_eq!(
        fan_wait_bias(&win_hand, &melds, &table, 0, 6, 4, &[4],),
        0.0
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
        score_cap: Some(16),
        allow_first_chi: true,
        ting_positions: Default::default(),
        claim_is_rob_gang: false,
        claim_window: None,
        seats,
    };
    let hand = vec![9, 13, 14, 15, 24, 24, 28, 29];

    assert_ne!(choose_discard_from_view(&hand, &table, 3), Some(9));
}
