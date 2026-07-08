use super::*;

pub(super) const FINAL_DEFENSE_WALL_COUNT: usize = 20;
pub(super) const LATE_PRESSURE_WALL_COUNT: usize = 42;
pub(super) const MID_BROKEN_HAND_WALL_COUNT: usize = 52;
pub(super) const MID_ROUND_WALL_COUNT: usize = 60;

pub(super) fn is_late_defense_round(table: &AiPublicTable) -> bool {
    table.wall_count <= FINAL_DEFENSE_WALL_COUNT
}

pub(super) fn is_late_round(table: &AiPublicTable) -> bool {
    table.wall_count <= LATE_PRESSURE_WALL_COUNT
}

pub(super) fn is_mid_round(table: &AiPublicTable) -> bool {
    table.wall_count <= MID_ROUND_WALL_COUNT
}

pub(super) fn is_mid_broken_hand_defense_round(table: &AiPublicTable) -> bool {
    table.wall_count <= MID_BROKEN_HAND_WALL_COUNT
}

pub(super) fn is_mid_opening_round(table: &AiPublicTable) -> bool {
    table.wall_count <= MID_BROKEN_HAND_WALL_COUNT
}
