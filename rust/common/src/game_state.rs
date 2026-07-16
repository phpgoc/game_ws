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
    /// 由服务器托管的虚拟玩家位置。AI 是房间成员，但不是 WebSocket session。
    pub ai_positions: HashSet<usize>,
    /// 房间生命周期已结束，游戏 loop 应尽快退出。
    pub stop_requested: bool,
}

/// Trait implemented by game-specific state objects.
/// Implementors provide a shared CommonGameState handle; all other methods are defaults.
pub trait GameState: Send {
    fn action_received(&self) -> bool {
        self.shared_common_state().lock().unwrap().action_received
    }

    fn add_player(&mut self, position: usize, session_id: SessionId, name: &str) {
        self.shared_common_state()
            .lock()
            .unwrap()
            .add_player(position, session_id, name);
    }

    fn can_accept_players(&self) -> bool {
        true
    }

    /// Whether a new human may join the room.
    ///
    /// This is intentionally separate from `can_accept_players`: a running
    /// game may keep its settings/AI/seat layout locked while still allowing
    /// spectators to join and wait for the next hand.
    fn can_join_players(&self) -> bool {
        self.can_accept_players()
    }

    /// Positions that are reserved by the current game and must not be
    /// assigned to a newly joining player.  A game can keep a disconnected or
    /// quit player's seat reserved until the current hand is over so a new
    /// player cannot inherit that hand's private state.
    fn position_reserved_for_join(&self, _position: usize) -> bool {
        false
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

    fn clear_disconnected_position(&mut self, pos: usize) {
        self.shared_common_state()
            .lock()
            .unwrap()
            .clear_disconnected_position(pos);
    }

    fn has_disconnected_players(&self) -> bool {
        self.shared_common_state()
            .lock()
            .unwrap()
            .has_disconnected_players()
    }

    fn is_ai_position(&self, pos: usize) -> bool {
        self.shared_common_state()
            .lock()
            .unwrap()
            .is_ai_position(pos)
    }

    fn is_away(&self, pos: usize) -> bool {
        self.shared_common_state().lock().unwrap().is_away(pos)
    }

    fn is_disconnected(&self, pos: usize) -> bool {
        self.shared_common_state()
            .lock()
            .unwrap()
            .is_disconnected(pos)
    }

    fn is_paused(&self) -> bool {
        self.shared_common_state().lock().unwrap().paused
    }

    fn mark_ai_position(&mut self, pos: usize) {
        self.shared_common_state()
            .lock()
            .unwrap()
            .mark_ai_position(pos);
    }

    fn mark_away(&mut self, pos: usize) {
        self.shared_common_state().lock().unwrap().mark_away(pos);
    }

    fn mark_disconnected(&mut self, pos: usize) {
        self.shared_common_state()
            .lock()
            .unwrap()
            .mark_disconnected(pos);
    }

    fn pause(&mut self) {
        self.shared_common_state().lock().unwrap().pause();
    }

    fn player_avatar(&self, position: usize) -> String {
        self.shared_common_state()
            .lock()
            .unwrap()
            .player_avatar(position)
    }

    fn player_name(&self, position: usize) -> String {
        self.shared_common_state()
            .lock()
            .unwrap()
            .player_name(position)
    }

    fn players(&self) -> HashMap<usize, (SessionId, String)> {
        self.shared_common_state().lock().unwrap().players.clone()
    }

    fn remove_player(&mut self, position: usize) {
        self.shared_common_state()
            .lock()
            .unwrap()
            .remove_player(position);
    }

    fn request_stop(&mut self) {
        self.shared_common_state().lock().unwrap().request_stop();
    }

    fn resume(&mut self) {
        self.shared_common_state().lock().unwrap().resume();
    }

    fn set_action_received(&mut self, received: bool) {
        self.shared_common_state().lock().unwrap().action_received = received;
    }

    fn set_avatar(&mut self, position: usize, avatar: &str) {
        self.shared_common_state()
            .lock()
            .unwrap()
            .set_avatar(position, avatar);
    }

    fn set_turn_countdown(&mut self, countdown: u32) {
        self.shared_common_state().lock().unwrap().turn_countdown = countdown;
    }

    fn shared_common_state(&self) -> Arc<Mutex<CommonGameState>>;

    fn stop_requested(&self) -> bool {
        self.shared_common_state().lock().unwrap().stop_requested()
    }

    fn swap_player(&mut self, pos_a: usize, pos_b: usize) {
        self.shared_common_state()
            .lock()
            .unwrap()
            .swap_player(pos_a, pos_b);
    }

    fn turn_countdown(&self) -> u32 {
        self.shared_common_state().lock().unwrap().turn_countdown
    }
}

/// Shared holder so room service and game loop can reference the same common state.
#[derive(Debug, Clone, Default)]
pub struct SharedGameState {
    common: Arc<Mutex<CommonGameState>>,
}

fn swap_map_entries<T>(values: &mut HashMap<usize, T>, pos_a: usize, pos_b: usize) {
    let a = values.remove(&pos_a);
    let b = values.remove(&pos_b);
    if let Some(value) = b {
        values.insert(pos_a, value);
    }
    if let Some(value) = a {
        values.insert(pos_b, value);
    }
}

fn swap_set_membership(values: &mut HashSet<usize>, pos_a: usize, pos_b: usize) {
    let a = values.remove(&pos_a);
    let b = values.remove(&pos_b);
    if b {
        values.insert(pos_a);
    }
    if a {
        values.insert(pos_b);
    }
}

impl CommonGameState {
    pub fn add_player(&mut self, position: usize, session_id: SessionId, name: &str) {
        self.players
            .insert(position, (session_id, name.to_string()));
        self.disconnected_positions.remove(&position);
        self.ai_positions.remove(&position);
    }
    pub fn clear_away(&mut self) {
        self.away_positions.clear();
    }
    pub fn clear_disconnected_position(&mut self, pos: usize) {
        self.disconnected_positions.remove(&pos);
    }
    pub fn has_disconnected_players(&self) -> bool {
        !self.disconnected_positions.is_empty()
    }
    pub fn is_ai_position(&self, pos: usize) -> bool {
        self.ai_positions.contains(&pos)
    }
    pub fn is_away(&self, pos: usize) -> bool {
        self.away_positions.contains(&pos)
    }
    pub fn is_disconnected(&self, pos: usize) -> bool {
        self.disconnected_positions.contains(&pos)
    }
    pub fn mark_ai_position(&mut self, pos: usize) -> bool {
        self.ai_positions.insert(pos)
    }
    pub fn mark_away(&mut self, pos: usize) -> bool {
        self.away_positions.insert(pos)
    }
    pub fn mark_disconnected(&mut self, pos: usize) -> bool {
        self.disconnected_positions.insert(pos)
    }

    pub fn new() -> Self {
        Self::default()
    }

    pub fn pause(&mut self) {
        self.paused = true;
    }

    pub fn player_avatar(&self, position: usize) -> String {
        self.avatars.get(&position).cloned().unwrap_or_default()
    }

    pub fn player_name(&self, position: usize) -> String {
        self.players
            .get(&position)
            .map(|(_, name)| name.clone())
            .unwrap_or_default()
    }

    pub fn remove_player(&mut self, position: usize) {
        self.players.remove(&position);
        self.avatars.remove(&position);
        self.away_positions.remove(&position);
        self.disconnected_positions.remove(&position);
        self.ai_positions.remove(&position);
    }

    pub fn request_stop(&mut self) {
        self.stop_requested = true;
        self.paused = false;
    }
    pub fn resume(&mut self) {
        self.paused = false;
    }

    pub fn set_avatar(&mut self, position: usize, avatar: &str) {
        if avatar.is_empty() {
            return;
        }
        self.avatars.insert(position, avatar.to_string());
    }

    pub fn stop_requested(&self) -> bool {
        self.stop_requested
    }

    pub fn swap_player(&mut self, pos_a: usize, pos_b: usize) {
        swap_map_entries(&mut self.players, pos_a, pos_b);
        swap_map_entries(&mut self.avatars, pos_a, pos_b);
        swap_set_membership(&mut self.away_positions, pos_a, pos_b);
        swap_set_membership(&mut self.disconnected_positions, pos_a, pos_b);
        swap_set_membership(&mut self.ai_positions, pos_a, pos_b);
    }
}

impl SharedGameState {
    pub fn from_common(common: Arc<Mutex<CommonGameState>>) -> Self {
        Self { common }
    }

    pub fn new() -> Self {
        Self::default()
    }
}

impl GameState for SharedGameState {
    fn shared_common_state(&self) -> Arc<Mutex<CommonGameState>> {
        Arc::clone(&self.common)
    }
}

#[cfg(test)]
mod tests {
    use super::CommonGameState;

    #[test]
    fn removing_player_clears_all_position_metadata() {
        let mut state = CommonGameState::new();
        state.add_player(1, 10, "player");
        state.set_avatar(1, "avatar");
        state.mark_away(1);
        state.mark_disconnected(1);
        state.mark_ai_position(1);

        state.remove_player(1);

        assert!(!state.players.contains_key(&1));
        assert!(!state.avatars.contains_key(&1));
        assert!(!state.is_away(1));
        assert!(!state.is_disconnected(1));
        assert!(!state.is_ai_position(1));
    }

    #[test]
    fn swapping_players_moves_all_position_metadata() {
        let mut state = CommonGameState::new();
        state.add_player(0, 10, "first");
        state.add_player(1, 11, "second");
        state.set_avatar(0, "first-avatar");
        state.set_avatar(1, "second-avatar");
        state.mark_away(0);
        state.mark_disconnected(1);
        state.mark_ai_position(0);

        state.swap_player(0, 1);

        assert_eq!(state.player_name(0), "second");
        assert_eq!(state.player_name(1), "first");
        assert_eq!(state.player_avatar(0), "second-avatar");
        assert_eq!(state.player_avatar(1), "first-avatar");
        assert!(state.is_away(1));
        assert!(!state.is_away(0));
        assert!(state.is_disconnected(0));
        assert!(!state.is_disconnected(1));
        assert!(state.is_ai_position(1));
        assert!(!state.is_ai_position(0));
    }

    #[test]
    fn swapping_with_empty_position_moves_player() {
        let mut state = CommonGameState::new();
        state.add_player(0, 10, "player");
        state.mark_away(0);

        state.swap_player(0, 2);

        assert!(!state.players.contains_key(&0));
        assert_eq!(state.player_name(2), "player");
        assert!(state.is_away(2));
    }
}
