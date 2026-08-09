pub mod action;
#[cfg(feature = "official")]
#[path = "../../../../ai/dominoes/src/embedded/mod.rs"]
pub(crate) mod ai;
#[cfg(not(feature = "official"))]
#[path = "ai_fallback.rs"]
pub(crate) mod ai;
pub mod core;
pub mod game;
pub mod game_loop;
pub mod game_setting;
pub mod game_state;
mod official;
pub mod server;
