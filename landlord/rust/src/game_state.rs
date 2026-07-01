use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ws_common::{
    SessionId,
    game_state::{CommonGameState, GameState},
};

use share_type_public::LandlordPhase;

/// Stored in RoomState as `Box<dyn GameState>` while a landlord game is running.
/// Wraps the exact same loop state used by the game loop.
#[derive(Debug, Clone)]
pub struct LandlordGameState {
    inner: Arc<Mutex<LandlordLoopState>>,
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
}

impl LandlordGameState {
    pub fn from_loop_state(inner: Arc<Mutex<LandlordLoopState>>) -> Self {
        Self { inner }
    }
}

impl GameState for LandlordGameState {
    fn shared_common_state(&self) -> Arc<Mutex<CommonGameState>> {
        Arc::clone(&self.inner.lock().unwrap().base)
    }
}

impl LandlordLoopState {
    pub fn action_received(&self) -> bool {
        self.base.lock().unwrap().action_received
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
        self.set_action_received(false);
        self.set_turn_countdown(0);
        self.clear_away();
    }

    pub fn apply_settlement_scores(&mut self, is_landlord_win: bool) {
        let Some(landlord_position) = self.landlord_position else {
            return;
        };
        let base_score = self.score.max(1) as i32;
        for position in self.players_snapshot().keys().copied() {
            let is_landlord = position == landlord_position;
            let delta = if is_landlord {
                if is_landlord_win {
                    base_score * 2
                } else {
                    base_score * -2
                }
            } else if is_landlord_win {
                -base_score
            } else {
                base_score
            };
            *self.player_scores.entry(position).or_insert(0) += delta;
        }
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
