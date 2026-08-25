//! 西洋骨牌 WebSocket 运行态包装。

use std::sync::{Arc, Mutex};

use ws_common::{CommonGameState, GameState};

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
}

impl GameState for DominoesGameState {
    fn can_accept_players(&self) -> bool {
        false
    }

    fn can_join_players(&self) -> bool {
        false
    }

    fn shared_common_state(&self) -> Arc<Mutex<CommonGameState>> {
        Arc::clone(&self.inner.lock().expect("dominoes state lock").base)
    }
}
