#[cfg(feature = "official")]
#[path = "../../../../ai/who_is_landlord/src/embedded/mod.rs"]
mod ai;
mod core;
mod game;
mod official;
pub mod server;
pub use game::WhoIsLandlordHandler;
