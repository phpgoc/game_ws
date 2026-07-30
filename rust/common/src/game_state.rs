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
    /// Human positions with active official membership at their latest JOIN.
    ///
    /// This capability is cached for the room lifetime so AWAY, disconnect,
    /// and turn-timeout handling never need to query the data service again.
    pub member_positions: HashSet<usize>,
    /// Human seats temporarily controlled by game AI on the official server.
    ///
    /// This is deliberately separate from `ai_positions`: a takeover seat keeps
    /// its human session and can reclaim control with BACK.
    pub ai_takeover_positions: HashSet<usize>,
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

    /// Vacant positions that are reserved by the current game and must not be
    /// assigned to a newly joining player. A game can keep an explicitly quit
    /// player's seat reserved until the current hand is over. A roster entry
    /// marked disconnected remains replaceable by design.
    fn position_reserved_for_join(&self, _position: usize) -> bool {
        false
    }

    fn clear_away(&mut self) {
        self.shared_common_state().lock().unwrap().clear_away();
    }

    fn clear_away_position(&mut self, pos: usize) {
        let common = self.shared_common_state();
        let mut common = common.lock().unwrap();
        common.away_positions.remove(&pos);
        common.ai_takeover_positions.remove(&pos);
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

    fn is_ai_takeover_position(&self, pos: usize) -> bool {
        self.shared_common_state()
            .lock()
            .unwrap()
            .is_ai_takeover_position(pos)
    }

    fn is_member_position(&self, pos: usize) -> bool {
        self.shared_common_state()
            .lock()
            .unwrap()
            .is_member_position(pos)
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

    fn mark_ai_takeover_position(&mut self, pos: usize) {
        self.shared_common_state()
            .lock()
            .unwrap()
            .mark_ai_takeover_position(pos);
    }

    fn set_member_position(&mut self, pos: usize, enabled: bool) {
        self.shared_common_state()
            .lock()
            .unwrap()
            .set_member_position(pos, enabled);
    }

    fn clear_ai_takeover_position(&mut self, pos: usize) {
        self.shared_common_state()
            .lock()
            .unwrap()
            .clear_ai_takeover_position(pos);
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
    /// 在指定座位登记玩家，并清除该座位遗留的离线、托管和会员状态。
    pub fn add_player(&mut self, position: usize, session_id: SessionId, name: &str) {
        self.players
            .insert(position, (session_id, name.to_string()));
        self.away_positions.remove(&position);
        self.disconnected_positions.remove(&position);
        self.ai_positions.remove(&position);
        self.member_positions.remove(&position);
        self.ai_takeover_positions.remove(&position);
    }
    /// 清除本局全部超时托管及 AI 接管标记。
    pub fn clear_away(&mut self) {
        self.away_positions.clear();
        self.ai_takeover_positions.clear();
    }
    /// 清除指定座位的断线标记，通常在玩家成功重连后调用。
    pub fn clear_disconnected_position(&mut self, pos: usize) {
        self.disconnected_positions.remove(&pos);
    }
    /// 判断房间中是否仍有保留座位的断线玩家。
    pub fn has_disconnected_players(&self) -> bool {
        !self.disconnected_positions.is_empty()
    }
    /// 判断指定座位是否为服务器创建的 AI 玩家。
    pub fn is_ai_position(&self, pos: usize) -> bool {
        self.ai_positions.contains(&pos)
    }
    /// 判断指定人类座位当前是否正由游戏 AI 接管。
    pub fn is_ai_takeover_position(&self, pos: usize) -> bool {
        self.ai_takeover_positions.contains(&pos)
    }
    /// 判断指定座位在最近一次加入时是否拥有有效官方会员资格。
    pub fn is_member_position(&self, pos: usize) -> bool {
        self.member_positions.contains(&pos)
    }
    /// 判断指定座位是否已因超时被标记为托管。
    pub fn is_away(&self, pos: usize) -> bool {
        self.away_positions.contains(&pos)
    }
    /// 判断指定座位是否已断开连接但仍保留在房间中。
    pub fn is_disconnected(&self, pos: usize) -> bool {
        self.disconnected_positions.contains(&pos)
    }
    /// 将空座位标记为服务器托管的 AI 玩家，返回此前是否未标记。
    pub fn mark_ai_position(&mut self, pos: usize) -> bool {
        self.ai_positions.insert(pos)
    }
    /// 标记人类座位由 AI 临时接管，返回此前是否未标记。
    pub fn mark_ai_takeover_position(&mut self, pos: usize) -> bool {
        self.ai_takeover_positions.insert(pos)
    }
    /// 设置或清除指定座位的有效官方会员标记，并返回集合操作结果。
    pub fn set_member_position(&mut self, pos: usize, enabled: bool) -> bool {
        if enabled {
            self.member_positions.insert(pos)
        } else {
            self.member_positions.remove(&pos)
        }
    }
    /// 结束指定座位的 AI 临时接管，并返回该标记是否存在。
    pub fn clear_ai_takeover_position(&mut self, pos: usize) -> bool {
        self.ai_takeover_positions.remove(&pos)
    }
    /// 将指定座位标记为超时托管，并返回此前是否未标记。
    pub fn mark_away(&mut self, pos: usize) -> bool {
        self.away_positions.insert(pos)
    }
    /// 将指定座位标记为已断线但仍保留位置，并返回此前是否未标记。
    pub fn mark_disconnected(&mut self, pos: usize) -> bool {
        self.disconnected_positions.insert(pos)
    }

    /// 创建空的公共游戏状态。
    pub fn new() -> Self {
        Self::default()
    }

    /// 暂停公共状态的计时推进。
    pub fn pause(&mut self) {
        self.paused = true;
    }

    /// 获取指定座位的头像地址；不存在时返回空字符串。
    pub fn player_avatar(&self, position: usize) -> String {
        self.avatars.get(&position).cloned().unwrap_or_default()
    }

    /// 获取指定座位的玩家名称；不存在时返回空字符串。
    pub fn player_name(&self, position: usize) -> String {
        self.players
            .get(&position)
            .map(|(_, name)| name.clone())
            .unwrap_or_default()
    }

    /// 移除指定座位的玩家及其全部公共元数据。
    pub fn remove_player(&mut self, position: usize) {
        self.players.remove(&position);
        self.avatars.remove(&position);
        self.away_positions.remove(&position);
        self.disconnected_positions.remove(&position);
        self.ai_positions.remove(&position);
        self.member_positions.remove(&position);
        self.ai_takeover_positions.remove(&position);
    }

    /// 请求结束房间生命周期，并恢复为非暂停状态以便游戏循环退出。
    pub fn request_stop(&mut self) {
        self.stop_requested = true;
        self.paused = false;
    }
    /// 恢复公共状态的计时推进。
    pub fn resume(&mut self) {
        self.paused = false;
    }

    /// 为指定座位保存非空头像地址。
    pub fn set_avatar(&mut self, position: usize, avatar: &str) {
        if avatar.is_empty() {
            return;
        }
        self.avatars.insert(position, avatar.to_string());
    }

    /// 判断房间是否已请求游戏循环停止。
    pub fn stop_requested(&self) -> bool {
        self.stop_requested
    }

    /// 交换两个座位的玩家、头像及全部公共状态标记。
    pub fn swap_player(&mut self, pos_a: usize, pos_b: usize) {
        swap_map_entries(&mut self.players, pos_a, pos_b);
        swap_map_entries(&mut self.avatars, pos_a, pos_b);
        swap_set_membership(&mut self.away_positions, pos_a, pos_b);
        swap_set_membership(&mut self.disconnected_positions, pos_a, pos_b);
        swap_set_membership(&mut self.ai_positions, pos_a, pos_b);
        swap_set_membership(&mut self.member_positions, pos_a, pos_b);
        swap_set_membership(&mut self.ai_takeover_positions, pos_a, pos_b);
    }
}

impl SharedGameState {
    /// 用已有的公共状态句柄构造可被房间服务和游戏循环共享的状态包装器。
    pub fn from_common(common: Arc<Mutex<CommonGameState>>) -> Self {
        Self { common }
    }

    /// 创建包含空公共状态的共享状态包装器。
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
        state.set_member_position(1, true);
        state.mark_ai_takeover_position(1);

        state.remove_player(1);

        assert!(!state.players.contains_key(&1));
        assert!(!state.avatars.contains_key(&1));
        assert!(!state.is_away(1));
        assert!(!state.is_disconnected(1));
        assert!(!state.is_ai_position(1));
        assert!(!state.is_member_position(1));
        assert!(!state.is_ai_takeover_position(1));
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
        state.set_member_position(0, true);
        state.mark_ai_takeover_position(0);

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
        assert!(state.is_member_position(1));
        assert!(!state.is_member_position(0));
        assert!(state.is_ai_takeover_position(1));
        assert!(!state.is_ai_takeover_position(0));
    }

    #[test]
    fn clearing_away_also_ends_ai_takeover() {
        let mut state = CommonGameState::new();
        state.add_player(0, 10, "player");
        state.mark_away(0);
        state.mark_ai_takeover_position(0);

        state.clear_away();

        assert!(!state.is_away(0));
        assert!(!state.is_ai_takeover_position(0));
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

#[cfg(test)]
#[path = "game_state/tests.rs"]
mod external_tests;
