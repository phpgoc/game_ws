//! 西洋骨牌 WebSocket 运行态包装。

use std::sync::{Arc, Mutex};

use share_type_public::DominoesPhase;
use ws_common::{CommonGameState, GameState, SessionId};

use crate::core::DominoesRoundState;

/// 西洋骨牌运行态。公共房间状态由 `base` 共享，牌局数据只在本类型内部持有。
#[derive(Debug, Clone)]
pub struct DominoesGameState {
    /// 通过通用 GameState 接口挂入 RoomService 的核心轮状态。
    pub inner: Arc<Mutex<DominoesRoundState>>,
}

impl DominoesGameState {
    pub fn new(state: DominoesRoundState) -> Self {
        Self {
            inner: Arc::new(Mutex::new(state)),
        }
    }

    fn refresh_returned_turn(round: &mut DominoesRoundState, position: usize, was_automatic: bool) {
        if was_automatic
            && round.current_position == position
            && matches!(
                round.phase,
                DominoesPhase::StarterSelection | DominoesPhase::Play
            )
        {
            // 接管/掉线已将本手压至短倒计时；本人回来后给完整操作时间，
            // 同时使循环睡眠前保存的旧 revision 失效。重复 JOIN 不续时。
            round.refresh_current_turn();
        }
    }
}

impl GameState for DominoesGameState {
    fn add_player(&mut self, position: usize, session_id: SessionId, name: &str) {
        let mut round = self.inner.lock().expect("dominoes state lock");
        let was_automatic = {
            let mut common = round.base.lock().expect("dominoes common state lock");
            let was_automatic = common.is_disconnected(position)
                || common.is_away(position)
                || common.is_ai_takeover_position(position);
            common.add_player(position, session_id, name);
            was_automatic
        };
        Self::refresh_returned_turn(&mut round, position, was_automatic);
    }

    fn clear_away_position(&mut self, position: usize) {
        let mut round = self.inner.lock().expect("dominoes state lock");
        let was_automatic = {
            let mut common = round.base.lock().expect("dominoes common state lock");
            let was_automatic = (common.is_away(position)
                || common.is_ai_takeover_position(position))
                && !common.is_disconnected(position)
                && !common.is_ai_position(position);
            common.away_positions.remove(&position);
            common.ai_takeover_positions.remove(&position);
            was_automatic
        };
        Self::refresh_returned_turn(&mut round, position, was_automatic);
    }

    fn can_accept_players(&self) -> bool {
        self.inner.lock().expect("dominoes state lock").phase == DominoesPhase::GameOver
    }

    fn shared_common_state(&self) -> Arc<Mutex<CommonGameState>> {
        Arc::clone(&self.inner.lock().expect("dominoes state lock").base)
    }
}

#[cfg(test)]
#[path = "game_state/tests.rs"]
mod tests;
