use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::fmt::Display;
use typeshare::typeshare;

pub const LANDLORD_CARDS: [i32; 54] = {
    let mut cards = [0; 54];
    let mut i = 0;
    while i < 54 {
        cards[i] = (i + 1) as i32;
        i += 1;
    }
    cards
};

#[typeshare]
#[repr(i32)]
#[derive(Debug, Clone, Copy, Serialize_repr, Deserialize_repr)]
#[allow(non_camel_case_types)]
pub enum LandlordCard {
    SPADE_3 = 1,
    SPADE_4 = 2,
    SPADE_5 = 3,
    SPADE_6 = 4,
    SPADE_7 = 5,
    SPADE_8 = 6,
    SPADE_9 = 7,
    SPADE_10 = 8,
    SPADE_J = 9,
    SPADE_Q = 10,
    SPADE_K = 11,
    SPADE_A = 12,
    SPADE_2 = 13,
    HEART_3 = 14,
    HEART_4 = 15,
    HEART_5 = 16,
    HEART_6 = 17,
    HEART_7 = 18,
    HEART_8 = 19,
    HEART_9 = 20,
    HEART_10 = 21,
    HEART_J = 22,
    HEART_Q = 23,
    HEART_K = 24,
    HEART_A = 25,
    HEART_2 = 26,
    CLUB_3 = 27,
    CLUB_4 = 28,
    CLUB_5 = 29,
    CLUB_6 = 30,
    CLUB_7 = 31,
    CLUB_8 = 32,
    CLUB_9 = 33,
    CLUB_10 = 34,
    CLUB_J = 35,
    CLUB_Q = 36,
    CLUB_K = 37,
    CLUB_A = 38,
    CLUB_2 = 39,
    DIAMOND_3 = 40,
    DIAMOND_4 = 41,
    DIAMOND_5 = 42,
    DIAMOND_6 = 43,
    DIAMOND_7 = 44,
    DIAMOND_8 = 45,
    DIAMOND_9 = 46,
    DIAMOND_10 = 47,
    DIAMOND_J = 48,
    DIAMOND_Q = 49,
    DIAMOND_K = 50,
    DIAMOND_A = 51,
    DIAMOND_2 = 52,
    JOKER_SMALL = 53,
    JOKER_BIG = 54,
}

#[typeshare]
#[repr(i8)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LandlordPhase {
    Start,
    CallLandlord,
    Play,
    Settlement,
}

#[typeshare]
#[repr(i32)]
#[derive(Debug, Clone, Copy, Serialize_repr, Deserialize_repr)]
#[allow(non_camel_case_types)]
pub enum LandlordRoutes {
    CALL_LANDLORD = 1001,
}

#[typeshare]
#[repr(i32)]
#[derive(Debug, Clone, Copy, Serialize_repr, Deserialize_repr)]
#[allow(non_camel_case_types)]
pub enum LandlordWsCode {
    CALL_LANDLORD = 1001,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsCallLandlordEvent {
    pub name: String,
    pub score: u8,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsCallLandlordRequest {
    pub score: u8,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsDealEvent {
    pub name: String,
    pub cards: Vec<i32>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsDealFaceDownCardsEvent {
    pub cards: Vec<i32>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsDealOpenCardsEvent {
    pub name: String,
    pub cards: Vec<i32>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsDealRequest {
    pub cards: Vec<i32>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsLandlordGameOverEvent {
    pub is_landlord: bool,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsPlayEvent {
    pub name: String,
    pub cards: Vec<i32>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsPlayRequest {
    pub cards: Vec<i32>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsShowHiddenCardsEvent {
    pub name: String,
    pub cards: Vec<i32>,
}

impl LandlordPhase {
    pub fn next(self) -> Self {
        match self {
            Self::Start => Self::CallLandlord,
            Self::CallLandlord => Self::Play,
            Self::Play => Self::Settlement,
            Self::Settlement => Self::Start,
        }
    }
}
impl Display for LandlordPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Start => write!(f, "Start"),
            Self::CallLandlord => write!(f, "CallLandlord"),
            Self::Play => write!(f, "Play"),
            Self::Settlement => write!(f, "Settlement"),
        }
    }
}
