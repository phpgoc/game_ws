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

pub(in crate::ai::decision) fn dealer_opponent_has_major_threat(
    table: &AiPublicTable,
    position: usize,
) -> bool {
    if position == table.dealer_position {
        return false;
    }
    let Some(dealer) = table.seats.get(&table.dealer_position) else {
        return false;
    };
    let piao_threat = piao_threat_level(&dealer.melds) >= 3
        && has_open_meld(&dealer.melds)
        && !piao_threat_cannot_satisfy_three_suits(&dealer.melds, dealer.hand_count);
    let closed_threat = closed_opponent_has_major_threat(dealer, table);
    piao_threat || pure_one_suit_threat_suit(dealer).is_some() || closed_threat
}

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
