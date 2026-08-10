use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use share_type_public::{
    DominoesNoPlayableTiles, DominoesOrientation, DominoesPhase, DominoesPort, DominoesRule,
    WsDominoesEndpoint, WsDominoesHandState, WsDominoesLegalPlay, WsDominoesPlacement,
    WsDominoesPlayerState, WsDominoesTableSnapshotEvent, WsDominoesTile,
};
use ws_common::CommonGameState;

pub const MIN_PLAYERS: usize = 3;
pub const MAX_PLAYERS: usize = 4;
pub const TILE_COUNT: usize = 28;
pub const DEFAULT_TURN_SECONDS: u32 = 30;
pub const DISCONNECTED_TURN_SECONDS: u32 = 5;
pub const ROUND_TRANSITION_SECONDS: u32 = 4;

const TILE_LONG_HALF: i32 = 2;
const TILE_SHORT_HALF: i32 = 1;
const LAYOUT_JUMP: i32 = 6;

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

    fn other_pip(self, matched: i32) -> i32 {
        if self.a == matched { self.b } else { self.a }
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
    pub anchor_x: i32,
    pub anchor_y: i32,
    pub direction: DominoesPort,
}

impl From<Endpoint> for WsDominoesEndpoint {
    fn from(endpoint: Endpoint) -> Self {
        Self {
            endpoint_id: endpoint.endpoint_id,
            placement_id: endpoint.placement_id,
            pip: endpoint.pip,
            port: endpoint.port,
            anchor_x: endpoint.anchor_x,
            anchor_y: endpoint.anchor_y,
            direction: endpoint.direction,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    pub placement_id: i32,
    pub tile: Tile,
    pub connected_endpoint_id: Option<i32>,
    pub connected_port: Option<DominoesPort>,
    pub center_x: i32,
    pub center_y: i32,
    pub orientation: DominoesOrientation,
    pub flipped: bool,
    pub new_endpoints: Vec<Endpoint>,
}

impl From<Placement> for WsDominoesPlacement {
    fn from(placement: Placement) -> Self {
        Self {
            placement_id: placement.placement_id,
            tile: placement.tile.into(),
            connected_endpoint_id: placement.connected_endpoint_id,
            connected_port: placement.connected_port,
            center_x: placement.center_x,
            center_y: placement.center_y,
            orientation: placement.orientation,
            flipped: placement.flipped,
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
    /// 每个座位在本轮获得的实际分数；Five-Up 包含出牌即时得分。
    pub score_changes: HashMap<usize, i32>,
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
    pub round_score_changes: HashMap<usize, i32>,
    pub current_position: usize,
    pub last_play_position: Option<usize>,
    pub consecutive_passes: usize,
    pub drawn_this_turn: bool,
    pub remaining_seconds: u32,
    pub turn_revision: i32,
    next_placement_id: i32,
    next_endpoint_id: i32,
    pub round_winner: Option<usize>,
    rng_state: u64,
}

#[derive(Debug, Clone, Copy)]
struct LayoutPlan {
    center_x: i32,
    center_y: i32,
    orientation: DominoesOrientation,
    flipped: bool,
    connected_port: Option<DominoesPort>,
    outward_direction: Option<DominoesPort>,
}

impl DominoesRoundState {
    pub fn new(
        base: Arc<Mutex<CommonGameState>>,
        rule: DominoesRule,
        no_playable_tiles: DominoesNoPlayableTiles,
        target_score: i32,
    ) -> Result<Self, CoreError> {
        Self::new_with_seed(base, rule, no_playable_tiles, target_score, random_seed())
    }

    pub fn new_with_seed(
        base: Arc<Mutex<CommonGameState>>,
        rule: DominoesRule,
        no_playable_tiles: DominoesNoPlayableTiles,
        target_score: i32,
        seed: u64,
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
            round_score_changes: HashMap::new(),
            current_position: 0,
            last_play_position: None,
            consecutive_passes: 0,
            drawn_this_turn: false,
            remaining_seconds: 0,
            turn_revision: 0,
            next_placement_id: 0,
            next_endpoint_id: 0,
            round_winner: None,
            rng_state: seed.max(1),
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
        let starter = match starter_hint {
            Some(position) => position,
            None => self.draw_initial_starter(),
        };
        let mut deck = Tile::all();
        shuffle(&mut deck, &mut self.rng_state);
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
        self.round_score_changes = self
            .positions
            .iter()
            .map(|position| (*position, 0))
            .collect();
        self.placements.clear();
        self.endpoints.clear();
        self.next_placement_id = 0;
        self.next_endpoint_id = 0;
        self.current_position = starter;
        self.phase = DominoesPhase::Play;
        self.last_play_position = None;
        self.consecutive_passes = 0;
        self.begin_turn();
        Ok(starter)
    }

    fn draw_initial_starter(&mut self) -> usize {
        let mut hidden_draw = Tile::all();
        shuffle(&mut hidden_draw, &mut self.rng_state);
        let best_pips = hidden_draw
            .iter()
            .take(self.positions.len())
            .map(|tile| tile.pip_sum())
            .max()
            .unwrap_or_default();
        let candidates = self
            .positions
            .iter()
            .zip(hidden_draw.iter())
            .filter(|(_, tile)| tile.pip_sum() == best_pips)
            .map(|(position, _)| *position)
            .collect::<Vec<_>>();
        let choice = next_random(&mut self.rng_state) as usize % candidates.len().max(1);
        candidates.get(choice).copied().unwrap_or(self.positions[0])
    }

    pub fn hand_size(&self) -> usize {
        5
    }

    pub fn legal_plays(&self, position: usize) -> Vec<WsDominoesLegalPlay> {
        let Some(hand) = self.hands.get(&position) else {
            return Vec::new();
        };
        let mut plays = Vec::new();
        if self.endpoints.is_empty() {
            plays.extend(hand.iter().map(|tile| WsDominoesLegalPlay {
                tile_id: tile.id,
                endpoint_id: None,
                score: self.preview_five_up_score(*tile, None),
            }));
        } else {
            for tile in hand {
                for endpoint in self
                    .endpoints
                    .iter()
                    .filter(|endpoint| tile.matches(endpoint.pip))
                {
                    plays.push(WsDominoesLegalPlay {
                        tile_id: tile.id,
                        endpoint_id: Some(endpoint.endpoint_id),
                        score: self.preview_five_up_score(*tile, Some(endpoint.endpoint_id)),
                    });
                }
            }
        }
        plays.sort_unstable_by_key(|play| (play.tile_id, play.endpoint_id.unwrap_or(-1)));
        plays
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
        let connected_endpoint = if self.endpoints.is_empty() {
            if endpoint_id.is_some() {
                return Err(CoreError::InvalidEndpoint);
            }
            None
        } else {
            let endpoint_id = endpoint_id.ok_or(CoreError::InvalidEndpoint)?;
            let endpoint = self
                .endpoints
                .iter()
                .find(|endpoint| endpoint.endpoint_id == endpoint_id)
                .copied()
                .ok_or(CoreError::InvalidEndpoint)?;
            if !tile.matches(endpoint.pip) {
                return Err(CoreError::TileDoesNotMatch);
            }
            Some(endpoint)
        };
        if let Some(endpoint) = connected_endpoint {
            self.endpoints
                .retain(|candidate| candidate.endpoint_id != endpoint.endpoint_id);
        }
        let placement_id = self.next_placement_id;
        self.next_placement_id = self.next_placement_id.saturating_add(1);
        let layout = self.layout_for(tile, connected_endpoint);
        let new_endpoints = self.create_endpoints(placement_id, tile, layout);
        let placement = Placement {
            placement_id,
            tile,
            connected_endpoint_id: connected_endpoint.map(|endpoint| endpoint.endpoint_id),
            connected_port: layout.connected_port,
            center_x: layout.center_x,
            center_y: layout.center_y,
            orientation: layout.orientation,
            flipped: layout.flipped,
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
        *self.round_score_changes.entry(position).or_default() += score;
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
        if !self.legal_plays(position).is_empty() {
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
        self.hands.entry(position).or_default().push(tile);
        self.hands
            .get_mut(&position)
            .expect("hand exists")
            .sort_unstable_by_key(|item| item.id);
        self.drawn_this_turn = true;
        let playable = !self.legal_plays(position).is_empty();
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
        if !self.legal_plays(position).is_empty() {
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
        self.begin_turn();
    }

    fn begin_turn(&mut self) {
        self.drawn_this_turn = false;
        self.turn_revision = self.turn_revision.wrapping_add(1).max(1);
        self.remaining_seconds = DEFAULT_TURN_SECONDS;
    }

    pub fn cap_remaining_seconds(&mut self, maximum: u32) -> bool {
        if self.remaining_seconds <= maximum {
            return false;
        }
        self.remaining_seconds = maximum;
        true
    }

    pub fn tick_remaining_seconds(&mut self, revision: i32) -> bool {
        if self.phase != DominoesPhase::Play || self.turn_revision != revision {
            return false;
        }
        self.remaining_seconds = self.remaining_seconds.saturating_sub(1);
        true
    }

    pub fn tick_round_transition(&mut self, revision: i32) -> bool {
        if self.phase != DominoesPhase::RoundOver || self.turn_revision != revision {
            return false;
        }
        self.remaining_seconds = self.remaining_seconds.saturating_sub(1);
        true
    }

    fn layout_for(&self, tile: Tile, endpoint: Option<Endpoint>) -> LayoutPlan {
        let Some(endpoint) = endpoint else {
            return LayoutPlan {
                center_x: 0,
                center_y: 0,
                orientation: if tile.is_double() {
                    DominoesOrientation::Vertical
                } else {
                    DominoesOrientation::Horizontal
                },
                flipped: false,
                connected_port: None,
                outward_direction: None,
            };
        };
        let direction = endpoint.direction;
        let orientation = if tile.is_double() {
            perpendicular_orientation(direction)
        } else {
            aligned_orientation(direction)
        };
        let flipped = if tile.is_double() {
            false
        } else {
            let connection_on_negative_side =
                matches!(direction, DominoesPort::Right | DominoesPort::Bottom);
            let a_matches = tile.a == endpoint.pip;
            if connection_on_negative_side {
                !a_matches
            } else {
                a_matches
            }
        };
        let extent = extent_in_direction(orientation, direction);
        let (dx, dy) = direction_vector(direction);
        let mut center_x = endpoint.anchor_x + dx * extent;
        let mut center_y = endpoint.anchor_y + dy * extent;
        while self.placement_collides(center_x, center_y, orientation) {
            center_x += dx * LAYOUT_JUMP;
            center_y += dy * LAYOUT_JUMP;
        }
        LayoutPlan {
            center_x,
            center_y,
            orientation,
            flipped,
            connected_port: Some(opposite(direction)),
            outward_direction: Some(direction),
        }
    }

    fn placement_collides(
        &self,
        center_x: i32,
        center_y: i32,
        orientation: DominoesOrientation,
    ) -> bool {
        let (half_width, half_height) = half_extents(orientation);
        self.placements.iter().any(|placement| {
            let (other_width, other_height) = half_extents(placement.orientation);
            (center_x - placement.center_x).abs() < half_width + other_width
                && (center_y - placement.center_y).abs() < half_height + other_height
        })
    }

    fn create_endpoints(
        &mut self,
        placement_id: i32,
        tile: Tile,
        layout: LayoutPlan,
    ) -> Vec<Endpoint> {
        let branches = match layout.outward_direction {
            None if tile.is_double() => vec![
                (DominoesPort::Left, tile.a),
                (DominoesPort::Right, tile.a),
                (DominoesPort::Top, tile.a),
                (DominoesPort::Bottom, tile.a),
            ],
            None => vec![(DominoesPort::Left, tile.a), (DominoesPort::Right, tile.b)],
            Some(direction) if tile.is_double() => vec![
                (direction, tile.a),
                (turn_left(direction), tile.a),
                (turn_right(direction), tile.a),
            ],
            Some(direction) => {
                let connected_pip = if layout.flipped {
                    if matches!(direction, DominoesPort::Right | DominoesPort::Bottom) {
                        tile.b
                    } else {
                        tile.a
                    }
                } else if matches!(direction, DominoesPort::Right | DominoesPort::Bottom) {
                    tile.a
                } else {
                    tile.b
                };
                vec![(direction, tile.other_pip(connected_pip))]
            }
        };
        branches
            .into_iter()
            .map(|(direction, pip)| {
                let extent = extent_in_direction(layout.orientation, direction);
                let (dx, dy) = direction_vector(direction);
                let endpoint = Endpoint {
                    endpoint_id: self.next_endpoint_id,
                    placement_id,
                    pip,
                    port: direction,
                    anchor_x: layout.center_x + dx * extent,
                    anchor_y: layout.center_y + dy * extent,
                    direction,
                };
                self.next_endpoint_id = self.next_endpoint_id.saturating_add(1);
                endpoint
            })
            .collect()
    }

    fn preview_five_up_score(&self, tile: Tile, endpoint_id: Option<i32>) -> i32 {
        if self.rule != DominoesRule::FiveUp {
            return 0;
        }
        let mut total = self
            .endpoints
            .iter()
            .map(|endpoint| endpoint.pip)
            .sum::<i32>();
        match endpoint_id {
            None if tile.is_double() => total = tile.a * 4,
            None => total = tile.a + tile.b,
            Some(endpoint_id) => {
                let Some(endpoint) = self
                    .endpoints
                    .iter()
                    .find(|endpoint| endpoint.endpoint_id == endpoint_id)
                else {
                    return 0;
                };
                total -= endpoint.pip;
                total += if tile.is_double() {
                    tile.a * 3
                } else {
                    tile.other_pip(endpoint.pip)
                };
            }
        }
        if total % 5 == 0 { total } else { 0 }
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
        *self.round_score_changes.entry(winner).or_default() += round_score;
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
        self.turn_revision = self.turn_revision.wrapping_add(1).max(1);
        self.remaining_seconds = if game_over {
            0
        } else {
            ROUND_TRANSITION_SECONDS
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
        let score_changes = self
            .positions
            .iter()
            .map(|position| {
                (
                    *position,
                    self.round_score_changes
                        .get(position)
                        .copied()
                        .unwrap_or_default(),
                )
            })
            .collect();
        RoundResult {
            winner_position: winner,
            blocked,
            round_score,
            scores: self.scores.clone(),
            score_changes,
            remaining_hands: self.hands.clone(),
            game_over,
            winner_positions,
        }
    }

    pub fn table_snapshot(&self) -> WsDominoesTableSnapshotEvent {
        let common = self.base.lock().unwrap();
        let mut players = self
            .positions
            .iter()
            .map(|position| WsDominoesPlayerState {
                position: *position as i32,
                hand_count: self.hands.get(position).map_or(0, Vec::len) as i32,
                score: self.scores.get(position).copied().unwrap_or_default(),
                is_ai: common.is_ai_position(*position),
                away: common.is_away(*position) || common.is_disconnected(*position),
                is_ai_takeover: common.is_ai_takeover_position(*position),
            })
            .collect::<Vec<_>>();
        drop(common);
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
            remaining_seconds: self.remaining_seconds as i32,
            turn_revision: self.turn_revision,
        }
    }

    pub fn hand_state(&self, position: usize) -> WsDominoesHandState {
        let is_current_turn =
            self.phase == DominoesPhase::Play && self.current_position == position;
        let legal_plays = if is_current_turn {
            self.legal_plays(position)
        } else {
            Vec::new()
        };
        let can_draw = self.phase == DominoesPhase::Play
            && is_current_turn
            && legal_plays.is_empty()
            && self.no_playable_tiles != DominoesNoPlayableTiles::PassWithoutDraw
            && !self.boneyard.is_empty();
        let can_pass = self.phase == DominoesPhase::Play
            && is_current_turn
            && legal_plays.is_empty()
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
            legal_plays,
            can_draw,
            can_pass,
        }
    }
}

fn aligned_orientation(direction: DominoesPort) -> DominoesOrientation {
    match direction {
        DominoesPort::Left | DominoesPort::Right => DominoesOrientation::Horizontal,
        DominoesPort::Top | DominoesPort::Bottom => DominoesOrientation::Vertical,
    }
}

fn perpendicular_orientation(direction: DominoesPort) -> DominoesOrientation {
    match direction {
        DominoesPort::Left | DominoesPort::Right => DominoesOrientation::Vertical,
        DominoesPort::Top | DominoesPort::Bottom => DominoesOrientation::Horizontal,
    }
}

fn half_extents(orientation: DominoesOrientation) -> (i32, i32) {
    match orientation {
        DominoesOrientation::Horizontal => (TILE_LONG_HALF, TILE_SHORT_HALF),
        DominoesOrientation::Vertical => (TILE_SHORT_HALF, TILE_LONG_HALF),
    }
}

fn extent_in_direction(orientation: DominoesOrientation, direction: DominoesPort) -> i32 {
    let (half_width, half_height) = half_extents(orientation);
    match direction {
        DominoesPort::Left | DominoesPort::Right => half_width,
        DominoesPort::Top | DominoesPort::Bottom => half_height,
    }
}

fn direction_vector(direction: DominoesPort) -> (i32, i32) {
    match direction {
        DominoesPort::Left => (-1, 0),
        DominoesPort::Right => (1, 0),
        DominoesPort::Top => (0, -1),
        DominoesPort::Bottom => (0, 1),
    }
}

fn opposite(direction: DominoesPort) -> DominoesPort {
    match direction {
        DominoesPort::Left => DominoesPort::Right,
        DominoesPort::Right => DominoesPort::Left,
        DominoesPort::Top => DominoesPort::Bottom,
        DominoesPort::Bottom => DominoesPort::Top,
    }
}

fn turn_left(direction: DominoesPort) -> DominoesPort {
    match direction {
        DominoesPort::Left => DominoesPort::Bottom,
        DominoesPort::Right => DominoesPort::Top,
        DominoesPort::Top => DominoesPort::Left,
        DominoesPort::Bottom => DominoesPort::Right,
    }
}

fn turn_right(direction: DominoesPort) -> DominoesPort {
    match direction {
        DominoesPort::Left => DominoesPort::Top,
        DominoesPort::Right => DominoesPort::Bottom,
        DominoesPort::Top => DominoesPort::Right,
        DominoesPort::Bottom => DominoesPort::Left,
    }
}

fn random_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos() as u64)
        .unwrap_or(1)
}

fn next_random(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

fn shuffle(deck: &mut [Tile], state: &mut u64) {
    for index in (1..deck.len()).rev() {
        let other = next_random(state) as usize % (index + 1);
        deck.swap(index, other);
    }
}

#[cfg(test)]
#[path = "core/tests.rs"]
mod tests;
