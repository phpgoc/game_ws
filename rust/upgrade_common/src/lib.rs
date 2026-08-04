//! “拖拉机”和“升级”共用的无状态规则原语。
//!
//! 这个 crate 只承载两个产品语义完全相同的部分。具体牌型、甩牌失败
//! 回退、扣底倍率和升级结算由各自的 server crate 决定。

#![forbid(unsafe_code)]

mod card;

pub use card::{Card, CardDecodeError, Rank, Suit, largest_identity_group_size};

/// 当前升级系游戏能够使用的最大牌副数。
pub const MAX_DECK_COUNT: u8 = 6;

#[cfg(test)]
mod tests;
