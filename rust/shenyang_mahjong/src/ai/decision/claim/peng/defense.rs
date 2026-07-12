use super::*;

pub(in crate::ai::decision) fn should_pass_peng_for_open_pure_defense(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    tile: i32,
) -> bool {
    if is_dragon(tile)
        || !has_open_meld(melds)
        || table.dealer_position == position
        || !is_late_round(table)
        || should_preserve_seven_pairs_plan_for_context(hand, melds, table, position, win_rule)
        || pure_one_suit_plan_score_for_context(hand, melds, table, position) > 0.0
        || piao_plan_score_for_context(hand, melds, table, position) >= 22.0
    {
        return false;
    }
    ready_tile_score(hand, melds, table, position, win_rule) <= 0.0
        && one_step_wait_potential(hand, melds, table, position, win_rule) <= 0.0
        && hand_power(hand) < 18.0
}
