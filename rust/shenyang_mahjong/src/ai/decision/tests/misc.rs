use super::*;

#[test]
fn round_thresholds_match_ai_phase_boundaries() {
    let mut table = table_with_discards(1, Vec::new());

    table.wall_count = FINAL_DEFENSE_WALL_COUNT + 1;
    assert!(!is_late_defense_round(&table));
    table.wall_count = FINAL_DEFENSE_WALL_COUNT;
    assert!(is_late_defense_round(&table));

    table.wall_count = LATE_PRESSURE_WALL_COUNT + 1;
    assert!(!is_late_round(&table));
    table.wall_count = LATE_PRESSURE_WALL_COUNT;
    assert!(is_late_round(&table));

    table.wall_count = MID_BROKEN_HAND_WALL_COUNT + 1;
    assert!(!is_mid_broken_hand_defense_round(&table));
    assert!(!is_mid_opening_round(&table));
    table.wall_count = MID_BROKEN_HAND_WALL_COUNT;
    assert!(is_mid_broken_hand_defense_round(&table));
    assert!(is_mid_opening_round(&table));

    table.wall_count = MID_ROUND_WALL_COUNT + 1;
    assert!(!is_mid_round(&table));
    table.wall_count = MID_ROUND_WALL_COUNT;
    assert!(is_mid_round(&table));
}

#[test]
fn hand_power_ignores_invalid_tiles() {
    let base_hand = vec![1, 2, 3, 11, 12, 13];
    let hand_with_invalid_triplet = vec![1, 2, 3, 11, 12, 13, 99, 99, 99];

    assert!(!is_valid_tile(99));
    assert_eq!(hand_power(&[99, 99, 99]), 0.0);
    assert!((hand_power(&hand_with_invalid_triplet) - hand_power(&base_hand)).abs() < 0.0001);
}

#[test]
fn unique_tiles_ignores_invalid_tiles() {
    assert_eq!(unique_tiles(&[99, 1, 1, 37, 0]), vec![1, 37]);
}

#[test]
fn discard_candidates_ignore_invalid_tiles() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    let hand = vec![1, 1, 4, 7, 9, 12, 14, 14, 17, 21, 23, 25, 31, 99];

    let choice = choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED);

    assert!(choice.is_some_and(is_valid_tile));
}

#[test]
fn capped_non_dealer_prefers_wider_wait_over_single_wait_fan() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(1);
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
    let hand = vec![2, 2, 4, 5, 7, 11, 12, 13, 21, 22, 23];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(7)
    );
}

#[test]
fn relaxed_near_ready_hand_does_not_use_defensive_opening() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 40;
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 31, 31, 35];

    assert!(
        ready_tile_score(&hand, &[], &table, 0, WIN_RULE_RELAXED) > 0.0
            || one_step_wait_potential(&hand, &[], &table, 0, WIN_RULE_RELAXED) > 0.0
    );
    assert!(!should_open_broken_closed_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_RELAXED
    ));
}

#[test]
fn late_ready_discard_still_preserves_wait_over_safe_tile() {
    let mut table = table_with_discards(1, vec![5]);
    table.wall_count = 16;
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 31, 31, 32];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(32)
    );
}

#[test]
fn late_unready_discard_uses_defense_before_hand_progress() {
    let mut table = table_with_discards(1, vec![14]);
    table.wall_count = 16;
    let hand = vec![1, 1, 4, 7, 9, 12, 14, 14, 17, 21, 23, 25, 31, 35];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(14)
    );
}

#[test]
fn late_broken_basic_discard_follows_public_tile_for_weak_recoverable_hand() {
    let mut table = table_with_discards(1, vec![31]);
    table.wall_count = 40;
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 11, 14, 19, 21, 31, 32, 33];

    assert_eq!(
        unrecoverable_basic_rule_requirement_count(&hand, &[], &table),
        0
    );
    assert!(hand_power(&hand) >= 16.0);
    assert!(hand_power(&hand) < 18.0);
    assert!(!should_lock_seven_pairs_plan(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_eq!(
        pure_one_suit_plan_score_for_context(&hand, &[], &table, 0),
        0.0
    );
    assert_eq!(piao_plan_score_for_context(&hand, &[], &table, 0), 0.0);
    assert_eq!(
        best_ready_score_after_discard(&hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC),
        0.0
    );
    assert_eq!(
        best_one_step_wait_potential_after_discard(&hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC),
        0.0
    );
    assert!(should_use_broken_hand_public_defense_discard(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(31)
    );
}
