use crate::core::play::{PlayValidationContext, validate_play};
use crate::game_state::LandlordLoopState;

/// Validate a play request. Takes a borrowed `LandlordLoopState` reference
/// (the caller should hold the lock).
pub(crate) fn validate_play_request(s: &LandlordLoopState, position: usize, cards: &[i32]) -> bool {
    validate_play(
        PlayValidationContext {
            phase: s.phase,
            current_position: s.current_position,
            hand: s.hands.get(&position).map(Vec::as_slice),
            last_play_position: s.last_play_position,
            last_play: &s.last_play,
        },
        position,
        cards,
    )
}
