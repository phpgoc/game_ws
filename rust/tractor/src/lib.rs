#[cfg(feature = "official")]
#[path = "../../../../ai/tractor/src/embedded/mod.rs"]
pub mod ai;
#[cfg(not(feature = "official"))]
#[path = "ai_fallback.rs"]
mod ai;
#[cfg(all(target_os = "android", feature = "android-jni"))]
mod android_jni;
pub mod combo;
pub mod game;
pub mod game_loop;
pub mod game_setting;
pub mod game_state;
mod official;
pub mod server;
