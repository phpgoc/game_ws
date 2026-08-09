use share_type_public::WsDominoesLegalPlay;

use crate::core::DominoesRoundState;

/// 公开构建只提供确定性的合法超时兜底，不包含任何官方 AI 策略。
pub fn choose_play(
    _state: &DominoesRoundState,
    _position: usize,
    legal_plays: &[WsDominoesLegalPlay],
) -> WsDominoesLegalPlay {
    legal_plays.first().copied().unwrap_or(WsDominoesLegalPlay {
        tile_id: -1,
        endpoint_id: None,
        score: 0,
    })
}
