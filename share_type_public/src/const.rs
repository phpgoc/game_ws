use serde_repr::{Deserialize_repr, Serialize_repr};
use typeshare::typeshare;

#[typeshare]
#[repr(i32)]
#[derive(Debug, Clone, Copy, Serialize_repr, Deserialize_repr)]
#[allow(non_camel_case_types)]
pub enum Routes {
    CREATE = 1,
    JOIN = 2,
    QUIT = 3,
    MESSAGE = 4,
    PAUSE = 5,
    RESUME = 6,
    DISBAND = 7,
    SETTING = 8,
    START = 10,
    AWAY = 12,
    BACK = 13,
    SWAP = 14,

    DEAL = 20,
    PLAY = 21,

    SHOW_HIDDEN_CARDS = 30,
}

#[typeshare]
#[repr(i32)]
#[derive(Debug, Clone, Copy, Serialize_repr, Deserialize_repr)]
#[allow(non_camel_case_types)]
pub enum WsCode {
    JOIN = 2,
    QUIT = 3,
    MESSAGE = 4,
    PAUSE = 5,
    RESUME = 6,
    DISBAND = 7,
    SETTING = 8,
    START = 10,
    GAME_OVER = 11,
    AWAY = 12,
    BACK = 13,
    SWAP = 14,

    DEAL = 20,
    PLAY = 21,
    DEAL_OPEN_CARDS = 24,
    DEAL_FACE_DOWN_CARDS = 25,
    CHANGE_ROUND = 26,

    SHOW_HIDDEN_CARDS = 30,

    TEST_PULSE = 999,
}

#[typeshare]
#[repr(i32)]
#[derive(Debug, Clone, Copy, Serialize_repr, Deserialize_repr)]
#[allow(non_camel_case_types)]
pub enum WsResponseCode{
    OK = 0,
    JOINED = 201,
    ERROR_FORMAT = 400,
    NOT_LOGIN = 401,
    NO_PERMISSION = 403,
    NOT_IN_RANGE = 410,
}
