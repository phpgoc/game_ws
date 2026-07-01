#[cfg(target_os = "android")]
mod android_jni;
pub mod core;
pub mod game;
mod game_loop;
pub mod game_setting;
pub mod game_state;
mod play_validator;
pub mod server;
