pub mod landlord;

pub use crate::GameParamRange;
use serde::{Deserialize, Serialize};
use typeshare::typeshare;

pub trait SettingTrait {}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomPlayerLimit {
    pub min_players: i32,
    pub max_players: i32,
}
