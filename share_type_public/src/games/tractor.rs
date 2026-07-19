use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::collections::HashMap;
use std::fmt::Display;
use typeshare::typeshare;

#[typeshare]
#[repr(i8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
pub enum TractorPhase {
    Start = 0,
    Deal = 1,
    Bury = 2,
    Play = 3,
    Settlement = 4,
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
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[allow(non_camel_case_types)]
pub enum TractorRoutes {
    DECLARE_TRUMP = 4001,
    BURY_BOTTOM = 4002,
    SELECT_TRUMP = 4003,
}

#[typeshare]
#[repr(i8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
pub enum TractorSuit {
    SPADE = 0,
    HEART = 1,
    CLUB = 2,
    DIAMOND = 3,
}

#[typeshare]
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[allow(non_camel_case_types)]
pub enum TractorWsCode {
    TRUMP_DECLARED = 4001,
    BOTTOM_CARDS = 4002,
    BOTTOM_BURIED = 4003,
    HAND_UPDATED = 4004,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsTractorBottomBuriedEvent {
    pub position: i32,
    pub name: String,
    pub bottom_card_count: i32,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsTractorBottomCardsEvent {
    pub position: i32,
    pub cards: Vec<i32>,
    pub required_count: i32,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsTractorBuryBottomRequest {
    pub cards: Vec<i32>,
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
    pub dealt_count: i32,
    pub total_deal_count: i32,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsTractorDeclareTrumpRequest {
    pub cards: Vec<i32>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsTractorHandEvent {
    pub position: i32,
    pub cards: Vec<i32>,
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
pub struct WsTractorPlayerHandCount {
    pub position: i32,
    pub hand_count: i32,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsTractorSelectTrumpRequest {
    pub trump_suit: TractorSuit,
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
    #[serde(default)]
    pub player_scores: HashMap<i32, i32>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsTractorTableSnapshotEvent {
    pub phase: TractorPhase,
    pub deck_count: i32,
    pub target_rank: TractorRank,
    pub final_target_rank: TractorRank,
    pub removed_rank_count: i32,
    pub round_index: i32,
    pub blood_enabled: bool,
    pub blood_start_score: i32,
    pub blood_score_per_unit: i32,
    pub bottom_card_count: i32,
    pub hand_count: i32,
    pub dealer_position: i32,
    pub trump_suit: Option<TractorSuit>,
    pub declaration: Option<WsTractorTrumpDeclaration>,
    pub dealt_count: i32,
    pub total_deal_count: i32,
    pub player_hand_counts: Vec<WsTractorPlayerHandCount>,
    pub current_position: i32,
    pub trick_index: i32,
    pub current_trick: Vec<WsTractorPlayedCards>,
    pub turn_countdown: i32,
    #[serde(default)]
    pub player_scores: HashMap<i32, i32>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsTractorTrumpDeclaration {
    pub position: i32,
    pub name: String,
    pub cards: Vec<i32>,
    pub trump_suit: TractorSuit,
    pub strength: i32,
    pub target_rank: TractorRank,
}

impl Display for TractorPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Start => write!(f, "Start"),
            Self::Deal => write!(f, "Deal"),
            Self::Bury => write!(f, "Bury"),
            Self::Play => write!(f, "Play"),
            Self::Settlement => write!(f, "Settlement"),
        }
    }
}
