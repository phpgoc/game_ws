use serde::{Deserialize, Serialize};
use typeshare::typeshare;

use crate::common::CommonMessage;

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WsMessage {
    pub common: CommonMessage,
    pub topic: String,
}
