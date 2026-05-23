use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "data")]
pub enum ClientEvent {
    Ping { ts: u64 },
    JoinTable { table_id: String, user_id: String },
    CallLandlord { score: u8 },
    PlayCards { cards: Vec<String> },
    Pass,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "data")]
pub enum ServerEvent {
    Welcome { service: String },
    Pong { ts: u64 },
    Joined { table_id: String, user_id: String },
    LandlordCalled { score: u8 },
    CardsPlayed { cards: Vec<String> },
    Passed,
    Error { reason: String },
}
