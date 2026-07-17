use super::*;

pub(in crate::ai::decision) fn should_claim_chi_to_open_broken_hand_for_defense(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
) -> bool {
    if has_open_meld(melds)
        || !is_late_round(table)
        || should_preserve_seven_pairs_plan_for_context(hand, melds, table, position)
        || pure_one_suit_plan_score_for_context(hand, melds, table, position) > 0.0
        || piao_plan_score_for_context(hand, melds, table, position) >= 22.0
    {
        return false;
    }
    ready_tile_score(hand, melds, table, position) <= 0.0
        && one_step_wait_potential(hand, melds, table, position) <= 0.0
        && hand_power(hand) < 18.0
}
