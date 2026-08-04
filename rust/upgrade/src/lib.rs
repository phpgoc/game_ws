//! 独立“升级”游戏的 server crate。
//!
//! 当前先固定产品边界；WebSocket 协议和运行时将在后续小提交中接入。

#![forbid(unsafe_code)]

pub mod combo;
pub mod game;
pub mod game_setting;
mod rules;
pub mod server;

pub use rules::{DeckCountError, UpgradeDeckCount};

#[cfg(test)]
mod tests;
