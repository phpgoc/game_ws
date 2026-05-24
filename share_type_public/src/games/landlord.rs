use serde::{Deserialize, Serialize};
use typeshare::typeshare;
use crate::{GameParam, GameSettings};
use serde_json::json;

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LandlordRoomSettings {
    pub round_time: GameParam,
    pub away_time: GameParam,
    pub play_time: GameParam,
    pub deal_time: GameParam,
}

impl GameSettings for LandlordRoomSettings {
    fn to_full_json(&self) -> serde_json::Value {
        serde_json::to_value(&self).unwrap_or(serde_json::Value::Null)
    }
    
    fn to_current_json(&self) -> serde_json::Value {
        json!({
            "round_time": self.round_time.current,
            "away_time": self.away_time.current,
            "play_time": self.play_time.current,
            "deal_time": self.deal_time.current,
        })
    }
    
    fn update_from_json(&mut self, data: &serde_json::Value) -> Result<(), String> {
        if let Some(round_time) = data.get("round_time").and_then(|v| v.as_i64()) {
            self.round_time.set_current(round_time as i32)?;
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
