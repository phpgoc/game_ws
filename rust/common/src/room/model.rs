use std::collections::HashMap;

use serde::Serialize;
use serde_json::Value;
use share_type_public::{CommonEvent, GameParam, WsRequest, WsWithoutDataResponse, ws::WsResponse};

use crate::GameSettings;

pub type ClientRequest = WsRequest<Value>;

#[derive(Debug, Clone, Serialize)]
pub struct Delivery {
    pub recipient: SessionId,
    pub payload: OutboundPayload,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct Dispatch {
    pub messages: Vec<Delivery>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum OutboundPayload {
    Response(RequestResponse),
    Event(CommonEvent<Value>),
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum RequestResponse {
    WithoutData(WsWithoutDataResponse),
    WithData(WsResponse<Value>),
}

pub type SessionId = u64;
pub type SettingsBuilderResult = (GameSettings, HashMap<String, GameParam>);
