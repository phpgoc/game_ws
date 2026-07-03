use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::fmt::Display;
use typeshare::typeshare;

#[typeshare]
#[repr(i8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
pub enum TractorPhase {
    Start,
    Deal,
    Play,
    Settlement,
}

#[typeshare]
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[allow(non_camel_case_types)]
pub enum TractorRank {
    TWO = 2,
    THREE = 3,
    FOUR = 4,
    FIVE = 5,
    SIX = 6,
    SEVEN = 7,
    EIGHT = 8,
    NINE = 9,
    TEN = 10,
    J = 11,
    Q = 12,
    K = 13,
    A = 14,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsTractorDealEvent {
    pub position: i32,
    pub cards: Vec<i32>,
    pub deck_count: i32,
    pub hand_count: i32,
    pub bottom_card_count: i32,
    pub target_rank: TractorRank,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsTractorPlayEvent {
    pub position: i32,
    pub name: String,
    pub cards: Vec<i32>,
    pub trick_index: i32,
    pub next_position: i32,
    pub remaining_hand_count: i32,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsTractorPlayRequest {
    pub cards: Vec<i32>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsTractorPlayedCards {
    pub position: i32,
    pub name: String,
    pub cards: Vec<i32>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsTractorSettlementEvent {
    pub winner_positions: Vec<i32>,
    pub score: i32,
    pub blood_units: i32,
    pub target_rank: TractorRank,
    pub match_finished: bool,
    pub next_target_rank: Option<TractorRank>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsTractorTableSnapshotEvent {
    pub phase: TractorPhase,
    pub deck_count: i32,
    pub target_rank: TractorRank,
    pub final_target_rank: TractorRank,
    pub removed_rank_mask: i32,
    pub round_index: i32,
    pub blood_enabled: bool,
    pub blood_start_score: i32,
    pub blood_score_per_unit: i32,
    pub bottom_card_count: i32,
    pub hand_count: i32,
    pub dealer_position: i32,
    pub current_position: i32,
    pub trick_index: i32,
    pub current_trick: Vec<WsTractorPlayedCards>,
    pub turn_countdown: i32,
}

impl Display for TractorPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Start => write!(f, "Start"),
            Self::Deal => write!(f, "Deal"),
            Self::Play => write!(f, "Play"),
            Self::Settlement => write!(f, "Settlement"),
        }
    }
}
