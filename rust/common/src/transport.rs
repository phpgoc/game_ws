//! WebSocket 传输层的最小序列化适配。
//!
//! 领域模块只关心 Rust 值，这里负责在文本帧和 JSON 之间转换，并把空帧、
//! 非文本帧和反序列化错误收敛成统一错误类型。

use serde::{Serialize, de::DeserializeOwned};
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("unexpected binary frame")]
    BinaryFrame,
    #[error("json parse/encode failed: {0}")]
    Json(#[from] serde_json::Error),
}

/// 将 WebSocket 文本帧反序列化为业务数据；控制帧返回 `None`，二进制帧视为错误。
pub fn from_message<T: DeserializeOwned>(message: Message) -> Result<Option<T>, TransportError> {
    // 非文本帧交给上层忽略；文本帧即使为空也要返回明确的 JSON 错误，不能
    // 把客户端协议错误静默当成“没有请求”。
    match message {
        Message::Text(text) => Ok(Some(serde_json::from_str(text.as_ref())?)),
        Message::Binary(_) => Err(TransportError::BinaryFrame),
        Message::Ping(_) | Message::Pong(_) | Message::Close(_) | Message::Frame(_) => Ok(None),
    }
}

/// 将可序列化的业务数据编码为 WebSocket 文本帧。
pub fn to_text_message<T: Serialize>(value: &T) -> Result<Message, TransportError> {
    // 所有出站事件统一编码成文本帧，保证浏览器原生 WebSocket 和 Tauri
    // native runtime 看到完全相同的协议。
    Ok(Message::Text(serde_json::to_string(value)?.into()))
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};
    use tokio_tungstenite::tungstenite::Message;

    use super::{TransportError, from_message, to_text_message};

    #[derive(Debug, Deserialize, PartialEq, Serialize)]
    struct Payload {
        value: u32,
    }

    #[test]
    fn binary_frames_are_rejected() {
        assert!(matches!(
            from_message::<Payload>(Message::Binary(Vec::new().into())),
            Err(TransportError::BinaryFrame)
        ));
    }

    #[test]
    fn control_frames_are_ignored() {
        assert_eq!(
            from_message::<Payload>(Message::Ping(Vec::new().into())).unwrap(),
            None
        );
    }

    #[test]
    fn invalid_json_is_reported() {
        assert!(matches!(
            from_message::<Payload>(Message::Text("invalid".into())),
            Err(TransportError::Json(_))
        ));
    }

    #[test]
    fn text_messages_round_trip_json() {
        let message = to_text_message(&Payload { value: 42 }).unwrap();
        let decoded = from_message::<Payload>(message).unwrap();

        assert_eq!(decoded, Some(Payload { value: 42 }));
    }
}
