use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use typeshare::typeshare;

#[typeshare]
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[allow(non_camel_case_types)]
pub enum WhoIsLandlordRoutes {
    ACTION = 6001,
}

#[typeshare]
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[allow(non_camel_case_types)]
pub enum WhoIsLandlordWsCode {
    STATE = 6001,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WhoIsLandlordActionRequest {
    pub action: WhoIsLandlordAction,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "data")]
pub enum WhoIsLandlordAction {
    Bury { cards: Vec<i32> },
    Play { cards: Vec<i32> },
    Pass,
}

#[typeshare]
#[repr(i8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[allow(non_camel_case_types)]
pub enum WhoIsLandlordPhase {
    DEAL = 0,
    BURY = 1,
    PLAY = 2,
    SETTLEMENT = 3,
}

/// 对局中的身份可见性。
///
/// `UNKNOWN` 只用于对局进行中查看其他玩家；每个客户端始终能看到自己的
/// 身份，结算后服务端才会把三人的身份全部揭示。
#[typeshare]
#[repr(i8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[allow(non_camel_case_types)]
pub enum WhoIsLandlordRole {
    UNKNOWN = 0,
    LANDLORD = 1,
    FARMER = 2,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WhoIsLandlordPlayerView {
    pub position: i32,
    pub name: String,
    pub card_count: i32,
    pub connected: bool,
    pub away: bool,
    pub role: WhoIsLandlordRole,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WhoIsLandlordStateEvent {
    pub match_id: String,
    pub self_position: i32,
    /// 对局进行中只对地主本人返回；结算时对所有人返回。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub landlord_position: Option<i32>,
    pub current_position: i32,
    pub phase: WhoIsLandlordPhase,
    pub players: Vec<WhoIsLandlordPlayerView>,
    pub hand: Vec<i32>,
    pub bottom_cards: Vec<i32>,
    pub last_play_position: Option<i32>,
    pub last_play_cards: Vec<i32>,
    pub multiplier: i32,
    pub bomb_count: i32,
    pub round_score: i32,
    pub scores: Vec<i32>,
}
