#[cfg(feature = "official")]
#[path = "../../../../ai/niangniang/src/embedded/mod.rs"]
mod ai;
#[cfg(feature = "official")]
mod ai_input;
mod core;
mod game;
mod official;
pub mod server;
pub use game::NiangniangHandler;
