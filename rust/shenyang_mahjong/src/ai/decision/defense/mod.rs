mod broken;
mod closed;
mod late;
mod missing_suit;
mod piao_threat;
mod public_safety;
mod pure_threat;

use super::*;

pub(super) use broken::*;
pub(super) use closed::*;
pub(super) use late::*;
pub(super) use missing_suit::*;
pub(super) use piao_threat::*;
pub(super) use public_safety::*;
pub(super) use pure_threat::*;

pub(in crate::ai::decision) fn dealer_opponent_threat_scale(
    table: &AiPublicTable,
    seat_position: usize,
) -> f64 {
    if seat_position == table.dealer_position {
        1.2
    } else {
        1.0
    }
}
