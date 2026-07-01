#[macro_export]
macro_rules! dlog {
    ($level:path, $($arg:tt)+) => {{
        #[cfg(debug_assertions)]
        {
            $crate::__dlog(&format!($($arg)+), $level, file!(), line!());
        }
    }};
    ($message:expr, $level:expr $(,)?) => {{
        #[cfg(debug_assertions)]
        {
            $crate::__dlog($message, $level, file!(), line!());
        }
    }};
}

pub mod cli;
pub mod client;
pub mod game_setting;
pub mod game_state;
pub mod net;
pub mod room;
pub mod runtime;
pub mod transport;

#[cfg(debug_assertions)]
use chrono::Local;
pub use cli::{BindCli, parse_bind_cli};
pub use client::{WsClientEvent, WsClientHandle, WsClientSendError, connect_ws_client};
pub use game_setting::GameSettings;
pub use net::{resolve_host, resolve_port};
pub use room::{
    ClientRequest, Delivery, Dispatch, OutboundPayload, RequestResponse, RoomService, SessionId,
    SettingsBuilderResult,
};
pub use runtime::{
    GameHandler, RuntimeConfig, RuntimeStats, RuntimeStopHandle, SessionSenders, StopSignal,
    run_game_server, run_game_server_with_cli, run_room_runtime, run_room_runtime_until_stopped,
    runtime_stop_channel,
};
pub use share_type_public::GameParamRange;
#[cfg(debug_assertions)]
use std::io::IsTerminal;
pub use tracing;

pub use transport::{TransportError, from_message, to_text_message};

#[cfg(debug_assertions)]
#[doc(hidden)]
pub fn __dlog(message: &str, level: tracing::Level, file: &str, line: u32) {
    let level_text = if std::io::stdout().is_terminal() {
        format!("{}{}\x1b[0m", level_color(level), level)
    } else {
        level.to_string()
    };
    println!(
        "{} {} {}:{} {}",
        Local::now().format("%Y-%m-%d %H:%M:%S"),
        level_text,
        file,
        line,
        message
    );
}

#[cfg(debug_assertions)]
fn level_color(level: tracing::Level) -> &'static str {
    match level {
        tracing::Level::ERROR => "\x1b[31m",
        tracing::Level::WARN => "\x1b[33m",
        tracing::Level::INFO => "\x1b[32m",
        tracing::Level::DEBUG => "\x1b[36m",
        tracing::Level::TRACE => "\x1b[90m",
    }
}
