use super::super::*;
use super::heng::can_recover_basic_heng;

pub(in crate::ai::decision) fn basic_heng_seed_discard_bias(
    hand: &[i32],
    tile: i32,
    melds: &[WsShenyangMahjongMeld],
) -> f64 {
    if has_triplet_or_dragon_pair(hand, melds) {
        return 0.0;
    }
    let count = hand.iter().filter(|item| **item == tile).count();
    if is_dragon(tile) && count == 1 {
        -7.0
    } else if count == 2 {
        -4.0
    } else {
        0.0
    }
}

pub(in crate::ai::decision) fn shenyang_rule_progress_score(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
) -> f64 {
    if should_lock_seven_pairs_plan(hand, melds, table, position) {
        return 0.0;
    }
    let pure_score = pure_one_suit_plan_score_for_context(hand, melds, table, position);
    if pure_score > 0.0 {
        return pure_score;
    }
    let mut score = 0.0;
    let suits = suit_presence(hand, melds);
    let suit_count = suits.into_iter().filter(|present| *present).count();
    score += match suit_count {
        3 => 10.0,
        2 => -6.0,
        1 => -14.0,
        _ => -20.0,
    };
    if has_door_opening_meld(melds, table) {
        score += 9.0;
    } else {
        score -= 8.0;
    }
    if has_terminal_or_honor_with_extra(hand, melds, None) {
        score += 7.0;
    } else {
        score -= 10.0;
    }
    if has_triplet_or_dragon_pair(hand, melds) {
        score += 8.0;
    } else if can_recover_basic_heng(hand, melds, table, position) {
        score -= 5.0;
    } else {
        score -= 16.0;
    }
    score
}

pub(in crate::ai::decision) fn unrecoverable_normal_hand_requirement_count(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
) -> usize {
    let missing_suits = missing_suits(hand, melds)
        .into_iter()
        .filter(|suit| live_tile_count_for_suit(hand, table, *suit) <= 0)
        .count();
    let missing_terminal_or_honor = !has_terminal_or_honor_with_extra(hand, melds, None)
        && live_terminal_or_honor_count(hand, table) <= 0;
    let missing_heng = !can_recover_basic_heng(hand, melds, table, position);
    missing_suits + usize::from(missing_terminal_or_honor) + usize::from(missing_heng)
}
