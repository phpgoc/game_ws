use serde::{Deserialize, Serialize};
use typeshare::typeshare;

use crate::games::{GameParam, SettingTrait};

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LandlordRoomSettings {
    pub round_time: GameParam,
    pub away_time: GameParam,
    pub play_time: GameParam,
    pub deal_time: GameParam,
}

impl SettingTrait for LandlordRoomSettings {}

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
