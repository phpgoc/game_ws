use super::*;

#[test]
fn basic_three_suits_filter_allows_locked_seven_pairs_route() {
    let table = table_with_discards(1, Vec::new());
    let hand_after_discard = vec![1, 1, 2, 2, 3, 3, 4, 4, 11, 11, 12, 12, 31];

    assert!(!violates_basic_three_suits_discard(
        &hand_after_discard,
        &[],
        &table,
        0,
        21
    ));
}

#[test]
fn broken_closed_defense_preserves_seven_pairs_route() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 40;
    let hand = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 12, 31, 35, 36];

    assert!(!should_open_broken_closed_hand_for_defense(
        &hand,
        &[],
        &table,
        0
    ));
}

#[test]
fn capped_discard_sets_seven_pairs_wait_on_live_wind_tiebreaker() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(4);
    let hand = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 21, 21, 22, 22, 31];

    assert_eq!(choose_discard_from_view(&hand, &table, 0), Some(5));
}

#[test]
fn capped_locked_seven_pairs_route_can_break_three_suits_requirement() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(1);
    let hand_after_discard = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 12, 13, 13, 5];

    assert!(should_preserve_seven_pairs_plan_for_context(
        &hand_after_discard,
        &[],
        &table,
        0
    ));
    assert!(!violates_basic_three_suits_discard(
        &hand_after_discard,
        &[],
        &table,
        0,
        21
    ));
}

#[test]
fn capped_locked_seven_pairs_route_can_discard_last_honor() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(1);
    let hand = vec![2, 2, 3, 3, 4, 4, 12, 12, 13, 13, 14, 14, 5, 31];
    let after_discard = remove_n_tiles(&hand, 31, 1);

    assert!(should_preserve_seven_pairs_plan_for_context(
        &after_discard,
        &[],
        &table,
        0
    ));
    assert_eq!(choose_discard_from_view(&hand, &table, 0), Some(31));
}

#[test]
fn dealer_discard_keeps_four_pairs_when_basic_hand_is_missing_suit() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    let hand = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 13, 14, 15, 31, 35];

    assert!(!matches!(
        choose_discard_from_view(&hand, &table, 0),
        Some(1 | 2 | 3 | 11)
    ));
}

#[test]
fn dealer_does_not_lock_five_pairs_when_basic_route_is_viable() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 21, 21, 22, 31, 35, 35, 36];

    assert!(!should_lock_seven_pairs_plan(&hand, &[], &table, 0));
    assert!(!should_lock_seven_pairs_plan(&hand, &[], &table, 0));
}

#[test]
fn discard_keeps_four_pairs_for_basic_seven_pairs_when_missing_suit() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 13, 14, 15, 31, 35];

    assert!(!matches!(
        choose_discard_from_view(&hand, &table, 0),
        Some(1 | 2 | 3 | 11)
    ));
}

#[test]
fn discard_keeps_pairs_for_basic_seven_pairs_plan_when_missing_suit() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 12, 31, 35, 36, 37];

    let discard = choose_discard_from_view(&hand, &table, 0);

    assert!(matches!(discard, Some(31 | 35 | 36 | 37)));
}

#[test]
fn discard_keeps_pairs_when_many_pairs_can_chase_seven_pairs() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 22, 23, 31, 35, 36];

    assert!(matches!(
        choose_discard_from_view(&hand, &table, 0),
        Some(21 | 22 | 23 | 31 | 35 | 36)
    ));
}

#[test]
fn discard_keeps_quad_pairs_for_basic_seven_pairs_when_missing_suit() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 1, 1, 2, 2, 3, 3, 11, 11, 12, 31, 35, 36];

    assert_eq!(pair_count(&hand), 5);
    assert!(should_lock_seven_pairs_plan(&hand, &[], &table, 0));
    assert!(!matches!(
        choose_discard_from_view(&hand, &table, 0),
        Some(1 | 2 | 3 | 11)
    ));
}

#[test]
fn discard_locked_five_pairs_prefers_honor_singleton_first() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 2, 2, 5, 9, 11, 11, 12, 12, 14, 21, 21, 31];

    assert_eq!(choose_discard_from_view(&hand, &table, 0), Some(31));
}

#[test]
fn discard_locked_five_pairs_prefers_non_terminal_singleton_over_terminal() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 2, 2, 5, 9, 11, 11, 12, 12, 14, 19, 21, 21];

    assert!(matches!(
        choose_discard_from_view(&hand, &table, 0),
        Some(5 | 14)
    ));
}

#[test]
fn discard_locked_five_pairs_prefers_single_dragon_before_wind() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 14, 21, 21, 31, 35];

    assert!(should_lock_seven_pairs_plan(&hand, &[], &table, 0));
    assert_eq!(choose_discard_from_view(&hand, &table, 0), Some(35));
}

#[test]
fn discard_returns_none_for_seven_pairs_win() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35];

    assert_eq!(choose_discard_from_view(&hand, &table, 0), None);
}

#[test]
fn discard_sets_seven_pairs_wait_away_from_public_middle_tile() {
    let table = table_with_discards(1, vec![5]);
    let hand = vec![1, 1, 2, 2, 5, 9, 11, 11, 12, 12, 21, 21, 22, 22];

    assert_eq!(choose_discard_from_view(&hand, &table, 0), Some(5));
}

#[test]
fn discard_sets_seven_pairs_wait_by_breaking_dead_triplet_wait() {
    let table = table_with_discards(1, vec![31]);
    let hand = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 21, 21, 31, 31, 31];
    let dead_wind_wait = remove_n_tiles(&hand, 5, 1);
    let live_middle_wait = remove_n_tiles(&hand, 31, 1);

    assert_eq!(remaining_tile_count(&dead_wind_wait, &table, 0, 31), 0);
    assert!(
        seven_pairs_wait_tile_score(5, &live_middle_wait, &table, 0)
            > seven_pairs_wait_tile_score(31, &dead_wind_wait, &table, 0)
    );
    assert_eq!(choose_discard_from_view(&hand, &table, 0), Some(31));
}

#[test]
fn discard_sets_seven_pairs_wait_on_live_terminal_before_middle_tile() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 2, 2, 5, 9, 11, 11, 12, 12, 21, 21, 22, 22];

    assert_eq!(choose_discard_from_view(&hand, &table, 0), Some(5));
}

#[test]
fn discard_sets_seven_pairs_wait_on_live_terminal_over_dead_wind() {
    let table = table_with_discards(1, vec![31, 31]);
    let hand = vec![1, 1, 2, 2, 9, 11, 11, 12, 12, 21, 21, 22, 22, 31];

    assert_eq!(choose_discard_from_view(&hand, &table, 0), Some(31));
}

#[test]
fn discard_sets_seven_pairs_wait_on_live_wind_before_middle_tile() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 21, 21, 22, 22, 31];

    assert_eq!(choose_discard_from_view(&hand, &table, 0), Some(5));
}

#[test]
fn discard_sets_seven_pairs_wait_on_wind_before_dragon() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 2, 2, 3, 3, 4, 4, 11, 11, 21, 21, 31, 35];

    assert_eq!(choose_discard_from_view(&hand, &table, 0), Some(35));
}

#[test]
fn five_pair_plan_unlocks_when_wall_cannot_supply_two_missing_pairs() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 1;
    let hand = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 12, 31, 35, 36];

    assert_eq!(pair_count(&hand), 5);
    assert!(!should_lock_seven_pairs_plan(&hand, &[], &table, 0));
    assert_eq!(seven_pairs_plan_score(&hand, &[], &table, 0), 0.0);
}

#[test]
fn four_pair_missing_suit_bias_ignores_malformed_meld() {
    let table = table_with_discards(1, Vec::new());
    let hand_after_discard = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 13, 14, 15, 31];
    let malformed_meld = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![31, 31],
        from_position: Some(1),
    };
    let base_bias = three_suits_discard_bias(&hand_after_discard, &[], &table, 0, 15);

    assert_eq!(pair_count(&hand_after_discard), 4);
    assert_eq!(missing_suits(&hand_after_discard, &[]), vec![2]);
    assert_eq!(base_bias, 0.0);
    assert_eq!(
        three_suits_discard_bias(&hand_after_discard, &[malformed_meld], &table, 0, 15,),
        base_bias
    );
    assert!(
        three_suits_discard_bias(&hand_after_discard, &[test_peng_meld(31)], &table, 0, 15,) < 0.0
    );
}

#[test]
fn half_capped_basic_foundation_does_not_lock_five_pairs() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(4);
    let hand = vec![1, 1, 2, 2, 11, 11, 21, 22, 31, 35, 35, 35, 35];

    assert_eq!(pair_count(&hand), 5);
    assert!(has_basic_normal_route_foundation(&hand, &[]));
    assert_eq!(estimated_visible_bonus_fan(&hand, &[]), 2);
    assert!(capped_basic_route_foundation_visible_fan_exceeds_half_cap(
        &hand,
        &[],
        &table
    ));
    assert!(!should_lock_seven_pairs_plan(&hand, &[], &table, 0));
    assert_eq!(seven_pairs_plan_score(&hand, &[], &table, 0), 0.0);
}

#[test]
fn late_six_pair_hand_breaks_public_pair_instead_of_setting_unsafe_wait() {
    let mut table = table_with_discards(1, vec![1]);
    table.wall_count = FINAL_DEFENSE_WALL_COUNT;
    let hand = vec![1, 1, 2, 2, 5, 9, 11, 11, 12, 12, 21, 21, 22, 22];

    assert_eq!(pair_count(&hand), 6);
    assert!(should_keep_pairs_for_seven_pairs_discard(
        &hand,
        &[],
        &table,
        0,
    ));
    assert_eq!(public_discard_count(&table, 1), 1);
    assert_eq!(choose_discard_from_view(&hand, &table, 0), Some(1));
}

#[test]
fn one_fan_capped_room_does_not_lock_five_pairs_when_basic_route_is_viable() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(1);
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 22, 31, 35, 35];

    assert!(has_basic_normal_route_foundation(&hand, &[]));
    assert!(!should_lock_seven_pairs_plan(&hand, &[], &table, 0));
}

#[test]
fn one_fan_capped_six_pairs_still_sets_better_seven_pairs_wait() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(1);
    let hand = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 21, 21, 22, 22, 31];

    assert_eq!(pair_count(&hand), 6);
    assert_eq!(choose_discard_from_view(&hand, &table, 0), Some(5));
}

#[test]
fn one_fan_room_only_locks_seven_pairs_when_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(1);
    let four_pairs = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 13, 21, 31, 35];
    let five_pairs = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 12, 21, 31, 35];
    let six_pairs = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 12, 21, 21, 35];

    assert_eq!(seven_pairs_plan_score(&four_pairs, &[], &table, 0), 0.0);
    assert_eq!(pair_count(&five_pairs), 5);
    assert!(!should_lock_seven_pairs_plan(&five_pairs, &[], &table, 0));
    assert_eq!(seven_pairs_plan_score(&five_pairs, &[], &table, 0), 0.0);
    assert_eq!(seven_pairs_plan_score(&five_pairs, &[], &table, 0), 0.0);

    assert!(is_seven_pairs_wait_shape(&six_pairs));
    assert!(should_lock_seven_pairs_plan(&six_pairs, &[], &table, 0));
    assert!(seven_pairs_plan_score(&six_pairs, &[], &table, 0) > 0.0);
}

#[test]
fn dealer_locks_five_pairs_when_normal_route_is_missing_a_suit() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 31, 32, 33];

    assert_eq!(pair_count(&hand), 5);
    assert!(!has_basic_normal_route_foundation(&hand, &[]));
    assert!(should_lock_seven_pairs_plan(&hand, &[], &table, 0));
    assert!(should_lock_seven_pairs_plan(&hand, &[], &table, 0));
}

#[test]
fn seven_pairs_plan_ignores_invalid_pairs() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 2, 2, 3, 4, 5, 6, 7, 31, 97, 97, 98, 98];

    assert_eq!(pair_count(&hand), 2);
    assert_eq!(seven_pairs_plan_score(&hand, &[], &table, 0), 0.0);
    assert!(!should_lock_seven_pairs_plan(&hand, &[], &table, 0));
}

#[test]
fn seven_pairs_plan_ignores_malformed_melds_but_rejects_valid_melds() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 12, 31, 35, 36];
    let discard_hand = [hand.as_slice(), &[37]].concat();
    let malformed_meld = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![21, 21],
        from_position: Some(1),
    };
    let valid_meld = test_peng_meld(21);
    let base_score = seven_pairs_plan_score(&hand, &[], &table, 0);
    let base_bias = seven_pairs_plan_discard_bias(&discard_hand, 1, &[], &table, 0);

    assert_eq!(valid_meld_count(&[malformed_meld.clone()]), 0);
    assert_eq!(valid_meld_count(&[valid_meld.clone()]), 1);
    assert!(should_lock_seven_pairs_plan(
        &hand,
        &[malformed_meld.clone()],
        &table,
        0,
    ));
    assert_eq!(
        seven_pairs_plan_score(&hand, &[malformed_meld.clone()], &table, 0,),
        base_score
    );
    assert_eq!(
        seven_pairs_plan_discard_bias(&discard_hand, 1, &[malformed_meld], &table, 0,),
        base_bias
    );

    assert!(!should_lock_seven_pairs_plan(
        &hand,
        &[valid_meld.clone()],
        &table,
        0,
    ));
    assert_eq!(
        seven_pairs_plan_score(&hand, &[valid_meld.clone()], &table, 0,),
        0.0
    );
    assert_eq!(
        seven_pairs_plan_discard_bias(&discard_hand, 1, &[valid_meld], &table, 0,),
        0.0
    );

    let wait_hand = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 12, 13, 13, 31, 5];
    let base_wait_bias = seven_pairs_wait_discard_bias(&wait_hand, 5, &[], &table, 0);
    assert!(base_wait_bias > 0.0);
    assert_eq!(
        seven_pairs_wait_discard_bias(
            &wait_hand,
            5,
            &[WsShenyangMahjongMeld {
                kind: ShenyangMahjongMeldKind::PENG,
                tiles: vec![21, 21],
                from_position: Some(1),
            }],
            &table,
            0,
        ),
        base_wait_bias
    );
    assert_eq!(
        seven_pairs_wait_discard_bias(&wait_hand, 5, &[test_peng_meld(21)], &table, 0,),
        0.0
    );
}

#[test]
fn seven_pairs_plan_protects_honor_and_terminal_pairs_more() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 5, 5, 11, 11, 12, 12, 21, 21, 31, 31, 35, 36];

    let middle_pair = seven_pairs_plan_discard_bias(&hand, 5, &[], &table, 0);
    let terminal_pair = seven_pairs_plan_discard_bias(&hand, 1, &[], &table, 0);
    let honor_pair = seven_pairs_plan_discard_bias(&hand, 31, &[], &table, 0);

    assert!(honor_pair < terminal_pair);
    assert!(terminal_pair < middle_pair);
}

#[test]
fn seven_pairs_plan_protects_live_middle_pair_over_dead_wind_pair() {
    let table = table_with_discards(1, vec![31, 31]);
    let hand = vec![1, 1, 5, 5, 11, 11, 12, 12, 21, 21, 31, 31, 35, 36];

    let dead_wind_pair = seven_pairs_plan_discard_bias(&hand, 31, &[], &table, 0);
    let live_middle_pair = seven_pairs_plan_discard_bias(&hand, 5, &[], &table, 0);

    assert_eq!(remaining_tile_count(&hand, &table, 0, 31), 0);
    assert!(remaining_tile_count(&hand, &table, 0, 5) > 0);
    assert!(live_middle_pair < dead_wind_pair);
}

#[test]
fn seven_pairs_plan_protects_live_pair_over_dead_pair() {
    let table = table_with_discards(1, vec![5, 5]);
    let hand = vec![1, 1, 5, 5, 11, 11, 12, 12, 21, 21, 31, 31, 35, 36];

    let dead_middle_pair = seven_pairs_plan_discard_bias(&hand, 5, &[], &table, 0);
    let live_middle_pair = seven_pairs_plan_discard_bias(&hand, 12, &[], &table, 0);

    assert_eq!(remaining_tile_count(&hand, &table, 0, 5), 0);
    assert!(remaining_tile_count(&hand, &table, 0, 12) > 0);
    assert!(live_middle_pair < dead_middle_pair);
}

#[test]
fn seven_pairs_wait_score_prefers_live_middle_over_public_wind() {
    let table = table_with_discards(1, vec![31]);
    let wind_wait = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 31];
    let middle_wait = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 21, 21, 22, 22];

    assert!(
        seven_pairs_wait_tile_score(5, &middle_wait, &table, 0)
            > seven_pairs_wait_tile_score(31, &wind_wait, &table, 0)
    );
}

#[test]
fn seven_pairs_wait_score_rejects_dead_exposed_wind_wait() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(31)];
    let wind_wait = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 31];
    let middle_wait = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 21, 21, 22, 22];
    let hand = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 21, 21, 22, 22, 31];

    assert_eq!(remaining_tile_count(&wind_wait, &table, 0, 31), 0);
    assert!(
        seven_pairs_wait_tile_score(5, &middle_wait, &table, 0)
            > seven_pairs_wait_tile_score(31, &wind_wait, &table, 0)
    );
    assert_eq!(choose_discard_from_view(&hand, &table, 0), Some(31));
}

#[test]
fn seven_pairs_wait_shape_ignores_invalid_singleton() {
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 99];

    assert_eq!(pair_count(&hand), 6);
    assert!(!is_seven_pairs_wait_shape(&hand));
    assert_eq!(single_tile(&hand), None);
}

#[test]
fn six_pair_plan_keeps_dead_singleton_when_wall_can_replace_the_wait() {
    let mut table = table_with_discards(1, vec![31, 31, 31]);
    table.wall_count = 2;
    let hand = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 12, 13, 13, 31];

    assert!(should_lock_seven_pairs_plan(&hand, &[], &table, 0));
    assert!(seven_pairs_plan_score(&hand, &[], &table, 0) > 0.0);
}

#[test]
fn six_pair_plan_keeps_live_singleton_with_one_wall_tile() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 1;
    let hand = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 12, 13, 13, 31];

    assert!(remaining_tile_count(&hand, &table, 0, 31) > 0);
    assert!(should_lock_seven_pairs_plan(&hand, &[], &table, 0));
}

#[test]
fn six_pair_plan_unlocks_when_dead_singleton_needs_two_draws() {
    let mut table = table_with_discards(1, vec![31, 31, 31]);
    table.wall_count = 1;
    let hand = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 12, 13, 13, 31];

    assert!(is_seven_pairs_wait_shape(&hand));
    assert_eq!(remaining_tile_count(&hand, &table, 0, 31), 0);
    assert!(!should_lock_seven_pairs_plan(&hand, &[], &table, 0));
    assert_eq!(seven_pairs_plan_score(&hand, &[], &table, 0), 0.0);
}

#[test]
fn speed_first_seven_pairs_wait_prefers_three_live_middle_copies_over_two_terminals() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    table.wall_count = 70;
    table.seats.get_mut(&1).unwrap().melds = vec![test_chi_meld(7)];
    let hand = vec![1, 1, 2, 2, 5, 9, 11, 11, 12, 12, 21, 21, 22, 22];
    let middle_wait = remove_n_tiles(&hand, 9, 1);
    let terminal_wait = remove_n_tiles(&hand, 5, 1);

    assert_eq!(remaining_tile_count(&middle_wait, &table, 0, 5), 3);
    assert_eq!(remaining_tile_count(&terminal_wait, &table, 0, 9), 2);
    assert!(
        seven_pairs_wait_tile_score(5, &middle_wait, &table, 0)
            > seven_pairs_wait_tile_score(9, &terminal_wait, &table, 0)
    );
    assert_eq!(choose_discard_from_view(&hand, &table, 0), Some(9));

    table.dealer_position = 3;
    table.wall_count = FINAL_DEFENSE_WALL_COUNT;
    assert!(
        seven_pairs_wait_tile_score(5, &middle_wait, &table, 0)
            > seven_pairs_wait_tile_score(9, &terminal_wait, &table, 0)
    );
}

#[test]
fn two_fan_capped_room_does_not_lock_five_pairs_when_basic_bonus_caps() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(2);
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 22, 35, 35, 35];

    assert_eq!(pair_count(&hand), 5);
    assert!(has_basic_normal_route_foundation(&hand, &[]));
    assert_eq!(estimated_visible_bonus_fan(&hand, &[]), 1);
    assert!(!should_lock_seven_pairs_plan(&hand, &[], &table, 0));
    assert_eq!(seven_pairs_plan_score(&hand, &[], &table, 0), 0.0);
}

#[test]
fn two_fan_room_does_not_lock_bonus_capped_five_pairs() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(2);
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 31, 35, 35, 35];

    assert_eq!(pair_count(&hand), 5);
    assert_eq!(estimated_visible_bonus_fan(&hand, &[]), 1);
    assert!(!should_lock_seven_pairs_plan(&hand, &[], &table, 0));
    assert_eq!(seven_pairs_plan_score(&hand, &[], &table, 0), 0.0);
}
