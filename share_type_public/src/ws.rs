use serde::{Deserialize, Serialize};
use typeshare::typeshare;

use crate::common::CommonResponse;

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WsMessage {
    pub common: CommonResponse<i32>,
    pub topic: String,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WsRequest<T> {
    // Use `Routes::*` from `src/const.rs`.
    pub route_code: i32,
    pub data: T,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateRequestData {
    pub name: String,
    pub password: String,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JoinRequestData {
    pub name: String,
    pub password: String,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SwapPositionCommonData{
    pub a : String,
    pub b: String,
}
