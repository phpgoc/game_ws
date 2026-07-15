use super::*;

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
fn half_capped_non_dealer_prefers_wider_wait_over_single_wait_fan() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 30;
    table.max_fan = Some(4);
    table.seats.get_mut(&0).unwrap().melds = vec![test_gang_meld(35)];
    let hand = vec![2, 2, 4, 5, 7, 11, 12, 13, 21, 22, 23];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(7)
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
fn dealer_prefers_more_live_copies_over_two_depleted_wait_kinds() {
    let mut table = table_with_discards(1, vec![24, 24, 24, 27]);
    table.dealer_position = 0;
    table.wall_count = 30;
    let hand = vec![2, 3, 4, 19, 19, 19, 21, 22, 23, 25, 26, 27, 27, 29];

    let after_19 = remove_n_tiles(&hand, 19, 1);
    let after_27 = remove_n_tiles(&hand, 27, 1);
    let after_29 = remove_n_tiles(&hand, 29, 1);
    assert_eq!(
        ready_live_tile_count_after_discard(&after_19, &[], &table, 0, WIN_RULE_RELAXED, 19,),
        4
    );
    assert_eq!(
        ready_live_tile_count_after_discard(&after_27, &[], &table, 0, WIN_RULE_RELAXED, 27,),
        3
    );
    assert_eq!(
        ready_live_tile_count_after_discard(&after_29, &[], &table, 0, WIN_RULE_RELAXED, 29,),
        2
    );

    let discard = choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED);

    assert_eq!(discard, Some(19));
}

#[test]
fn capped_non_dealer_prefers_more_live_copies_over_depleted_wait_kinds() {
    let mut table = table_with_discards(1, vec![24, 24, 24, 27]);
    table.max_fan = Some(1);
    table.wall_count = 30;
    let hand = vec![2, 3, 4, 19, 19, 19, 21, 22, 23, 25, 26, 27, 27, 29];

    assert_ne!(table.dealer_position, 0);
    assert!(ready_visible_fan_reaches_cap(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_RELAXED,
    ));
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(19)
    );
}

#[test]
fn late_non_dealer_prefers_public_discard_that_keeps_ready() {
    let mut table = table_with_discards(1, vec![24, 24, 24, 27]);
    table.wall_count = FINAL_DEFENSE_WALL_COUNT;
    let hand = vec![2, 3, 4, 19, 19, 19, 21, 22, 23, 25, 26, 27, 27, 29];

    assert_ne!(table.dealer_position, 0);
    assert_eq!(table.max_fan, None);
    assert_eq!(public_discard_count(&table, 27), 1);
    let after_27 = remove_n_tiles(&hand, 27, 1);
    assert_eq!(
        ready_live_tile_count_after_discard(&after_27, &[], &table, 0, WIN_RULE_RELAXED, 27,),
        3
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(27)
    );
}

#[test]
fn late_ready_equal_live_waits_prefer_public_honor_over_repeated_suited_discard() {
    let mut table = table_with_discards(1, vec![1, 1, 1, 31, 3, 3, 6, 6]);
    table.wall_count = FINAL_DEFENSE_WALL_COUNT;
    table.seats.get_mut(&0).unwrap().discards = vec![5, 5, 5];
    let hand = vec![1, 2, 3, 4, 5, 11, 12, 13, 21, 22, 23, 31, 31, 31];
    let after_five = remove_n_tiles(&hand, 5, 1);
    let after_honor = remove_n_tiles(&hand, 31, 1);

    assert_eq!(
        ready_live_tile_count_after_discard(&after_five, &[], &table, 0, WIN_RULE_RELAXED, 5,),
        3
    );
    assert_eq!(
        ready_live_tile_count_after_discard(&after_honor, &[], &table, 0, WIN_RULE_RELAXED, 31,),
        3
    );
    assert_eq!(
        choose_late_ready_discard(&hand, &[], &table, 0, WIN_RULE_RELAXED),
        Some(31)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(31)
    );
}
