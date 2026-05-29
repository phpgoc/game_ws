use serde::{Deserialize, Serialize};
use typeshare::typeshare;
use crate::{GameParam, GameSettings};
use serde_json::json;
use serde_repr::{Deserialize_repr, Serialize_repr};

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
pub struct LandlordRoomSettings {
    pub animation_time: GameParam, //出牌动画时间 毫秒
    pub away_time: GameParam, // away状态出牌时间
    pub play_time: GameParam, //出牌时间
    pub deal_time: GameParam, //暂时没用
}

impl Default for LandlordRoomSettings {
    fn default() -> Self {
        Self {
            animation_time: GameParam { current: 200, min: 50, max: 2000 },
            away_time: GameParam { current: 5, min: 2, max: 5 },
            play_time: GameParam { current: 30, min: 20, max: 50 },
            deal_time: GameParam { current: 3000, min: 500, max: 4000 },
        }
    }
}

impl GameSettings for LandlordRoomSettings {
    fn to_full_json(&self) -> serde_json::Value {
        serde_json::to_value(&self).unwrap_or(serde_json::Value::Null)
    }
    
    fn to_current_json(&self) -> serde_json::Value {
        json!({
            "round_time": self.animation_time.current,
            "away_time": self.away_time.current,
            "play_time": self.play_time.current,
            "deal_time": self.deal_time.current,
        })
    }
    
    fn update_from_json(&mut self, data: &serde_json::Value) -> Result<(), String> {
        if let Some(round_time) = data.get("round_time").and_then(|v| v.as_i64()) {
            self.animation_time.set_current(round_time as i32)?;
        }
        if let Some(away_time) = data.get("away_time").and_then(|v| v.as_i64()) {
            self.away_time.set_current(away_time as i32)?;
        }
        if let Some(play_time) = data.get("play_time").and_then(|v| v.as_i64()) {
            self.play_time.set_current(play_time as i32)?;
        }
        if let Some(deal_time) = data.get("deal_time").and_then(|v| v.as_i64()) {
            self.deal_time.set_current(deal_time as i32)?;
        }
        Ok(())
    }
    
    fn clone_box(&self) -> Box<dyn GameSettings> {
        Box::new(self.clone())
    }
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsCallLandlordRequest {
    pub score: u8,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsCallLandlordEvent {
    pub name: String,
    pub score: u8,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsDealRequest {
    pub cards: Vec<i32>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsDealEvent {
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
pub struct WsPlayEvent {
    pub name: String,
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
pub struct WsDealFaceDownCardsEvent {
    pub cards: Vec<i32>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsShowHiddenCardsEvent {
    pub cards: Vec<String>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsLandlordGameOverEvent {
    pub winner: String,
    pub is_landlord: bool,
}
