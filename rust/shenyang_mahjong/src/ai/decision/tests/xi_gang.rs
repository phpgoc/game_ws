use super::*;

#[test]
fn ai_declares_wind_xi_gang_before_dragon_xi_gang() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 3, 11, 12, 13, 21, 31, 32, 33, 34, 35, 36, 37];
    let options = vec![vec![35, 36, 37], vec![31, 32, 33, 34]];

    assert_eq!(
        choose_xi_gang_from_view(&hand, &options, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(vec![31, 32, 33, 34])
    );
}

#[test]
fn ai_normally_declares_dragon_xi_gang() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 35, 36, 37];

    assert_eq!(
        choose_xi_gang_from_view(
            &hand,
            &[vec![37, 35, 36]],
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC,
        ),
        Some(vec![35, 36, 37])
    );
}

#[test]
fn ai_preserves_multiple_dragon_pairs_over_dragon_xi_gang() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 35, 35, 36, 36, 37];

    assert_eq!(
        choose_xi_gang_from_view(
            &hand,
            &[vec![35, 36, 37]],
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC,
        ),
        None
    );
}

#[test]
fn ai_preserves_locked_seven_pairs_over_wind_xi_gang() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 31, 32, 33, 34];

    assert!(should_lock_seven_pairs_plan(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC,
    ));
    assert_eq!(
        choose_xi_gang_from_view(
            &hand,
            &[vec![31, 32, 33, 34]],
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC,
        ),
        None
    );
}

#[test]
fn ai_preserves_live_pure_one_suit_plan_over_dragon_xi_gang() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 31, 32, 33, 35, 36, 37];

    assert!(
        pure_one_suit_plan_score_for_context(&hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC,) > 0.0
    );
    assert_eq!(
        choose_xi_gang_from_view(
            &hand,
            &[vec![35, 36, 37]],
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC,
        ),
        None
    );
}

#[test]
fn ai_does_not_declare_wind_xi_gang_without_replacement_tile() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 0;
    let hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 32, 33, 34, 35];

    assert_eq!(
        choose_xi_gang_from_view(
            &hand,
            &[vec![31, 32, 33, 34]],
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC,
        ),
        None
    );
}
