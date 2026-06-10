use serde::{Deserialize, Serialize};
use typeshare::typeshare;

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommonEvent<T> {
    pub code: i32,
    pub data: T,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommonWithoutDataEvent {
    pub code: i32,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommonResponse<T> {
    pub code: i32,
    pub data: T,
}

#[cfg(test)]
mod tests {
    use crate::r#const::WsCode;
    #[test]
    fn common_event_uses_ws_code() {
        let item = super::CommonEvent::<i32> {
            code: WsCode::JOIN as i32,
            data: 200,
        };
        assert_eq!(item.code, WsCode::JOIN as i32);
        assert_eq!(item.data, 200);
    }
}
