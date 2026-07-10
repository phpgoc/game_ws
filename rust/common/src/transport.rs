use serde::{Serialize, de::DeserializeOwned};
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("unexpected binary frame")]
    BinaryFrame,
    #[error("json parse/encode failed: {0}")]
    Json(#[from] serde_json::Error),
}

pub fn from_message<T: DeserializeOwned>(message: Message) -> Result<Option<T>, TransportError> {
    match message {
        Message::Text(text) => Ok(Some(serde_json::from_str(text.as_ref())?)),
        Message::Binary(_) => Err(TransportError::BinaryFrame),
        Message::Ping(_) | Message::Pong(_) | Message::Close(_) | Message::Frame(_) => Ok(None),
    }
}

pub fn to_text_message<T: Serialize>(value: &T) -> Result<Message, TransportError> {
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
