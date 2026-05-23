use serde::{Serialize, de::DeserializeOwned};
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("unexpected binary frame")]
    BinaryFrame,
    #[error("json parse/encode failed: {0}")]
    Json(#[from] serde_json::Error),
}

pub fn to_text_message<T: Serialize>(value: &T) -> Result<Message, TransportError> {
    Ok(Message::Text(serde_json::to_string(value)?.into()))
}

pub fn from_message<T: DeserializeOwned>(message: Message) -> Result<Option<T>, TransportError> {
    match message {
        Message::Text(text) => Ok(Some(serde_json::from_str(text.as_ref())?)),
        Message::Binary(_) => Err(TransportError::BinaryFrame),
        Message::Ping(_) | Message::Pong(_) | Message::Close(_) | Message::Frame(_) => Ok(None),
    }
}
