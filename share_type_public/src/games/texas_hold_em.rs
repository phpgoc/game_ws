use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::fmt::Display;
use typeshare::typeshare;

pub const TEXAS_HOLD_EM_CARDS: [i32; 52] = {
    let mut cards = [0; 52];
    let mut i = 0;
    while i < 52 {
        cards[i] = (i + 1) as i32;
        i += 1;
    }
    cards
};

#[typeshare]
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[allow(non_camel_case_types)]
pub enum TexasHoldEmAction {
    FOLD = 1,
    CHECK = 2,
    CALL = 3,
    BET = 4,
    RAISE = 5,
    ALL_IN = 6,
}

#[typeshare]
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[allow(non_camel_case_types)]
pub enum TexasHoldEmAutoStrategy {
    CHECK_FOLD = 1,
    CHECK_CALL = 2,
}

#[typeshare]
#[repr(i8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
pub enum TexasHoldEmPhase {
    Start,
    PreFlop,
    Flop,
    Turn,
    River,
    Settlement,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsTexasHoldEmActionEvent {
    pub name: String,
    pub position: i32,
    pub action: TexasHoldEmAction,
    pub amount: i32,
    pub committed: i32,
    pub current_bet: i32,
    pub pot: i32,
    pub chips: i32,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsTexasHoldEmAutoStrategyRequest {
    pub strategy: TexasHoldEmAutoStrategy,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsTexasHoldEmDealEvent {
    pub my_cards: Vec<i32>,
    pub open_cards: Vec<i32>,
    pub public_hole_cards: Vec<WsTexasHoldEmPublicHoleCards>,
    pub dealer_position: i32,
    pub small_blind_position: i32,
    pub big_blind_position: i32,
    pub chips: i32,
    pub pot: i32,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsTexasHoldEmPlayRequest {
    pub action: TexasHoldEmAction,
    #[serde(default)]
    pub amount: i32,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsTexasHoldEmPublicHoleCards {
    pub position: i32,
    pub cards: Vec<i32>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsTexasHoldEmPublicCardsEvent {
    pub phase: TexasHoldEmPhase,
    pub cards: Vec<i32>,
    pub pot: i32,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsTexasHoldEmPublicHoleCards {
    pub position: i32,
    pub cards: Vec<i32>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsTexasHoldEmSettlementEvent {
    pub winners: Vec<i32>,
    pub pot: i32,
    pub public_cards: Vec<i32>,
    pub players: Vec<WsTexasHoldEmSettlementPlayer>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsTexasHoldEmSettlementPlayer {
    pub position: i32,
    pub name: String,
    pub cards: Vec<i32>,
    pub open_cards: Vec<i32>,
    pub folded: bool,
    pub chips: i32,
    pub hand_rank: i32,
    pub hand_name: String,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsTexasHoldEmTurnEvent {
    pub position: i32,
    pub phase: TexasHoldEmPhase,
    pub call_amount: i32,
    pub min_raise: i32,
    pub current_bet: i32,
    pub pot: i32,
    pub turn_countdown: i32,
}

impl Display for TexasHoldEmPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Start => write!(f, "Start"),
            Self::PreFlop => write!(f, "PreFlop"),
            Self::Flop => write!(f, "Flop"),
            Self::Turn => write!(f, "Turn"),
            Self::River => write!(f, "River"),
            Self::Settlement => write!(f, "Settlement"),
        }
    }
}
