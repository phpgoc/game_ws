use super::*;

pub(in crate::ai::decision) fn ting_opponent_threat_discard_bias(
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    own_tile_count: usize,
) -> f64 {
    if !is_valid_tile(tile)
        || public_discard_count(table, tile) > 0
        || late_defense_tile_fully_accounted(table, tile, own_tile_count)
    {
        return 0.0;
    }

    let exposure_scale = live_risk_exposure_scale(table, tile);
    let base_risk = if is_honor(tile) {
        10.0
    } else if tile_is_terminal(tile) {
        14.0
    } else {
        20.0
    };
    let pair_risk = if own_tile_count >= 2 {
        if is_honor(tile) || tile_is_terminal(tile) {
            4.0
        } else {
            3.0
        }
    } else {
        0.0
    };

    table
        .ting_positions
        .iter()
        .filter(|ting_position| {
            **ting_position != position && table.seats.contains_key(ting_position)
        })
        .map(|ting_position| {
            -(base_risk + pair_risk)
                * exposure_scale
                * dealer_opponent_threat_scale(table, *ting_position)
        })
        .sum()
}
