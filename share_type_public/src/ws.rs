use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use typeshare::typeshare;

use crate::r#const::WsResponseCode;

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
    pub route: i32,
    pub code: WsResponseCode,
    pub data: T,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsWithoutDataResponse {
    pub route: i32,
    pub code: WsResponseCode,
}

/// 首个 JOIN 建房后的房主参数响应，以及 SWAP 成房主时的响应。
#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsCreateResponse {
    pub param_descriptions: std::collections::HashMap<String, crate::settings::GameParam>,
    pub start_time: i32,
    pub settlement_time: i32,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsJoinRequest {
    pub name: String,
    pub password: String,
}

/// JOIN 响应，发给新人。
#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsJoinResponse {
    pub current_configs: HashMap<String, i32>,
    pub existing_members: Vec<WsMemberInfo>,
    pub rejoin_data: Option<WsReJoinResponse>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsReJoinResponse {
    pub other_cards_numbers: Option<HashMap<i32, i32>>,
    pub my_cards: Vec<i32>,
    pub now_playing: i32,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsMemberInfo {
    pub name: String,
    pub position: i32,
    pub is_active: bool,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsNameEvent {
    pub name: String,
}

/// SETTING 请求和响应的 payload。
/// 请求：{ current_configs: { key: value, ... } }
/// 响应（事件）：{ current_configs: { key: value, ... } }
#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsSettingPayload {
    pub current_configs: std::collections::HashMap<String, i32>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsPositionEvent {
    pub position: i32,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsMessageRequest {
    pub message: String,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsMessageEvent {
    pub name: String,
    pub message: String,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsSwapPositionPayload {
    pub a: usize,
    pub b: usize,
}
