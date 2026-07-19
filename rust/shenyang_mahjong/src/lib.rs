mod ai;
#[cfg(all(target_os = "android", feature = "android-jni"))]
mod android_jni;
pub mod game;
mod game_loop;
pub mod game_setting;
pub mod game_state;
mod official;
mod rules;
pub mod server;
