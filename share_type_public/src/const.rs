use serde_repr::{Deserialize_repr, Serialize_repr};
use typeshare::typeshare;

#[typeshare]
#[repr(i32)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[allow(non_camel_case_types)]
pub enum GameId {
    #[default]
    ALL = 0,
    LANDLORD = 1,
    SHENYANG_MAHJONG = 2,
    TEXAS_HOLD_EM = 3,
    TRACTOR = 4,
    OPEN_HOLD_EM = 5,
    SHORT_DECK_HOLD_EM = 6,
    OMAHA_HOLD_EM = 7,
}

impl From<GameId> for i32 {
    fn from(value: GameId) -> Self {
        value as i32
    }
}

impl TryFrom<i32> for GameId {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::ALL),
            1 => Ok(Self::LANDLORD),
            2 => Ok(Self::SHENYANG_MAHJONG),
            3 => Ok(Self::TEXAS_HOLD_EM),
            4 => Ok(Self::TRACTOR),
            5 => Ok(Self::OPEN_HOLD_EM),
            6 => Ok(Self::SHORT_DECK_HOLD_EM),
            7 => Ok(Self::OMAHA_HOLD_EM),
            _ => Err(()),
        }
    }
}

#[typeshare]
#[repr(i32)]
#[derive(Debug, Clone, Copy, Serialize_repr, Deserialize_repr)]
#[allow(non_camel_case_types)]
pub enum Routes {
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
    ADD_AI = 15,
    AUTO_STRATEGY = 16,

    DEAL = 20,
    PLAY = 21,

    SHOW_HIDDEN_CARDS = 30,

    CALL_LANDLORD = 1001,
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
    CHANGE_DEAL = 26,
    CHANGE_PHASE = 27,
    TABLE_SNAPSHOT = 28,

    SHOW_HIDDEN_CARDS = 30,

    CALL_LANDLORD = 1001,
    CLAIM_WINDOW = 2001,
}

#[typeshare]
#[repr(i32)]
#[derive(Debug, Clone, Copy, Serialize_repr, Deserialize_repr)]
#[allow(non_camel_case_types)]
pub enum WsResponseCode {
    OK = 0,
    JOINED = 201,
    ERROR_FORMAT = 400,
    NOT_LOGIN = 401,
    WRONG_GAME = 402,
    NO_PERMISSION = 403,
    NOT_IN_RANGE = 410,
}
