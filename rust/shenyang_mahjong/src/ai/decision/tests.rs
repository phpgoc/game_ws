use std::collections::HashMap;

use super::*;
use crate::ai::observation::{AiClaimView, AiSeatView};
use crate::rules::{WIN_RULE_RELAXED, WIN_RULE_SHENYANG_BASIC};

#[test]
fn basic_three_suits_filter_allows_locked_seven_pairs_route() {
    let table = table_with_discards(1, Vec::new());
    let hand_after_discard = vec![1, 1, 2, 2, 3, 3, 4, 4, 11, 11, 12, 12, 31];

    assert!(!violates_basic_three_suits_discard(
        &hand_after_discard,
        &[],
        &table,
        0,
        21,
        WIN_RULE_SHENYANG_BASIC
    ));
}

#[test]
fn basic_heng_filter_ignores_chi_tile_plus_hand_pair() {
    let table = table_with_discards(1, Vec::new());
    let melds = vec![test_chi_meld(1)];
    let hand_after_discard = vec![1, 1, 11, 12, 13, 21, 22, 23, 31, 35];

    assert!(violates_basic_heng_discard(
        &hand_after_discard,
        &melds,
        &table,
        0,
        35,
        WIN_RULE_SHENYANG_BASIC
    ));
}

#[test]
fn basic_heng_filter_ignores_short_triplet_like_meld() {
    let melds = vec![WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![1, 1],
        from_position: Some(1),
    }];
    let hand = vec![11, 12, 13, 21, 22, 23, 31, 35];

    assert!(!is_triplet_like_meld(&melds[0]));
    assert!(!has_open_meld(&melds));
    assert!(!has_peng_meld(&melds, 1));
    assert!(!has_triplet_or_dragon_pair(&hand, &melds));
    assert_eq!(piao_threat_level(&melds), 0);
}

#[test]
fn basic_heng_filter_ignores_short_gang_meld() {
    let melds = vec![WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::GANG,
        tiles: vec![1, 1, 1],
        from_position: Some(1),
    }];
    let hand = vec![11, 12, 13, 21, 22, 23, 31, 35];

    assert!(!is_triplet_like_meld(&melds[0]));
    assert!(!has_open_meld(&melds));
    assert!(!has_triplet_or_dragon_pair(&hand, &melds));
    assert_eq!(piao_threat_level(&melds), 0);
}

#[test]
fn open_meld_filter_ignores_malformed_melds() {
    let malformed_peng = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![1, 1],
        from_position: Some(1),
    };
    let malformed_chi = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::CHI,
        tiles: vec![1, 1, 1],
        from_position: Some(1),
    };
    let invalid_tile_peng = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![99, 99, 99],
        from_position: Some(1),
    };

    assert!(!has_open_meld(&[malformed_peng]));
    assert!(!has_open_meld(&[malformed_chi]));
    assert!(!has_open_meld(&[invalid_tile_peng.clone()]));
    assert!(!is_triplet_like_meld(&invalid_tile_peng));
    assert_eq!(piao_threat_level(&[invalid_tile_peng]), 0);
    assert!(has_open_meld(&[test_chi_meld(1)]));
}

#[test]
fn route_requirement_scans_ignore_malformed_meld_tiles() {
    let malformed_terminal = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![1, 1],
        from_position: Some(1),
    };
    let malformed_third_suit = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![21, 21],
        from_position: Some(1),
    };
    let hand = vec![2, 3, 4, 5, 6, 7, 12, 13, 14, 15, 16, 17, 18, 18];

    assert!(!has_terminal_or_honor_with_extra(
        &hand,
        &[malformed_terminal.clone()],
        None
    ));
    assert_eq!(terminal_or_honor_count(&hand, &[malformed_terminal]), 0);
    assert_eq!(
        missing_suits(&hand, &[malformed_third_suit.clone()]),
        vec![2]
    );
    assert_eq!(
        suited_tile_count_for_suit(&hand, &[malformed_third_suit], 2),
        0
    );
}

#[test]
fn route_requirement_scans_ignore_invalid_hand_tiles() {
    let hand = vec![2, 3, 4, 5, 6, 7, 12, 13, 14, 15, 16, 17, 18, 99];

    assert!(!is_honor(99));
    assert_eq!(terminal_or_honor_count(&hand, &[]), 0);
    assert!(!has_terminal_or_honor_with_extra(&hand, &[], None));
}

#[test]
fn visible_tile_counts_ignore_malformed_meld_tiles() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&1).unwrap().melds = vec![
        WsShenyangMahjongMeld {
            kind: ShenyangMahjongMeldKind::PENG,
            tiles: vec![14, 14],
            from_position: Some(0),
        },
        WsShenyangMahjongMeld {
            kind: ShenyangMahjongMeldKind::PENG,
            tiles: vec![99, 99, 99],
            from_position: Some(0),
        },
        test_peng_meld(14),
    ];

    assert_eq!(visible_tile_count(&table, 14), 3);
    assert_eq!(exposed_meld_tile_count(&table, 14), 3);
    assert_eq!(open_meld_tile_count(&table, 14), 3);
    assert_eq!(remaining_tile_count(&[14], &table, 0, 14), 0);
}

#[test]
fn pure_one_suit_plan_ignores_malformed_off_suit_meld() {
    let hand = vec![1, 1, 2, 2, 3, 3, 4, 5, 6, 7, 8, 9, 9];
    let malformed_off_suit = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![11, 11],
        from_position: Some(1),
    };
    let valid_off_suit = test_peng_meld(11);

    assert!(pure_one_suit_plan_score(&hand, &[]) > 0.0);
    assert_eq!(
        pure_one_suit_plan_score(&hand, &[malformed_off_suit]),
        pure_one_suit_plan_score(&hand, &[])
    );
    assert_eq!(pure_one_suit_plan_score(&hand, &[valid_off_suit]), 0.0);
}

#[test]
fn piao_plan_ignores_malformed_chi_meld() {
    let hand = vec![1, 1, 2, 3, 4, 5, 6, 11, 11, 21, 21, 35, 35];
    let malformed_chi = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::CHI,
        tiles: vec![7, 7, 8],
        from_position: Some(1),
    };
    let valid_chi = test_chi_meld(7);

    assert!(piao_plan_score(&hand, &[]) > 0.0);
    assert_eq!(
        piao_plan_score(&hand, &[malformed_chi.clone()]),
        piao_plan_score(&hand, &[])
    );
    assert_eq!(piao_plan_score(&hand, &[valid_chi]), 0.0);
    assert_eq!(
        piao_threat_level(&[
            malformed_chi,
            test_peng_meld(1),
            test_peng_meld(11),
            test_peng_meld(21),
        ]),
        3
    );
}

#[test]
fn basic_heng_heuristic_uses_complete_decomposition_for_fake_triplet() {
    let melds = vec![test_chi_meld(11)];
    let hand = vec![1, 2, 2, 3, 3, 3, 4, 4, 5, 26, 26];

    assert!(is_complete_win(&hand, melds.len()));
    assert!(hand.iter().filter(|tile| **tile == 3).count() >= 3);
    assert!(!has_triplet_in_standard_decomposition(&hand));
    assert!(!has_triplet_or_dragon_pair(&hand, &melds));
}

#[test]
fn broken_closed_defense_uses_basic_rule_instead_of_relaxed_near_ready_shape() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 40;
    let hand = vec![2, 2, 3, 4, 5, 11, 12, 13, 21, 22, 23, 31, 35];

    assert!(
        ready_tile_score(&hand, &[], &table, 0, WIN_RULE_RELAXED) > 0.0
            || one_step_wait_potential(&hand, &[], &table, 0, WIN_RULE_RELAXED) > 0.0
    );
    assert_eq!(
        ready_tile_score(&hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC),
        0.0
    );
    assert_eq!(
        one_step_wait_potential(&hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC),
        0.0
    );
    assert!(should_open_broken_closed_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
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
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
}

#[test]
fn broken_closed_defense_opens_mid_severely_broken_hand() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 52;
    let hand = vec![2, 5, 8, 11, 14, 17, 19, 31, 31, 33, 35, 36, 37];

    assert!(should_open_broken_closed_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
}

#[test]
fn unrecoverable_basic_rule_counts_dead_heng_requirement() {
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 14, 15, 21, 22];
    let table = table_with_discards(1, dead_basic_heng_discards(&hand));

    assert!(missing_suits(&hand, &[]).is_empty());
    assert!(has_terminal_or_honor_with_extra(&hand, &[], None));
    assert!(!has_triplet_or_dragon_pair(&hand, &[]));
    assert!(!can_recover_basic_heng(&hand, &[], &table));
    assert_eq!(
        unrecoverable_basic_rule_requirement_count(&hand, &[], &table),
        1
    );
}

#[test]
fn recoverable_basic_heng_counts_live_dragon_pair_without_hand_seed() {
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 14, 15, 21, 22];
    let mut discards = SHENYANG_MAHJONG_TILE_KINDS
        .into_iter()
        .flat_map(|tile| {
            let visible = if tile == 35 {
                2
            } else if is_dragon(tile) {
                3
            } else {
                2
            };
            std::iter::repeat_n(tile, visible)
        })
        .collect::<Vec<_>>();
    sort_tiles(&mut discards);
    let table = table_with_discards(1, discards);

    assert!(!has_triplet_or_dragon_pair(&hand, &[]));
    assert_eq!(remaining_tile_count(&hand, &table, 0, 35), 2);
    assert!(can_recover_basic_heng(&hand, &[], &table));
    assert_eq!(
        unrecoverable_basic_rule_requirement_count(&hand, &[], &table),
        0
    );
}

#[test]
fn broken_closed_defense_opens_mid_when_heng_is_unrecoverable() {
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 14, 15, 21, 22];
    let mut table = table_with_discards(1, dead_basic_heng_discards(&hand));
    table.wall_count = 52;

    assert!(hand_power(&hand) >= 14.0);
    assert_eq!(
        ready_tile_score(&hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC),
        0.0
    );
    assert_eq!(
        one_step_wait_potential(&hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC),
        0.0
    );
    assert_eq!(
        unrecoverable_basic_rule_requirement_count(&hand, &[], &table),
        1
    );
    assert!(should_open_broken_closed_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
}

#[test]
fn broken_closed_defense_waits_mid_when_basic_requirements_are_intact() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 52;
    let hand = vec![1, 1, 1, 2, 3, 4, 11, 12, 13, 21, 22, 23, 35];

    assert!(!should_open_broken_closed_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
}

#[test]
fn relaxed_closed_defense_ignores_basic_terminal_requirement() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 40;
    let hand = vec![2, 2, 2, 5, 5, 5, 12, 12, 12, 14, 17, 23, 27];

    assert_eq!(
        ready_tile_score(&hand, &[], &table, 0, WIN_RULE_RELAXED),
        0.0
    );
    assert_eq!(
        one_step_wait_potential(&hand, &[], &table, 0, WIN_RULE_RELAXED),
        0.0
    );
    assert!(hand_power(&hand) >= 18.0);
    assert!(should_open_broken_closed_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert!(!should_open_broken_closed_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_RELAXED
    ));
}

#[test]
fn capped_discard_does_not_chase_pure_one_suit_when_three_suits_remain() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(1);
    let hand = vec![1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 7, 8, 11, 21];

    assert!(!matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(11 | 21)
    ));
}

#[test]
fn capped_pure_one_suit_route_can_discard_last_honor_when_suits_are_missing() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(1);
    let hand = vec![2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 8, 8, 12, 31];

    assert!(
        pure_one_suit_plan_score_for_context(&remove_n_tiles(&hand, 31, 1), &[], &table, 0) > 0.0
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(31)
    );
}

#[test]
fn pure_one_suit_rule_progress_does_not_require_opening() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 1, 2, 2, 3, 3, 4, 5, 6, 7, 8, 9];
    let pure_score = pure_one_suit_plan_score_for_context(&hand, &[], &table, 0);

    assert!(pure_score > 0.0);
    assert!(!has_open_meld(&[]));
    assert_eq!(
        shenyang_rule_progress_score(&hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC),
        pure_score
    );
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
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(31)
    );
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
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert!(!violates_basic_three_suits_discard(
        &hand_after_discard,
        &[],
        &table,
        0,
        21,
        WIN_RULE_SHENYANG_BASIC
    ));
}

#[test]
fn uncapped_room_keeps_piao_plan_biases() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 13, 21, 21, 22, 31, 32];

    assert!(piao_plan_score_for_context(&hand, &[], &table, 0) >= 20.0);
    assert!(piao_discard_bias(&hand, 1, &[], &table, 0, WIN_RULE_SHENYANG_BASIC) < 0.0);
    assert!(early_piao_candidate_discard_bias(&hand, 1, &[], &table, 0) < 0.0);
}

#[test]
fn dealer_ignores_marginal_piao_discard_bias_for_speed() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 13, 21, 21, 22, 31, 32];

    assert!(piao_plan_score(&hand, &[]) >= 20.0);
    assert!(piao_plan_score_for_context(&hand, &[], &table, 0) < 20.0);
    assert_eq!(
        piao_discard_bias(&hand, 1, &[], &table, 0, WIN_RULE_SHENYANG_BASIC),
        0.0
    );
    assert_eq!(
        early_piao_candidate_discard_bias(&hand, 1, &[], &table, 0),
        0.0
    );
}

#[test]
fn one_fan_capped_room_disables_piao_plan_biases() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(1);
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 13, 21, 21, 22, 31, 32];

    assert!(piao_plan_score(&hand, &[]) >= 20.0);
    assert_eq!(piao_plan_score_for_context(&hand, &[], &table, 0), 0.0);
    assert_eq!(
        piao_discard_bias(&hand, 1, &[], &table, 0, WIN_RULE_SHENYANG_BASIC),
        0.0
    );
    assert_eq!(
        early_piao_candidate_discard_bias(&hand, 1, &[], &table, 0),
        0.0
    );
}

#[test]
fn piao_plan_counts_open_triplet_with_two_pairs_as_route() {
    let hand = vec![11, 11, 12, 13, 14, 21, 21, 23, 24, 31];
    let melds = vec![test_peng_meld(1)];

    assert!(piao_plan_score(&hand, &melds) >= 20.0);
}

#[test]
fn piao_context_requires_terminal_or_honor() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![2, 2, 3, 3, 12, 12, 13, 13, 22, 22, 23, 23, 24];

    assert!(piao_plan_score(&hand, &[]) >= 20.0);
    assert_eq!(piao_plan_score_for_context(&hand, &[], &table, 0), 0.0);
}

#[test]
fn piao_context_requires_three_suits() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 2, 2, 3, 3, 4, 4, 11, 11, 12, 13, 31];

    assert!(piao_plan_score(&hand, &[]) >= 20.0);
    assert_eq!(piao_plan_score_for_context(&hand, &[], &table, 0), 0.0);
}

#[test]
fn claim_peng_passes_raw_piao_shape_without_terminal_or_honor() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(2)];
    table.claim_window = Some(AiClaimView {
        tile: 13,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![12, 13, 13, 14, 15, 22, 22, 23, 25, 25];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();

    assert!(piao_plan_score(&hand, melds) >= 32.0);
    assert_eq!(piao_plan_score_for_context(&hand, melds, &table, 0), 0.0);
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_ready_hand_pengs_dragon_when_it_keeps_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![11, 12, 13, 21, 22, 23, 24, 25, 35, 35];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();
    let current_ready_score = ready_tile_score(&hand, melds, &table, 0, WIN_RULE_SHENYANG_BASIC);

    assert!(current_ready_score > 0.0);
    assert!(!is_complete_win_with_melds(
        &[11, 12, 13, 21, 22, 23, 24, 25, 35, 35, 35],
        melds,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert!(should_claim_ready_dragon_peng_from_discard(
        &hand,
        melds,
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC,
        35,
        1,
        current_ready_score
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_ready_hand_passes_dragon_peng_when_visible_fan_is_capped() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(1);
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![11, 12, 13, 21, 22, 23, 24, 25, 35, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn one_fan_capped_room_does_not_lock_five_pairs_when_basic_route_is_viable() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(1);
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 22, 31, 35, 35];

    assert!(has_basic_normal_route_foundation(
        &hand,
        &[],
        WIN_RULE_SHENYANG_BASIC
    ));
    assert!(!should_lock_seven_pairs_plan(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
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
fn claim_chi_can_fill_missing_third_suit() {
    let mut table = table_with_discards(3, Vec::new());
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 22,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 23, 31, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Chi {
            consume_tiles: vec![21, 23]
        })
    );
}

#[test]
fn claim_chi_takes_mid_round_when_it_reaches_ready() {
    let mut table = table_with_discards(3, Vec::new());
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 3,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 4, 5, 6, 7, 8, 9, 11, 12, 13, 31, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Chi {
            consume_tiles: vec![1, 2]
        })
    );
}

#[test]
fn claim_chi_takes_mid_round_ready_without_defensive_open() {
    let mut table = table_with_discards(3, Vec::new());
    table.wall_count = 52;
    table.claim_window = Some(AiClaimView {
        tile: 3,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 4, 5, 6, 7, 8, 9, 11, 12, 13, 31, 35];

    assert!(is_mid_opening_round(&table));
    assert!(!should_claim_chi_to_open_broken_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_RELAXED
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Chi {
            consume_tiles: vec![1, 2]
        })
    );
}

#[test]
fn claim_chi_can_use_claim_tile_as_low_edge() {
    let mut table = table_with_discards(3, Vec::new());
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 2, 3, 5, 8, 11, 14, 17, 21, 24, 31, 32, 33];

    assert!(should_claim_chi_to_open_broken_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_RELAXED
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Chi {
            consume_tiles: vec![2, 3]
        })
    );
}

#[test]
fn claim_chi_passes_late_ready_hand() {
    let mut table = table_with_discards(3, Vec::new());
    table.wall_count = 36;
    table.claim_window = Some(AiClaimView {
        tile: 3,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 31, 31];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_chi_preserves_pure_one_suit_plan_from_off_suit_chi() {
    let mut table = table_with_discards(3, Vec::new());
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 13,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 11, 12, 35];

    assert!(pure_one_suit_plan_score_for_context(&hand, &[], &table, 0) > 0.0);
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_chi_opens_late_broken_hand_for_defense() {
    let mut table = table_with_discards(3, Vec::new());
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 3,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 5, 8, 11, 14, 17, 21, 24, 31, 32, 33];

    assert!(should_claim_chi_to_open_broken_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_RELAXED
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Chi {
            consume_tiles: vec![1, 2]
        })
    );
}

#[test]
fn claim_chi_opens_mid_broken_hand_for_defense() {
    let mut table = table_with_discards(3, Vec::new());
    table.wall_count = 52;
    table.claim_window = Some(AiClaimView {
        tile: 3,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 5, 8, 11, 14, 17, 21, 24, 31, 32, 33];

    assert!(should_claim_chi_to_open_broken_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_RELAXED
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Chi {
            consume_tiles: vec![1, 2]
        })
    );
}

#[test]
fn claim_chi_does_not_rush_opening_closed_basic_hand_early() {
    let mut table = table_with_discards(3, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 3,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_chi_passes_early_even_when_it_can_fill_missing_third_suit() {
    let mut table = table_with_discards(3, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 22,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 23, 31, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_chi_passes_for_shenyang_basic_rule_even_late() {
    let mut table = table_with_discards(3, Vec::new());
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 3,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_chi_passes_mid_round_when_it_does_not_make_ready_or_defensive_open() {
    let mut table = table_with_discards(3, Vec::new());
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 3,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 5, 5, 5, 9, 9, 9, 11, 14, 17, 21, 24];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_chi_passes_when_five_pairs_missing_suit_can_chase_seven_pairs() {
    let mut table = table_with_discards(3, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 23,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 3, 3, 11, 11, 21, 21, 22, 31, 32];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_chi_passes_when_piao_plan_is_stronger() {
    let mut table = table_with_discards(3, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![1, 1, 1],
        from_position: Some(2),
    }];
    table.claim_window = Some(AiClaimView {
        tile: 22,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![11, 11, 21, 23, 31, 31, 35, 35, 36, 37];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_chi_passes_for_open_triplet_two_pair_piao_route_even_when_chi_reaches_ready() {
    let mut table = table_with_discards(3, Vec::new());
    table.wall_count = 40;
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    table.claim_window = Some(AiClaimView {
        tile: 22,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![11, 11, 12, 13, 14, 21, 21, 23, 24, 31];

    assert!(should_preserve_piao_plan_for_chi(
        &hand,
        table.seats.get(&0).unwrap().melds.as_slice(),
        &table,
        0
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_chi_passes_for_four_pair_piao_candidate_in_relaxed_rule() {
    let mut table = table_with_discards(3, Vec::new());
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 7,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 3, 3, 4, 4, 5, 6, 11, 12, 21];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn piao_chi_preservation_uses_dealer_and_cap_context() {
    let table = table_with_discards(3, Vec::new());
    let hand = vec![1, 1, 2, 2, 3, 3, 4, 4, 5, 6, 11, 12, 21];

    assert!(should_preserve_piao_plan_for_chi(&hand, &[], &table, 0));

    let mut dealer_table = table.clone();
    dealer_table.dealer_position = 0;
    assert!(!should_preserve_piao_plan_for_chi(
        &hand,
        &[],
        &dealer_table,
        0
    ));

    let mut capped_table = table.clone();
    capped_table.max_fan = Some(1);
    assert!(!should_preserve_piao_plan_for_chi(
        &hand,
        &[],
        &capped_table,
        0
    ));
}

#[test]
fn claim_chi_passes_for_three_pair_piao_candidate_even_when_chi_reaches_ready() {
    let mut table = table_with_discards(3, Vec::new());
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 27,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 5, 5, 11, 12, 13, 22, 23, 24, 24, 28, 29];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_gang_beats_peng_when_not_winning() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 35, 35, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Gang)
    );
}

#[test]
fn claim_gang_takes_dragon_gang_to_open_basic_hand_before_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 7, 11, 12, 14, 21, 35, 35, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Gang)
    );
}

#[test]
fn one_fan_capped_claim_gang_penges_dragon_for_speed_before_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(1);
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 7, 11, 12, 14, 21, 35, 35, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_gang_delays_open_piao_plain_gang_until_ready_and_pengs() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    table.claim_window = Some(AiClaimView {
        tile: 21,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![11, 11, 21, 21, 21, 31, 31, 35, 35, 36];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_gang_delays_open_plain_gang_when_not_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_chi_meld(1)];
    table.claim_window = Some(AiClaimView {
        tile: 9,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![4, 5, 6, 9, 9, 9, 11, 12, 14, 21];

    assert_ne!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Gang)
    );
}

#[test]
fn claim_gang_opens_closed_plain_basic_hand_before_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 3,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![3, 3, 3, 4, 5, 7, 8, 11, 12, 14, 21, 22, 31];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Gang)
    );
}

#[test]
fn claim_gang_penges_closed_early_piao_candidate() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 1, 4, 5, 6, 11, 11, 12, 13, 21, 21, 22];

    assert!(is_closed_early_piao_candidate(&hand, &[], &table, 0));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_gang_opens_broken_closed_hand_late_for_defense() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 2,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 2, 2, 4, 7, 12, 14, 17, 31, 32, 33, 34, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Gang)
    );
}

#[test]
fn dealer_claim_gang_opens_broken_closed_hand_for_speed() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 2,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 2, 2, 4, 7, 12, 14, 17, 31, 32, 33, 34, 35];

    assert!(should_open_broken_closed_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Gang)
    );
}

#[test]
fn claim_gang_passes_final_unready_broken_hand_for_defense() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.claim_window = Some(AiClaimView {
        tile: 2,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 2, 2, 4, 7, 12, 14, 17, 31, 32, 33, 34, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_gang_opens_broken_closed_hand_for_defense_in_relaxed_rule() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 31,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 5, 8, 12, 14, 17, 21, 24, 27, 31, 31, 31, 34];

    assert!(should_open_broken_closed_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_RELAXED
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Gang)
    );
}

#[test]
fn claim_gang_opens_mid_missing_suit_no_terminal_hand_for_defense() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 52;
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 3, 5, 5, 5, 7, 8, 12, 14, 15, 16, 17, 18];

    assert!(should_open_broken_closed_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Gang)
    );
}

#[test]
fn claim_gang_passes_when_it_breaks_locked_pure_one_suit_plan() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 11,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 11, 11, 11];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_gang_passes_dragon_when_pure_one_suit_plan_starts_at_eight_tiles() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 11, 21, 35, 35, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_gang_passes_closed_pure_one_suit_plan_before_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 11];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_gang_takes_ready_main_suit_pure_one_suit_when_not_capped() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 9];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Gang)
    );
}

#[test]
fn claim_gang_passes_ready_pure_one_suit_when_visible_fan_capped() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(4);
    table.seats.get_mut(&0).unwrap().melds = vec![test_chi_meld(2)];
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 1, 5, 6, 7, 8, 8, 9, 9];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_gang_passes_capped_closed_pure_one_suit_wait() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(4);
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 9];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_gang_passes_when_it_breaks_locked_seven_pairs_plan() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 31];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_gang_preserves_five_pairs_even_for_dragon_gang() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 22, 35, 35, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_gang_skips_plain_gang_when_ready_fan_already_capped() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(3);
    table.seats.get_mut(&0).unwrap().melds = vec![test_gang_meld(35)];
    table.claim_window = Some(AiClaimView {
        tile: 9,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 9, 9, 9, 11, 12, 13, 21];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_gang_takes_open_plain_gang_when_it_reaches_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_chi_meld(1)];
    table.claim_window = Some(AiClaimView {
        tile: 9,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![4, 5, 6, 9, 9, 9, 11, 12, 13, 21];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Gang)
    );
}

#[test]
fn claim_gang_penges_to_preserve_four_gui_yi_when_peng_stays_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    table.seats.get_mut(&0).unwrap().melds = vec![test_chi_meld(11)];
    table.claim_window = Some(AiClaimView {
        tile: 4,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 2, 2, 4, 4, 4, 5, 21, 21, 21];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_gang_takes_late_ready_dragon_gang_when_it_keeps_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 36;
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 11, 12, 13, 21, 35, 35, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Gang)
    );
}

#[test]
fn claim_gang_passes_late_ready_hand_when_gang_breaks_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 36;
    table.claim_window = Some(AiClaimView {
        tile: 6,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 4, 6, 6, 6, 7, 8, 13, 14, 15, 23, 24, 25];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_hu_accepts_open_meld_remainder() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    table.seats.get_mut(&0).unwrap().melds = vec![
        share_type_public::games::shenyang_mahjong::WsShenyangMahjongMeld {
            kind: share_type_public::games::shenyang_mahjong::ShenyangMahjongMeldKind::PENG,
            tiles: vec![1, 1, 1],
            from_position: Some(2),
        },
    ];
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Hu)
    );
}

#[test]
fn claim_hu_accepts_seven_pairs() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Hu)
    );
}

#[test]
fn claim_hu_beats_other_claims() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Hu)
    );
}

#[test]
fn claim_hu_still_wins_during_final_defense() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Hu)
    );
}

#[test]
fn claim_peng_allows_dragon_when_missing_suit_can_still_be_recovered() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 31, 35, 35, 36];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
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
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_takes_open_main_suit_pure_one_suit_when_it_reaches_ready() {
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
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
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
fn claim_peng_opens_broken_closed_hand_late_for_defense() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 2,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 2, 4, 7, 12, 14, 17, 31, 32, 33, 34, 35, 36];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_passes_final_unready_broken_hand_for_defense() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.claim_window = Some(AiClaimView {
        tile: 2,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 2, 4, 7, 12, 14, 17, 31, 32, 33, 34, 35, 36];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_opens_mid_severely_broken_closed_hand_for_defense() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 52;
    table.claim_window = Some(AiClaimView {
        tile: 31,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 5, 8, 11, 14, 17, 19, 31, 31, 33, 35, 36, 37];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_opens_missing_suit_basic_hand_despite_relaxed_near_ready_shape() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 52;
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 3, 4, 5, 6, 11, 12, 13, 14, 15, 31];

    assert!(
        one_step_wait_potential(&hand, &[], &table, 0, WIN_RULE_RELAXED) > 0.0,
        "the relaxed shape is close enough that it used to block defensive opening"
    );
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_opens_broken_closed_hand_for_defense_in_relaxed_rule() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 31,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 5, 8, 12, 14, 17, 21, 24, 27, 31, 31, 33, 34];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Peng)
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
fn dealer_claim_peng_can_ignore_early_eight_tile_pure_one_suit_plan() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 11, 21, 35, 35, 36];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_passes_when_five_pairs_missing_suit_can_chase_seven_pairs() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 21,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 12, 22, 31, 32];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_preserves_quad_as_two_pairs_seven_pairs_route() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 2,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 1, 1, 2, 2, 3, 3, 11, 11, 12, 31, 35];

    assert_eq!(pair_count(&hand), 5);
    assert!(should_lock_seven_pairs_plan(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
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
fn claim_peng_passes_when_it_breaks_seven_pairs_shape() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 6,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 31];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_passes_when_missing_suit_is_unrecoverable_even_for_dragon() {
    let dead_bamboo = (21..=29)
        .flat_map(|tile| std::iter::repeat_n(tile, 4))
        .collect::<Vec<_>>();
    let mut table = table_with_discards(1, dead_bamboo);
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 31, 35, 35, 36];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_opens_late_broken_missing_suit_hand_even_for_dragon() {
    let dead_bamboo = (21..=29)
        .flat_map(|tile| std::iter::repeat_n(tile, 4))
        .collect::<Vec<_>>();
    let mut table = table_with_discards(1, dead_bamboo);
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 4, 6, 8, 11, 13, 16, 19, 31, 35, 35, 36];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_passes_when_terminal_or_honor_is_unrecoverable_for_basic() {
    let mut table = table_with_discards(1, dead_terminal_or_honor_discards());
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 4, 5, 5, 12, 13, 14, 15, 16, 22, 23, 24, 25];

    assert!(claim_leaves_unrecoverable_terminal_or_honor(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC,
        ShenyangMahjongMeldKind::PENG,
        5,
        1
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_gang_passes_when_terminal_or_honor_is_unrecoverable_for_basic() {
    let mut table = table_with_discards(1, dead_terminal_or_honor_discards());
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 4, 5, 5, 5, 12, 13, 14, 15, 16, 22, 23, 24];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_opens_late_broken_no_terminal_hand_for_defense() {
    let mut table = table_with_discards(1, dead_terminal_or_honor_discards());
    table.wall_count = 40;
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 4, 5, 5, 7, 12, 14, 16, 18, 22, 24, 26, 28];

    assert!(should_open_broken_closed_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_opens_mid_unrecoverable_no_terminal_hand_for_defense() {
    let mut table = table_with_discards(1, dead_terminal_or_honor_discards());
    table.wall_count = 52;
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![2, 3, 4, 5, 5, 6, 7, 12, 13, 14, 22, 23, 24];

    assert_eq!(
        unrecoverable_basic_rule_requirement_count(&hand, &[], &table),
        1
    );
    assert!(should_open_broken_closed_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn broken_closed_defense_waits_mid_recoverable_no_terminal_hand() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 52;
    let hand = vec![2, 2, 2, 5, 5, 6, 7, 12, 13, 14, 22, 23, 24];

    assert_eq!(
        unrecoverable_basic_rule_requirement_count(&hand, &[], &table),
        0
    );
    assert!(!should_open_broken_closed_hand_for_defense(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
}

#[test]
fn claim_peng_takes_late_ready_dragon_when_it_keeps_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 36;
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 35, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_takes_ready_dragon_before_late_round_when_it_keeps_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 35, 35];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();

    assert!(ready_tile_score(&hand, melds, &table, 0, WIN_RULE_SHENYANG_BASIC) > 0.0);
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_preserves_five_pairs_even_with_three_suits() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 21,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 31, 32];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_preserves_pinghu_sequence_when_open_and_heng_is_ready() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![4, 5, 5, 6, 11, 12, 21, 22, 35, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_pursues_piao_plan_after_open_triplet() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![1, 1, 1],
        from_position: Some(2),
    }];
    table.claim_window = Some(AiClaimView {
        tile: 21,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![11, 11, 21, 21, 31, 31, 35, 35, 36, 37];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_still_opens_closed_basic_hand_despite_sequence_shape() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![4, 5, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_still_preserves_locked_seven_pairs_over_dragon_pair() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 12, 35, 35, 36];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_takes_dragon_pair_for_open_and_fan() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 4, 5, 7, 9, 11, 12, 14, 17, 21, 35, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_takes_four_pair_three_suit_piao_start() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 21,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 13, 21, 21, 22, 31, 32];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn relaxed_claim_peng_takes_closed_early_piao_candidate_over_sequence_shape() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 4, 5, 5, 6, 11, 12, 13, 21, 21, 35, 35];

    assert!(tile_is_middle_of_sequence(&hand, 5));
    assert!(should_claim_peng_for_closed_early_piao_candidate(
        &hand,
        &[],
        &table,
        0,
        5,
        1
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_RELAXED),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_takes_fourth_piao_meld_to_set_up_shou_ba_yi() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds =
        vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![35, 35, 36, 37];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_takes_three_pair_three_suit_piao_start() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 4, 5, 6, 11, 11, 12, 13, 21, 21, 22, 23];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_takes_basic_heng_and_opening_when_no_heng() {
    let mut table = table_with_discards(1, Vec::new());
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 4, 5, 5, 7, 8, 11, 13, 15, 21, 24, 31];

    assert!(!has_open_meld(
        table.seats.get(&0).unwrap().melds.as_slice()
    ));
    assert!(!has_triplet_or_dragon_pair(&hand, &[]));
    assert_eq!(
        ready_tile_score(&hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC),
        0.0
    );
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_opens_mid_basic_hand_with_existing_heng() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 52;
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 1, 2, 3, 5, 5, 11, 12, 13, 21, 22, 23];

    assert!(has_triplet_or_dragon_pair(&hand, &[]));
    assert!(!has_open_meld(
        table.seats.get(&0).unwrap().melds.as_slice()
    ));
    assert_eq!(
        ready_tile_score(&hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC),
        0.0
    );
    assert!(!claim_leaves_unrecoverable_basic_requirement(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC,
        ShenyangMahjongMeldKind::PENG,
        5,
        1
    ));
    assert!(should_claim_peng_to_open_mid_basic_hand(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC,
        5,
        1
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn claim_peng_preserves_closed_mid_basic_sequence_when_heng_exists() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 52;
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 1, 4, 5, 5, 6, 11, 12, 13, 21, 22, 23];

    assert!(has_triplet_or_dragon_pair(&hand, &[]));
    assert!(has_triplet_like_group(&hand, &[]));
    assert!(tile_is_middle_of_sequence(&hand, 5));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn claim_peng_opens_later_closed_basic_hand_over_sequence_preservation() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 42;
    table.claim_window = Some(AiClaimView {
        tile: 5,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 1, 4, 5, 5, 6, 11, 12, 13, 21, 22, 23];

    assert!(has_triplet_or_dragon_pair(&hand, &[]));
    assert!(tile_is_middle_of_sequence(&hand, 5));
    assert!(should_claim_peng_to_open_mid_basic_hand(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC,
        5,
        1
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn closed_opponent_threat_does_not_penalize_public_safe_tile() {
    let mut table = table_with_discards(1, vec![31]);
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().hand_count = 13;

    assert_eq!(closed_opponent_threat_discard_bias(&table, 0, 31, 1), 0.0);
    assert!(closed_opponent_threat_discard_bias(&table, 0, 32, 1) < 0.0);
}

#[test]
fn closed_opponent_threat_discounts_exposed_meld_tiles() {
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

    let exposed_terminal_bias = closed_opponent_threat_discard_bias(&table, 0, 9, 1);
    let cold_honor_bias = closed_opponent_threat_discard_bias(&table, 0, 31, 1);

    assert!(exposed_terminal_bias < 0.0);
    assert!(exposed_terminal_bias > cold_honor_bias);
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

#[test]
fn late_defense_can_follow_exposed_terminal_over_live_wind() {
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
    let hand = vec![2, 4, 6, 8, 9, 12, 14, 16, 18, 22, 24, 26, 28, 31];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(9)
    );
}

#[test]
fn late_defense_avoids_breaking_cold_terminal_pair_against_closed_opponent() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().hand_count = 13;
    let hand = vec![2, 4, 6, 8, 9, 9, 12, 14, 16, 18, 19, 22, 24, 26];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(19)
    );
}

#[test]
fn dealer_claim_chi_passes_for_shenyang_basic_rule() {
    let mut table = table_with_discards(3, Vec::new());
    table.dealer_position = 0;
    table.claim_window = Some(AiClaimView {
        tile: 3,
        from_position: 3,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 2, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn dealer_claim_peng_does_not_chase_early_pure_one_suit_plan() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    table.claim_window = Some(AiClaimView {
        tile: 1,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 3, 4, 5, 6, 7, 8, 11, 12, 31, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn dealer_claim_peng_preserves_five_pairs_when_basic_hand_is_missing_suit() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    table.claim_window = Some(AiClaimView {
        tile: 11,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 12, 31, 32, 33];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn dealer_claim_peng_uses_dragon_pair_for_speed_when_basic_route_is_viable() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 21, 21, 22, 31, 35, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn one_fan_capped_claim_peng_uses_dragon_pair_for_speed_over_five_pairs() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(1);
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 22, 31, 35, 35];

    assert!(!should_lock_seven_pairs_plan(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Peng)
    );
}

#[test]
fn dealer_does_not_lock_five_pairs_when_basic_route_is_viable() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 21, 21, 22, 31, 35, 35, 36];

    assert!(!should_lock_seven_pairs_plan(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
}

#[test]
fn dealer_claim_peng_preserves_six_pairs_seven_pairs_plan() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    table.claim_window = Some(AiClaimView {
        tile: 35,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 31, 35, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn dealer_claim_peng_preserves_four_pairs_when_basic_hand_is_missing_suit() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    table.claim_window = Some(AiClaimView {
        tile: 11,
        from_position: 1,
        eligible_positions: vec![0],
    });
    let claim = table.claim_window.clone().unwrap();
    let hand = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 13, 14, 31, 35];

    assert_eq!(
        choose_claim_from_view(&hand, &claim, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(AiClaimChoice::Pass)
    );
}

#[test]
fn dealer_discard_does_not_chase_early_pure_one_suit_by_breaking_second_suit() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 11, 12, 31, 35];

    assert!(!matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(11 | 12)
    ));
}

#[test]
fn dealer_does_not_start_pure_one_suit_plan_at_eight_main_suit_tiles() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 11, 12, 21, 22, 31, 35];

    assert!(!matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(11 | 12 | 21 | 22)
    ));
}

#[test]
fn dealer_can_chase_overwhelming_pure_one_suit_shape() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    let hand = vec![1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 11, 35];

    assert!(matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(11 | 35)
    ));
}

#[test]
fn dealer_can_start_overwhelming_pure_one_suit_by_clearing_third_blocker() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    let hand = vec![1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 7, 11, 12, 13];

    assert!(matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(11 | 12 | 13)
    ));
}

#[test]
fn dealer_prefers_wider_wait_over_single_wait_fan() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
    let hand = vec![2, 2, 4, 5, 7, 11, 12, 13, 21, 22, 23];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(7)
    );
}

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
fn discard_after_four_piao_melds_keeps_live_single_wait() {
    let mut table = table_with_discards(1, vec![36, 36, 36]);
    table.seats.get_mut(&0).unwrap().melds = vec![
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(21),
        test_peng_meld(31),
    ];
    let hand = vec![36, 37];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(36)
    );
}

#[test]
fn discard_after_four_piao_melds_rejects_dead_exposed_wind_wait() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(21),
        test_peng_meld(31),
    ];
    let hand = vec![5, 31];

    assert_eq!(remaining_tile_count(&[31], &table, 0, 31), 0);
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(31)
    );
}

#[test]
fn dealer_four_piao_melds_prefers_live_middle_over_low_live_wind_wait() {
    let mut table = table_with_discards(1, vec![31, 31]);
    table.dealer_position = 0;
    table.seats.get_mut(&0).unwrap().melds = vec![
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(21),
        test_peng_meld(32),
    ];
    let hand = vec![5, 31];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();

    assert!(
        piao_single_wait_tile_score(5, &[5], melds, &table, 0, WIN_RULE_SHENYANG_BASIC)
            > piao_single_wait_tile_score(31, &[31], melds, &table, 0, WIN_RULE_SHENYANG_BASIC)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(31)
    );
}

#[test]
fn mid_round_non_dealer_piao_single_wait_can_chase_wind_fan() {
    let mut table = table_with_discards(1, vec![31]);
    table.wall_count = 32;
    table.seats.get_mut(&0).unwrap().melds = vec![
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(21),
        test_peng_meld(35),
    ];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();

    assert_eq!(remaining_tile_count(&[31], &table, 0, 31), 2);
    assert_eq!(remaining_tile_count(&[5], &table, 0, 5), 3);
    assert!(
        piao_single_wait_tile_score(31, &[31], melds, &table, 0, WIN_RULE_SHENYANG_BASIC)
            > piao_single_wait_tile_score(5, &[5], melds, &table, 0, WIN_RULE_SHENYANG_BASIC)
    );
}

#[test]
fn dealer_piao_single_wait_still_prefers_wider_middle_wait() {
    let mut table = table_with_discards(1, vec![31]);
    table.dealer_position = 0;
    table.wall_count = 32;
    table.seats.get_mut(&0).unwrap().melds = vec![
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(21),
        test_peng_meld(35),
    ];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();

    assert!(
        piao_single_wait_tile_score(5, &[5], melds, &table, 0, WIN_RULE_SHENYANG_BASIC)
            > piao_single_wait_tile_score(31, &[31], melds, &table, 0, WIN_RULE_SHENYANG_BASIC)
    );
}

#[test]
fn capped_four_piao_melds_prefers_wider_wait_over_honor_shape() {
    let mut table = table_with_discards(1, vec![31]);
    table.max_fan = Some(4);
    table.seats.get_mut(&0).unwrap().melds = vec![
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(21),
        test_peng_meld(32),
    ];
    let hand = vec![5, 31];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();

    assert!(
        piao_single_wait_tile_score(5, &[5], melds, &table, 0, WIN_RULE_SHENYANG_BASIC)
            > piao_single_wait_tile_score(31, &[31], melds, &table, 0, WIN_RULE_SHENYANG_BASIC)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(31)
    );
}

#[test]
fn piao_single_wait_discard_avoids_pure_one_suit_threat_tile() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 32;
    table.seats.get_mut(&0).unwrap().melds = vec![
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(21),
        test_peng_meld(32),
    ];
    table.seats.get_mut(&1).unwrap().melds = vec![test_chi_meld(2), test_peng_meld(7)];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();
    let hand = vec![5, 31];

    assert!(
        piao_single_wait_tile_score(31, &[31], melds, &table, 0, WIN_RULE_SHENYANG_BASIC)
            > piao_single_wait_tile_score(5, &[5], melds, &table, 0, WIN_RULE_SHENYANG_BASIC)
    );
    assert!(
        pure_one_suit_threat_discard_bias(&table, 0, 5, 1)
            < pure_one_suit_threat_discard_bias(&table, 0, 31, 1)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(31)
    );
}

#[test]
fn discard_avoids_live_pair_against_piao_threat() {
    let mut table = table_with_discards(1, vec![31]);
    table.wall_count = 32;
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];
    let hand = vec![2, 3, 4, 5, 5, 6, 7, 11, 12, 13, 21, 22, 23, 31];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(31)
    );
}

#[test]
fn discard_follows_public_tile_over_live_pair_against_piao_threat() {
    let mut table = table_with_discards(1, vec![14]);
    table.wall_count = 32;
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];
    let hand = vec![2, 3, 4, 5, 5, 6, 7, 11, 12, 13, 14, 21, 22, 23];

    assert!(
        opponent_threat_discard_bias(&table, 0, 5, 2)
            < opponent_threat_discard_bias(&table, 0, 14, 1)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(14)
    );
}

#[test]
fn discard_can_pursue_pure_one_suit_when_shape_is_strong() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 7, 8, 9, 11];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(11)
    );
}

#[test]
fn discard_clears_honor_when_early_pure_one_suit_plan_is_available() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 11, 12, 31, 35];

    assert!(matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(31 | 35)
    ));
}

#[test]
fn discard_clears_honor_before_off_suit_singleton_for_pure_one_suit_plan() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 11, 31, 35, 36];

    assert!(matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(31 | 35 | 36)
    ));
}

#[test]
fn discard_clears_last_honor_for_pure_one_suit_without_terminal_need() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 8, 8, 12, 31];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(31)
    );
}

#[test]
fn non_dealer_relaxed_pure_one_suit_plan_can_break_three_suits() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 1;
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 11, 12, 13, 21];
    let after_discard = remove_n_tiles(&hand, 21, 1);

    assert!(pure_one_suit_plan_score_for_context(&after_discard, &[], &table, 0) > 0.0);
    assert_eq!(
        three_suits_discard_bias(&after_discard, &[], &table, 0, 21, WIN_RULE_RELAXED),
        0.0
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(21)
    );
}

#[test]
fn discard_keeps_pairs_for_basic_seven_pairs_plan_when_missing_suit() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 12, 31, 35, 36, 37];

    let discard = choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC);

    assert!(matches!(discard, Some(31 | 35 | 36 | 37)));
}

#[test]
fn discard_keeps_four_pairs_for_basic_seven_pairs_when_missing_suit() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 13, 14, 15, 31, 35];

    assert!(!matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(1 | 2 | 3 | 11)
    ));
}

#[test]
fn discard_keeps_quad_pairs_for_basic_seven_pairs_when_missing_suit() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 1, 1, 2, 2, 3, 3, 11, 11, 12, 31, 35, 36];

    assert_eq!(pair_count(&hand), 5);
    assert!(should_lock_seven_pairs_plan(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert!(!matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(1 | 2 | 3 | 11)
    ));
}

#[test]
fn dealer_discard_keeps_four_pairs_when_basic_hand_is_missing_suit() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    let hand = vec![1, 1, 2, 2, 3, 3, 11, 11, 12, 13, 14, 15, 31, 35];

    assert!(!matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(1 | 2 | 3 | 11)
    ));
}

#[test]
fn discard_keeps_pairs_when_many_pairs_can_chase_seven_pairs() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 22, 23, 31, 35, 36];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(31)
    );
}

#[test]
fn seven_pairs_plan_protects_honor_and_terminal_pairs_more() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 5, 5, 11, 11, 12, 12, 21, 21, 31, 31, 35, 36];

    let middle_pair =
        seven_pairs_plan_discard_bias(&hand, 5, &[], &table, 0, WIN_RULE_SHENYANG_BASIC);
    let terminal_pair =
        seven_pairs_plan_discard_bias(&hand, 1, &[], &table, 0, WIN_RULE_SHENYANG_BASIC);
    let honor_pair =
        seven_pairs_plan_discard_bias(&hand, 31, &[], &table, 0, WIN_RULE_SHENYANG_BASIC);

    assert!(honor_pair < terminal_pair);
    assert!(terminal_pair < middle_pair);
}

#[test]
fn seven_pairs_plan_protects_live_pair_over_dead_pair() {
    let table = table_with_discards(1, vec![5, 5]);
    let hand = vec![1, 1, 5, 5, 11, 11, 12, 12, 21, 21, 31, 31, 35, 36];

    let dead_middle_pair =
        seven_pairs_plan_discard_bias(&hand, 5, &[], &table, 0, WIN_RULE_SHENYANG_BASIC);
    let live_middle_pair =
        seven_pairs_plan_discard_bias(&hand, 12, &[], &table, 0, WIN_RULE_SHENYANG_BASIC);

    assert_eq!(remaining_tile_count(&hand, &table, 0, 5), 0);
    assert!(remaining_tile_count(&hand, &table, 0, 12) > 0);
    assert!(live_middle_pair < dead_middle_pair);
}

#[test]
fn discard_locked_five_pairs_prefers_honor_singleton_first() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 2, 2, 5, 9, 11, 11, 12, 12, 14, 21, 21, 31];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(31)
    );
}

#[test]
fn discard_locked_five_pairs_prefers_single_dragon_before_wind() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 14, 21, 21, 31, 35];

    assert!(should_lock_seven_pairs_plan(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(35)
    );
}

#[test]
fn discard_locked_five_pairs_prefers_non_terminal_singleton_over_terminal() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 2, 2, 5, 9, 11, 11, 12, 12, 14, 19, 21, 21];

    assert!(matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(5 | 14)
    ));
}

#[test]
fn late_defense_breaks_locked_five_pairs_for_only_public_tile() {
    let mut table = table_with_discards(1, vec![1]);
    table.wall_count = 20;
    let hand = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 14, 21, 21, 31, 35];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(1)
    );
}

#[test]
fn late_defense_preserves_locked_five_pairs_without_public_tile() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 20;
    let hand = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 14, 21, 21, 31, 35];

    assert!(!matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(1 | 2 | 11 | 12 | 21)
    ));
}

#[test]
fn late_defense_locked_five_pairs_follows_public_singleton_without_breaking_pairs() {
    let mut table = table_with_discards(1, vec![5]);
    table.wall_count = 20;
    let hand = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 14, 21, 21, 31, 35];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(5)
    );
}

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
fn incomplete_sequence_bias_does_not_override_piao_pair_plan() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 4, 5, 11, 11, 12, 13, 21, 21, 22, 23, 35, 35];

    assert_eq!(
        incomplete_sequence_discard_bias(&hand, 1, &[], &table, 0, WIN_RULE_SHENYANG_BASIC),
        0.0
    );
    assert!(!matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(11 | 21 | 35)
    ));
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
fn discard_three_pair_piao_candidate_still_prefers_wind_before_single_dragon() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 4, 5, 11, 11, 12, 13, 21, 21, 22, 23, 31, 35];

    assert_eq!(pair_count(&hand), 3);
    assert!(is_closed_early_piao_candidate(&hand, &[], &table, 0));
    assert!(piao_plan_score_for_context(&hand, &[], &table, 0) > 0.0);
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(31)
    );
}

#[test]
fn piao_plan_scores_three_pair_three_suit_candidate() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 4, 5, 11, 11, 12, 13, 21, 21, 22, 23, 31, 35];

    assert_eq!(pair_count(&hand), 3);
    assert_eq!(piao_plan_score_for_context(&hand, &[], &table, 0), 15.0);
}

#[test]
fn piao_plan_rejects_three_pair_candidate_missing_suit() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 4, 5, 11, 11, 12, 13, 14, 14, 16, 17, 31, 35];

    assert_eq!(pair_count(&hand), 3);
    assert_eq!(piao_plan_score_for_context(&hand, &[], &table, 0), 0.0);
}

#[test]
fn dealer_discounts_three_pair_piao_candidate_for_speed() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    let hand = vec![1, 1, 4, 5, 11, 11, 12, 13, 21, 21, 22, 23, 31, 35];

    assert_eq!(piao_plan_score_for_context(&hand, &[], &table, 0), 5.25);
}

#[test]
fn discard_four_pair_piao_candidate_clears_single_dragon_before_wind() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 4, 4, 11, 11, 12, 13, 21, 21, 22, 23, 31, 35];

    assert_eq!(pair_count(&hand), 4);
    assert!(piao_plan_score_for_context(&hand, &[], &table, 0) > 0.0);
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(35)
    );
}

#[test]
fn discard_preserves_four_pair_piao_candidate_over_public_pair_tile() {
    let mut table = table_with_discards(1, vec![11, 11]);
    table.wall_count = 36;
    let hand = vec![1, 1, 4, 5, 11, 11, 12, 13, 21, 21, 22, 23, 31, 31];

    assert!(!matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(1 | 11 | 21 | 31)
    ));
}

#[test]
fn discard_preserves_open_piao_pairs_over_public_pair_tile() {
    let mut table = table_with_discards(1, vec![11, 11]);
    table.wall_count = 36;
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    let hand = vec![11, 11, 12, 21, 21, 22, 23, 24, 31, 35, 36];

    assert!(!matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(11 | 21)
    ));
}

#[test]
fn piao_discard_bias_locks_pairs_after_two_triplet_groups() {
    let table = table_with_discards(1, Vec::new());
    let one_group_melds = vec![test_peng_meld(1)];
    let one_group_hand = vec![11, 11, 21, 21, 22, 23, 31, 35, 36, 37];
    let two_group_melds = vec![test_peng_meld(1), test_peng_meld(11)];
    let two_group_hand = vec![21, 21, 22, 23, 31, 35, 36, 37];

    assert_eq!(
        piao_discard_bias(
            &one_group_hand,
            21,
            &one_group_melds,
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC
        ),
        -16.0
    );
    assert_eq!(
        piao_discard_bias(
            &two_group_hand,
            21,
            &two_group_melds,
            &table,
            0,
            WIN_RULE_SHENYANG_BASIC
        ),
        -24.0
    );
}

#[test]
fn piao_discard_bias_protects_live_dragon_pair() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 4, 5, 11, 11, 12, 13, 21, 21, 22, 23, 35, 35];

    let middle_pair = piao_discard_bias(&hand, 11, &[], &table, 0, WIN_RULE_SHENYANG_BASIC);
    let dragon_pair = piao_discard_bias(&hand, 35, &[], &table, 0, WIN_RULE_SHENYANG_BASIC);

    assert_eq!(pair_count(&hand), 4);
    assert!(remaining_tile_count(&hand, &table, 0, 35) > 0);
    assert!(dragon_pair < middle_pair);
}

#[test]
fn piao_discard_bias_protects_live_pair_over_dead_pair() {
    let table = table_with_discards(1, vec![11, 11]);
    let hand = vec![1, 1, 4, 4, 11, 11, 12, 13, 21, 21, 22, 23, 31, 35];

    let dead_pair = piao_discard_bias(&hand, 11, &[], &table, 0, WIN_RULE_SHENYANG_BASIC);
    let live_pair = piao_discard_bias(&hand, 21, &[], &table, 0, WIN_RULE_SHENYANG_BASIC);

    assert_eq!(pair_count(&hand), 4);
    assert_eq!(remaining_tile_count(&hand, &table, 0, 11), 0);
    assert!(remaining_tile_count(&hand, &table, 0, 21) > 0);
    assert!(live_pair < dead_pair);
}

#[test]
fn discard_preserves_committed_piao_pair_over_public_pair_tile() {
    let mut table = table_with_discards(1, vec![21, 21]);
    table.wall_count = 36;
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1), test_peng_meld(11)];
    let hand = vec![21, 21, 22, 23, 24, 31, 35, 36];

    assert!(!matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(21)
    ));
}

#[test]
fn discard_preserves_only_terminal_or_honor_for_piao_plan_even_relaxed() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![2, 2, 5, 5, 8, 8, 12, 12, 15, 16, 18, 22, 24, 31];

    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(31)
    );
}

#[test]
fn discard_preserves_only_third_suit_for_piao_plan_even_relaxed() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![2, 2, 2, 5, 5, 8, 8, 12, 12, 12, 15, 15, 22, 31];

    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(22)
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
fn discard_preserves_three_pair_three_suit_piao_candidate() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 4, 5, 6, 11, 11, 12, 13, 14, 21, 21, 22, 23];

    assert!(!matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(1 | 11 | 21)
    ));
}

#[test]
fn discard_preserves_three_pair_piao_candidate_over_public_pair_tile() {
    let mut table = table_with_discards(1, vec![11, 11]);
    table.wall_count = 36;
    let hand = vec![1, 1, 4, 5, 6, 11, 11, 12, 13, 14, 21, 21, 22, 23];

    assert!(is_closed_early_piao_candidate(&hand, &[], &table, 0));
    let chosen = choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC);
    assert!(
        !matches!(chosen, Some(1 | 11 | 21)),
        "unexpected pair discard: {chosen:?}"
    );
}

#[test]
fn discard_preserves_only_terminal_or_honor_for_three_pair_piao_candidate_even_relaxed() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![2, 2, 5, 5, 8, 8, 12, 14, 15, 16, 22, 24, 26, 31];

    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(31)
    );
}

#[test]
fn discard_preserves_only_third_suit_for_three_pair_piao_candidate_even_relaxed() {
    let mut table = table_with_discards(1, vec![24]);
    table.wall_count = 36;
    let hand = vec![2, 2, 5, 5, 8, 8, 12, 14, 15, 16, 24, 31, 35, 37];

    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(24)
    );
}

#[test]
fn discard_returns_none_for_seven_pairs_win() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        None
    );
}

#[test]
fn discard_sets_seven_pairs_wait_on_live_terminal_over_dead_wind() {
    let table = table_with_discards(1, vec![31, 31]);
    let hand = vec![1, 1, 2, 2, 9, 11, 11, 12, 12, 21, 21, 22, 22, 31];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(31)
    );
}

#[test]
fn discard_sets_seven_pairs_wait_away_from_public_middle_tile() {
    let table = table_with_discards(1, vec![5]);
    let hand = vec![1, 1, 2, 2, 5, 9, 11, 11, 12, 12, 21, 21, 22, 22];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(5)
    );
}

#[test]
fn discard_sets_seven_pairs_wait_on_live_wind_before_middle_tile() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 21, 21, 22, 22, 31];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(5)
    );
}

#[test]
fn discard_sets_seven_pairs_wait_on_wind_before_dragon() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 2, 2, 3, 3, 4, 4, 11, 11, 21, 21, 31, 35];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(35)
    );
}

#[test]
fn discard_sets_seven_pairs_wait_on_live_terminal_before_middle_tile() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 1, 2, 2, 5, 9, 11, 11, 12, 12, 21, 21, 22, 22];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(5)
    );
}

#[test]
fn seven_pairs_wait_discard_avoids_pure_one_suit_threat_tile() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 32;
    table.seats.get_mut(&1).unwrap().melds = vec![test_chi_meld(2), test_peng_meld(7)];
    let hand = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 18, 21, 21, 22, 22];

    assert_eq!(pair_count(&hand), 6);
    assert!(
        pure_one_suit_threat_discard_bias(&table, 0, 5, 1)
            < pure_one_suit_threat_discard_bias(&table, 0, 18, 1)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(18)
    );
}

#[test]
fn seven_pairs_wait_discard_avoids_piao_missing_suit_threat_tile() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 32;
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(11), test_peng_meld(21), test_peng_meld(35)];
    let hand = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 21, 21, 22, 22, 31];
    let wind_wait = remove_n_tiles(&hand, 5, 1);
    let middle_wait = remove_n_tiles(&hand, 31, 1);

    assert_eq!(pair_count(&hand), 6);
    assert!(
        seven_pairs_wait_tile_score(31, &wind_wait, &table, 0)
            > seven_pairs_wait_tile_score(5, &middle_wait, &table, 0)
    );
    assert!(
        opponent_threat_discard_bias(&table, 0, 5, 1)
            < opponent_threat_discard_bias(&table, 0, 31, 1)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(31)
    );
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
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(31)
    );
}

#[test]
fn ready_score_values_live_wind_over_middle_for_dealer_seven_pairs() {
    let mut table = table_with_discards(1, Vec::new());
    table.dealer_position = 0;
    let wind_wait = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 31];
    let middle_wait = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 21, 21, 22, 22];

    assert!(
        ready_tile_score(&wind_wait, &[], &table, 0, WIN_RULE_SHENYANG_BASIC)
            > ready_tile_score(&middle_wait, &[], &table, 0, WIN_RULE_SHENYANG_BASIC)
    );
}

#[test]
fn capped_ready_score_keeps_wind_shape_as_seven_pairs_tiebreaker() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(4);
    let wind_wait = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 31];
    let middle_wait = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 21, 21, 22, 22];

    assert!(
        ready_tile_score(&wind_wait, &[], &table, 0, WIN_RULE_SHENYANG_BASIC)
            > ready_tile_score(&middle_wait, &[], &table, 0, WIN_RULE_SHENYANG_BASIC)
    );
}

#[test]
fn capped_ready_score_prefers_live_middle_over_public_wind_wait() {
    let mut table = table_with_discards(1, vec![31]);
    table.max_fan = Some(4);
    let wind_wait = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 31];
    let middle_wait = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 21, 21, 22, 22];

    assert!(
        ready_tile_score(&middle_wait, &[], &table, 0, WIN_RULE_SHENYANG_BASIC)
            > ready_tile_score(&wind_wait, &[], &table, 0, WIN_RULE_SHENYANG_BASIC)
    );
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
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(31)
    );
}

#[test]
fn capped_discard_sets_seven_pairs_wait_on_live_wind_tiebreaker() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(4);
    let hand = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 21, 21, 22, 22, 31];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(5)
    );
}

#[test]
fn one_fan_capped_six_pairs_still_sets_better_seven_pairs_wait() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(1);
    let hand = vec![1, 1, 2, 2, 5, 11, 11, 12, 12, 21, 21, 22, 22, 31];

    assert_eq!(pair_count(&hand), 6);
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(5)
    );
}

#[test]
fn discard_starts_pure_one_suit_plan_at_eight_main_suit_tiles() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 11, 12, 21, 22, 31, 35];

    assert!(matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(11 | 12 | 21 | 22 | 31 | 35)
    ));
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
fn mid_round_discard_follows_public_honor_over_live_dragon() {
    let mut table = table_with_discards(1, vec![31]);
    table.wall_count = 46;
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 36];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(31)
    );
}

#[test]
fn mid_round_discard_follows_public_dragon_over_multiple_public_terminal() {
    let mut table = table_with_discards(1, vec![9, 9, 35]);
    table.wall_count = 46;
    let hand = vec![1, 2, 3, 9, 11, 12, 14, 16, 18, 21, 22, 24, 26, 35];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(35)
    );
}

#[test]
fn mid_round_discard_follows_public_middle_before_late_round() {
    let mut table = table_with_discards(1, vec![14]);
    table.wall_count = 55;
    let hand = vec![1, 2, 3, 9, 11, 12, 14, 16, 18, 21, 22, 24, 26, 35];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(14)
    );
}

#[test]
fn mid_round_live_dragon_risk_grows_when_opponents_are_open() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 42;
    let base = mid_round_live_honor_risk_bias(&table, 0, 35, 1);
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(9)];
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_peng_meld(16)],
        },
    );

    assert!(mid_round_live_honor_risk_bias(&table, 0, 35, 1) < base);
}

#[test]
fn mid_round_live_dragon_risk_ignores_concealed_gang_opponent() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 42;
    let base = mid_round_live_honor_risk_bias(&table, 0, 35, 1);

    table.seats.get_mut(&1).unwrap().melds = vec![test_concealed_gang_meld(9)];
    assert_eq!(mid_round_live_honor_risk_bias(&table, 0, 35, 1), base);

    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(9)];
    assert!(mid_round_live_honor_risk_bias(&table, 0, 35, 1) < base);
}

#[test]
fn mid_round_open_dragon_meld_does_not_add_live_dragon_pressure() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 42;
    let base = mid_round_live_honor_risk_bias(&table, 0, 35, 1);

    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(35)];
    assert_eq!(open_opponent_live_dragon_risk(&table, 0, 35), 0.0);
    assert_eq!(mid_round_live_honor_risk_bias(&table, 0, 35, 1), base);

    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(9)];
    assert!(open_opponent_live_dragon_risk(&table, 0, 35) > 0.0);
    assert!(mid_round_live_honor_risk_bias(&table, 0, 35, 1) < base);
}

#[test]
fn mid_round_live_dragon_risk_discounts_exposed_meld_tiles() {
    let mut exposed_table = table_with_discards(1, Vec::new());
    exposed_table.wall_count = 42;
    exposed_table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(9)];
    exposed_table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_peng_meld(35)],
        },
    );

    let mut live_table = exposed_table.clone();
    live_table.seats.get_mut(&2).unwrap().melds = vec![test_peng_meld(16)];

    assert!(live_risk_exposure_scale(&exposed_table, 35) < 1.0);
    assert!(
        open_opponent_live_dragon_risk(&exposed_table, 0, 35)
            < open_opponent_live_dragon_risk(&live_table, 0, 35)
    );
}

#[test]
fn mid_round_open_honor_meld_tile_is_safer_than_live_dragon() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 42;
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(35)];

    let exposed_dragon_safety = mid_round_open_meld_safety_bias(&table, 35);
    let live_dragon_safety = mid_round_open_meld_safety_bias(&table, 36);
    assert!(exposed_dragon_safety > 0.0);
    assert_eq!(live_dragon_safety, 0.0);

    let exposed_dragon_score =
        exposed_dragon_safety + mid_round_live_honor_risk_bias(&table, 0, 35, 1);
    let live_dragon_score = live_dragon_safety + mid_round_live_honor_risk_bias(&table, 0, 36, 1);
    assert!(exposed_dragon_score > live_dragon_score);
}

#[test]
fn mid_round_discard_avoids_live_dragon_against_open_opponent() {
    let mut table = table_with_discards(1, vec![14]);
    table.wall_count = 42;
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(9)];
    let hand = vec![1, 2, 3, 11, 12, 13, 14, 16, 18, 21, 22, 23, 31, 35];

    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(35)
    );
}

#[test]
fn mid_round_discard_follows_public_middle_over_live_terminal() {
    let mut table = table_with_discards(1, vec![14]);
    table.wall_count = 37;
    let hand = vec![1, 2, 3, 9, 11, 12, 14, 16, 18, 21, 22, 24, 26, 31];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(14)
    );
}

#[test]
fn mid_round_discard_follows_public_middle_over_cold_wind_against_closed_opponent() {
    let mut table = table_with_discards(1, vec![14]);
    table.wall_count = 37;
    table.seats.get_mut(&1).unwrap().hand_count = 13;
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 14, 21, 22, 23, 31];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(14)
    );
}

#[test]
fn mid_round_live_suited_risk_grows_when_opponents_are_open() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 37;
    let hand = vec![1, 2, 3, 9, 11, 12, 14, 16, 18, 21, 22, 24, 26, 31];
    let base = mid_round_live_suited_risk_bias(&hand, &[], &table, 0, 9, 1, WIN_RULE_RELAXED);
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(16)];

    assert!(mid_round_live_suited_risk_bias(&hand, &[], &table, 0, 9, 1, WIN_RULE_RELAXED) < base);
}

#[test]
fn mid_round_live_suited_risk_ignores_concealed_gang_opponent() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 37;
    let hand = vec![1, 2, 3, 9, 11, 12, 14, 16, 18, 21, 22, 24, 26, 31];
    let base = mid_round_live_suited_risk_bias(&hand, &[], &table, 0, 9, 1, WIN_RULE_RELAXED);

    table.seats.get_mut(&1).unwrap().melds = vec![test_concealed_gang_meld(16)];
    assert_eq!(
        mid_round_live_suited_risk_bias(&hand, &[], &table, 0, 9, 1, WIN_RULE_RELAXED),
        base
    );

    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(16)];
    assert!(mid_round_live_suited_risk_bias(&hand, &[], &table, 0, 9, 1, WIN_RULE_RELAXED) < base);
}

#[test]
fn mid_round_open_meld_tile_is_safer_than_live_suited_tile() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 37;
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(14)];
    let hand = vec![1, 2, 3, 9, 11, 12, 14, 16, 18, 21, 22, 24, 26, 28];

    assert!(mid_round_open_meld_safety_bias(&table, 14) > 0.0);
    assert_eq!(
        open_opponent_live_suited_risk(&table, 0, 14),
        0.0,
        "an opponent who already opened this tile should not add live-tile pressure for it"
    );
    assert!(
        mid_round_open_meld_safety_bias(&table, 14) > mid_round_open_meld_safety_bias(&table, 9)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(14)
    );
}

#[test]
fn mid_round_live_suited_risk_discounts_exposed_meld_tiles() {
    let mut exposed_table = table_with_discards(1, Vec::new());
    exposed_table.wall_count = 37;
    exposed_table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(16)];
    exposed_table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_peng_meld(9)],
        },
    );
    let hand = vec![1, 2, 3, 9, 11, 12, 14, 16, 18, 21, 22, 24, 26, 31];

    let mut live_table = exposed_table.clone();
    live_table.seats.get_mut(&2).unwrap().melds = vec![test_peng_meld(35)];

    assert!(live_risk_exposure_scale(&exposed_table, 9) < 1.0);
    assert!(
        mid_round_live_suited_risk_bias(&hand, &[], &exposed_table, 0, 9, 1, WIN_RULE_RELAXED)
            > mid_round_live_suited_risk_bias(&hand, &[], &live_table, 0, 9, 1, WIN_RULE_RELAXED)
    );
}

#[test]
fn mid_round_values_two_open_meld_tiles_over_live_dragon() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 37;
    table.seats.get_mut(&1).unwrap().melds = vec![test_chi_meld(4)];
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_chi_meld(6)],
        },
    );
    let hand = vec![1, 2, 3, 6, 9, 11, 12, 14, 16, 18, 21, 22, 24, 35];

    assert_eq!(open_meld_tile_count(&table, 6), 2);
    assert!(
        mid_round_open_meld_safety_bias(&table, 6)
            + mid_round_live_honor_risk_bias(&table, 0, 6, 1)
            > mid_round_open_meld_safety_bias(&table, 35)
                + mid_round_live_honor_risk_bias(&table, 0, 35, 1)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(6)
    );
}

#[test]
fn open_meld_tile_count_ignores_malformed_meld_tiles() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&1).unwrap().melds = vec![
        WsShenyangMahjongMeld {
            kind: ShenyangMahjongMeldKind::PENG,
            tiles: vec![14, 14],
            from_position: Some(0),
        },
        test_peng_meld(14),
    ];

    assert_eq!(open_meld_tile_count(&table, 14), 3);
}

#[test]
fn open_opponent_exists_ignores_tile_from_its_open_meld() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 37;
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(14)];

    assert!(!open_opponent_exists_for_tile(&table, 0, 14));
    assert!(open_opponent_exists_for_tile(&table, 0, 15));
}

#[test]
fn own_open_live_suited_pressure_ignores_opponent_open_meld_tile() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 37;
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(14)];
    let melds = vec![test_peng_meld(1), test_peng_meld(11)];

    assert_eq!(own_open_live_suited_pressure(&melds, &table, 0, 14), 0.0);
    assert!(own_open_live_suited_pressure(&melds, &table, 0, 15) > 0.0);
}

#[test]
fn mid_round_discard_avoids_live_terminal_against_open_opponent() {
    let mut table = table_with_discards(1, vec![14]);
    table.wall_count = 37;
    table.seats.get_mut(&1).unwrap().melds = vec![test_peng_meld(16)];
    let hand = vec![1, 2, 3, 9, 11, 12, 14, 16, 18, 21, 22, 24, 26, 31];

    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(9)
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
        max_fan: Some(4),
        claim_window: None,
        seats,
    };
    let hand = vec![9, 13, 14, 15, 24, 24, 28, 29];

    assert_ne!(
        choose_discard_from_view(&hand, &table, 3, WIN_RULE_SHENYANG_BASIC),
        Some(9)
    );
}

#[test]
fn late_open_hand_avoids_live_tile_against_four_piao_melds() {
    let mut seats = HashMap::new();
    seats.insert(
        0,
        AiSeatView {
            position: 0,
            hand_count: 1,
            discards: vec![31, 33, 19, 16, 1, 31, 21, 8, 12, 32, 3, 4, 2, 15, 22, 4],
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
            hand_count: 11,
            discards: vec![21, 4, 15, 35, 11, 12, 16, 34, 33, 33, 35, 35],
            melds: vec![test_peng_meld(19)],
        },
    );
    seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 13,
            discards: vec![34, 1, 22, 33, 12, 23, 5, 3, 25, 28, 1, 29],
            melds: Vec::new(),
        },
    );
    seats.insert(
        3,
        AiSeatView {
            position: 3,
            hand_count: 8,
            discards: vec![34, 32, 22, 8, 35, 16, 11, 12, 17, 3, 28, 28],
            melds: vec![test_peng_meld(7), test_peng_meld(26)],
        },
    );
    let table = AiPublicTable {
        current_position: 1,
        dealer_position: 0,
        wall_count: 31,
        max_fan: Some(4),
        claim_window: None,
        seats,
    };
    let hand = vec![7, 8, 9, 9, 9, 13, 22, 23, 24, 36, 36];

    assert_ne!(
        choose_discard_from_view(&hand, &table, 1, WIN_RULE_SHENYANG_BASIC),
        Some(13)
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

#[test]
fn estimated_visible_fan_counts_four_gui_yi_before_wait_fan() {
    let win_hand = vec![2, 3, 4, 11, 12, 13, 21, 22, 23, 35, 35];
    let melds = vec![test_peng_meld(2)];

    assert_eq!(
        estimated_visible_fan_without_wait(&win_hand, &melds, WIN_RULE_SHENYANG_BASIC),
        2
    );
}

#[test]
fn estimated_four_gui_yi_ignores_malformed_melds() {
    let short_peng = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![2, 2],
        from_position: Some(1),
    };
    let invalid_chi = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::CHI,
        tiles: vec![2, 2, 2],
        from_position: Some(1),
    };
    let invalid_tile_peng = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![99, 99, 99],
        from_position: Some(1),
    };

    assert_eq!(estimated_four_gui_yi_fan(&[2, 2], &[short_peng]), 0);
    assert_eq!(estimated_four_gui_yi_fan(&[2], &[test_peng_meld(2)]), 1);
    assert_eq!(estimated_four_gui_yi_fan(&[2], &[invalid_chi]), 0);
    assert_eq!(estimated_four_gui_yi_fan(&[99], &[invalid_tile_peng]), 0);
    assert_eq!(
        estimated_four_gui_yi_fan(&[2, 2, 2], &[test_chi_meld(2)]),
        1
    );
}

#[test]
fn estimated_visible_fan_counts_concealed_dragon_triplet() {
    let win_hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 35, 35, 35];

    assert_eq!(
        estimated_visible_fan_without_wait(&win_hand, &[], WIN_RULE_RELAXED),
        2
    );
}

#[test]
fn estimated_meld_fan_ignores_short_dragon_melds() {
    let short_gang = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::GANG,
        tiles: vec![35, 35, 35],
        from_position: None,
    };
    let short_peng = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![35, 35],
        from_position: Some(1),
    };
    let invalid_tile_gang = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::GANG,
        tiles: vec![99, 99, 99, 99],
        from_position: None,
    };

    assert_eq!(estimated_meld_fan(&[short_gang]), 0);
    assert_eq!(estimated_meld_fan(&[short_peng]), 0);
    assert_eq!(estimated_meld_fan(&[invalid_tile_gang]), 0);
}

#[test]
fn estimated_visible_fan_counts_four_concealed_dragons_as_triplet_and_four_gui_yi() {
    let win_hand = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 35, 35, 35, 35];

    assert_eq!(
        estimated_visible_fan_without_wait(&win_hand, &[], WIN_RULE_RELAXED),
        6
    );
}

#[test]
fn estimated_visible_fan_counts_piao_shou_ba_yi_before_wait_fan() {
    let win_hand = vec![35, 35];
    let melds = vec![
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(21),
        test_peng_meld(31),
    ];

    assert_eq!(
        estimated_visible_fan_without_wait(&win_hand, &melds, WIN_RULE_SHENYANG_BASIC),
        4
    );
}

#[test]
fn estimated_visible_fan_requires_open_meld_for_piao() {
    let closed_triplet_hand = vec![1, 1, 1, 11, 11, 11, 21, 21, 21, 31, 31, 31, 35, 35];

    assert_eq!(
        estimated_visible_fan_without_wait(&closed_triplet_hand, &[], WIN_RULE_RELAXED),
        1
    );
    assert_eq!(
        estimated_visible_fan_without_wait(&closed_triplet_hand, &[], WIN_RULE_SHENYANG_BASIC),
        0
    );
}

#[test]
fn estimated_visible_fan_uses_win_rule_for_closed_pure_one_suit() {
    let win_hand = vec![1, 2, 3, 2, 3, 4, 4, 5, 6, 7, 7, 7, 9, 9];

    assert_eq!(
        estimated_visible_fan_without_wait(&win_hand, &[], WIN_RULE_RELAXED),
        4
    );
    assert_eq!(
        estimated_visible_fan_without_wait(&win_hand, &[], WIN_RULE_SHENYANG_BASIC),
        4
    );
}

#[test]
fn estimated_visible_fan_does_not_add_closed_winner_fan() {
    let closed_pure_one_suit = vec![1, 2, 3, 2, 3, 4, 4, 5, 6, 7, 7, 7, 9, 9];
    let closed_seven_pairs = vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35];

    assert_eq!(
        estimated_visible_fan_without_wait(&closed_pure_one_suit, &[], WIN_RULE_SHENYANG_BASIC),
        4
    );
    assert_eq!(
        estimated_visible_fan_without_wait(&closed_seven_pairs, &[], WIN_RULE_SHENYANG_BASIC),
        4
    );
}

#[test]
fn estimated_fan_counts_single_yaojiu_terminal_wait_extra() {
    let win_hand = vec![11, 11, 14, 15, 15, 16, 16, 17, 17, 17, 17];
    let melds = vec![test_chi_meld(12)];

    assert_eq!(
        estimated_fan_with_wait(&win_hand, &melds, 11, WIN_RULE_SHENYANG_BASIC),
        7
    );
}

#[test]
fn estimated_fan_counts_single_yaojiu_honor_wait_extra() {
    let win_hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35];

    assert_eq!(
        estimated_fan_with_wait(&win_hand, &[], 35, WIN_RULE_RELAXED),
        3
    );
}

#[test]
fn estimated_fan_rejects_invalid_meld_for_single_wait() {
    let win_hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 35, 35];
    let invalid_meld = WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![99, 99, 99],
        from_position: Some(1),
    };

    assert_eq!(
        estimated_fan_with_wait(&win_hand, &[invalid_meld], 35, WIN_RULE_RELAXED),
        0
    );
}

#[test]
fn fan_wait_bias_uses_win_rule_for_closed_basic_hand() {
    let table = table_with_discards(1, Vec::new());
    let win_hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35];

    assert!(fan_wait_bias(&win_hand, &[], &table, 0, WIN_RULE_RELAXED, 35, 2) > 0.0);
    assert_eq!(
        fan_wait_bias(&win_hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC, 35, 2),
        0.0
    );
}

#[test]
fn fan_wait_bias_counts_middle_tile_seven_pairs_single_wait() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(5);
    let win_hand = vec![1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 11, 11, 21, 21];

    assert!(fan_wait_bias(&win_hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC, 5, 3) > 0.0);

    table.dealer_position = 0;
    assert_eq!(
        fan_wait_bias(&win_hand, &[], &table, 0, WIN_RULE_SHENYANG_BASIC, 5, 3),
        0.0
    );
}

#[test]
fn fan_wait_bias_counts_single_yaojiu_terminal_wait_extra_for_cap() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(7);
    let win_hand = vec![11, 11, 14, 15, 15, 16, 16, 17, 17, 17, 17];
    let melds = vec![test_chi_meld(12)];

    assert_eq!(
        estimated_visible_fan_without_wait(&win_hand, &melds, WIN_RULE_SHENYANG_BASIC),
        5
    );
    assert_eq!(
        estimated_fan_with_wait(&win_hand, &melds, 11, WIN_RULE_SHENYANG_BASIC),
        7
    );
    assert_eq!(
        fan_wait_bias(&win_hand, &melds, &table, 0, WIN_RULE_SHENYANG_BASIC, 11, 2),
        0.0
    );
    assert_eq!(
        fan_wait_bias(&win_hand, &melds, &table, 0, WIN_RULE_SHENYANG_BASIC, 11, 3),
        14.0
    );
}

#[test]
fn late_defense_avoids_cold_honor_against_closed_opponent() {
    let mut table = table_with_discards(1, vec![11, 14, 19]);
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().hand_count = 13;
    let hand = vec![2, 3, 4, 5, 6, 7, 8, 12, 13, 15, 16, 17, 18, 31];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(12)
    );
}

#[test]
fn late_defense_does_not_mark_exposed_suit_as_missing() {
    let mut table = table_with_discards(1, vec![11, 14, 19]);
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().melds = vec![WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![12, 12, 12],
        from_position: Some(0),
    }];

    assert_eq!(opponent_missing_suit_safety_bias(&table, 0, 12), 0.0);
}

#[test]
fn late_defense_does_not_mark_piao_needed_suit_as_missing() {
    let mut table = table_with_discards(1, vec![1, 4, 9]);
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(11), test_peng_meld(21), test_peng_meld(31)];

    assert_eq!(
        piao_missing_suits_from_melds(&table.seats.get(&1).unwrap().melds),
        vec![0]
    );
    assert_eq!(opponent_missing_suit_safety_bias(&table, 0, 5), 0.0);
}

#[test]
fn late_defense_piao_needed_suit_blocks_other_missing_suit_reads() {
    let mut table = table_with_discards(1, vec![1, 4, 9]);
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(11), test_peng_meld(21), test_peng_meld(31)];
    for position in [2, 3] {
        table.seats.insert(
            position,
            AiSeatView {
                position,
                hand_count: 10,
                discards: vec![1, 4, 9],
                melds: Vec::new(),
            },
        );
    }
    let mut no_piao_table = table.clone();
    no_piao_table.seats.get_mut(&1).unwrap().melds.clear();

    assert!(opponent_missing_suit_safety_bias(&no_piao_table, 0, 5) > 0.0);
    assert_eq!(opponent_missing_suit_safety_bias(&table, 0, 5), 0.0);
}

#[test]
fn late_defense_closed_opponent_blocks_other_missing_suit_reads() {
    let mut table = table_with_discards(1, vec![1, 4, 9]);
    table.wall_count = 16;
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: vec![1, 4, 9],
            melds: Vec::new(),
        },
    );
    let mut closed_threat_table = table.clone();
    closed_threat_table.seats.insert(
        3,
        AiSeatView {
            position: 3,
            hand_count: 13,
            discards: Vec::new(),
            melds: Vec::new(),
        },
    );

    assert!(opponent_missing_suit_safety_bias(&table, 0, 5) > 0.0);
    assert_eq!(
        opponent_missing_suit_safety_bias(&closed_threat_table, 0, 5),
        0.0
    );
}

#[test]
fn late_defense_concealed_gang_opponent_blocks_other_missing_suit_reads() {
    let mut table = table_with_discards(1, vec![1, 4, 9]);
    table.wall_count = 16;
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: vec![1, 4, 9],
            melds: Vec::new(),
        },
    );

    let mut short_closed_table = table.clone();
    short_closed_table.seats.insert(
        3,
        AiSeatView {
            position: 3,
            hand_count: 9,
            discards: Vec::new(),
            melds: Vec::new(),
        },
    );

    let mut concealed_gang_table = short_closed_table.clone();
    concealed_gang_table.seats.get_mut(&3).unwrap().melds = vec![test_concealed_gang_meld(9)];

    assert!(opponent_missing_suit_safety_bias(&short_closed_table, 0, 5) > 0.0);
    assert_eq!(
        opponent_missing_suit_safety_bias(&concealed_gang_table, 0, 5),
        0.0
    );
}

#[test]
fn late_defense_candidates_avoid_piao_needed_suit_over_missing_suit_read() {
    let mut table = table_with_discards(1, vec![1, 4, 9]);
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(11), test_peng_meld(21), test_peng_meld(31)];
    let hand = vec![5, 12];

    assert_eq!(
        choose_late_defense_discard_from_candidates(&hand, &table, 0, vec![5, 12]),
        Some(12)
    );
}

#[test]
fn late_defense_follows_public_tile_before_live_missing_suit_read() {
    let missing_suit_discards = vec![11, 13, 14, 19, 11, 13, 14, 19, 11, 13];
    let mut table = table_with_discards(1, {
        let mut discards = missing_suit_discards.clone();
        discards.push(5);
        discards
    });
    table.wall_count = 16;
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: missing_suit_discards.clone(),
            melds: Vec::new(),
        },
    );
    table.seats.insert(
        3,
        AiSeatView {
            position: 3,
            hand_count: 10,
            discards: missing_suit_discards,
            melds: Vec::new(),
        },
    );
    let hand = vec![2, 5, 7, 9, 12, 16, 18, 21, 23, 25, 27, 31, 33, 35];

    assert!(
        late_defense_tile_safety_score(&table, 0, 12, 1)
            > late_defense_tile_safety_score(&table, 0, 5, 1)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(5)
    );
}

#[test]
fn late_defense_prefers_opponent_missing_suit_tile() {
    let mut table = table_with_discards(1, vec![11, 14, 19]);
    table.wall_count = 16;
    let hand = vec![2, 3, 4, 5, 6, 7, 8, 12, 13, 15, 16, 17, 18, 22];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(12)
    );
}

#[test]
fn late_defense_missing_suit_read_can_beat_live_wind() {
    let mut table = table_with_discards(1, vec![11, 14, 19]);
    table.wall_count = 16;
    let hand = vec![2, 3, 4, 5, 6, 7, 8, 12, 13, 15, 16, 17, 18, 31];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(12)
    );
}

#[test]
fn late_defense_prefers_public_honor_over_multiple_public_suited_tile() {
    let mut table = table_with_discards(1, vec![5, 5, 31]);
    table.wall_count = 16;

    assert!(
        late_defense_tile_safety_score(&table, 0, 31, 1)
            > late_defense_tile_safety_score(&table, 0, 5, 1)
    );
}

#[test]
fn late_defense_bias_keeps_public_honor_above_four_public_middle_tiles() {
    let mut table = table_with_discards(1, vec![5, 5, 5, 5, 31]);
    table.wall_count = 16;

    assert!(late_defense_discard_bias(&table, 0, 31) > late_defense_discard_bias(&table, 0, 5));
}

#[test]
fn late_defense_prefers_public_middle_tile_over_public_terminal() {
    let mut table = table_with_discards(1, vec![5, 9]);
    table.wall_count = 16;

    assert!(
        late_defense_tile_safety_score(&table, 0, 5, 1)
            > late_defense_tile_safety_score(&table, 0, 9, 1)
    );
}

#[test]
fn late_defense_prefers_public_tile_seen_from_multiple_seats() {
    let mut table = table_with_discards(1, vec![5, 5]);
    table.wall_count = 16;
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: vec![6],
            melds: Vec::new(),
        },
    );
    table.seats.insert(
        3,
        AiSeatView {
            position: 3,
            hand_count: 10,
            discards: vec![6],
            melds: Vec::new(),
        },
    );

    assert_eq!(
        public_discard_count(&table, 5),
        public_discard_count(&table, 6)
    );
    assert!(public_discard_seat_count(&table, 6) > public_discard_seat_count(&table, 5));
    assert!(late_defense_discard_bias(&table, 0, 6) > late_defense_discard_bias(&table, 0, 5));
}

#[test]
fn late_defense_prefers_own_previous_middle_discard_over_other_public_middle() {
    let mut table = table_with_discards(1, vec![5]);
    table.wall_count = 16;
    table.seats.get_mut(&0).unwrap().discards = vec![8];
    let hand = vec![2, 3, 5, 7, 8, 12, 14, 16, 18, 21, 23, 25, 31, 35];

    assert!(
        late_defense_tile_safety_score(&table, 0, 8, 1)
            > late_defense_tile_safety_score(&table, 0, 5, 1)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(8)
    );
}

#[test]
fn late_defense_prefers_public_middle_tile_over_live_wind() {
    let mut table = table_with_discards(1, vec![5]);
    table.wall_count = 16;

    assert!(
        late_defense_tile_safety_score(&table, 0, 5, 1)
            > late_defense_tile_safety_score(&table, 0, 31, 1)
    );
}

#[test]
fn late_defense_prefers_live_wind_then_terminal_then_middle() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;

    assert!(
        late_defense_tile_safety_score(&table, 0, 31, 1)
            > late_defense_tile_safety_score(&table, 0, 9, 1)
    );
    assert!(
        late_defense_tile_safety_score(&table, 0, 9, 1)
            > late_defense_tile_safety_score(&table, 0, 5, 1)
    );
}

#[test]
fn late_defense_values_three_exposed_meld_tiles_over_live_wind() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_peng_meld(6)],
        },
    );

    assert!(
        late_defense_tile_safety_score(&table, 0, 6, 1)
            > late_defense_tile_safety_score(&table, 0, 31, 1)
    );
}

#[test]
fn late_defense_values_two_exposed_meld_tiles_over_live_wind() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().melds = vec![test_chi_meld(4)];
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_chi_meld(6)],
        },
    );
    table.seats.insert(
        3,
        AiSeatView {
            position: 3,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_chi_meld(11)],
        },
    );

    assert_eq!(exposed_meld_tile_count(&table, 6), 2);
    assert!(
        late_defense_tile_safety_score(&table, 0, 6, 1)
            > late_defense_tile_safety_score(&table, 0, 31, 1)
    );
}

#[test]
fn late_defense_discards_three_exposed_meld_tile_before_live_wind() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_peng_meld(6)],
        },
    );
    let hand = vec![2, 4, 6, 8, 12, 14, 16, 18, 22, 24, 26, 28, 31, 35];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(6)
    );
}

#[test]
fn late_defense_prefers_lone_wind_before_breaking_wind_pair() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    let hand = vec![1, 2, 4, 6, 8, 11, 13, 15, 17, 21, 23, 31, 31, 32];

    assert!(
        late_defense_tile_safety_score(&table, 0, 32, 1)
            > late_defense_tile_safety_score(&table, 0, 31, 2)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(32)
    );
}

#[test]
fn late_defense_prefers_live_middle_before_breaking_terminal_pair() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;

    assert!(
        late_defense_tile_safety_score(&table, 0, 5, 1)
            > late_defense_tile_safety_score(&table, 0, 9, 2)
    );
}

#[test]
fn late_defense_discards_live_middle_before_breaking_terminal_pair() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    let hand = vec![2, 4, 5, 6, 8, 9, 9, 12, 14, 16, 18, 22, 24, 26];

    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(9)
    );
}

#[test]
fn late_discard_follows_safe_tile_over_hand_efficiency() {
    let mut table = table_with_discards(1, vec![5]);
    table.wall_count = 16;
    let hand = vec![3, 4, 5, 5, 6, 7, 11, 12, 13, 21, 22, 23, 31, 35];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(5)
    );
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
fn mid_round_discard_follows_multiple_public_terminal_over_live_wind() {
    let mut table = table_with_discards(1, vec![9, 9]);
    table.wall_count = 36;
    let hand = vec![1, 2, 4, 6, 8, 9, 11, 12, 14, 16, 21, 23, 25, 31];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(9)
    );
}

#[test]
fn mid_round_public_discard_prefers_own_previous_middle_over_other_public_middle() {
    let mut table = table_with_discards(1, vec![5]);
    table.wall_count = 36;
    table.seats.get_mut(&0).unwrap().discards = vec![8];
    let hand = vec![2, 3, 5, 7, 8, 12, 14, 16, 18, 21, 23, 25, 31, 35];

    assert!(
        mid_round_public_discard_bias(&table, 0, 8) > mid_round_public_discard_bias(&table, 0, 5)
    );
    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(8)
    );
}

#[test]
fn mid_round_public_discard_prefers_tile_seen_from_multiple_seats() {
    let mut table = table_with_discards(1, vec![5, 5]);
    table.wall_count = 36;
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: vec![6],
            melds: Vec::new(),
        },
    );
    table.seats.insert(
        3,
        AiSeatView {
            position: 3,
            hand_count: 10,
            discards: vec![6],
            melds: Vec::new(),
        },
    );

    assert_eq!(
        public_discard_count(&table, 5),
        public_discard_count(&table, 6)
    );
    assert!(public_discard_seat_count(&table, 6) > public_discard_seat_count(&table, 5));
    assert!(
        mid_round_public_discard_bias(&table, 0, 6) > mid_round_public_discard_bias(&table, 0, 5)
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
fn mid_broken_basic_public_defense_preserves_only_recoverable_heng_seed() {
    let hand = vec![1, 2, 5, 8, 11, 12, 14, 17, 21, 22, 24, 27, 29, 35];
    let mut discards = dead_basic_heng_discards(&hand);
    if let Some(index) = discards.iter().position(|tile| *tile == 35) {
        discards.remove(index);
    }
    let mut table = table_with_discards(1, discards);
    table.wall_count = 40;

    assert!(can_recover_basic_heng(&hand, &[], &table));
    let after_dragon = remove_n_tiles(&hand, 35, 1);
    assert!(!can_recover_basic_heng_after_discard(
        &after_dragon,
        &[],
        &table,
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
fn dealer_mid_unrecoverable_basic_hand_uses_public_defense_discard() {
    let mut discards = dead_terminal_or_honor_discards();
    discards.push(5);
    let mut table = table_with_discards(1, discards);
    table.dealer_position = 0;
    table.wall_count = 52;
    let hand = vec![2, 3, 4, 5, 5, 6, 7, 12, 13, 14, 22, 23, 24, 25];

    assert_eq!(
        unrecoverable_basic_rule_requirement_count(&hand, &[], &table),
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
fn missing_suits_tracks_three_suits_need() {
    let hand = vec![1, 2, 3, 11, 18, 19, 21, 22, 23, 24, 25, 26, 35, 36];

    assert!(missing_suits(&hand, &[]).is_empty());
    assert_eq!(missing_suits(&hand[0..6], &[]), vec![2]);
}

#[test]
fn near_capped_non_dealer_prefers_wider_wait_over_single_wait_fan() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(4);
    table.seats.get_mut(&0).unwrap().melds = vec![test_gang_meld(35)];
    let hand = vec![2, 2, 4, 5, 7, 11, 12, 13, 21, 22, 23];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(7)
    );
}

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
fn non_dealer_can_choose_single_wait_for_extra_fan_before_late_round() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
    let hand = vec![2, 2, 4, 5, 7, 11, 12, 13, 21, 22, 23];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(4)
    );
}

#[test]
fn non_dealer_avoids_nearly_dead_single_wait_before_late_round() {
    let mut table = table_with_discards(1, vec![6, 6, 6]);
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
    let hand = vec![2, 2, 4, 5, 7, 11, 12, 13, 21, 22, 23];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(7)
    );
}

#[test]
fn one_step_wait_potential_values_near_ready_shape() {
    let table = table_with_discards(1, Vec::new());
    let hand = vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 31, 35];

    assert!(
        one_step_wait_potential(&hand, &[], &table, 0, WIN_RULE_RELAXED) > 0.0,
        "near-ready hand should see useful draws"
    );
}

#[test]
fn one_step_wait_potential_values_open_basic_route_foundation() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();
    let hand = vec![1, 2, 3, 5, 11, 12, 13, 21, 35, 35];

    assert!(hand_power(&hand) < 50.0);
    assert!(pair_count(&hand) < 4);
    assert!(has_open_meld(melds));
    assert!(missing_suits(&hand, melds).is_empty());
    assert!(has_terminal_or_honor_with_extra(&hand, melds, None));
    assert!(has_triplet_or_dragon_pair(&hand, melds));
    assert!(
        one_step_wait_potential(&hand, melds, &table, 0, WIN_RULE_SHENYANG_BASIC) > 0.0,
        "open basic hand with all hard requirements should value one-step ready draws"
    );
}

#[test]
fn opponent_threat_starts_after_three_triplet_melds() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];

    assert!(opponent_threat_discard_bias(&table, 0, 5, 2) < -9.0);

    table.seats.get_mut(&1).unwrap().melds.pop();
    assert_eq!(opponent_threat_discard_bias(&table, 0, 5, 2), 0.0);
}

#[test]
fn opponent_piao_threat_ignores_player_after_chi_meld() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&1).unwrap().melds = vec![
        test_chi_meld(2),
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(21),
    ];

    assert_eq!(piao_threat_level(&table.seats.get(&1).unwrap().melds), 0);
    assert_eq!(opponent_threat_discard_bias(&table, 0, 5, 2), 0.0);
}

#[test]
fn pure_one_suit_threat_penalizes_live_main_suit_tile() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 32;
    table.seats.get_mut(&1).unwrap().melds = vec![test_chi_meld(11), test_peng_meld(14)];

    assert_eq!(
        pure_one_suit_threat_suit(table.seats.get(&1).unwrap()),
        Some((1, 2))
    );
    assert!(
        pure_one_suit_threat_discard_bias(&table, 0, 18, 1)
            < pure_one_suit_threat_discard_bias(&table, 0, 22, 1)
    );
    assert_eq!(
        pure_one_suit_threat_discard_bias(&table, 0, 14, 1),
        0.0,
        "a tile already exposed by that opponent should not be treated as live"
    );
}

#[test]
fn pure_one_suit_threat_uses_opponent_discards_as_route_evidence() {
    let mut base_table = table_with_discards(1, Vec::new());
    base_table.wall_count = 32;
    base_table.seats.get_mut(&1).unwrap().melds = vec![test_chi_meld(11), test_peng_meld(14)];

    let mut same_suit_discards = base_table.clone();
    same_suit_discards.seats.get_mut(&1).unwrap().discards = vec![15, 16];

    let mut off_suit_discards = base_table.clone();
    off_suit_discards.seats.get_mut(&1).unwrap().discards = vec![2, 22, 31, 35];

    let base_bias = pure_one_suit_threat_discard_bias(&base_table, 0, 18, 1);
    let same_suit_bias = pure_one_suit_threat_discard_bias(&same_suit_discards, 0, 18, 1);
    let off_suit_bias = pure_one_suit_threat_discard_bias(&off_suit_discards, 0, 18, 1);

    assert!(
        same_suit_bias > base_bias,
        "discarding the same suit should make the pure-one-suit route less credible"
    );
    assert!(
        off_suit_bias < base_bias,
        "clearing other suits should make the pure-one-suit route more credible"
    );
}

#[test]
fn pure_one_suit_threat_reads_single_meld_with_strong_off_suit_discards() {
    let mut table = table_with_discards(1, vec![2, 22, 31, 35]);
    table.wall_count = 32;
    table.seats.get_mut(&1).unwrap().melds = vec![test_chi_meld(11)];

    assert_eq!(
        pure_one_suit_threat_suit(table.seats.get(&1).unwrap()),
        Some((1, 1))
    );
    assert!(
        pure_one_suit_threat_discard_bias(&table, 0, 18, 1)
            < pure_one_suit_threat_discard_bias(&table, 0, 24, 1)
    );
}

#[test]
fn pure_one_suit_threat_reads_closed_discard_pattern() {
    let mut table = table_with_discards(1, vec![1, 2, 11, 12, 31]);
    table.wall_count = 32;

    assert_eq!(
        pure_one_suit_threat_suit(table.seats.get(&1).unwrap()),
        Some((2, 0))
    );
    assert!(
        pure_one_suit_threat_discard_bias(&table, 0, 24, 1)
            < pure_one_suit_threat_discard_bias(&table, 0, 14, 1)
    );
}

#[test]
fn pure_one_suit_threat_ignores_weak_single_meld_evidence() {
    let mut weak_table = table_with_discards(1, vec![2, 22, 31]);
    weak_table.wall_count = 32;
    weak_table.seats.get_mut(&1).unwrap().melds = vec![test_chi_meld(11)];

    assert_eq!(
        pure_one_suit_threat_suit(weak_table.seats.get(&1).unwrap()),
        None
    );

    let mut same_suit_table = table_with_discards(1, vec![2, 22, 31, 35, 15]);
    same_suit_table.wall_count = 32;
    same_suit_table.seats.get_mut(&1).unwrap().melds = vec![test_chi_meld(11)];

    assert_eq!(
        pure_one_suit_threat_suit(same_suit_table.seats.get(&1).unwrap()),
        None
    );
}

#[test]
fn pure_one_suit_threat_ignores_ambiguous_closed_discards() {
    let only_honors = table_with_discards(1, vec![31, 32, 33, 35, 36]);
    let one_suit_only = table_with_discards(1, vec![1, 2, 3, 4, 5]);

    assert_eq!(
        pure_one_suit_threat_suit(only_honors.seats.get(&1).unwrap()),
        None
    );
    assert_eq!(
        pure_one_suit_threat_suit(one_suit_only.seats.get(&1).unwrap()),
        None
    );
}

#[test]
fn mid_round_discard_avoids_pure_one_suit_threat_suit() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 32;
    table.seats.get_mut(&1).unwrap().melds = vec![test_chi_meld(11), test_peng_meld(14)];
    let hand = vec![2, 3, 4, 6, 7, 8, 12, 16, 18, 22, 24, 26, 31, 35];

    assert!(
        pure_one_suit_threat_discard_bias(&table, 0, 18, 1)
            < pure_one_suit_threat_discard_bias(&table, 0, 22, 1)
    );
    assert_ne!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(18)
    );
}

#[test]
fn opponent_four_piao_threat_penalizes_live_pair_more_than_singleton() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().hand_count = 2;
    table.seats.get_mut(&1).unwrap().melds = vec![
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(21),
        test_peng_meld(31),
    ];

    assert!(
        opponent_threat_discard_bias(&table, 0, 5, 2)
            < opponent_threat_discard_bias(&table, 0, 6, 1)
    );
}

#[test]
fn opponent_four_piao_threat_ignores_impossible_two_missing_suits() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().hand_count = 2;
    table.seats.get_mut(&1).unwrap().melds = vec![
        test_peng_meld(2),
        test_peng_meld(3),
        test_peng_meld(4),
        test_peng_meld(5),
    ];

    assert_eq!(
        piao_missing_suits_from_melds(&table.seats.get(&1).unwrap().melds),
        vec![1, 2]
    );
    assert!(piao_threat_cannot_satisfy_three_suits(
        &table.seats.get(&1).unwrap().melds,
        table.seats.get(&1).unwrap().hand_count
    ));
    assert_eq!(opponent_threat_discard_bias(&table, 0, 31, 1), 0.0);

    table.seats.get_mut(&1).unwrap().melds = vec![
        test_peng_meld(2),
        test_peng_meld(3),
        test_peng_meld(12),
        test_peng_meld(13),
    ];
    assert_eq!(
        piao_missing_suits_from_melds(&table.seats.get(&1).unwrap().melds),
        vec![2]
    );
    assert!(!piao_threat_cannot_satisfy_three_suits(
        &table.seats.get(&1).unwrap().melds,
        table.seats.get(&1).unwrap().hand_count
    ));
    assert!(opponent_threat_discard_bias(&table, 0, 21, 1) < 0.0);
}

#[test]
fn opponent_four_piao_threat_penalizes_missing_suit_wait_tile() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().hand_count = 2;
    table.seats.get_mut(&1).unwrap().melds = vec![
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(12),
        test_peng_meld(31),
    ];

    assert_eq!(
        piao_missing_suits_from_melds(&table.seats.get(&1).unwrap().melds),
        vec![2]
    );
    assert!(
        opponent_threat_discard_bias(&table, 0, 25, 1)
            < opponent_threat_discard_bias(&table, 0, 15, 1)
    );
}

#[test]
fn piao_threat_penalizes_live_wind_pair_more_than_terminal_singleton() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];

    assert!(
        opponent_threat_discard_bias(&table, 0, 31, 2)
            < opponent_threat_discard_bias(&table, 0, 9, 1)
    );
}

#[test]
fn piao_threat_needing_yaojiu_penalizes_live_terminal_over_middle() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 32;
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(2), test_peng_meld(12), test_peng_meld(22)];

    assert!(piao_needs_terminal_or_honor_from_melds(
        &table.seats.get(&1).unwrap().melds
    ));
    assert!(
        opponent_threat_discard_bias(&table, 0, 9, 1)
            < opponent_threat_discard_bias(&table, 0, 5, 1)
    );
    assert!(
        opponent_threat_discard_bias(&table, 0, 31, 1)
            < opponent_threat_discard_bias(&table, 0, 5, 1)
    );
}

#[test]
fn piao_threat_discounts_exposed_meld_tiles() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_peng_meld(6)],
        },
    );

    assert!(
        opponent_threat_discard_bias(&table, 0, 6, 1)
            > opponent_threat_discard_bias(&table, 0, 5, 1)
    );
}

#[test]
fn late_defense_can_follow_exposed_middle_against_piao_threat() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];
    table.seats.insert(
        2,
        AiSeatView {
            position: 2,
            hand_count: 10,
            discards: Vec::new(),
            melds: vec![test_peng_meld(6)],
        },
    );
    let hand = vec![2, 3, 4, 5, 6, 7, 8, 12, 14, 16, 18, 22, 24, 26];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(6)
    );
}

#[test]
fn late_defense_avoids_breaking_wind_pair_against_piao_threat() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];
    let hand = vec![2, 4, 6, 8, 9, 12, 14, 16, 18, 22, 24, 26, 31, 31];

    assert_eq!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(9)
    );
}

#[test]
fn late_defense_avoids_piao_threat_missing_suit_wait_tiles() {
    let mut table = table_with_discards(1, Vec::new());
    table.wall_count = 16;
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(11), test_peng_meld(21), test_peng_meld(31)];
    let hand = vec![2, 3, 5, 6, 8, 12, 13, 15, 16, 18, 22, 23, 25, 28];

    assert!(!matches!(
        choose_discard_from_view(&hand, &table, 0, WIN_RULE_RELAXED),
        Some(2 | 3 | 5 | 6 | 8)
    ));
}

#[test]
fn opponent_threat_counts_ai_controlled_table_seat() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&1).unwrap().melds =
        vec![test_peng_meld(1), test_peng_meld(11), test_peng_meld(21)];

    assert!(opponent_threat_discard_bias(&table, 0, 5, 1) < 0.0);
}

#[test]
fn opponent_missing_suit_read_counts_ai_controlled_table_seat() {
    let mut table = table_with_discards(1, vec![11, 12, 13]);
    table.wall_count = 16;

    assert!(opponent_missing_suit_safety_bias(&table, 0, 14) > 0.0);
}

#[test]
fn ready_visible_cap_counts_four_gui_yi() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(2);
    let hand = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 9, 9, 21, 21];

    assert!(ready_visible_fan_reaches_cap(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_RELAXED
    ));
}

#[test]
fn ready_visible_cap_counts_concealed_dragon_triplet() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(2);
    let hand = vec![1, 2, 3, 11, 12, 13, 22, 23, 31, 31, 35, 35, 35];

    assert!(ready_visible_fan_reaches_cap(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_RELAXED
    ));
}

#[test]
fn ready_cap_counts_single_wait_fan() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(2);
    let hand = vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35];

    assert!(ready_visible_fan_reaches_cap(
        &hand,
        &[],
        &table,
        0,
        WIN_RULE_RELAXED
    ));
}

#[test]
fn ready_visible_cap_counts_piao_shou_ba_yi() {
    let mut table = table_with_discards(1, Vec::new());
    table.max_fan = Some(4);
    table.seats.get_mut(&0).unwrap().melds = vec![
        test_peng_meld(1),
        test_peng_meld(11),
        test_peng_meld(21),
        test_peng_meld(31),
    ];
    let hand = vec![35];
    let melds = table.seats.get(&0).unwrap().melds.as_slice();

    assert!(ready_visible_fan_reaches_cap(
        &hand,
        melds,
        &table,
        0,
        WIN_RULE_SHENYANG_BASIC
    ));
}

#[test]
fn remaining_tile_count_counts_own_public_tiles() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().discards = vec![31];
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(31)];

    assert_eq!(remaining_tile_count(&[], &table, 0, 31), 0);
}

#[test]
fn self_gang_allows_dragon_gang_after_opening_basic_hand() {
    let mut table = table_with_discards(1, Vec::new());
    table.seats.get_mut(&0).unwrap().melds = vec![test_peng_meld(1)];
    let hand = vec![11, 12, 13, 21, 22, 23, 31, 35, 35, 35, 35];

    assert_eq!(
        choose_self_gang_from_view(&hand, &[35], &table, 0, WIN_RULE_SHENYANG_BASIC),
        Some(35)
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

fn table_with_discards(position: usize, discards: Vec<i32>) -> AiPublicTable {
    let mut seats = HashMap::new();
    seats.insert(
        0,
        AiSeatView {
            position: 0,
            hand_count: 14,
            discards: Vec::new(),
            melds: Vec::new(),
        },
    );
    seats.insert(
        position,
        AiSeatView {
            position,
            hand_count: 10,
            discards,
            melds: Vec::new(),
        },
    );
    AiPublicTable {
        current_position: 0,
        dealer_position: 1,
        wall_count: 60,
        max_fan: None,
        claim_window: None,
        seats,
    }
}

fn dead_basic_heng_discards(hand: &[i32]) -> Vec<i32> {
    let mut counts = HashMap::<i32, usize>::new();
    for tile in hand.iter().copied() {
        *counts.entry(tile).or_default() += 1;
    }

    let mut discards = SHENYANG_MAHJONG_TILE_KINDS
        .into_iter()
        .flat_map(|tile| {
            let count = counts.get(&tile).copied().unwrap_or(0);
            let visible = if is_dragon(tile) && count < 2 {
                3
            } else if !is_dragon(tile) && count < 3 {
                2
            } else {
                0
            };
            std::iter::repeat_n(tile, visible)
        })
        .collect::<Vec<_>>();
    sort_tiles(&mut discards);
    discards
}

fn dead_terminal_or_honor_discards() -> Vec<i32> {
    SHENYANG_MAHJONG_TILE_KINDS
        .into_iter()
        .filter(|tile| is_honor(*tile) || tile_is_terminal(*tile))
        .flat_map(|tile| std::iter::repeat_n(tile, 4))
        .collect()
}

fn test_chi_meld(start_tile: i32) -> WsShenyangMahjongMeld {
    WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::CHI,
        tiles: vec![start_tile, start_tile + 1, start_tile + 2],
        from_position: Some(1),
    }
}

fn test_gang_meld(tile: i32) -> WsShenyangMahjongMeld {
    WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::GANG,
        tiles: vec![tile, tile, tile, tile],
        from_position: Some(1),
    }
}

fn test_concealed_gang_meld(tile: i32) -> WsShenyangMahjongMeld {
    WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::GANG,
        tiles: vec![tile, tile, tile, tile],
        from_position: None,
    }
}

fn test_peng_meld(tile: i32) -> WsShenyangMahjongMeld {
    WsShenyangMahjongMeld {
        kind: ShenyangMahjongMeldKind::PENG,
        tiles: vec![tile, tile, tile],
        from_position: Some(1),
    }
}
