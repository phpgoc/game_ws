use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::collections::HashMap;
use std::fmt::Display;
use typeshare::typeshare;

#[typeshare]
#[repr(i8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
pub enum UpgradePhase {
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
pub enum UpgradeRank {
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
pub enum UpgradeRoutes {
    DECLARE_TRUMP = 5001,
    BURY_BOTTOM = 5002,
    SELECT_TRUMP = 5003,
}

#[typeshare]
#[repr(i8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
pub enum UpgradeSuit {
    SPADE = 0,
    HEART = 1,
    CLUB = 2,
    DIAMOND = 3,
}

#[typeshare]
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[allow(non_camel_case_types)]
pub enum UpgradeWsCode {
    TRUMP_DECLARED = 5001,
    BOTTOM_CARDS = 5002,
    BOTTOM_BURIED = 5003,
    HAND_UPDATED = 5004,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsUpgradeBottomBuriedEvent {
    pub position: i32,
    pub name: String,
    pub bottom_card_count: i32,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsUpgradeBottomCardsEvent {
    pub position: i32,
    pub cards: Vec<i32>,
    pub required_count: i32,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsUpgradeBuryBottomRequest {
    pub cards: Vec<i32>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsUpgradeDealEvent {
    pub position: i32,
    pub cards: Vec<i32>,
    pub deck_count: i32,
    pub hand_count: i32,
    pub bottom_card_count: i32,
    pub target_rank: UpgradeRank,
    pub dealt_count: i32,
    pub total_deal_count: i32,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsUpgradeDeclareTrumpRequest {
    pub cards: Vec<i32>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsUpgradeHandEvent {
    pub position: i32,
    pub cards: Vec<i32>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsUpgradeFailedThrowEvent {
    pub position: i32,
    pub attempted_cards: Vec<i32>,
    pub played_cards: Vec<i32>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsUpgradePlayEvent {
    pub position: i32,
    pub name: String,
    pub cards: Vec<i32>,
    pub trick_index: i32,
    pub next_position: i32,
    pub remaining_hand_count: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed_throw: Option<WsUpgradeFailedThrowEvent>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsUpgradePlayRequest {
    pub cards: Vec<i32>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsUpgradePlayedCards {
    pub position: i32,
    pub name: String,
    pub cards: Vec<i32>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsUpgradePlayerHandCount {
    pub position: i32,
    pub hand_count: i32,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsUpgradeSelectTrumpRequest {
    pub trump_suit: UpgradeSuit,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsUpgradeSettlementEvent {
    pub winner_positions: Vec<i32>,
    pub score: i32,
    pub level_change: i32,
    pub target_rank: UpgradeRank,
    pub match_finished: bool,
    pub next_target_rank: Option<UpgradeRank>,
    #[serde(default)]
    pub player_scores: HashMap<i32, i32>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsUpgradeTableSnapshotEvent {
    pub phase: UpgradePhase,
    pub deck_count: i32,
    pub target_rank: UpgradeRank,
    pub final_target_rank: UpgradeRank,
    pub removed_rank_count: i32,
    pub round_index: i32,
    pub attacking_win_score: i32,
    pub score_per_level: i32,
    pub shutout_bonus_levels: i32,
    pub bottom_card_count: i32,
    pub hand_count: i32,
    pub dealer_position: i32,
    pub trump_suit: Option<UpgradeSuit>,
    pub declaration: Option<WsUpgradeTrumpDeclaration>,
    pub dealt_count: i32,
    pub total_deal_count: i32,
    pub player_hand_counts: Vec<WsUpgradePlayerHandCount>,
    pub current_position: i32,
    pub trick_index: i32,
    pub current_trick: Vec<WsUpgradePlayedCards>,
    pub turn_countdown: i32,
    #[serde(default)]
    pub player_scores: HashMap<i32, i32>,
    #[serde(default)]
    pub failed_throws: Vec<WsUpgradeFailedThrowEvent>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsUpgradeTrumpDeclaration {
    pub position: i32,
    pub name: String,
    pub cards: Vec<i32>,
    pub trump_suit: UpgradeSuit,
    pub strength: i32,
    pub target_rank: UpgradeRank,
}

impl Display for UpgradePhase {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Start => write!(formatter, "Start"),
            Self::Deal => write!(formatter, "Deal"),
            Self::Bury => write!(formatter, "Bury"),
            Self::Play => write!(formatter, "Play"),
            Self::Settlement => write!(formatter, "Settlement"),
        }
    }
}

#[cfg(test)]
#[path = "upgrade/tests.rs"]
mod tests;
