use serde::{Deserialize, Serialize};
use typeshare::typeshare;

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommonMessage {
    pub code: i32,
    pub message: String,
}

#[cfg(test)]
mod tests {
    #[test]
    fn common_message_works() {
        let item = super::CommonMessage {
            code: 200,
            message: "ok".to_string(),
        };
        assert_eq!(item.code, 200);
        assert_eq!(item.message, "ok");
    }
}
