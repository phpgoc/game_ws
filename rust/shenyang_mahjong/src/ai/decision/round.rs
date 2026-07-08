use super::*;

pub(super) fn is_late_defense_round(table: &AiPublicTable) -> bool {
    table.wall_count <= 20
}

pub(super) fn is_late_round(table: &AiPublicTable) -> bool {
    table.wall_count <= 42
}

pub(super) fn is_mid_round(table: &AiPublicTable) -> bool {
    table.wall_count <= 60
}

pub(super) fn is_mid_broken_hand_defense_round(table: &AiPublicTable) -> bool {
    table.wall_count <= 52
}

pub(super) fn is_mid_opening_round(table: &AiPublicTable) -> bool {
    table.wall_count <= 52
}
