use super::*;

#[test]
fn claim_peng_passes_dragon_when_pure_one_suit_plan_is_strong() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 35, 35, 36];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_passes_dragon_when_pure_one_suit_plan_starts_at_eight_tiles() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 11, 21, 35, 35, 36];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_passes_main_suit_pure_one_suit_when_opening_is_not_required() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 11, 12];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_passes_main_suit_when_closed_pure_one_suit_plan_is_strong() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 11, 12];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_passes_nine_tile_pure_one_suit_plan() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 3, 4, 5, 6, 7, 8, 11, 12, 31, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_passes_open_main_suit_pure_one_suit_even_when_it_reaches_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    table.claim_window = Some(AiClaimView {
        tile: 2,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 2, 3, 3, 3, 3, 4, 4, 7];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();

    assert!(pure_one_suit_plan_score_for_context(&hand, melds, &table, 0) > 0.0);
    assert_eq!(
        ready_tile_score(&hand, melds, &table, 0, WIN_RULE_SHENYANG_BASIC),
        0.0
    );
    let mut next = remove_n_tiles(&hand, 2, 2);
    sort_tiles(&mut next);
    let mut next_melds = melds.to_vec();
    next_melds.push(claim_peng_meld(2, 1));
    assert!(unique_tiles(&next).into_iter().any(|discard| {
        let mut after_discard = remove_n_tiles(&next, discard, 1);
        sort_tiles(&mut after_discard);
        ready_has_pure_one_suit_win_after_discard(
            &after_discard,
            &next_melds,
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC,
            discard,
        )
    }));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_passes_weak_main_suit_pure_one_suit_start() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 3, 4, 5, 6, 7, 11, 12, 21, 31, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_passes_when_it_breaks_locked_pure_one_suit_plan() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 11,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 11, 11];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_preserves_pure_one_suit_seven_pairs_wait() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 8];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn one_fan_claim_peng_ignores_unfinished_pure_plan() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(1);
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 11, 12];

    assert_eq!(
        pure_one_suit_plan_score_for_context(&hand, &[], &table, 0),
        0.0
    );
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}
