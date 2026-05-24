use serde::{Deserialize, Serialize};
use typeshare::typeshare;

use crate::r#const::WsCode;


#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommonEvent<T> {
    pub code: WsCode,
    pub data: T,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommonWithoutDataEvent {
    pub code: WsCode,
}



#[cfg(test)]
mod tests {
    #[test]
    fn common_event_uses_ws_code() {
        let item = super::CommonEvent::<i32> {
            code: super::WsCode::JOIN,
            data: 200,
        };
        assert_eq!(item.code as i32, super::WsCode::JOIN as i32);
        assert_eq!(item.data, 200);
    }
}
