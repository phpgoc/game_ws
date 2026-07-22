use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ws_common::{CommonGameState, GameState, SessionId};

use share_type_public::LandlordPhase;

use crate::core::play::{ComboKind, classify};

/// Stored in RoomState as `Box<dyn GameState>` while a landlord game is running.
/// Wraps the exact same loop state used by the game loop.
#[derive(Debug, Clone)]
pub struct LandlordGameState {
    inner: Arc<Mutex<LandlordLoopState>>,
}

/// 一次公开的出牌动作。`benchmark` 是行动前需要压过的牌；
/// `cards` 为空表示不出。AI 只使用这些公开信息推断未知手牌。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LandlordPlayRecord {
    pub position: usize,
    pub cards: Vec<i32>,
    pub benchmark: Vec<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LandlordSettlementSummary {
    /// 单个农民本局支付或获得的分数，地主按两倍结算。
    pub round_score: u32,
    pub multiplier: u32,
    pub bomb_count: u32,
    pub spring: bool,
}

/// Held exclusively by the game loop behind `Arc<std::sync::Mutex<>>`.
/// Contains all in-game mutable state.
/// `base` is shared with RoomService common state.
#[derive(Debug)]
pub struct LandlordLoopState {
    pub base: Arc<Mutex<CommonGameState>>,
    pub phase: LandlordPhase,
    /// The position that starts the call-landlord phase each deal.
    /// Rotates by 1 on redeal so a different player calls first.
    pub call_position: usize,
    pub current_position: usize,
    pub hands: HashMap<usize, Vec<i32>>,
    pub hidden_cards: Vec<i32>,
    pub landlord_position: Option<usize>,
    /// 当前最高叫分: 0 = 未叫, 1/2/3
    pub score: u32,
    /// 累计分数，按座位保存。只属于斗地主运行态，用于断线重连恢复显示。
    pub player_scores: HashMap<usize, i32>,
    /// 叫分记录: (position, score)
    pub call_history: Vec<(usize, u8)>,
    pub last_play_position: usize,
    pub last_play: Vec<i32>,
    pub current_play: Vec<i32>,
    /// 本副牌从开打起的完整公开动作历史，结算或重发牌时清空。
    pub play_history: Vec<LandlordPlayRecord>,
    /// 本副牌是否已经使用过一次 AI 炸弹延迟信号。
    pub ai_bomb_signal_used: bool,
    /// 当前仍持有炸弹的信号方；仅供两名 AI 农民内部协调角色。
    pub ai_bomb_signal_position: Option<usize>,
}

impl LandlordGameState {
    pub fn from_loop_state(inner: Arc<Mutex<LandlordLoopState>>) -> Self {
        Self { inner }
    }
}

impl GameState for LandlordGameState {
    fn can_accept_players(&self) -> bool {
        false
    }

    fn shared_common_state(&self) -> Arc<Mutex<CommonGameState>> {
        Arc::clone(&self.inner.lock().unwrap().base)
    }
}

impl LandlordLoopState {
    pub fn action_received(&self) -> bool {
        self.base.lock().unwrap().action_received
    }

    pub fn settlement_summary(&self, is_landlord_win: bool) -> LandlordSettlementSummary {
        let bomb_count = self
            .play_history
            .iter()
            .filter_map(|record| classify(&record.cards))
            .filter(|combo| matches!(combo.kind, ComboKind::Bomb | ComboKind::Rocket))
            .count() as u32;
        let spring = self.landlord_position.is_some_and(|landlord_position| {
            if is_landlord_win {
                self.play_history
                    .iter()
                    .all(|record| record.position == landlord_position || record.cards.is_empty())
            } else {
                self.play_history
                    .iter()
                    .filter(|record| {
                        record.position == landlord_position && !record.cards.is_empty()
                    })
                    .count()
                    == 1
            }
        });
        let doubling_count = bomb_count.saturating_add(u32::from(spring));
        let multiplier = 1_u32.checked_shl(doubling_count).unwrap_or(u32::MAX);
        LandlordSettlementSummary {
            round_score: self.score.max(1).saturating_mul(multiplier),
            multiplier,
            bomb_count,
            spring,
        }
    }

    pub fn apply_settlement_scores(&mut self, is_landlord_win: bool) -> LandlordSettlementSummary {
        let summary = self.settlement_summary(is_landlord_win);
        let Some(landlord_position) = self.landlord_position else {
            return summary;
        };
        let round_score = i32::try_from(summary.round_score).unwrap_or(i32::MAX);
        for position in self.players_snapshot().keys().copied() {
            let is_landlord = position == landlord_position;
            let delta = if is_landlord {
                if is_landlord_win {
                    round_score.saturating_mul(2)
                } else {
                    round_score.saturating_mul(-2)
                }
            } else if is_landlord_win {
                -round_score
            } else {
                round_score
            };
            let current = self.player_scores.entry(position).or_insert(0);
            *current = current.saturating_add(delta);
        }
        summary
    }

    pub fn clear_away(&self) {
        self.base.lock().unwrap().clear_away();
    }

    /// Shuffle a 54-card deck and deal 17 cards to each of the 3 sorted
    /// positions, storing the remaining 3 as hidden cards (底牌).
    /// Each player's hand is sorted for convenience.
    pub fn generate_card(&mut self) {
        let deck = Self::shuffle();
        let mut sorted: Vec<usize> = self.players_snapshot().keys().copied().collect();
        sorted.sort();
        self.hands.clear();
        for (i, &pos) in sorted.iter().enumerate() {
            let start = i * 17;
            let mut hand = deck[start..start + 17].to_vec();
            hand.sort_unstable();
            self.hands.insert(pos, hand);
        }
        self.hidden_cards = deck[51..54].to_vec();
    }

    pub fn has_disconnected_players(&self) -> bool {
        self.base.lock().unwrap().has_disconnected_players()
    }

    pub fn is_away(&self, pos: usize) -> bool {
        self.base.lock().unwrap().is_away(pos)
    }

    pub fn is_ai_position(&self, pos: usize) -> bool {
        self.base.lock().unwrap().is_ai_position(pos)
    }

    pub fn is_ai_takeover_position(&self, pos: usize) -> bool {
        self.base.lock().unwrap().is_ai_takeover_position(pos)
    }

    pub fn is_ai_controlled_position(&self, pos: usize) -> bool {
        self.is_ai_position(pos) || self.is_ai_takeover_position(pos)
    }

    pub fn is_disconnected(&self, pos: usize) -> bool {
        self.base.lock().unwrap().is_disconnected(pos)
    }

    pub fn is_paused(&self) -> bool {
        self.base.lock().unwrap().paused
    }

    pub fn mark_away(&self, pos: usize) -> bool {
        self.base.lock().unwrap().mark_away(pos)
    }

    pub fn new(base: Arc<Mutex<CommonGameState>>) -> Self {
        let call_position = {
            let state = base.lock().unwrap();
            state.players.keys().copied().min().unwrap_or(0)
        };
        Self {
            base,
            phase: LandlordPhase::Start,
            call_position,
            current_position: call_position,
            hands: HashMap::new(),
            hidden_cards: Vec::new(),
            landlord_position: None,
            score: 0,
            player_scores: HashMap::new(),
            call_history: Vec::new(),
            last_play_position: call_position,
            last_play: Vec::new(),
            current_play: Vec::new(),
            play_history: Vec::new(),
            ai_bomb_signal_used: false,
            ai_bomb_signal_position: None,
        }
    }

    /// Advance to the next game phase.
    /// Start → CallLandlord: current_position = call_position (first to call).
    /// CallLandlord → Play:  current_position = landlord_position.
    /// Play → Settlement:    current_position unchanged.
    pub fn next_phase(&mut self) {
        self.phase = self.phase.next();
        match self.phase {
            LandlordPhase::CallLandlord => {
                self.current_position = self.call_position;
            }
            LandlordPhase::Play => {
                if let Some(pos) = self.landlord_position {
                    self.current_position = pos;
                }
            }
            _ => {}
        }
    }

    pub fn player_name(&self, position: usize) -> String {
        self.base.lock().unwrap().player_name(position)
    }

    pub fn players_snapshot(&self) -> HashMap<usize, (SessionId, String)> {
        self.base.lock().unwrap().players.clone()
    }

    /// Reset for a new deal — called after settlement or all-pass.
    /// Rotates the starting caller so a different player calls first next deal.
    pub fn redeal(&mut self) {
        self.call_position = (self.call_position + 1) % 3;
        self.current_position = self.call_position;
        self.phase = LandlordPhase::Start;
        self.hands.clear();
        self.hidden_cards.clear();
        self.landlord_position = None;
        self.score = 0;
        self.call_history.clear();
        self.last_play_position = self.call_position;
        self.last_play.clear();
        self.current_play.clear();
        self.play_history.clear();
        self.ai_bomb_signal_used = false;
        self.ai_bomb_signal_position = None;
        self.set_action_received(false);
        self.set_turn_countdown(0);
        self.clear_away();
    }

    pub fn request_stop(&self) {
        self.base.lock().unwrap().request_stop();
    }

    pub fn set_action_received(&self, action_received: bool) {
        self.base.lock().unwrap().action_received = action_received;
    }

    pub fn set_turn_countdown(&self, turn_countdown: u32) {
        self.base.lock().unwrap().turn_countdown = turn_countdown;
    }

    fn shuffle() -> Vec<i32> {
        let mut deck: Vec<i32> = (1..=54).collect();
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(42);
        let mut rng = seed;
        for i in (1..deck.len()).rev() {
            rng = rng
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let j = (rng >> 33) as usize % (i + 1);
            deck.swap(i, j);
        }
        deck
    }

    pub fn stop_requested(&self) -> bool {
        self.base.lock().unwrap().stop_requested()
    }

    pub fn turn_countdown(&self) -> u32 {
        self.base.lock().unwrap().turn_countdown
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use ws_common::CommonGameState;

    use super::{LandlordLoopState, LandlordPlayRecord};

    fn state() -> LandlordLoopState {
        let mut common = CommonGameState::new();
        for position in 0..3 {
            common.add_player(position, position as u64 + 1, &format!("P{position}"));
        }
        let mut state = LandlordLoopState::new(Arc::new(Mutex::new(common)));
        state.landlord_position = Some(0);
        state.score = 2;
        state
    }

    fn record(position: usize, cards: Vec<i32>) -> LandlordPlayRecord {
        LandlordPlayRecord {
            position,
            cards,
            benchmark: Vec::new(),
        }
    }

    #[test]
    fn bombs_and_rocket_each_double_the_settlement_score() {
        let mut state = state();
        state.play_history = vec![
            record(0, vec![1, 14, 27, 40]),
            record(1, vec![2]),
            record(2, vec![53, 54]),
        ];

        let summary = state.apply_settlement_scores(true);

        assert_eq!(summary.bomb_count, 2);
        assert!(!summary.spring);
        assert_eq!(summary.multiplier, 4);
        assert_eq!(summary.round_score, 8);
        assert_eq!(state.player_scores[&0], 16);
        assert_eq!(state.player_scores[&1], -8);
        assert_eq!(state.player_scores[&2], -8);
    }

    #[test]
    fn landlord_spring_doubles_when_farmers_never_play() {
        let mut state = state();
        state.play_history = vec![
            record(0, vec![2]),
            record(1, Vec::new()),
            record(2, Vec::new()),
            record(0, vec![3]),
        ];

        let summary = state.apply_settlement_scores(true);

        assert!(summary.spring);
        assert_eq!(summary.multiplier, 2);
        assert_eq!(summary.round_score, 4);
        assert_eq!(state.player_scores[&0], 8);
        assert_eq!(state.player_scores[&1], -4);
        assert_eq!(state.player_scores[&2], -4);
    }

    #[test]
    fn farmer_anti_spring_requires_landlord_to_play_only_one_hand() {
        let mut anti_spring = state();
        anti_spring.play_history = vec![record(0, vec![2]), record(1, vec![3])];
        let summary = anti_spring.apply_settlement_scores(false);
        assert!(summary.spring);
        assert_eq!(summary.round_score, 4);
        assert_eq!(anti_spring.player_scores[&0], -8);
        assert_eq!(anti_spring.player_scores[&1], 4);
        assert_eq!(anti_spring.player_scores[&2], 4);

        let mut ordinary = state();
        ordinary.play_history = vec![
            record(0, vec![2]),
            record(1, vec![3]),
            record(0, vec![4]),
            record(2, vec![5]),
        ];
        let summary = ordinary.apply_settlement_scores(false);
        assert!(!summary.spring);
        assert_eq!(summary.round_score, 2);

        let mut invalid_without_landlord_play = state();
        invalid_without_landlord_play.play_history = vec![record(1, vec![3])];
        let summary = invalid_without_landlord_play.apply_settlement_scores(false);
        assert!(!summary.spring);
        assert_eq!(summary.round_score, 2);
    }

    #[test]
    fn settlement_scores_accumulate_across_redeals_and_remain_zero_sum() {
        let mut state = state();
        state.score = 1;
        state.play_history = vec![
            record(0, vec![2]),
            record(1, vec![3]),
            record(2, vec![4]),
            record(0, vec![5]),
        ];

        let first = state.apply_settlement_scores(true);
        assert_eq!(first.round_score, 1);
        assert_eq!(
            state.player_scores,
            HashMap::from([(0, 2), (1, -1), (2, -1)])
        );

        state.redeal();
        assert_eq!(
            state.player_scores,
            HashMap::from([(0, 2), (1, -1), (2, -1)])
        );

        state.landlord_position = Some(1);
        state.score = 2;
        state.play_history = vec![
            record(1, vec![2]),
            record(2, vec![3]),
            record(1, vec![4]),
            record(0, vec![5]),
        ];

        let second = state.apply_settlement_scores(false);
        assert_eq!(second.round_score, 2);
        assert_eq!(
            state.player_scores,
            HashMap::from([(0, 4), (1, -5), (2, 1)])
        );
        assert_eq!(state.player_scores.values().sum::<i32>(), 0);
    }
}
