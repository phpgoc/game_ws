#[cfg(feature = "official")]
#[path = "../../../../ai/landlord/src/embedded/mod.rs"]
pub mod ai;
#[cfg(not(feature = "official"))]
#[path = "ai_fallback.rs"]
mod ai;
#[cfg(all(target_os = "android", feature = "android-jni"))]
mod android_jni;
pub mod core;
pub mod game;
mod game_loop;
pub mod game_setting;
pub mod game_state;
mod official;
mod play_validator;
pub mod server;
