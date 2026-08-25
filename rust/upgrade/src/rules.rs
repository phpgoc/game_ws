//! 升级牌副数设置的边界校验。

use std::fmt;

use upgrade_common::MAX_DECK_COUNT;

pub const MIN_UPGRADE_DECK_COUNT: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpgradeDeckCount(u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeckCountError(pub u8);

impl fmt::Display for DeckCountError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "upgrade requires {MIN_UPGRADE_DECK_COUNT}..={MAX_DECK_COUNT} decks, got {}",
            self.0
        )
    }
}

impl std::error::Error for DeckCountError {}

impl UpgradeDeckCount {
    pub fn new(count: u8) -> Result<Self, DeckCountError> {
        // 不把越界值截断，调用方可以把错误原样转换成设置响应。
        if (MIN_UPGRADE_DECK_COUNT..=MAX_DECK_COUNT).contains(&count) {
            Ok(Self(count))
        } else {
            Err(DeckCountError(count))
        }
    }

    pub const fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for UpgradeDeckCount {
    type Error = DeckCountError;

    fn try_from(count: u8) -> Result<Self, Self::Error> {
        Self::new(count)
    }
}

impl From<UpgradeDeckCount> for u8 {
    fn from(count: UpgradeDeckCount) -> Self {
        count.get()
    }
}
