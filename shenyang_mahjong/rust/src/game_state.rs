use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use share_type_public::games::shenyang_mahjong::{
    SHENYANG_MAHJONG_TILE_KINDS, ShenyangMahjongAction, ShenyangMahjongMeldKind,
    ShenyangMahjongPhase, WsShenyangMahjongMeld,
};
use ws_common::{
    SessionId,
    game_state::{CommonGameState, GameState},
};

use crate::rules::{remove_tiles, sort_tiles};

#[derive(Debug, Clone)]
pub enum ClaimResponse {
    Pass,
    Chi { consume_tiles: Vec<i32> },
    Peng,
    Hu,
}

#[derive(Debug, Clone)]
pub struct ClaimWindowState {
    pub tile: i32,
    pub from_position: usize,
    pub eligible_positions: Vec<usize>,
    pub responses: HashMap<usize, ClaimResponse>,
}

#[derive(Debug, Clone)]
pub struct SettlementState {
    pub winner_positions: Vec<usize>,
    pub from_position: Option<usize>,
    pub win_tile: Option<i32>,
    pub is_self_draw: bool,
}

#[derive(Debug, Clone)]
pub struct ShenyangMahjongGameState {
    inner: Arc<Mutex<ShenyangMahjongLoopState>>,
}

#[derive(Debug)]
pub struct ShenyangMahjongLoopState {
    pub base: Arc<Mutex<CommonGameState>>,
    pub phase: ShenyangMahjongPhase,
    pub dealer_position: usize,
    pub current_position: usize,
    pub wall: Vec<i32>,
    pub hands: HashMap<usize, Vec<i32>>,
    pub discards: HashMap<usize, Vec<i32>>,
    pub melds: HashMap<usize, Vec<WsShenyangMahjongMeld>>,
    pub claim_window: Option<ClaimWindowState>,
    pub last_drawn_tile: Option<i32>,
    pub settlement: Option<SettlementState>,
}

pub fn build_meld(
    kind: ShenyangMahjongMeldKind,
    mut tiles: Vec<i32>,
    from_position: Option<usize>,
) -> WsShenyangMahjongMeld {
    sort_tiles(&mut tiles);
    WsShenyangMahjongMeld {
        kind,
        tiles,
        from_position: from_position.map(|position| position as i32),
    }
}

pub fn claim_action_to_play_action(response: &ClaimResponse) -> ShenyangMahjongAction {
    match response {
        ClaimResponse::Pass => ShenyangMahjongAction::PASS,
        ClaimResponse::Chi { .. } => ShenyangMahjongAction::CHI,
        ClaimResponse::Peng => ShenyangMahjongAction::PENG,
        ClaimResponse::Hu => ShenyangMahjongAction::HU,
    }
}

impl ShenyangMahjongGameState {
    pub fn from_loop_state(inner: Arc<Mutex<ShenyangMahjongLoopState>>) -> Self {
        Self { inner }
    }
}

impl GameState for ShenyangMahjongGameState {
    fn shared_common_state(&self) -> Arc<Mutex<CommonGameState>> {
        Arc::clone(&self.inner.lock().unwrap().base)
    }
}

impl ShenyangMahjongLoopState {
    pub fn action_received(&self) -> bool {
        self.base.lock().unwrap().action_received
    }

    pub fn clear_away(&self) {
        self.base.lock().unwrap().clear_away();
    }

    pub fn deal_new_round(&mut self) {
        self.phase = ShenyangMahjongPhase::Play;
        self.claim_window = None;
        self.last_drawn_tile = None;
        self.settlement = None;
        self.wall = Self::shuffle_wall();
        self.hands.clear();
        self.discards.clear();
        self.melds.clear();

        let mut positions: Vec<usize> = self.players_snapshot().keys().copied().collect();
        positions.sort_unstable();
        for position in &positions {
            self.discards.insert(*position, Vec::new());
            self.melds.insert(*position, Vec::new());
        }

        for _ in 0..13 {
            for position in &positions {
                if let Some(tile) = self.wall.pop() {
                    self.hands.entry(*position).or_default().push(tile);
                }
            }
        }
        if let Some(tile) = self.wall.pop() {
            self.hands
                .entry(self.dealer_position)
                .or_default()
                .push(tile);
            self.last_drawn_tile = Some(tile);
        }
        for hand in self.hands.values_mut() {
            sort_tiles(hand);
        }
        self.current_position = self.dealer_position;
        self.set_action_received(false);
        self.clear_away();
    }

    pub fn draw_for_position(&mut self, position: usize) -> Option<i32> {
        let tile = self.wall.pop()?;
        let hand = self.hands.entry(position).or_default();
        hand.push(tile);
        sort_tiles(hand);
        self.current_position = position;
        self.last_drawn_tile = Some(tile);
        Some(tile)
    }

    pub fn enter_settlement(
        &mut self,
        winner_positions: Vec<usize>,
        from_position: Option<usize>,
        win_tile: Option<i32>,
        is_self_draw: bool,
    ) {
        self.phase = ShenyangMahjongPhase::Settlement;
        self.claim_window = None;
        self.last_drawn_tile = None;
        self.settlement = Some(SettlementState {
            winner_positions,
            from_position,
            win_tile,
            is_self_draw,
        });
        self.set_action_received(false);
    }

    pub fn new(base: Arc<Mutex<CommonGameState>>) -> Self {
        let dealer_position = {
            let state = base.lock().unwrap();
            state.players.keys().copied().min().unwrap_or(0)
        };
        Self {
            base,
            phase: ShenyangMahjongPhase::Start,
            dealer_position,
            current_position: dealer_position,
            wall: Vec::new(),
            hands: HashMap::new(),
            discards: HashMap::new(),
            melds: HashMap::new(),
            claim_window: None,
            last_drawn_tile: None,
            settlement: None,
        }
    }

    pub fn next_phase(&mut self, phase: ShenyangMahjongPhase) {
        self.phase = phase;
    }

    pub fn next_position(&self, position: usize) -> usize {
        let mut positions: Vec<usize> = self.players_snapshot().keys().copied().collect();
        positions.sort_unstable();
        if positions.is_empty() {
            return position;
        }
        let current_index = positions
            .iter()
            .position(|item| *item == position)
            .unwrap_or(0);
        positions[(current_index + 1) % positions.len()]
    }

    pub fn player_name(&self, position: usize) -> String {
        self.base.lock().unwrap().player_name(position)
    }

    pub fn players_snapshot(&self) -> HashMap<usize, (SessionId, String)> {
        self.base.lock().unwrap().players.clone()
    }

    pub fn redeal(&mut self) {
        self.dealer_position = self.next_position(self.dealer_position);
        self.current_position = self.dealer_position;
        self.phase = ShenyangMahjongPhase::Start;
        self.wall.clear();
        self.hands.clear();
        self.discards.clear();
        self.melds.clear();
        self.claim_window = None;
        self.last_drawn_tile = None;
        self.settlement = None;
        self.set_action_received(false);
        self.set_turn_countdown(0);
        self.clear_away();
    }

    pub fn remove_last_discard(&mut self, position: usize) {
        if let Some(discards) = self.discards.get_mut(&position) {
            let _ = discards.pop();
        }
    }

    pub fn remove_tiles_from_hand(&mut self, position: usize, tiles: &[i32]) -> bool {
        let Some(hand) = self.hands.get_mut(&position) else {
            return false;
        };
        remove_tiles(hand, tiles)
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

    fn shuffle_wall() -> Vec<i32> {
        let mut wall = Vec::with_capacity(136);
        for tile in SHENYANG_MAHJONG_TILE_KINDS {
            for _ in 0..4 {
                wall.push(tile);
            }
        }
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(42);
        let mut rng = seed;
        for index in (1..wall.len()).rev() {
            rng = rng
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let swap_index = (rng >> 33) as usize % (index + 1);
            wall.swap(index, swap_index);
        }
        wall
    }

    pub fn stop_requested(&self) -> bool {
        self.base.lock().unwrap().stop_requested()
    }

    pub fn turn_countdown(&self) -> u32 {
        self.base.lock().unwrap().turn_countdown
    }

    pub fn wall_count(&self) -> usize {
        self.wall.len()
    }
}
