use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::collections::HashMap;
use typeshare::typeshare;

#[typeshare]
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
pub enum DominoesRule {
    Simple = 0,
    FiveUp = 1,
}

#[typeshare]
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
pub enum DominoesNoPlayableTiles {
    KeepDrawing = 0,
    DrawOne = 1,
    PassWithoutDraw = 2,
}

#[typeshare]
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
pub enum DominoesPhase {
    Play = 0,
    RoundOver = 1,
    GameOver = 2,
}

#[typeshare]
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
pub enum DominoesPort {
    Left = 0,
    Right = 1,
    Top = 2,
    Bottom = 3,
}

#[typeshare]
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
pub enum DominoesOrientation {
    Horizontal = 0,
    Vertical = 1,
}

#[typeshare]
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
pub enum DominoesActionSource {
    Human = 0,
    NativeAi = 1,
    AiTakeover = 2,
    Timeout = 3,
}

#[typeshare]
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[allow(non_camel_case_types)]
pub enum DominoesRoutes {
    PLAY_TILE = 5001,
    DRAW_TILE = 5002,
    PASS = 5003,
}

#[typeshare]
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[allow(non_camel_case_types)]
pub enum DominoesWsCode {
    ROUND_START = 5001,
    DEAL = 5002,
    PLAY_TILE = 5003,
    DRAW_TILE = 5004,
    DRAWN_TILE = 5005,
    PASS = 5006,
    TURN = 5007,
    TABLE_SNAPSHOT = 5008,
    ROUND_OVER = 5009,
    GAME_OVER = 5010,
    HAND_STATE = 5011,
}

/// 双六骨牌中的一张牌。`id` 在 0..28 内稳定且一局中唯一。
#[typeshare]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WsDominoesTile {
    pub id: i32,
    pub a: i32,
    pub b: i32,
}

/// 当前桌面可接牌的一个端点。
#[typeshare]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WsDominoesEndpoint {
    pub endpoint_id: i32,
    pub placement_id: i32,
    pub pip: i32,
    pub port: DominoesPort,
    pub anchor_x: i32,
    pub anchor_y: i32,
    pub direction: DominoesPort,
}

/// 已落桌骨牌。首张牌的 `connected_endpoint_id` 为 `None`。
#[typeshare]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WsDominoesPlacement {
    pub placement_id: i32,
    pub tile: WsDominoesTile,
    pub connected_endpoint_id: Option<i32>,
    pub connected_port: Option<DominoesPort>,
    pub center_x: i32,
    pub center_y: i32,
    pub orientation: DominoesOrientation,
    pub flipped: bool,
    pub new_endpoints: Vec<WsDominoesEndpoint>,
}

/// 服务端计算出的一个合法落点；`endpoint_id=None` 表示首张牌的中央落点。
#[typeshare]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WsDominoesLegalPlay {
    pub tile_id: i32,
    pub endpoint_id: Option<i32>,
    /// Five-Up 在此落点的即时预期得分；Simple 始终为 0。
    pub score: i32,
}

#[typeshare]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WsDominoesPlayRequest {
    pub tile_id: i32,
    pub endpoint_id: Option<i32>,
}

#[typeshare]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WsDominoesRoundStartEvent {
    pub round: i32,
    pub starter_position: i32,
    pub hand_size: i32,
    pub boneyard_count: i32,
    pub remaining_seconds: i32,
    pub turn_revision: i32,
}

#[typeshare]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WsDominoesDealEvent {
    pub position: i32,
    pub hand: Vec<WsDominoesTile>,
}

#[typeshare]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WsDominoesPlayEvent {
    pub position: i32,
    pub placement: WsDominoesPlacement,
    /// Five-Up 本次落牌所得分；Simple 始终为 0。
    pub score: i32,
    pub total_score: i32,
    pub source: DominoesActionSource,
}

/// 公开摸牌事件，不泄露摸到的牌面。
#[typeshare]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WsDominoesDrawEvent {
    pub position: i32,
    pub boneyard_count: i32,
    pub source: DominoesActionSource,
}

/// 只发送给摸牌者的私有牌面事件。
#[typeshare]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WsDominoesDrawnTileEvent {
    pub tile: WsDominoesTile,
    pub playable: bool,
}

#[typeshare]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WsDominoesPassEvent {
    pub position: i32,
    pub consecutive_passes: i32,
    pub source: DominoesActionSource,
}

#[typeshare]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WsDominoesTurnEvent {
    pub position: i32,
    pub boneyard_count: i32,
    pub remaining_seconds: i32,
    pub turn_revision: i32,
}

#[typeshare]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WsDominoesPlayerState {
    pub position: i32,
    pub hand_count: i32,
    pub score: i32,
    pub is_ai: bool,
    pub away: bool,
    pub is_ai_takeover: bool,
}

#[typeshare]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WsDominoesHandState {
    pub hand: Vec<WsDominoesTile>,
    pub legal_plays: Vec<WsDominoesLegalPlay>,
    pub can_draw: bool,
    pub can_pass: bool,
}

#[typeshare]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WsDominoesTableSnapshotEvent {
    pub phase: DominoesPhase,
    pub round: i32,
    pub current_position: i32,
    pub rule: DominoesRule,
    pub no_playable_tiles: DominoesNoPlayableTiles,
    pub target_score: i32,
    pub boneyard_count: i32,
    pub placements: Vec<WsDominoesPlacement>,
    pub endpoints: Vec<WsDominoesEndpoint>,
    pub players: Vec<WsDominoesPlayerState>,
    pub last_play_position: Option<i32>,
    pub consecutive_passes: i32,
    pub remaining_seconds: i32,
    pub turn_revision: i32,
}

#[typeshare]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WsDominoesReJoinResponse {
    pub table: WsDominoesTableSnapshotEvent,
    pub hand: WsDominoesHandState,
}

#[typeshare]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WsDominoesRoundOverEvent {
    pub round: i32,
    pub winner_position: i32,
    pub blocked: bool,
    pub round_score: i32,
    pub scores: HashMap<i32, i32>,
    pub remaining_hands: HashMap<i32, Vec<WsDominoesTile>>,
    pub remaining_seconds: i32,
}

#[typeshare]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WsDominoesGameOverEvent {
    pub winner_positions: Vec<i32>,
    pub target_score: i32,
    pub scores: HashMap<i32, i32>,
}
