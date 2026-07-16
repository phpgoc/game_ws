use super::*;

#[test]
fn dealer_mid_unrecoverable_basic_hand_uses_public_defense_discard() {
    let mut discards = dead_terminal_or_honor_discards();
    discards.push(5);
    let mut table = table_with_discards(1, discards);
    table.dealer_position = 0;
    table.wall_count = 52;
    let hand = vec![2, 3, 4, 5, 5, 6, 7, 12, 13, 14, 22, 23, 24, 25];

    assert_eq!(
        unrecoverable_basic_rule_requirement_count(&hand, &[], &table, 0),
        1
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
        Some(5)
    );
}

#[test]
fn mid_broken_basic_discard_follows_open_meld_tile_without_public_discards() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 40;
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(14)];
    let hand = vec![2, 5, 8, 12, 14, 17, 22, 24, 27, 31, 32, 33, 35, 37];

    assert_eq!(public_discard_count(&table, 14), 0);
    assert!(mid_round_open_meld_safety_bias(&table, 14) > 0.0);
    assert!(should_use_broken_hand_public_defense_discard(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(14)
    );
}

#[test]
fn mid_broken_basic_discard_follows_public_tile_before_hand_shape() {
    let mut table = table_with_discards(1, vec![5]);
    table.wall_count = 40;
    let hand = vec![2, 5, 8, 12, 14, 17, 22, 24, 27, 31, 32, 33, 35, 37];

    assert!(should_use_broken_hand_public_defense_discard(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(5)
    );
}

#[test]
fn mid_broken_basic_public_defense_preserves_only_recoverable_heng_seed() {
    let hand = vec![1, 2, 5, 8, 11, 12, 14, 17, 21, 22, 24, 27, 29, 35];
    let mut discards = dead_basic_heng_discards(&hand);
    if let Some(index) = discards.iter().position(|tile| *tile == 35) {
        discards.remove(index);
    }
    let mut table = table_with_discards(1, discards);
    table.wall_count = 40;

    assert!(can_recover_basic_heng(&hand, &[], &table, 0));
    let after_dragon = remove_n_tiles(&hand, 35, 1);
    assert!(!can_recover_basic_heng_after_discard(
        &after_dragon,
        &[],
        &table,
        0,
        35
    ));
    assert!(should_use_broken_hand_public_defense_discard(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(35)
    );
}

#[test]
fn mid_broken_discard_uses_opponent_missing_suit_read_without_public_tile() {
    let mut table = table_with_discards(1, vec![11, 13, 15, 18]);
    table.wall_count = 40;
    let hand = vec![2, 5, 8, 12, 14, 17, 22, 24, 27, 31, 32, 33, 35, 37];

    assert_eq!(public_discard_count(&table, 12), 0);
    assert_eq!(mid_round_open_meld_safety_bias(&table, 12), 0.0);
    assert!(mid_broken_opponent_missing_suit_safety_bias(&table, 0, 12) > 0.0);
    assert!(should_use_broken_hand_public_defense_discard(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_RELAXED
    ));
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(12)
    );
}

#[test]
fn mid_broken_public_defense_prefers_honor_between_singleton_safe_tiles() {
    let mut table = table_with_discards(1, vec![31, 6, 9]);
    table.wall_count = 40;
    table.seats.get_mut(&0).unwrap().discards = vec![5, 5, 5];
    let hand = vec![5, 31];

    assert_eq!(public_discard_count(&table, 5), 3);
    assert_eq!(public_discard_count(&table, 31), 1);
    assert!(
        defense_tile_safety_priority(&table, 31, 1) > defense_tile_safety_priority(&table, 5, 1)
    );
    assert_eq!(
        choose_public_defense_discard_from_candidates(
            &hand,
            &[],
            &table,
            0,
            WIN_RULE_RELAXED,
            vec![5, 31]
        ),
        Some(31)
    );
}

#[test]
fn mid_broken_public_defense_preserves_dragon_pair_over_public_singleton() {
    let mut table = table_with_discards(1, vec![5, 35]);
    table.wall_count = 40;
    let hand = vec![2, 5, 8, 12, 14, 17, 22, 24, 27, 31, 32, 33, 35, 35];

    assert!(should_use_broken_hand_public_defense_discard(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_RELAXED
    ));
    assert!(
        public_defense_tile_safety_score(&table, 0, 5, 1)
            > public_defense_tile_safety_score(&table, 0, 35, 2)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(5)
    );
}

#[test]
fn mid_broken_public_defense_preserves_locked_seven_pairs_route() {
    let mut table = table_with_discards(1, vec![11]);
    table.wall_count = 40;
    let hand = vec![1, 1, 2, 2, 3, 3, 4, 4, 11, 11, 12, 12, 21, 31];

    assert!(!should_use_broken_hand_public_defense_discard(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(11)
    );
}

#[test]
fn mid_broken_public_defense_preserves_triplet_over_public_pair() {
    let mut table = table_with_discards(1, vec![5, 7]);
    table.wall_count = 40;
    let hand = vec![2, 5, 5, 5, 7, 7, 12, 14, 17, 22, 24, 27, 31, 33];

    assert!(
        public_defense_tile_safety_score(&table, 0, 7, 2)
            > public_defense_tile_safety_score(&table, 0, 5, 3)
    );
    assert_eq!(
        choose_public_defense_discard_from_candidates(
            &hand,
            &[],
            &table,
            0,
            WIN_RULE_RELAXED,
            vec![5, 7]
        ),
        Some(7)
    );
}

#[test]
fn mid_broken_relaxed_discard_follows_public_tile_before_hand_shape() {
    let mut table = table_with_discards(1, vec![5]);
    table.wall_count = 40;
    let hand = vec![2, 5, 8, 12, 14, 17, 22, 24, 27, 31, 32, 33, 35, 37];

    assert!(should_use_broken_hand_public_defense_discard(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_RELAXED
    ));
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(5)
    );
}
