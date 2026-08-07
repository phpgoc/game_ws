use share_type_public::UpgradeSuit;

use crate::state::UpgradeGameState;

pub(crate) fn choose_bury(state: &UpgradeGameState) -> Option<Vec<i32>> {
    state.choose_fallback_bury()
}

pub(crate) fn best_trump_suit(_state: &UpgradeGameState, _position: usize) -> UpgradeSuit {
    UpgradeSuit::SPADE
}

pub fn decide(state: &UpgradeGameState, position: usize) -> Option<Vec<i32>> {
    state.choose_fallback_play(position)
}

#[cfg(test)]
#[path = "ai_fallback/tests.rs"]
mod tests;
