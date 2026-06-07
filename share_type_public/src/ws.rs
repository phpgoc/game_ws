use serde::{Deserialize, Serialize};
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

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsCreateRequest {
    pub name: String,
    pub password: String,
}

/// CREATE 响应和 SWAP 成房主时的事件。
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
    pub current_configs: std::collections::HashMap<String, i32>,
    pub existing_members: Vec<WsMemberInfo>,
    pub start_time: i32,
    pub settlement_time: i32,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsMemberInfo {
    pub name: String,
    pub position: i32,
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
