pub mod landlord;

use serde::{Deserialize, Serialize};
use typeshare::typeshare;

pub trait SettingTrait {}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomPlayerLimit {
    pub min_players: i32,
    pub max_players: i32,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameParam {
    pub default: i32,
    pub min: i32,
    pub max: i32,
}
