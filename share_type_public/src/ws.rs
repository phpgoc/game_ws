use serde::{Deserialize, Serialize};
use typeshare::typeshare;

use crate::r#const::WsResponseCode;
use crate::games::SettingTrait;

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsRequest<T> {
    pub route: i32,
    pub data: T,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsWithoutDataRequest {
    pub route: i32,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsResponse<T> {
    pub code: WsResponseCode,
    pub data: T,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsWithoutDataResponse {
    pub code: WsResponseCode,
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
pub struct WsJoinResponse<T: SettingTrait> {
    pub settings: T,
}


#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsJoinEvent {
    pub name: String,
    pub position: i32,
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
pub struct WsDisbandEvent {
    pub name: String,
}


#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsResumeEvent {
    pub name: String,

}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsStartEvent {
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
pub struct WsAwayEvent {
    pub name: String,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsChangeTurnEvent {
    pub position: i32,
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
