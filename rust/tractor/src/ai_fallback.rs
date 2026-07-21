use share_type_public::TractorSuit;

use crate::game_state::TractorGameState;

#[derive(Debug, Clone)]
pub(crate) struct DeclarationDecision {
    pub(crate) cards: Vec<i32>,
    pub(crate) assessment: TrumpAssessment,
}

#[derive(Debug, Clone)]
pub(crate) struct TrumpAssessment {
    pub(crate) score: i32,
    pub(crate) success_probability: f64,
}

pub(crate) fn declaration_decision(
    _state: &TractorGameState,
    _position: usize,
    _current_strength: i32,
    _forced: bool,
) -> Option<DeclarationDecision> {
    None
}

pub(crate) fn choose_bury(state: &TractorGameState) -> Option<Vec<i32>> {
    state.choose_timeout_bury()
}

pub(crate) fn best_trump_suit(_state: &TractorGameState, _position: usize) -> TractorSuit {
    TractorSuit::SPADE
}

pub fn decide(state: &TractorGameState, position: usize) -> Option<Vec<i32>> {
    state.choose_auto_play(position)
}
