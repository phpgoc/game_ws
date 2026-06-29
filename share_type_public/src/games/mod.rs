pub mod landlord;
pub mod shenyang_mahjong;

pub use crate::GameParamRange;
use serde::{Deserialize, Serialize};
use typeshare::typeshare;

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomPlayerLimit {
    pub min_players: i32,
    pub max_players: i32,
}

pub trait SettingTrait {}
