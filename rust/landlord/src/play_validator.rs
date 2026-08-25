use crate::core::play::{PlayValidationContext, validate_play};
use crate::game_state::LandlordLoopState;

/// Validate a play request. Takes a borrowed `LandlordLoopState` reference
/// (the caller should hold the lock).
pub(crate) fn validate_play_request(s: &LandlordLoopState, position: usize, cards: &[i32]) -> bool {
    // 请求层只组装不可变上下文，实际规则仍由 core::play 执行，避免 HTTP/WS
    // handler 和 AI 各自复制一份斗地主牌型判断。
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
