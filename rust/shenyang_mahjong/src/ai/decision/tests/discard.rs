use super::*;

#[test]
fn discard_prefers_isolated_honor() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(31)
    );
}

#[test]
fn discard_clears_isolated_edge_before_core_middle() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![5, 8, 11, 11, 11, 19, 19, 19, 21, 21, 21, 22, 22, 22];

    assert!(isolated_suited_singleton_discard_bias(8) > isolated_suited_singleton_discard_bias(5));
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(8)
    );
}

#[test]
fn discard_breaks_weak_edge_wait_before_core_two_sided_wait() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 4, 5, 11, 12, 13, 21, 22, 23, 24, 25, 35, 35];

    assert!(
        incomplete_sequence_discard_bias(&hand, 1, &[], &table, 0, WIN_RULE_SHENYANG_BASIC)
            > incomplete_sequence_discard_bias(&hand, 4, &[], &table, 0, WIN_RULE_SHENYANG_BASIC)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(1)
    );
}

#[test]
fn discard_breaks_weak_edge_wait_before_core_closed_middle_wait() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 4, 6, 11, 12, 13, 21, 22, 23, 24, 25, 35, 35];

    assert!(tile_is_core_closed_middle_wait_member(&hand, 4));
    assert!(
        incomplete_sequence_discard_bias(&hand, 1, &[], &table, 0, WIN_RULE_SHENYANG_BASIC)
            > incomplete_sequence_discard_bias(&hand, 4, &[], &table, 0, WIN_RULE_SHENYANG_BASIC)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(1)
    );
}

#[test]
fn discard_breaks_weak_edge_closed_wait_before_core_closed_middle_wait() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 3, 4, 6, 11, 12, 13, 21, 22, 23, 24, 25, 35, 35];

    assert!(tile_is_weak_edge_wait_terminal(&hand, 1));
    assert!(tile_is_core_closed_middle_wait_member(&hand, 4));
    assert!(
        incomplete_sequence_discard_bias(&hand, 1, &[], &table, 0, WIN_RULE_SHENYANG_BASIC)
            > incomplete_sequence_discard_bias(&hand, 4, &[], &table, 0, WIN_RULE_SHENYANG_BASIC)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(1)
    );
}

#[test]
fn discard_preserves_middle_of_complete_sequence() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![4, 5, 6, 8, 11, 11, 11, 19, 19, 19, 21, 21, 22, 22];

    assert!(
        complete_sequence_discard_bias(&hand, 5, &[], &table, 0)
            < complete_sequence_discard_bias(&hand, 8, &[], &table, 0)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(8)
    );
}

#[test]
fn discard_preserves_edge_of_complete_sequence() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    let hand = vec![4, 5, 6, 8, 11, 11, 11, 19, 19, 19, 21, 21, 22, 22];

    assert!(tile_is_part_of_complete_sequence(&hand, 4));
    assert!(
        complete_sequence_discard_bias(&hand, 4, &[], &table, 0)
            < complete_sequence_discard_bias(&hand, 8, &[], &table, 0)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(8)
    );
}

#[test]
fn discard_prefers_wind_before_single_dragon() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(31)
    );
}

#[test]
fn discard_can_clear_single_dragon_when_pairs_are_many() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 23, 24, 26, 35];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(35)
    );
}

#[test]
fn discard_preserves_last_honor_for_basic_rule() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![2, 3, 4, 5, 6, 7, 12, 13, 14, 22, 23, 24, 31, 5];

    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(31)
    );
}

#[test]
fn discard_preserves_only_dragon_pair_for_basic_heng() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 35, 35];

    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(35)
    );
}

#[test]
fn discard_preserves_only_pair_as_basic_heng_seed() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 3, 5, 5, 6, 7, 8, 11, 12, 13, 21, 22, 23];

    assert!(!has_triplet_or_dragon_pair(&hand, &[]));
    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(5)
    );
}

#[test]
fn discard_preserves_single_dragon_as_basic_heng_seed() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 3, 4, 5, 6, 8, 11, 12, 13, 21, 22, 24, 35];

    assert!(!has_triplet_or_dragon_pair(&hand, &[]));
    assert!(basic_heng_seed_discard_bias(&hand, 35, &[], WIN_RULE_SHENYANG_BASIC) < 0.0);
    assert_eq!(
        basic_heng_seed_discard_bias(&hand, 35, &[], WIN_RULE_RELAXED),
        0.0
    );
    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(35)
    );
}

#[test]
fn discard_preserves_only_recoverable_heng_seed() {
    let hand = vec![1, 2, 3, 4, 5, 11, 12, 13, 14, 15, 21, 22, 23, 35];
    let mut discards = dead_basic_heng_discards(&hand);
    if let Some(index) = discards.iter().position(|tile| *tile == 35) {
        discards.remove(index);
    }
    let table = table_with_discards(1, discards);

    assert!(!has_triplet_or_dragon_pair(&hand, &[]));
    assert!(can_recover_basic_heng(&hand, &[], &table));
    let after_dragon = remove_n_tiles(&hand, 35, 1);
    assert!(!can_recover_basic_heng_after_discard(
        &after_dragon,
        &[],
        &table,
        35
    ));
    assert!(loses_basic_heng_recovery_after_discard(
        &hand,
        &[],
        &table,
        35,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert!(violates_basic_heng_discard(
        &after_dragon,
        &[],
        &table,
        0,
        35,
        WIN_RULE_SHENYANG_BASIC
    ));
    let after_one = remove_n_tiles(&hand, 1, 1);
    assert!(!violates_basic_heng_discard(
        &after_one,
        &[],
        &table,
        0,
        1,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(35)
    );
}

#[test]
fn discard_preserves_ready_four_gui_yi_route() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(2)];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();
    let hand = vec![2, 3, 4, 11, 12, 13, 21, 22, 23, 35, 36];
    let after_safe_discard = remove_n_tiles(&hand, 36, 1);
    let after_four_gui_yi_discard = remove_n_tiles(&hand, 2, 1);

    assert_eq!(estimated_four_gui_yi_fan(&hand, melds), 1);
    assert_eq!(
        estimated_four_gui_yi_fan(&after_four_gui_yi_discard, melds),
        0
    );
    assert!(
        ready_tile_score(
            &after_safe_discard,
            melds,
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC
        ) > 0.0
    );
    assert!(
        four_gui_yi_discard_bias(&hand, 2, melds, &table, 0, WIN_RULE_SHENYANG_BASIC)
            < four_gui_yi_discard_bias(&hand, 36, melds, &table, 0, WIN_RULE_SHENYANG_BASIC)
    );
    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(2)
    );
}

#[test]
fn discard_preserves_only_triplet_for_basic_heng() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 1, 2, 3, 4, 11, 12, 13, 21, 22, 23, 35, 36];

    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(1)
    );
}

#[test]
fn discard_preserves_last_suit_for_basic_rule() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 12, 13, 21, 31];

    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(21)
    );
}

#[test]
fn discard_preserves_last_tile_of_a_suit_for_three_suits() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 11, 12, 13, 14, 15, 16, 21, 22, 23, 24, 25, 26, 31];

    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(1)
    );
}

#[test]
fn discard_preserves_only_terminal_or_honor_for_basic_rule() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 5, 6];

    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(1)
    );
}

#[test]
fn discard_preserves_ready_hand_instead_of_breaking_wait() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 31, 31, 32];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(32)
    );
}

#[test]
fn discard_uses_public_discard_safety() {
    let table = table_with_discards(1, vec![31]);
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 32];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(31)
    );
}

#[test]
fn discard_uses_own_previous_discard_as_public_safety() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&0).unwrap().discards = vec![5];
    let hand = vec![1, 1, 4, 5, 7, 9, 12, 14, 17, 21, 23, 25, 31, 35];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(5)
    );
}
