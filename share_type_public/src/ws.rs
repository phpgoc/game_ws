use serde::{Deserialize, Serialize};
use typeshare::typeshare;

use crate::r#const::Routes;
use crate::games::SettingTrait;

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsRequest<T> {
    pub code: Routes,
    pub data: T,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsWithoutDataRequest {
    pub code: Routes,
}


#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsCreateRequest {
    pub name: String,
    pub password: String,
}


#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsJoinRequest {
    pub name: String,
    pub password: String,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsJoinEvent<T: SettingTrait> {
    pub name: String,
    pub settings: T,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsQuitEvent {
    pub name: String,
}


#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsPauseEvent {
    pub name: String,
}


#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsResumeEvent {
    pub name: String,

}


#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsSettingRequest<T: SettingTrait> {
    pub settings: T,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsSettingEvent<T: SettingTrait> {
    pub name: String,
    pub settings: T,
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
pub struct WsAwayEvent {
    pub name: String,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsDealOpenCardsEvent {
    pub name : String,
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
pub struct WsMessageRequest {
    pub message : String,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsMessageEvent {
    pub name: String,
    pub message : String,
}





#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapPositionPayload {
    pub a: String,
    pub b: String,
}
