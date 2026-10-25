use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use typeshare::typeshare;

#[typeshare]
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[allow(non_camel_case_types)]
pub enum NiangniangRoutes {
    ACTION = 7001,
}

#[typeshare]
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[allow(non_camel_case_types)]
pub enum NiangniangWsCode {
    STATE = 7001,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NiangniangActionRequest {
    pub action: NiangniangAction,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "data")]
pub enum NiangniangAction {
    Play { cards: Vec<i32> },
    Pass,
    SetAuto { intelligent: bool },
}

#[typeshare]
#[repr(i8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[allow(non_camel_case_types)]
pub enum NiangniangPhase {
    PLAY = 0,
    ROUND_OVER = 1,
    MATCH_OVER = 2,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NiangniangPlayerView {
    pub position: i32,
    pub name: String,
    pub card_count: i32,
    pub connected: bool,
    pub away: bool,
}
/// 下一局发牌后自动执行：上家最大牌换收贡人的最小牌，按该局级牌计算。
#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NiangniangTribute {
    pub from_position: i32,
    pub to_position: i32,
    pub tribute_card: i32,
    pub return_card: i32,
}
#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NiangniangRoundResult {
    pub winning_team: i32,
    pub dealer_won: bool,
    pub advanced: i32,
    pub tribute_count: i32,
    pub next_dealer_position: i32,
    /// 两名队友先出完即结算，此时其余两人的名次不作虚构。
    pub finish_order: Vec<i32>,
}
#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NiangniangStateEvent {
    pub match_id: String,
    pub self_position: i32,
    pub phase: NiangniangPhase,
    pub round_number: i32,
    pub deck_count: i32,
    pub dealer_position: i32,
    /// 两队各自的进度，3..14；14 表示 A，升到 A 后还须赢一局。
    pub team_levels: Vec<i32>,
    pub level_rank: i32,
    pub current_position: i32,
    pub players: Vec<NiangniangPlayerView>,
    /// 物理牌 ID：第一副 1..54，第二副 55..108，点数按 (id-1)%54 计算。
    pub hand: Vec<i32>,
    pub auto_intelligent: bool,
    pub last_play_position: Option<i32>,
    pub last_play_cards: Vec<i32>,
    pub passed_positions: Vec<i32>,
    pub finish_order: Vec<i32>,
    pub tributes: Vec<NiangniangTribute>,
    pub round_result: Option<NiangniangRoundResult>,
    pub match_winner: Option<i32>,
}
