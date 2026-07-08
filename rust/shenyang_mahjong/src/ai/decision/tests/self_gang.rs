use super::*;

#[test]
fn dealer_self_gang_delays_closed_plain_gang_before_opening_basic_hand() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    let hand = vec![3, 3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31];

    assert_eq!(
        choose_self_gang_from_view(&hand, &[3], &table, 0, WIN_RULE_SHENYANG_BASIC),
        None
    );
}

#[test]
fn self_gang_allows_dragon_gang_after_opening_basic_hand() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_chi_meld(1)];
    let hand = vec![11, 12, 13, 21, 22, 23, 31, 35, 35, 35, 35];

    assert_eq!(
        piao_plan_score_for_context(
            &hand,
            table.seats.get(&0).unwrap().melds.as_slice(),
            &table,
            0
        ),
        0.0
    );
    assert_eq!(
        choose_self_gang_from_view(&hand, &[35], &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(35)
    );
}

#[test]
fn self_gang_delays_open_piao_dragon_gang_until_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    let hand = vec![2, 5, 8, 11, 14, 17, 21, 35, 35, 35, 35];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();

    assert!(piao_plan_score_for_context(&hand, melds, &table, 0) >= 22.0);
    assert_eq!(
        best_ready_score_after_discard(&hand, melds, &table, 0, WIN_RULE_SHENYANG_BASIC),
        0.0
    );
    assert_eq!(
        choose_self_gang_from_view(&hand, &[35], &table, 0, WIN_RULE_SHENYANG_BASIC),
        None
    );
}

#[test]
fn self_gang_allows_added_dragon_after_opening_before_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(35)];
    let hand = vec![2, 5, 8, 11, 14, 17, 21, 31, 32, 33, 35];

    assert_eq!(
        ready_tile_score(
            &hand,
            table.seats.get(&0).unwrap().melds.as_slice(),
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC
        ),
        0.0
    );
    assert_eq!(
        choose_self_gang_from_view(&hand, &[35], &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(35)
    );
}

#[test]
fn one_fan_capped_self_gang_delays_dragon_before_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(1);
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    let hand = vec![2, 5, 8, 11, 14, 17, 21, 35, 35, 35, 35];

    assert_eq!(
        choose_self_gang_from_view(&hand, &[35], &table, 0, WIN_RULE_SHENYANG_BASIC),
        None
    );
}

#[test]
fn one_fan_capped_self_gang_delays_added_dragon_before_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(1);
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(35)];
    let hand = vec![2, 5, 8, 11, 14, 17, 21, 31, 32, 33, 35];

    assert_eq!(
        choose_self_gang_from_view(&hand, &[35], &table, 0, WIN_RULE_SHENYANG_BASIC),
        None
    );
}

#[test]
fn one_fan_capped_self_gang_delays_added_plain_before_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(1);
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(9)];
    let hand = vec![1, 2, 4, 6, 8, 9, 11, 13, 16, 21, 24];

    assert_eq!(
        choose_self_gang_from_view(&hand, &[9], &table, 0, WIN_RULE_SHENYANG_BASIC),
        None
    );
}

#[test]
fn self_gang_allows_open_plain_gang_when_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
    let hand = vec![1, 2, 3, 9, 9, 9, 9, 11, 12, 13, 21];

    assert_eq!(
        choose_self_gang_from_view(&hand, &[9], &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(9)
    );
}

#[test]
fn self_gang_allows_final_ready_hand_when_gang_keeps_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
    let hand = vec![1, 2, 3, 9, 9, 9, 9, 11, 12, 13, 21];

    assert!(
        best_ready_score_after_discard(
            &hand,
            table.seats.get(&0).unwrap().melds.as_slice(),
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC
        ) > 0.0
    );
    assert_eq!(
        choose_self_gang_from_view(&hand, &[9], &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(9)
    );
}

#[test]
fn self_gang_allows_ready_main_suit_added_gang_for_pure_one_suit_plan() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 9];

    assert_eq!(
        choose_self_gang_from_view(&hand, &[1], &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(1)
    );
}

#[test]
fn self_gang_delays_main_suit_added_gang_when_pure_one_suit_plan_not_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    let hand = vec![1, 2, 4, 5, 7, 8, 9, 11, 12, 21, 31];

    assert_eq!(
        choose_self_gang_from_view(&hand, &[1], &table, 0, WIN_RULE_SHENYANG_BASIC),
        None
    );
}

#[test]
fn self_gang_delays_closed_dragon_gang_before_opening_basic_hand() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 35, 35, 35, 35, 31];

    assert_eq!(
        choose_self_gang_from_view(&hand, &[35], &table, 0, WIN_RULE_SHENYANG_BASIC),
        None
    );
}

#[test]
fn self_gang_delays_closed_plain_gang_before_opening_basic_hand() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![3, 3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31];

    assert_eq!(
        choose_self_gang_from_view(&hand, &[3], &table, 0, WIN_RULE_SHENYANG_BASIC),
        None
    );
}

#[test]
fn self_gang_delays_closed_pure_one_suit_gang_before_opening_basic_hand() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 1, 1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 9];

    assert_eq!(
        choose_self_gang_from_view(&hand, &[1], &table, 0, WIN_RULE_SHENYANG_BASIC),
        None
    );
}

#[test]
fn self_gang_skips_ready_pure_one_suit_when_visible_fan_capped() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(4);
    let hand = vec![1, 1, 1, 1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 9];

    assert!(ready_visible_fan_reaches_cap(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_eq!(
        choose_self_gang_from_view(&hand, &[1], &table, 0, WIN_RULE_SHENYANG_BASIC),
        None
    );
}

#[test]
fn self_gang_allows_same_closed_plain_gang_when_opening_is_not_required() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![3, 3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31];

    assert_eq!(
        choose_self_gang_from_view(&hand, &[3], &table, 0, WIN_RULE_RELAXED),
        Some(3)
    );
}

#[test]
fn one_fan_capped_self_gang_delays_closed_plain_before_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(1);
    let hand = vec![3, 3, 3, 3, 4, 6, 8, 11, 13, 15, 21, 24, 27, 31];

    assert_eq!(
        choose_self_gang_from_view(&hand, &[3], &table, 0, WIN_RULE_RELAXED),
        None
    );
}

#[test]
fn self_gang_skips_plain_gang_when_concealed_dragon_triplet_caps_ready_hand() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(2);
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(11)];
    let hand = vec![9, 9, 9, 9, 22, 23, 31, 31, 35, 35, 35];

    assert!(ready_visible_fan_reaches_cap(
        &remove_n_tiles(&hand, 9, 1),
        table.seats.get(&0).unwrap().melds.as_slice(),
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_eq!(
        choose_self_gang_from_view(&hand, &[9], &table, 0, WIN_RULE_SHENYANG_BASIC),
        None
    );
}

#[test]
fn self_gang_delays_open_piao_plain_gang_until_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
    let hand = vec![1, 2, 4, 5, 7, 9, 9, 9, 9, 11, 21];

    assert_eq!(
        choose_self_gang_from_view(&hand, &[9], &table, 0, WIN_RULE_SHENYANG_BASIC),
        None
    );
}

#[test]
fn self_gang_delays_open_piao_added_plain_gang_until_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(9)];
    let hand = vec![1, 2, 4, 5, 7, 9, 11, 11, 21, 21, 31];

    assert_eq!(
        choose_self_gang_from_view(&hand, &[9], &table, 0, WIN_RULE_SHENYANG_BASIC),
        None
    );
}

#[test]
fn self_gang_delays_relaxed_piao_plain_gang_until_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
    let hand = vec![1, 2, 4, 5, 7, 9, 9, 9, 9, 11, 21];

    assert_eq!(
        choose_self_gang_from_view(&hand, &[9], &table, 0, WIN_RULE_RELAXED),
        None
    );
}

#[test]
fn self_gang_prefers_dragon_gang_over_plain_gang() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 3, 3, 3, 3, 11, 12, 21, 22, 35, 35, 35, 35];

    assert_eq!(
        choose_self_gang_from_view(&hand, &[3, 35], &table, 0, WIN_RULE_RELAXED),
        Some(35)
    );
}

#[test]
fn self_gang_ignores_invalid_candidate_tile() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![3, 3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 35];

    assert_eq!(
        choose_self_gang_from_view(&hand, &[3, 35], &table, 0, WIN_RULE_RELAXED),
        Some(3)
    );
    assert_eq!(
        choose_self_gang_from_view(&hand, &[35], &table, 0, WIN_RULE_RELAXED),
        None
    );
}

#[test]
fn self_gang_passes_final_unready_hand_for_defense() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    let hand = vec![1, 2, 3, 3, 3, 3, 11, 12, 21, 22, 35, 35, 35, 35];

    assert_eq!(
        best_ready_score_after_discard(&hand, &[], &table, 0, WIN_RULE_RELAXED),
        0.0
    );
    assert_eq!(
        choose_self_gang_from_view(&hand, &[3, 35], &table, 0, WIN_RULE_RELAXED),
        None
    );
}

#[test]
fn self_gang_preserves_basic_four_pairs_missing_suit_seven_pairs_plan() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 1, 1, 2, 2, 3, 3, 11, 11, 12, 31, 35, 36];

    assert_eq!(
        choose_self_gang_from_view(&hand, &[1], &table, 0, WIN_RULE_SHENYANG_BASIC),
        None
    );
}

#[test]
fn dealer_self_gang_preserves_basic_four_pairs_missing_suit_seven_pairs_plan() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    let hand = vec![1, 1, 1, 1, 2, 2, 3, 3, 11, 11, 12, 31, 35, 36];

    assert_eq!(
        choose_self_gang_from_view(&hand, &[1], &table, 0, WIN_RULE_SHENYANG_BASIC),
        None
    );
}

#[test]
fn self_gang_preserves_five_pairs_even_for_dragon_gang() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 35, 35, 35, 35];

    assert_eq!(
        choose_self_gang_from_view(&hand, &[35], &table, 0, WIN_RULE_SHENYANG_BASIC),
        None
    );
}

#[test]
fn self_gang_preserves_four_gui_yi_when_gang_breaks_ready_hand() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
    let hand = vec![1, 2, 3, 3, 3, 3, 11, 12, 13, 21, 35];

    assert_eq!(
        choose_self_gang_from_view(&hand, &[3], &table, 0, WIN_RULE_SHENYANG_BASIC),
        None
    );
}

#[test]
fn self_gang_preserves_added_four_gui_yi_when_gang_breaks_ready_hand() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(3)];
    let hand = vec![1, 2, 3, 4, 5, 7, 11, 12, 13, 21, 21];

    assert_eq!(
        choose_self_gang_from_view(&hand, &[3], &table, 0, WIN_RULE_SHENYANG_BASIC),
        None
    );
}

#[test]
fn self_gang_preserves_added_four_gui_yi_when_added_gang_has_no_fan_gain() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(3)];
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 21];

    assert_eq!(
        choose_self_gang_from_view(&hand, &[3], &table, 0, WIN_RULE_SHENYANG_BASIC),
        None
    );
}

#[test]
fn self_gang_preserves_locked_seven_pairs_plan() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 31];

    assert_eq!(
        choose_self_gang_from_view(&hand, &[1], &table, 0, WIN_RULE_RELAXED),
        None
    );
}

#[test]
fn self_gang_refuses_honor_gang_when_pure_one_suit_plan_is_strong() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 35, 35, 35, 35];

    assert_eq!(
        choose_self_gang_from_view(&hand, &[35], &table, 0, WIN_RULE_SHENYANG_BASIC),
        None
    );
}

#[test]
fn self_gang_skips_plain_gang_when_ready_fan_already_capped() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(1);
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
    let hand = vec![1, 2, 3, 9, 9, 9, 9, 11, 12, 13, 21];

    assert_eq!(
        choose_self_gang_from_view(&hand, &[9], &table, 0, WIN_RULE_SHENYANG_BASIC),
        None
    );
}

#[test]
fn self_gang_skips_plain_gang_when_single_wait_fan_caps_ready_hand() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(2);
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
    let hand = vec![1, 2, 3, 9, 9, 9, 9, 11, 12, 13, 21];

    assert_eq!(
        choose_self_gang_from_view(&hand, &[9], &table, 0, WIN_RULE_SHENYANG_BASIC),
        None
    );
}
