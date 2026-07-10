use super::*;

pub(in crate::ai::decision) fn own_previous_discard_safety_bias(
    table: &AiPublicTable,
    position: usize,
    tile: i32,
) -> f64 {
    if !is_late_defense_round(table) {
        return 0.0;
    }
    own_previous_discard_count(table, position, tile) as f64 * 4.0
}

pub(in crate::ai::decision) fn wait_setting_discard_safety_adjustment(
    table: &AiPublicTable,
    position: usize,
    discard_tile: i32,
    own_tile_count: usize,
) -> f64 {
    let piao_threat = opponent_threat_discard_bias(table, position, discard_tile, own_tile_count);
    let pure_one_suit_threat =
        pure_one_suit_threat_discard_bias(table, position, discard_tile, own_tile_count);
    let safety = late_defense_tile_safety_score(table, position, discard_tile, own_tile_count)
        + mid_round_public_discard_bias(table, position, discard_tile)
        + mid_round_open_meld_safety_bias(table, discard_tile)
        + mid_broken_opponent_missing_suit_safety_bias(table, position, discard_tile);
    safety.clamp(-36.0, 36.0) * 0.6
        + piao_threat.min(0.0) * 1.5
        + pure_one_suit_threat.min(0.0) * 1.0
}

pub(in crate::ai::decision) fn should_open_broken_closed_hand_for_defense(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> bool {
    let already_open = if win_rule == WIN_RULE_SHENYANG_BASIC {
        has_door_opening_meld(melds, table)
    } else {
        has_open_meld(melds)
    };
    if already_open || !is_mid_broken_hand_defense_round(table) {
        return false;
    }
    if should_preserve_seven_pairs_plan_for_context(hand, melds, table, position, win_rule)
        || pure_one_suit_plan_score_for_context(hand, melds, table, position) > 0.0
        || piao_plan_score_for_context(hand, melds, table, position) >= 22.0
    {
        return false;
    }
    if ready_tile_score(hand, melds, table, position, win_rule) > 0.0
        || one_step_wait_potential(hand, melds, table, position, win_rule) > 0.0
    {
        return false;
    }

    let (missing_rule_requirements, unrecoverable_rule_requirements) =
        if win_rule == WIN_RULE_SHENYANG_BASIC {
            let missing_rule_requirements = [
                !missing_suits(hand, melds).is_empty(),
                !has_terminal_or_honor_with_extra(hand, melds, None),
                !has_triplet_or_dragon_pair(hand, melds),
            ]
            .into_iter()
            .filter(|missing| *missing)
            .count();
            let unrecoverable_rule_requirements =
                unrecoverable_basic_rule_requirement_count(hand, melds, table, position);
            (missing_rule_requirements, unrecoverable_rule_requirements)
        } else {
            (0, 0)
        };
    let power = hand_power(hand);
    if !is_late_round(table) {
        return unrecoverable_rule_requirements >= 1
            || missing_rule_requirements >= 2
            || power < 14.0;
    }
    unrecoverable_rule_requirements >= 1 || missing_rule_requirements >= 1 || power < 18.0
}

pub(in crate::ai::decision) fn should_pass_late_unready_claim_for_defense(
    table: &AiPublicTable,
    current_ready_score: f64,
) -> bool {
    is_late_defense_round(table) && current_ready_score <= 0.0
}

pub(in crate::ai::decision) fn should_use_broken_hand_public_defense_discard(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> bool {
    if is_late_defense_round(table)
        || !is_mid_broken_hand_defense_round(table)
        || !unique_tiles(hand).into_iter().any(|tile| {
            public_discard_count(table, tile) > 0
                || mid_round_open_meld_safety_bias(table, tile) > 0.0
                || mid_broken_opponent_missing_suit_safety_bias(table, position, tile) > 0.0
        })
    {
        return false;
    }
    if should_lock_seven_pairs_plan(hand, melds, table, position, win_rule)
        || pure_one_suit_plan_score_for_context(hand, melds, table, position) > 0.0
        || piao_plan_score_for_context(hand, melds, table, position) >= 22.0
    {
        return false;
    }
    if best_ready_score_after_discard(hand, melds, table, position, win_rule) > 0.0
        || best_one_step_wait_potential_after_discard(hand, melds, table, position, win_rule) > 0.0
    {
        return false;
    }

    let missing_rule_requirements = if win_rule == WIN_RULE_SHENYANG_BASIC {
        [
            !missing_suits(hand, melds).is_empty(),
            !has_terminal_or_honor_with_extra(hand, melds, None),
            !has_triplet_or_dragon_pair(hand, melds),
        ]
        .into_iter()
        .filter(|missing| *missing)
        .count()
    } else {
        0
    };
    let unrecoverable_rule_requirements = if win_rule == WIN_RULE_SHENYANG_BASIC {
        unrecoverable_basic_rule_requirement_count(hand, melds, table, position)
    } else {
        0
    };
    if table.dealer_position == position && unrecoverable_rule_requirements == 0 {
        return false;
    }
    let power_threshold = if is_late_round(table) { 18.0 } else { 16.0 };
    unrecoverable_rule_requirements >= 1
        || missing_rule_requirements >= 2
        || hand_power(hand) < power_threshold
}
