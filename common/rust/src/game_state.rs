use std::collections::{HashMap, HashSet};

use crate::SessionId;

/// Common player roster shared by all game states.
/// Handles the 3 roster mutations; game-specific state embeds this.
#[derive(Debug, Default)]
pub struct CommonGameState {
    pub players: HashMap<usize, (SessionId, String)>,
    /// 游戏暂停时 tick 不递减。
    pub paused: bool,
    /// 当前轮是否已收到有效操作（由游戏循环消费输入后置 true）。
    pub action_received: bool,
    /// 当前轮剩余倒计时（秒）。
    pub turn_countdown: u32,
    /// 本局中已超时被标记为 away 的 position 集合。
    pub away_positions: HashSet<usize>,
}

impl CommonGameState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_player(&mut self, position: usize, session_id: SessionId, name: &str) {
        self.players.insert(position, (session_id, name.to_string()));
    }

    pub fn swap_player(&mut self, pos_a: usize, pos_b: usize) {
        let a = self.players.remove(&pos_a);
        let b = self.players.remove(&pos_b);
        if let Some(p) = b { self.players.insert(pos_a, p); }
        if let Some(p) = a { self.players.insert(pos_b, p); }
    }

    pub fn remove_player(&mut self, position: usize) {
        self.players.remove(&position);
    }

    pub fn player_name(&self, position: usize) -> String {
        self.players.get(&position).map(|(_, name)| name.clone()).unwrap_or_default()
    }

    pub fn pause(&mut self) { self.paused = true; }
    pub fn resume(&mut self) { self.paused = false; }
    pub fn mark_away(&mut self, pos: usize) { self.away_positions.insert(pos); }
    pub fn is_away(&self, pos: usize) -> bool { self.away_positions.contains(&pos) }
    pub fn clear_away(&mut self) { self.away_positions.clear(); }
}

/// Trait implemented by game-specific state objects.
/// Implementors provide `common_state[_mut]()`; all other methods are defaults.
pub trait GameState: Send {
    fn common_state(&self) -> &CommonGameState;
    fn common_state_mut(&mut self) -> &mut CommonGameState;

    fn players(&self) -> &HashMap<usize, (SessionId, String)> {
        &self.common_state().players
    }
    fn add_player(&mut self, position: usize, session_id: SessionId, name: &str) {
        self.common_state_mut().add_player(position, session_id, name);
    }
    fn swap_player(&mut self, pos_a: usize, pos_b: usize) {
        self.common_state_mut().swap_player(pos_a, pos_b);
    }
    fn remove_player(&mut self, position: usize) {
        self.common_state_mut().remove_player(position);
    }
    fn is_paused(&self) -> bool { self.common_state().paused }
    fn pause(&mut self) { self.common_state_mut().pause(); }
    fn resume(&mut self) { self.common_state_mut().resume(); }
    fn mark_away(&mut self, pos: usize) { self.common_state_mut().mark_away(pos); }
    fn is_away(&self, pos: usize) -> bool { self.common_state().is_away(pos) }
    fn clear_away(&mut self) { self.common_state_mut().clear_away(); }
}

/// Wrap CommonGameState so it can be used as Box<dyn GameState>.
impl GameState for CommonGameState {
    fn common_state(&self) -> &CommonGameState {
        self
    }
    fn common_state_mut(&mut self) -> &mut CommonGameState {
        self
    }
}
