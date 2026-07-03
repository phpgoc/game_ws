use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::fmt::Display;
use typeshare::typeshare;

pub const SHENYANG_MAHJONG_TILE_KINDS: [i32; 34] = [
    1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 12, 13, 14, 15, 16, 17, 18, 19, 21, 22, 23, 24, 25, 26, 27, 28,
    29, 31, 32, 33, 34, 35, 36, 37,
];

#[typeshare]
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[allow(non_camel_case_types)]
pub enum ShenyangMahjongAction {
    DRAW = 1,
    DISCARD = 2,
    CHI = 3,
    PENG = 4,
    HU = 5,
    PASS = 6,
    GANG = 7,
}

#[typeshare]
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[allow(non_camel_case_types)]
pub enum ShenyangMahjongMeldKind {
    CHI = 1,
    PENG = 2,
    GANG = 3,
}

#[typeshare]
#[repr(i8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
pub enum ShenyangMahjongPhase {
    Start,
    Play,
    Settlement,
}

#[typeshare]
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
pub enum ShenyangMahjongWinPattern {
    Standard = 1,
    SevenPairs = 2,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsShenyangMahjongClaimOption {
    pub position: i32,
    pub can_hu: bool,
    pub can_peng: bool,
    pub can_gang: bool,
    pub chi_options: Vec<Vec<i32>>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsShenyangMahjongClaimWindowEvent {
    pub tile: i32,
    pub from_position: i32,
    pub eligible_positions: Vec<i32>,
    pub seconds: i32,
    #[serde(default)]
    pub is_rob_gang: bool,
    #[serde(default)]
    pub options: Vec<WsShenyangMahjongClaimOption>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsShenyangMahjongDealEvent {
    pub my_tiles: Vec<i32>,
    pub dealer_position: i32,
    pub current_position: i32,
    pub wall_count: i32,
    pub turn_countdown: i32,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsShenyangMahjongMeld {
    pub kind: ShenyangMahjongMeldKind,
    pub tiles: Vec<i32>,
    pub from_position: Option<i32>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsShenyangMahjongPlayEvent {
    pub name: String,
    pub position: i32,
    pub action: ShenyangMahjongAction,
    pub tiles: Vec<i32>,
    pub target_tile: Option<i32>,
    pub from_position: Option<i32>,
    pub wall_count: i32,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsShenyangMahjongPlayRequest {
    pub action: ShenyangMahjongAction,
    pub tiles: Vec<i32>,
    pub target_tile: Option<i32>,
    pub from_position: Option<i32>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsShenyangMahjongPlayerSnapshot {
    pub position: i32,
    pub name: String,
    pub hand_tiles: Vec<i32>,
    pub discards: Vec<i32>,
    pub melds: Vec<WsShenyangMahjongMeld>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsShenyangMahjongPublicPlayerSnapshot {
    pub position: i32,
    pub name: String,
    #[serde(default)]
    pub away: bool,
    #[serde(default)]
    pub is_ai: bool,
    pub hand_count: i32,
    pub discards: Vec<i32>,
    pub melds: Vec<WsShenyangMahjongMeld>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsShenyangMahjongScoreChange {
    pub position: i32,
    pub score: i32,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsShenyangMahjongSettlementEvent {
    pub winner_positions: Vec<i32>,
    pub from_position: Option<i32>,
    pub win_tile: Option<i32>,
    pub is_self_draw: bool,
    #[serde(default)]
    pub is_reverse_win: bool,
    pub score_changes: Vec<WsShenyangMahjongScoreChange>,
    #[serde(default)]
    pub winner_details: Vec<WsShenyangMahjongWinnerDetail>,
    pub players: Vec<WsShenyangMahjongPlayerSnapshot>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsShenyangMahjongTableSnapshotEvent {
    pub my_tiles: Vec<i32>,
    pub players: Vec<WsShenyangMahjongPublicPlayerSnapshot>,
    pub phase: ShenyangMahjongPhase,
    pub current_position: i32,
    pub dealer_position: i32,
    pub wall_count: i32,
    pub turn_countdown: i32,
    #[serde(default)]
    pub last_drawn_tile: Option<i32>,
    #[serde(default)]
    pub settlement: Option<WsShenyangMahjongSettlementEvent>,
    pub claim_window: Option<WsShenyangMahjongClaimWindowEvent>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsShenyangMahjongWinnerDetail {
    pub position: i32,
    pub pattern: ShenyangMahjongWinPattern,
    pub is_self_draw: bool,
    #[serde(default)]
    pub is_reverse_win: bool,
    pub score: i32,
}

impl Display for ShenyangMahjongPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Start => write!(f, "Start"),
            Self::Play => write!(f, "Play"),
            Self::Settlement => write!(f, "Settlement"),
        }
    }
}
