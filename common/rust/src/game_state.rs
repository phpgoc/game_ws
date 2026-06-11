use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use crate::SessionId;

/// Common player roster shared by all game states.
/// Handles the 3 roster mutations; game-specific state embeds this.
#[derive(Debug, Default)]
pub struct CommonGameState {
    pub players: HashMap<usize, (SessionId, String)>,
    /// 各 position 的头像 URL。
    pub avatars: HashMap<usize, String>,
    /// 游戏暂停时 tick 不递减。
    pub paused: bool,
    /// 当前轮是否已收到有效操作（由游戏循环消费输入后置 true）。
    pub action_received: bool,
    /// 当前轮剩余倒计时（秒）。
    pub turn_countdown: u32,
    /// 本局中已超时被标记为 away 的 position 集合。
    pub away_positions: HashSet<usize>,
    /// WebSocket 已断开但仍保留座位、允许按 name 重连的 position 集合。
    pub disconnected_positions: HashSet<usize>,
}

impl CommonGameState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_player(&mut self, position: usize, session_id: SessionId, name: &str) {
        self.players
            .insert(position, (session_id, name.to_string()));
        self.disconnected_positions.remove(&position);
    }

    pub fn set_avatar(&mut self, position: usize, avatar: &str) {
        if avatar.is_empty() {
            return;
        }
        self.avatars.insert(position, avatar.to_string());
    }

    pub fn player_avatar(&self, position: usize) -> String {
        self.avatars.get(&position).cloned().unwrap_or_default()
    }

    pub fn swap_player(&mut self, pos_a: usize, pos_b: usize) {
        let a = self.players.remove(&pos_a);
        let b = self.players.remove(&pos_b);
        let a_avatar = self.avatars.remove(&pos_a);
        let b_avatar = self.avatars.remove(&pos_b);
        if let Some(av) = b_avatar {
            self.avatars.insert(pos_a, av);
        }
        if let Some(av) = a_avatar {
            self.avatars.insert(pos_b, av);
        }
        let a_away = self.away_positions.remove(&pos_a);
        let b_away = self.away_positions.remove(&pos_b);
        let a_disconnected = self.disconnected_positions.remove(&pos_a);
        let b_disconnected = self.disconnected_positions.remove(&pos_b);
        if let Some(p) = b {
            self.players.insert(pos_a, p);
        }
        if let Some(p) = a {
            self.players.insert(pos_b, p);
        }
        if b_away {
            self.away_positions.insert(pos_a);
        }
        if a_away {
            self.away_positions.insert(pos_b);
        }
        if b_disconnected {
            self.disconnected_positions.insert(pos_a);
        }
        if a_disconnected {
            self.disconnected_positions.insert(pos_b);
        }
    }

    pub fn remove_player(&mut self, position: usize) {
        self.players.remove(&position);
        self.avatars.remove(&position);
        self.away_positions.remove(&position);
        self.disconnected_positions.remove(&position);
    }

    pub fn player_name(&self, position: usize) -> String {
        self.players
            .get(&position)
            .map(|(_, name)| name.clone())
            .unwrap_or_default()
    }

    pub fn pause(&mut self) {
        self.paused = true;
    }
    pub fn resume(&mut self) {
        self.paused = false;
    }
    pub fn mark_away(&mut self, pos: usize) -> bool {
        self.away_positions.insert(pos)
    }
    pub fn is_away(&self, pos: usize) -> bool {
        self.away_positions.contains(&pos)
    }
    pub fn clear_away(&mut self) {
        self.away_positions.clear();
    }
    pub fn mark_disconnected(&mut self, pos: usize) -> bool {
        self.disconnected_positions.insert(pos)
    }
    pub fn is_disconnected(&self, pos: usize) -> bool {
        self.disconnected_positions.contains(&pos)
    }
    pub fn has_disconnected_players(&self) -> bool {
        !self.disconnected_positions.is_empty()
    }
    pub fn clear_disconnected_position(&mut self, pos: usize) {
        self.disconnected_positions.remove(&pos);
    }
}

/// Shared holder so room service and game loop can reference the same common state.
#[derive(Debug, Clone, Default)]
pub struct SharedGameState {
    common: Arc<Mutex<CommonGameState>>,
}

impl SharedGameState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_common(common: Arc<Mutex<CommonGameState>>) -> Self {
        Self { common }
    }
}

/// Trait implemented by game-specific state objects.
/// Implementors provide a shared CommonGameState handle; all other methods are defaults.
pub trait GameState: Send {
    fn shared_common_state(&self) -> Arc<Mutex<CommonGameState>>;

    fn players(&self) -> HashMap<usize, (SessionId, String)> {
        self.shared_common_state().lock().unwrap().players.clone()
    }

    fn player_name(&self, position: usize) -> String {
        self.shared_common_state()
            .lock()
            .unwrap()
            .player_name(position)
    }

    fn add_player(&mut self, position: usize, session_id: SessionId, name: &str) {
        self.shared_common_state()
            .lock()
            .unwrap()
            .add_player(position, session_id, name);
    }

    fn set_avatar(&mut self, position: usize, avatar: &str) {
        self.shared_common_state()
            .lock()
            .unwrap()
            .set_avatar(position, avatar);
    }

    fn player_avatar(&self, position: usize) -> String {
        self.shared_common_state()
            .lock()
            .unwrap()
            .player_avatar(position)
    }

    fn swap_player(&mut self, pos_a: usize, pos_b: usize) {
        self.shared_common_state()
            .lock()
            .unwrap()
            .swap_player(pos_a, pos_b);
    }

    fn remove_player(&mut self, position: usize) {
        self.shared_common_state()
            .lock()
            .unwrap()
            .remove_player(position);
    }

    fn is_paused(&self) -> bool {
        self.shared_common_state().lock().unwrap().paused
    }

    fn pause(&mut self) {
        self.shared_common_state().lock().unwrap().pause();
    }

    fn resume(&mut self) {
        self.shared_common_state().lock().unwrap().resume();
    }

    fn mark_away(&mut self, pos: usize) {
        self.shared_common_state().lock().unwrap().mark_away(pos);
    }

    fn is_away(&self, pos: usize) -> bool {
        self.shared_common_state().lock().unwrap().is_away(pos)
    }

    fn clear_away(&mut self) {
        self.shared_common_state().lock().unwrap().clear_away();
    }

    fn clear_away_position(&mut self, pos: usize) {
        self.shared_common_state()
            .lock()
            .unwrap()
            .away_positions
            .remove(&pos);
    }

    fn mark_disconnected(&mut self, pos: usize) {
        self.shared_common_state()
            .lock()
            .unwrap()
            .mark_disconnected(pos);
    }

    fn is_disconnected(&self, pos: usize) -> bool {
        self.shared_common_state()
            .lock()
            .unwrap()
            .is_disconnected(pos)
    }

    fn has_disconnected_players(&self) -> bool {
        self.shared_common_state()
            .lock()
            .unwrap()
            .has_disconnected_players()
    }

    fn clear_disconnected_position(&mut self, pos: usize) {
        self.shared_common_state()
            .lock()
            .unwrap()
            .clear_disconnected_position(pos);
    }

    fn action_received(&self) -> bool {
        self.shared_common_state().lock().unwrap().action_received
    }

    fn set_action_received(&mut self, received: bool) {
        self.shared_common_state().lock().unwrap().action_received = received;
    }

    fn turn_countdown(&self) -> u32 {
        self.shared_common_state().lock().unwrap().turn_countdown
    }

    fn set_turn_countdown(&mut self, countdown: u32) {
        self.shared_common_state().lock().unwrap().turn_countdown = countdown;
    }
}

impl GameState for SharedGameState {
    fn shared_common_state(&self) -> Arc<Mutex<CommonGameState>> {
        Arc::clone(&self.common)
    }
}
