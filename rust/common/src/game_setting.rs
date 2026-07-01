use std::collections::HashMap;

/// 游戏设置 — 基于 HashMap 存储当前值。
/// 每个游戏的参数描述（GameParam）由各游戏在 build_room_settings 时提供。
#[derive(Debug, Clone, Default)]
pub struct GameSettings {
    /// 玩家数量限制
    pub min_players: usize,
    pub max_players: usize,
    /// 当前值 key-value 存储
    pub values: HashMap<String, i32>,
}

impl GameSettings {
    pub fn new(min_players: usize, max_players: usize) -> Self {
        Self {
            min_players,
            max_players,
            values: HashMap::new(),
        }
    }
}
