use serde::{Deserialize, Serialize};
use typeshare::typeshare;

use crate::games::{RoomPlayerLimit, SettingTrait};

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LandlordRoomSettings {
    pub limits: RoomPlayerLimit,
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
