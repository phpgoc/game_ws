use serde::{Deserialize, Serialize};
use typeshare::typeshare;

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommonResponse<T> {
    pub code: T,
    pub message: String,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeResponse {
    pub code: i32,
}

#[cfg(test)]
mod tests {
    #[test]
    fn common_response_works() {
        let item = super::CommonResponse::<i32> {
            code: 200,
            message: "ok".to_string(),
        };
        assert_eq!(item.code, 200);
        assert_eq!(item.message, "ok");
    }

    #[test]
    fn code_response_works() {
        let item = super::CodeResponse { code: 40001 };
        assert_eq!(item.code, 40001);
    }
}
