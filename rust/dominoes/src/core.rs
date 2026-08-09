use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use share_type_public::{
    DominoesNoPlayableTiles, DominoesPhase, DominoesPort, DominoesRule, WsDominoesEndpoint,
    WsDominoesHandState, WsDominoesPlacement, WsDominoesPlayerState, WsDominoesTableSnapshotEvent,
    WsDominoesTile,
};
use ws_common::CommonGameState;

pub const MIN_PLAYERS: usize = 3;
pub const MAX_PLAYERS: usize = 4;
pub const TILE_COUNT: usize = 28;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tile {
    pub id: i32,
    pub a: i32,
    pub b: i32,
}

impl Tile {
    pub fn all() -> Vec<Self> {
        let mut tiles = Vec::with_capacity(TILE_COUNT);
        let mut id = 0;
        for a in 0..=6 {
            for b in a..=6 {
                tiles.push(Self { id, a, b });
                id += 1;
            }
        }
        tiles
    }

    pub fn from_id(id: i32) -> Option<Self> {
        Self::all().into_iter().find(|tile| tile.id == id)
    }

    pub fn pip_sum(self) -> i32 {
        self.a + self.b
    }

    pub fn is_double(self) -> bool {
        self.a == self.b
    }

    pub fn matches(self, pip: i32) -> bool {
        self.a == pip || self.b == pip
    }
}

impl From<Tile> for WsDominoesTile {
    fn from(tile: Tile) -> Self {
        Self {
            id: tile.id,
            a: tile.a,
            b: tile.b,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Endpoint {
    pub endpoint_id: i32,
    pub placement_id: i32,
    pub pip: i32,
    pub port: DominoesPort,
}

impl From<Endpoint> for WsDominoesEndpoint {
    fn from(endpoint: Endpoint) -> Self {
        Self {
            endpoint_id: endpoint.endpoint_id,
            placement_id: endpoint.placement_id,
            pip: endpoint.pip,
            port: endpoint.port,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    pub placement_id: i32,
    pub tile: Tile,
    pub connected_endpoint_id: Option<i32>,
    pub connected_port: Option<DominoesPort>,
    pub new_endpoints: Vec<Endpoint>,
}

impl From<Placement> for WsDominoesPlacement {
    fn from(placement: Placement) -> Self {
        Self {
            placement_id: placement.placement_id,
            tile: placement.tile.into(),
            connected_endpoint_id: placement.connected_endpoint_id,
            connected_port: placement.connected_port,
            new_endpoints: placement
                .new_endpoints
                .into_iter()
                .map(Into::into)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoundResult {
    pub winner_position: usize,
    pub blocked: bool,
    pub round_score: i32,
    pub scores: HashMap<usize, i32>,
    pub remaining_hands: HashMap<usize, Vec<Tile>>,
    pub game_over: bool,
    pub winner_positions: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrawResult {
    pub tile: Option<Tile>,
    pub playable: bool,
    pub passed: bool,
    pub round_result: Option<RoundResult>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreError {
    NotPlaying,
    NotYourTurn,
    InvalidTile,
    InvalidEndpoint,
    TileDoesNotMatch,
    PlayAvailable,
    DrawNotAllowed,
    PassNotAllowed,
    NoPlayers,
    WrongPhase,
}

impl CoreError {
    pub fn message(self) -> &'static str {
        match self {
            Self::NotPlaying => "dominoes round is not in play",
            Self::NotYourTurn => "it is not your turn",
            Self::InvalidTile => "tile is not in your hand",
            Self::InvalidEndpoint => "endpoint is not open",
            Self::TileDoesNotMatch => "tile does not match the endpoint",
            Self::PlayAvailable => "a playable tile is already in your hand",
            Self::DrawNotAllowed => "drawing is not allowed now",
            Self::PassNotAllowed => "passing is not allowed now",
            Self::NoPlayers => "dominoes needs 3 to 4 players",
            Self::WrongPhase => "the round is waiting for the next start",
        }
    }
}

#[derive(Debug)]
pub struct DominoesRoundState {
    pub base: Arc<Mutex<CommonGameState>>,
    pub phase: DominoesPhase,
    pub round: i32,
    pub rule: DominoesRule,
    pub no_playable_tiles: DominoesNoPlayableTiles,
    pub target_score: i32,
    pub positions: Vec<usize>,
    pub hands: HashMap<usize, Vec<Tile>>,
    pub boneyard: Vec<Tile>,
    pub placements: Vec<Placement>,
    pub endpoints: Vec<Endpoint>,
    pub scores: HashMap<usize, i32>,
    pub current_position: usize,
    pub last_play_position: Option<usize>,
    pub consecutive_passes: usize,
    pub drawn_this_turn: bool,
    next_placement_id: i32,
    next_endpoint_id: i32,
    pub round_winner: Option<usize>,
}

impl DominoesRoundState {
    pub fn new(
        base: Arc<Mutex<CommonGameState>>,
        rule: DominoesRule,
        no_playable_tiles: DominoesNoPlayableTiles,
        target_score: i32,
    ) -> Result<Self, CoreError> {
        let mut positions = base
            .lock()
            .unwrap()
            .players
            .keys()
            .copied()
            .collect::<Vec<_>>();
        positions.sort_unstable();
        if !(MIN_PLAYERS..=MAX_PLAYERS).contains(&positions.len()) {
            return Err(CoreError::NoPlayers);
        }
        Ok(Self {
            base,
            phase: DominoesPhase::RoundOver,
            round: 0,
            rule,
            no_playable_tiles,
            target_score: if target_score == 61 { 61 } else { 35 },
            positions,
            hands: HashMap::new(),
            boneyard: Vec::new(),
            placements: Vec::new(),
            endpoints: Vec::new(),
            scores: HashMap::new(),
            current_position: 0,
            last_play_position: None,
            consecutive_passes: 0,
            drawn_this_turn: false,
            next_placement_id: 0,
            next_endpoint_id: 0,
            round_winner: None,
        })
    }

    pub fn start_new_game(&mut self) -> Result<usize, CoreError> {
        self.scores = self
            .positions
            .iter()
            .map(|position| (*position, 0))
            .collect();
        self.round_winner = None;
        self.round = 1;
        self.deal_round(None)
    }

    pub fn start_next_round(&mut self) -> Result<usize, CoreError> {
        if self.phase != DominoesPhase::RoundOver {
            return Err(CoreError::WrongPhase);
        }
        self.round = self.round.saturating_add(1);
        self.deal_round(self.round_winner)
    }

    fn deal_round(&mut self, starter_hint: Option<usize>) -> Result<usize, CoreError> {
        if self.positions.is_empty() {
            return Err(CoreError::NoPlayers);
        }
        let mut deck = Tile::all();
        shuffle(&mut deck, self.round as u64);
        let starter = starter_hint.unwrap_or_else(|| {
            self.positions
                .iter()
                .enumerate()
                .max_by_key(|(index, _)| deck[*index].pip_sum())
                .map(|(_, position)| *position)
                .unwrap_or(self.positions[0])
        });
        let hand_size = self.hand_size();
        self.hands.clear();
        let mut cursor = 0;
        for position in &self.positions {
            let mut hand = deck[cursor..cursor + hand_size].to_vec();
            cursor += hand_size;
            hand.sort_unstable_by_key(|tile| tile.id);
            self.hands.insert(*position, hand);
        }
        self.boneyard = deck[cursor..].to_vec();
        self.placements.clear();
        self.endpoints.clear();
        self.next_placement_id = 0;
        self.next_endpoint_id = 0;
        self.current_position = starter;
        self.phase = DominoesPhase::Play;
        self.last_play_position = None;
        self.consecutive_passes = 0;
        self.drawn_this_turn = false;
        Ok(starter)
    }

    pub fn hand_size(&self) -> usize {
        5
    }

    pub fn legal_tile_ids(&self, position: usize) -> Vec<i32> {
        let Some(hand) = self.hands.get(&position) else {
            return Vec::new();
        };
        let mut ids = hand
            .iter()
            .filter(|tile| {
                self.endpoints.is_empty() || self.endpoints.iter().any(|e| tile.matches(e.pip))
            })
            .map(|tile| tile.id)
            .collect::<Vec<_>>();
        ids.sort_unstable();
        ids
    }

    pub fn play_tile(
        &mut self,
        position: usize,
        tile_id: i32,
        endpoint_id: Option<i32>,
    ) -> Result<(Placement, i32, Option<RoundResult>), CoreError> {
        self.ensure_turn(position)?;
        let hand = self.hands.get(&position).ok_or(CoreError::InvalidTile)?;
        let hand_index = hand
            .iter()
            .position(|tile| tile.id == tile_id)
            .ok_or(CoreError::InvalidTile)?;
        let tile = hand[hand_index];
        let (connected_endpoint_id, connected_port, new_endpoints) = if self.endpoints.is_empty() {
            if endpoint_id.is_some() {
                return Err(CoreError::InvalidEndpoint);
            }
            (None, None, self.new_endpoints(tile, None))
        } else {
            let endpoint_id = endpoint_id.ok_or(CoreError::InvalidEndpoint)?;
            let endpoint_index = self
                .endpoints
                .iter()
                .position(|endpoint| endpoint.endpoint_id == endpoint_id)
                .ok_or(CoreError::InvalidEndpoint)?;
            let endpoint = self.endpoints[endpoint_index];
            if !tile.matches(endpoint.pip) {
                return Err(CoreError::TileDoesNotMatch);
            }
            self.endpoints.remove(endpoint_index);
            let connected_port = if tile.is_double() {
                DominoesPort::Bottom
            } else if tile.a == endpoint.pip {
                DominoesPort::Left
            } else {
                DominoesPort::Right
            };
            (
                Some(endpoint_id),
                Some(connected_port),
                self.new_endpoints(tile, Some(connected_port)),
            )
        };
        let placement_id = self.next_placement_id;
        self.next_placement_id = self.next_placement_id.saturating_add(1);
        let placement = Placement {
            placement_id,
            tile,
            connected_endpoint_id,
            connected_port,
            new_endpoints,
        };
        self.endpoints
            .extend(placement.new_endpoints.iter().copied());
        self.placements.push(placement.clone());
        self.hands
            .get_mut(&position)
            .expect("hand checked above")
            .remove(hand_index);
        self.drawn_this_turn = false;
        self.consecutive_passes = 0;
        self.last_play_position = Some(position);
        let score = self.five_up_score();
        *self.scores.entry(position).or_default() += score;
        let round_result = if self.hands.get(&position).is_some_and(Vec::is_empty) {
            Some(self.finish_round(position, false))
        } else {
            self.advance_turn();
            None
        };
        Ok((placement, score, round_result))
    }

    pub fn draw_tile(&mut self, position: usize) -> Result<DrawResult, CoreError> {
        self.ensure_turn(position)?;
        if !self.legal_tile_ids(position).is_empty() {
            return Err(CoreError::PlayAvailable);
        }
        if self.no_playable_tiles == DominoesNoPlayableTiles::PassWithoutDraw {
            return Err(CoreError::DrawNotAllowed);
        }
        let Some(tile) = self.boneyard.pop() else {
            let round_result = self.record_pass(position)?;
            return Ok(DrawResult {
                tile: None,
                playable: false,
                passed: true,
                round_result,
            });
        };
        let playable = self.endpoints.is_empty()
            || self
                .endpoints
                .iter()
                .any(|endpoint| tile.matches(endpoint.pip));
        self.hands.entry(position).or_default().push(tile);
        self.hands
            .get_mut(&position)
            .expect("hand exists")
            .sort_unstable_by_key(|item| item.id);
        self.drawn_this_turn = true;
        if self.no_playable_tiles == DominoesNoPlayableTiles::DrawOne && !playable {
            let round_result = self.record_pass(position)?;
            return Ok(DrawResult {
                tile: Some(tile),
                playable,
                passed: true,
                round_result,
            });
        }
        Ok(DrawResult {
            tile: Some(tile),
            playable,
            passed: false,
            round_result: None,
        })
    }

    pub fn pass(&mut self, position: usize) -> Result<Option<RoundResult>, CoreError> {
        self.ensure_turn(position)?;
        if !self.legal_tile_ids(position).is_empty() {
            return Err(CoreError::PlayAvailable);
        }
        let allowed = match self.no_playable_tiles {
            DominoesNoPlayableTiles::PassWithoutDraw => true,
            DominoesNoPlayableTiles::DrawOne => self.drawn_this_turn || self.boneyard.is_empty(),
            DominoesNoPlayableTiles::KeepDrawing => self.boneyard.is_empty(),
        };
        if !allowed {
            return Err(CoreError::PassNotAllowed);
        }
        self.record_pass(position)
    }

    fn record_pass(&mut self, position: usize) -> Result<Option<RoundResult>, CoreError> {
        self.drawn_this_turn = false;
        self.consecutive_passes = self.consecutive_passes.saturating_add(1);
        if self.consecutive_passes >= self.positions.len() {
            let winner = self.last_play_position.unwrap_or(position);
            return Ok(Some(self.finish_round(winner, true)));
        }
        self.advance_turn();
        Ok(None)
    }

    fn ensure_turn(&self, position: usize) -> Result<(), CoreError> {
        if self.phase != DominoesPhase::Play {
            return Err(CoreError::NotPlaying);
        }
        if self.current_position != position {
            return Err(CoreError::NotYourTurn);
        }
        Ok(())
    }

    fn advance_turn(&mut self) {
        let index = self
            .positions
            .iter()
            .position(|item| *item == self.current_position)
            .unwrap_or(0);
        self.current_position = self.positions[(index + 1) % self.positions.len()];
    }

    fn new_endpoints(&mut self, tile: Tile, connected_port: Option<DominoesPort>) -> Vec<Endpoint> {
        let placement_id = self.next_placement_id;
        let ports = if tile.is_double() {
            match connected_port {
                Some(_) => vec![
                    (DominoesPort::Left, tile.a),
                    (DominoesPort::Right, tile.a),
                    (DominoesPort::Top, tile.a),
                ],
                None => vec![
                    (DominoesPort::Left, tile.a),
                    (DominoesPort::Right, tile.a),
                    (DominoesPort::Top, tile.a),
                    (DominoesPort::Bottom, tile.a),
                ],
            }
        } else if connected_port == Some(DominoesPort::Left) {
            vec![(DominoesPort::Right, tile.b)]
        } else if connected_port == Some(DominoesPort::Right) {
            vec![(DominoesPort::Left, tile.a)]
        } else {
            vec![(DominoesPort::Left, tile.a), (DominoesPort::Right, tile.b)]
        };
        ports
            .into_iter()
            .map(|(port, pip)| {
                let endpoint = Endpoint {
                    endpoint_id: self.next_endpoint_id,
                    placement_id,
                    pip,
                    port,
                };
                self.next_endpoint_id = self.next_endpoint_id.saturating_add(1);
                endpoint
            })
            .collect()
    }

    fn five_up_score(&self) -> i32 {
        if self.rule != DominoesRule::FiveUp {
            return 0;
        }
        let total = self
            .endpoints
            .iter()
            .map(|endpoint| endpoint.pip)
            .sum::<i32>();
        if total % 5 == 0 { total } else { 0 }
    }

    fn finish_round(&mut self, winner: usize, blocked: bool) -> RoundResult {
        let round_score = self
            .hands
            .iter()
            .filter(|(position, _)| **position != winner)
            .map(|(_, hand)| hand.iter().map(|tile| tile.pip_sum()).sum::<i32>())
            .sum::<i32>();
        let round_score = match self.rule {
            DominoesRule::Simple => round_score,
            DominoesRule::FiveUp => round_score / 5,
        };
        *self.scores.entry(winner).or_default() += round_score;
        self.round_winner = Some(winner);
        let game_over = self
            .scores
            .values()
            .any(|score| *score >= self.target_score);
        self.phase = if game_over {
            DominoesPhase::GameOver
        } else {
            DominoesPhase::RoundOver
        };
        let best = self.scores.values().copied().max().unwrap_or_default();
        let winner_positions = if game_over {
            self.scores
                .iter()
                .filter(|(_, score)| **score == best)
                .map(|(position, _)| *position)
                .collect()
        } else {
            Vec::new()
        };
        RoundResult {
            winner_position: winner,
            blocked,
            round_score,
            scores: self.scores.clone(),
            remaining_hands: self.hands.clone(),
            game_over,
            winner_positions,
        }
    }

    pub fn table_snapshot(&self) -> WsDominoesTableSnapshotEvent {
        let mut players = self
            .positions
            .iter()
            .map(|position| WsDominoesPlayerState {
                position: *position as i32,
                hand_count: self.hands.get(position).map_or(0, Vec::len) as i32,
                score: self.scores.get(position).copied().unwrap_or_default(),
            })
            .collect::<Vec<_>>();
        players.sort_by_key(|player| player.position);
        WsDominoesTableSnapshotEvent {
            phase: self.phase,
            round: self.round,
            current_position: self.current_position as i32,
            rule: self.rule,
            no_playable_tiles: self.no_playable_tiles,
            target_score: self.target_score,
            boneyard_count: self.boneyard.len() as i32,
            placements: self
                .placements
                .clone()
                .into_iter()
                .map(Into::into)
                .collect(),
            endpoints: self.endpoints.iter().copied().map(Into::into).collect(),
            players,
            last_play_position: self.last_play_position.map(|position| position as i32),
            consecutive_passes: self.consecutive_passes as i32,
        }
    }

    pub fn hand_state(&self, position: usize) -> WsDominoesHandState {
        let playable_tile_ids = self.legal_tile_ids(position);
        let can_draw = self.phase == DominoesPhase::Play
            && self.current_position == position
            && playable_tile_ids.is_empty()
            && self.no_playable_tiles != DominoesNoPlayableTiles::PassWithoutDraw
            && !self.boneyard.is_empty();
        let can_pass = self.phase == DominoesPhase::Play
            && self.current_position == position
            && playable_tile_ids.is_empty()
            && match self.no_playable_tiles {
                DominoesNoPlayableTiles::PassWithoutDraw => true,
                DominoesNoPlayableTiles::DrawOne => {
                    self.drawn_this_turn || self.boneyard.is_empty()
                }
                DominoesNoPlayableTiles::KeepDrawing => self.boneyard.is_empty(),
            };
        WsDominoesHandState {
            hand: self
                .hands
                .get(&position)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(Into::into)
                .collect(),
            playable_tile_ids,
            can_draw,
            can_pass,
        }
    }
}

fn shuffle(deck: &mut [Tile], salt: u64) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos() as u64)
        .unwrap_or(0);
    let mut seed = now ^ salt.rotate_left(17);
    for index in (1..deck.len()).rev() {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let other = (seed >> 32) as usize % (index + 1);
        deck.swap(index, other);
    }
}

#[cfg(test)]
#[path = "core/tests.rs"]
mod tests;
