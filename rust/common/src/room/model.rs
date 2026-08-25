//! 房间服务和游戏 handler 之间共享的消息模型。

use std::collections::HashMap;

use serde::Serialize;

use serde_json::Value;
use share_type_public::{CommonEvent, GameParam, WsRequest, WsWithoutDataResponse, ws::WsResponse};

use crate::GameSettings;

pub type ClientRequest = WsRequest<Value>;

#[derive(Debug, Clone, Serialize)]
pub struct Delivery {
    /// 目标 session；None 表示广播给当前房间的所有连接。
    pub recipient: SessionId,
    pub payload: OutboundPayload,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct Dispatch {
    /// 一个请求可能产生多条事件，先收集再由 runtime 统一发送。
    pub messages: Vec<Delivery>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum OutboundPayload {
    /// 给客户端的请求响应或房间事件。
    Response(RequestResponse),
    Event(CommonEvent<Value>),
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum RequestResponse {
    /// handler 对请求的最终结果，事件仍通过 `Dispatch` 单独返回。
    WithoutData(WsWithoutDataResponse),
    WithData(WsResponse<Value>),
}

pub type SessionId = u64;
pub type SettingsBuilderResult = (GameSettings, HashMap<String, GameParam>);
