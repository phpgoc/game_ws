use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use share_type_public::games::shenyang_mahjong::{
    SHENYANG_MAHJONG_TILE_KINDS, ShenyangMahjongAction, ShenyangMahjongMeldKind,
    ShenyangMahjongPhase, WsShenyangMahjongMeld,
};
use ws_common::{CommonGameState, GameState, SessionId};

use crate::rules::{XI_GANG_WINDS, is_valid_meld, remove_tiles, sort_tiles, xi_gang_options};

const WALL_SEED_ENV: &str = "SHENYANG_MAHJONG_WALL_SEED";

#[derive(Debug, Clone)]
pub enum ClaimResponse {
    Pass,
    Chi { consume_tiles: Vec<i32> },
    Peng,
    Gang,
    Hu,
}

#[derive(Debug, Clone)]
pub enum ClaimWindowKind {
    Discard,
    RobGang,
}

#[derive(Debug, Clone)]
pub struct ClaimWindowState {
    pub tile: i32,
    pub from_position: usize,
    pub kind: ClaimWindowKind,
    pub eligible_positions: Vec<usize>,
    pub responses: HashMap<usize, ClaimResponse>,
}

#[derive(Debug, Clone)]
pub struct SettlementState {
    pub winner_positions: Vec<usize>,
    pub from_position: Option<usize>,
    pub win_tile: Option<i32>,
    pub is_self_draw: bool,
    pub is_reverse_win: bool,
    pub is_gang_draw: bool,
    pub is_haidilao: bool,
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
    pub pending_gang_draw: bool,
    pub first_normal_draw_positions: HashSet<usize>,
    pub xi_gang_options: HashMap<usize, Vec<Vec<i32>>>,
    pub ting_positions: HashSet<usize>,
    pub settlement: Option<SettlementState>,
    wall_seed_base: Option<u64>,
    wall_round_index: u64,
    last_wall_seed: Option<u64>,
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
        ClaimResponse::Gang => ShenyangMahjongAction::GANG,
        ClaimResponse::Hu => ShenyangMahjongAction::HU,
    }
}

pub(crate) fn meld_source_is_valid_for_positions(
    meld: &WsShenyangMahjongMeld,
    position: usize,
    player_positions: &HashSet<usize>,
) -> bool {
    match (meld.kind, meld.from_position) {
        (ShenyangMahjongMeldKind::GANG | ShenyangMahjongMeldKind::XI_GANG, None) => true,
        (_, Some(source)) => usize::try_from(source).ok().is_some_and(|source| {
            source != position
                && player_positions.contains(&source)
                && (meld.kind != ShenyangMahjongMeldKind::CHI
                    || next_player_position(source, player_positions) == Some(position))
        }),
        _ => false,
    }
}

fn next_player_position(current: usize, player_positions: &HashSet<usize>) -> Option<usize> {
    player_positions
        .iter()
        .copied()
        .filter(|position| *position > current)
        .min()
        .or_else(|| player_positions.iter().copied().min())
}

fn system_wall_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(42)
}

fn wall_seed_base_from_env() -> Option<u64> {
    let value = std::env::var(WALL_SEED_ENV).ok()?;
    match value.parse::<u64>() {
        Ok(seed) => Some(seed),
        Err(_) => {
            ws_common::dlog!(
                ws_common::tracing::Level::WARN,
                "ignoring invalid {WALL_SEED_ENV}={value:?}; expected unsigned integer"
            );
            None
        }
    }
}

impl SettlementState {
    pub fn unique_winner_positions(&self) -> Vec<usize> {
        let mut seen = HashSet::new();
        self.winner_positions
            .iter()
            .copied()
            .filter(|position| seen.insert(*position))
            .collect()
    }
}

impl ShenyangMahjongGameState {
    pub fn from_loop_state(inner: Arc<Mutex<ShenyangMahjongLoopState>>) -> Self {
        Self { inner }
    }
}

impl GameState for ShenyangMahjongGameState {
    fn can_accept_players(&self) -> bool {
        false
    }

    fn shared_common_state(&self) -> Arc<Mutex<CommonGameState>> {
        Arc::clone(&self.inner.lock().unwrap().base)
    }
}

impl ShenyangMahjongLoopState {
    pub fn action_received(&self) -> bool {
        self.base.lock().unwrap().action_received
    }

    pub fn clear_xi_gang_options(&mut self, position: usize) {
        self.xi_gang_options.remove(&position);
    }

    pub fn deal_new_round(&mut self) {
        self.phase = ShenyangMahjongPhase::Play;
        self.claim_window = None;
        self.last_drawn_tile = None;
        self.pending_gang_draw = false;
        self.first_normal_draw_positions.clear();
        self.xi_gang_options.clear();
        self.ting_positions.clear();
        self.settlement = None;
        let seed = self.next_wall_seed();
        self.wall = Self::shuffle_wall_with_seed(seed);
        self.last_wall_seed = Some(seed);
        self.wall_round_index = self.wall_round_index.wrapping_add(1);
        ws_common::dlog!(
            ws_common::tracing::Level::INFO,
            "shenyang mahjong deal wall_seed={seed} wall_round_index={}",
            self.wall_round_index.saturating_sub(1)
        );
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
    }

    pub fn draw_for_next_turn(&mut self, position: usize) -> Option<i32> {
        let tile = self.draw_for_position(position)?;
        if position != self.dealer_position && self.first_normal_draw_positions.insert(position) {
            let can_draw_replacement = self.has_drawable_wall_tile();
            let mut options = self
                .hands
                .get(&position)
                .map(|hand| xi_gang_options(hand))
                .unwrap_or_default();
            options.retain(|option| option.as_slice() != XI_GANG_WINDS || can_draw_replacement);
            if options.is_empty() {
                self.xi_gang_options.remove(&position);
            } else {
                self.xi_gang_options.insert(position, options);
            }
        }
        Some(tile)
    }

    pub fn draw_for_position(&mut self, position: usize) -> Option<i32> {
        let tile = loop {
            let tile = self.wall.pop()?;
            if SHENYANG_MAHJONG_TILE_KINDS.contains(&tile) && self.known_tile_count(tile) < 4 {
                break tile;
            }
        };
        let hand = self.hands.entry(position).or_default();
        hand.push(tile);
        sort_tiles(hand);
        self.current_position = position;
        self.last_drawn_tile = Some(tile);
        self.pending_gang_draw = false;
        Some(tile)
    }

    pub fn enter_settlement(
        &mut self,
        winner_positions: Vec<usize>,
        from_position: Option<usize>,
        win_tile: Option<i32>,
        is_self_draw: bool,
    ) {
        self.enter_settlement_with_reverse_win(
            winner_positions,
            from_position,
            win_tile,
            is_self_draw,
            false,
            false,
            false,
        );
    }

    pub fn enter_settlement_with_reverse_win(
        &mut self,
        winner_positions: Vec<usize>,
        from_position: Option<usize>,
        win_tile: Option<i32>,
        is_self_draw: bool,
        is_reverse_win: bool,
        is_gang_draw: bool,
        is_haidilao: bool,
    ) {
        self.phase = ShenyangMahjongPhase::Settlement;
        self.claim_window = None;
        self.last_drawn_tile = None;
        self.pending_gang_draw = false;
        self.xi_gang_options.clear();
        self.settlement = Some(SettlementState {
            winner_positions,
            from_position,
            win_tile,
            is_self_draw,
            is_reverse_win,
            is_gang_draw,
            is_haidilao,
        });
        self.set_action_received(false);
    }

    pub fn has_drawable_wall_tile(&self) -> bool {
        let counts = self.known_tile_counts();
        self.wall
            .iter()
            .rev()
            .any(|tile| SHENYANG_MAHJONG_TILE_KINDS.contains(tile) && counts[*tile as usize] < 4)
    }

    pub fn is_ai_controlled_position(&self, position: usize) -> bool {
        let state = self.base.lock().unwrap();
        state.is_ai_position(position) || state.is_away(position) || state.is_disconnected(position)
    }

    pub fn is_ai_position(&self, position: usize) -> bool {
        self.base.lock().unwrap().is_ai_position(position)
    }

    pub fn is_away(&self, position: usize) -> bool {
        self.base.lock().unwrap().is_away(position)
    }

    pub fn is_disconnected(&self, position: usize) -> bool {
        self.base.lock().unwrap().is_disconnected(position)
    }

    pub fn is_paused(&self) -> bool {
        self.base.lock().unwrap().paused
    }

    pub(crate) fn known_tile_count(&self, tile: i32) -> usize {
        if !SHENYANG_MAHJONG_TILE_KINDS.contains(&tile) {
            return 0;
        }
        self.known_tile_counts()[tile as usize]
    }

    fn known_tile_counts(&self) -> [usize; 38] {
        let player_positions = self
            .players_snapshot()
            .keys()
            .copied()
            .collect::<HashSet<_>>();
        let mut counts = [0usize; 38];
        for tile in self
            .hands
            .values()
            .flat_map(|hand| hand.iter().copied())
            .chain(
                self.discards
                    .values()
                    .flat_map(|discards| discards.iter().copied()),
            )
            .filter(|tile| SHENYANG_MAHJONG_TILE_KINDS.contains(tile))
        {
            counts[tile as usize] += 1;
        }
        for (position, melds) in &self.melds {
            for tile in melds
                .iter()
                .filter(|meld| {
                    meld_source_is_valid_for_positions(meld, *position, &player_positions)
                })
                .filter(|meld| is_valid_meld(meld))
                .flat_map(|meld| meld.tiles.iter().copied())
            {
                counts[tile as usize] += 1;
            }
        }
        counts
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
            pending_gang_draw: false,
            first_normal_draw_positions: HashSet::new(),
            xi_gang_options: HashMap::new(),
            ting_positions: HashSet::new(),
            settlement: None,
            wall_seed_base: wall_seed_base_from_env(),
            wall_round_index: 0,
            last_wall_seed: None,
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

    fn next_wall_seed(&self) -> u64 {
        self.wall_seed_base
            .map(|seed| seed.wrapping_add(self.wall_round_index))
            .unwrap_or_else(|| system_wall_seed().wrapping_add(self.wall_round_index))
    }

    pub fn player_name(&self, position: usize) -> String {
        self.base.lock().unwrap().player_name(position)
    }

    pub fn players_snapshot(&self) -> HashMap<usize, (SessionId, String)> {
        self.base.lock().unwrap().players.clone()
    }

    pub fn redeal(&mut self) {
        let dealer_should_continue = self
            .settlement
            .as_ref()
            .map(|settlement| {
                settlement.winner_positions.is_empty()
                    || settlement.winner_positions.contains(&self.dealer_position)
            })
            .unwrap_or(false);
        if !dealer_should_continue {
            self.dealer_position = self.next_position(self.dealer_position);
        }
        self.current_position = self.dealer_position;
        self.phase = ShenyangMahjongPhase::Start;
        self.wall.clear();
        self.hands.clear();
        self.discards.clear();
        self.melds.clear();
        self.claim_window = None;
        self.last_drawn_tile = None;
        self.pending_gang_draw = false;
        self.first_normal_draw_positions.clear();
        self.xi_gang_options.clear();
        self.ting_positions.clear();
        self.settlement = None;
        self.set_action_received(false);
        self.set_turn_countdown(0);
    }

    pub fn remove_last_discard(&mut self, position: usize) {
        if let Some(discards) = self.discards.get_mut(&position) {
            let _ = discards.pop();
        }
    }

    pub fn is_ting(&self, position: usize) -> bool {
        self.ting_positions.contains(&position)
    }

    pub fn declare_ting(&mut self, position: usize) {
        self.ting_positions.insert(position);
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

    #[cfg(test)]
    pub(crate) fn set_wall_seed_base_for_test(&mut self, seed: Option<u64>) {
        self.wall_seed_base = seed;
        self.wall_round_index = 0;
        self.last_wall_seed = None;
    }

    fn shuffle_wall_with_seed(seed: u64) -> Vec<i32> {
        let mut wall = Vec::with_capacity(136);
        for tile in SHENYANG_MAHJONG_TILE_KINDS {
            for _ in 0..4 {
                wall.push(tile);
            }
        }
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
        let known_counts = self.known_tile_counts();
        let mut wall_counts = [0usize; 38];
        for tile in self
            .wall
            .iter()
            .copied()
            .filter(|tile| SHENYANG_MAHJONG_TILE_KINDS.contains(tile))
        {
            wall_counts[tile as usize] += 1;
        }
        SHENYANG_MAHJONG_TILE_KINDS
            .into_iter()
            .map(|tile| {
                wall_counts[tile as usize].min(4usize.saturating_sub(known_counts[tile as usize]))
            })
            .sum()
    }

    pub fn xi_gang_options_for_position(&self, position: usize) -> Vec<Vec<i32>> {
        self.xi_gang_options
            .get(&position)
            .cloned()
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn away_state_persists_through_new_round_deal() {
        let mut state = state_with_players();
        state.base.lock().unwrap().mark_away(2);

        state.deal_new_round();

        assert!(state.is_away(2));
    }

    #[test]
    fn away_state_persists_through_redeal() {
        let mut state = state_with_players();
        state.base.lock().unwrap().mark_away(2);
        state.enter_settlement(vec![0], None, Some(35), true);

        state.redeal();

        assert!(state.is_away(2));
    }

    #[test]
    fn dealer_never_receives_a_xi_gang_window() {
        let mut state = state_with_players();
        state.dealer_position = 0;
        state
            .hands
            .insert(0, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 31, 32, 33]);
        state.wall = vec![34];

        assert_eq!(state.draw_for_next_turn(0), Some(34));
        assert!(state.xi_gang_options_for_position(0).is_empty());
        assert!(!state.first_normal_draw_positions.contains(&0));
    }

    #[test]
    fn disconnected_position_is_ai_controlled_until_rejoin() {
        let state = state_with_players();
        state.base.lock().unwrap().mark_disconnected(2);

        assert!(state.is_disconnected(2));
        assert!(state.is_ai_controlled_position(2));
        assert!(!state.is_ai_position(2));
    }

    #[test]
    fn draw_skips_impossible_fifth_wall_copy() {
        let mut state = state_with_players();
        state.hands.insert(0, vec![3, 3, 3, 3]);
        state.wall = vec![35, 3];

        assert_eq!(state.draw_for_position(0), Some(35));
        let hand = state.hands.get(&0).expect("hand");
        assert_eq!(hand.iter().filter(|tile| **tile == 3).count(), 4);
        assert!(hand.contains(&35));
        assert!(state.wall.is_empty());
        assert_eq!(state.wall_count(), 0);
        assert_eq!(state.last_drawn_tile, Some(35));
    }

    #[test]
    fn draw_skips_invalid_wall_tiles() {
        let mut state = state_with_players();
        state.wall = vec![35, 99, -1];

        assert_eq!(state.draw_for_position(0), Some(35));
        assert_eq!(state.hands.get(&0), Some(&vec![35]));
        assert!(state.wall.is_empty());
        assert_eq!(state.wall_count(), 0);
        assert_eq!(state.last_drawn_tile, Some(35));
    }

    #[test]
    fn first_non_dealer_normal_draw_freezes_xi_gang_options() {
        let mut state = state_with_players();
        state.dealer_position = 0;
        state
            .hands
            .insert(1, vec![1, 2, 3, 4, 5, 6, 7, 31, 32, 33, 34, 35, 36]);
        state.wall = vec![9, 37];

        assert_eq!(state.draw_for_next_turn(1), Some(37));
        assert_eq!(
            state.xi_gang_options_for_position(1),
            vec![vec![31, 32, 33, 34], vec![35, 36, 37]]
        );
        assert!(state.first_normal_draw_positions.contains(&1));
    }

    #[test]
    fn last_wall_draw_filters_wind_xi_gang_that_cannot_replace() {
        let mut state = state_with_players();
        state.dealer_position = 0;
        state
            .hands
            .insert(1, vec![1, 2, 3, 4, 5, 6, 7, 31, 32, 33, 34, 35, 36]);
        state.wall = vec![37];

        assert_eq!(state.draw_for_next_turn(1), Some(37));
        assert_eq!(
            state.xi_gang_options_for_position(1),
            vec![vec![35, 36, 37]]
        );
    }

    #[test]
    fn redeal_advances_dealer_after_non_dealer_win() {
        let mut state = state_with_players();
        state.dealer_position = 0;
        state.enter_settlement(vec![2], Some(1), Some(35), false);

        state.redeal();

        assert_eq!(state.dealer_position, 1);
        assert_eq!(state.current_position, 1);
    }

    #[test]
    fn redeal_keeps_dealer_after_dealer_win() {
        let mut state = state_with_players();
        state.dealer_position = 0;
        state.enter_settlement(vec![0], None, Some(35), true);

        state.redeal();

        assert_eq!(state.dealer_position, 0);
        assert_eq!(state.current_position, 0);
    }

    #[test]
    fn redeal_keeps_dealer_after_draw() {
        let mut state = state_with_players();
        state.dealer_position = 0;
        state.enter_settlement(Vec::new(), None, None, false);

        state.redeal();

        assert_eq!(state.dealer_position, 0);
        assert_eq!(state.current_position, 0);
    }

    #[test]
    fn replacement_draw_does_not_create_a_late_xi_gang_option() {
        let mut state = state_with_players();
        state.dealer_position = 0;
        state
            .hands
            .insert(1, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 12, 35, 36]);
        state.wall = vec![37, 21];

        assert_eq!(state.draw_for_next_turn(1), Some(21));
        assert!(state.xi_gang_options_for_position(1).is_empty());
        assert_eq!(state.draw_for_position(1), Some(37));
        assert!(state.xi_gang_options_for_position(1).is_empty());
    }

    #[test]
    fn seeded_deal_advances_round_seed() {
        let mut state = state_with_players();
        state.wall_seed_base = Some(2026070401);

        state.deal_new_round();
        assert_eq!(state.last_wall_seed, Some(2026070401));

        state.enter_settlement(Vec::new(), None, None, false);
        state.redeal();
        state.deal_new_round();

        assert_eq!(state.last_wall_seed, Some(2026070402));
    }

    #[test]
    fn seeded_wall_shuffle_is_reproducible() {
        assert_eq!(
            ShenyangMahjongLoopState::shuffle_wall_with_seed(2026070401),
            ShenyangMahjongLoopState::shuffle_wall_with_seed(2026070401)
        );
        assert_ne!(
            ShenyangMahjongLoopState::shuffle_wall_with_seed(2026070401),
            ShenyangMahjongLoopState::shuffle_wall_with_seed(2026070402)
        );
    }

    fn state_with_players() -> ShenyangMahjongLoopState {
        let base = Arc::new(Mutex::new(CommonGameState::default()));
        {
            let mut common = base.lock().unwrap();
            for position in 0..4 {
                common.add_player(position, position as u64 + 1, &format!("P{}", position));
            }
        }
        ShenyangMahjongLoopState::new(base)
    }

    #[test]
    fn wall_count_ignores_impossible_fifth_wall_copy() {
        let mut state = state_with_players();
        state.hands.insert(0, vec![3, 3, 3, 3]);
        state.wall = vec![3, 35];

        assert_eq!(state.wall_count(), 1);
    }

    #[test]
    fn wall_count_ignores_invalid_wall_tiles() {
        let mut state = state_with_players();
        state.wall = vec![35, 99, -1];

        assert_eq!(state.wall_count(), 1);
    }
}
