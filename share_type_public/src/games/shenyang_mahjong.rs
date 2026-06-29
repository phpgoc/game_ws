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
}

#[typeshare]
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[allow(non_camel_case_types)]
pub enum ShenyangMahjongMeldKind {
    CHI = 1,
    PENG = 2,
}

#[typeshare]
#[repr(i8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShenyangMahjongPhase {
    Start,
    Play,
    Settlement,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsShenyangMahjongClaimWindowEvent {
    pub tile: i32,
    pub from_position: i32,
    pub eligible_positions: Vec<i32>,
    pub seconds: i32,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsShenyangMahjongDealEvent {
    pub my_tiles: Vec<i32>,
    pub dealer_position: i32,
    pub current_position: i32,
    pub wall_count: i32,
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
pub struct WsShenyangMahjongSettlementEvent {
    pub winner_positions: Vec<i32>,
    pub from_position: Option<i32>,
    pub win_tile: Option<i32>,
    pub is_self_draw: bool,
    pub players: Vec<WsShenyangMahjongPlayerSnapshot>,
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
