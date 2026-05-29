use std::collections::HashMap;

use ws_common::{SessionId, game_state::{CommonGameState, GameState}};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LandlordPhase {
    Start,
    CallLandlord,
    Play,
    Settlement,
}

impl LandlordPhase {
    fn next(self) -> Self {
        match self {
            Self::Start        => Self::CallLandlord,
            Self::CallLandlord => Self::Play,
            Self::Play         => Self::Settlement,
            Self::Settlement   => Self::Start,
        }
    }
}

/// Stored in RoomState as `Box<dyn GameState>`.
/// Only holds the player roster; implements GameState via CommonGameState.
#[derive(Debug, Default)]
pub struct LandlordGameState {
    pub base: CommonGameState,
}

impl LandlordGameState {
    pub fn new() -> Self { Self::default() }
}

impl GameState for LandlordGameState {
    fn common_state(&self) -> &CommonGameState { &self.base }
    fn common_state_mut(&mut self) -> &mut CommonGameState { &mut self.base }
}

/// Held exclusively by the game loop behind `Arc<std::sync::Mutex<>>`.
/// Contains all in-game mutable state.
/// `base` holds players, paused flag, and away_positions (shared concepts across all games).
#[derive(Debug)]
pub struct LandlordLoopState {
    pub base: CommonGameState,
    pub phase: LandlordPhase,
    /// The position that starts the call-landlord phase each deal.
    /// Rotates by 1 on redeal so a different player calls first.
    call_position: usize,
    pub current_position: usize,
    pub hands: HashMap<usize, Vec<i32>>,
    pub hidden_cards: Vec<i32>,
    pub landlord_position: Option<usize>,
    /// 叫分: 0 = 未叫, 1 = 叫地主, 2 = 抢地主, 3 = 超级抢
    pub score: u32,
    pub call_round_count: usize,
    pub last_play: Vec<i32>,
    pub current_play: Vec<i32>,
}

impl LandlordLoopState {
    pub fn new(players: HashMap<usize, (SessionId, String)>) -> Self {
        let call_position = players.keys().copied().min().unwrap_or(0);
        let mut base = CommonGameState::new();
        base.players = players;
        Self {
            base,
            phase: LandlordPhase::Start,
            call_position,
            current_position: call_position,
            hands: HashMap::new(),
            hidden_cards: Vec::new(),
            landlord_position: None,
            score: 0,
            call_round_count: 0,
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

    /// Reset for a new deal — called after settlement or all-pass.
    /// Rotates the starting caller so a different player calls first next deal.
    pub fn redeal(&mut self) {
        let n = self.base.players.len().max(1);
        self.call_position = (self.call_position + 1) % n;
        self.current_position = self.call_position;
        self.phase = LandlordPhase::Start;
        self.hands.clear();
        self.hidden_cards.clear();
        self.landlord_position = None;
        self.score = 0;
        self.call_round_count = 0;
        self.last_play.clear();
        self.current_play.clear();
        self.base.action_received = false;
        self.base.turn_countdown = 0;
        self.base.clear_away();
    }

    /// Shuffle a 54-card deck and deal 17 cards to each of the 3 sorted
    /// positions, storing the remaining 3 as hidden cards (底牌).
    pub fn generate_card(&mut self) {
        let deck = Self::shuffle();
        let mut sorted: Vec<usize> = self.base.players.keys().copied().collect();
        sorted.sort();
        self.hands.clear();
        for (i, &pos) in sorted.iter().enumerate() {
            let start = i * 17;
            self.hands.insert(pos, deck[start..start + 17].to_vec());
        }
        self.hidden_cards = deck[51..54].to_vec();
    }

    fn shuffle() -> Vec<i32> {
        let mut deck: Vec<i32> = (1..=54).collect();
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(42);
        let mut rng = seed;
        for i in (1..deck.len()).rev() {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let j = (rng >> 33) as usize % (i + 1);
            deck.swap(i, j);
        }
        deck
    }
}
