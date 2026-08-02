#[macro_export]
macro_rules! dlog {
    ($level:path, $($arg:tt)+) => {{
        $crate::tracing::event!(
            target: module_path!(),
            $level,
            source_file = file!(),
            source_line = line!(),
            message = %format_args!($($arg)+),
        );
    }};
    ($message:expr, $level:expr $(,)?) => {{
        $crate::tracing::event!(
            target: module_path!(),
            $level,
            source_file = file!(),
            source_line = line!(),
            message = %$message,
        );
    }};
}

mod android;
mod cli;
mod client;
mod game_setting;
mod game_state;
mod net;
mod official;
mod room;
mod runtime;
mod transport;

pub use client::{WsClientEvent, WsClientHandle, WsClientSendError, connect_ws_client};
pub use game_setting::GameSettings;
pub use game_state::{CommonGameState, GameState, SharedGameState};
pub use official::OfficialPlayerSession;
pub use room::{
    ClientRequest, Delivery, Dispatch, OutboundPayload, RequestResponse, RoomService, SessionId,
    SettingsBuilderResult,
};
pub use runtime::{
    GameHandler, JoinAuthorization, JoinAuthorizationFuture, RuntimeConfig, RuntimeStats,
    RuntimeStopHandle, SessionSender, SessionSenders, StopSignal, run_game_server_with_cli,
    run_room_runtime, run_room_runtime_until_stopped, run_room_runtime_until_stopped_with_ready,
    runtime_stop_channel, session_sender_channel,
};
pub use tracing;

pub use transport::{TransportError, from_message, to_text_message};

#[cfg(test)]
mod tests;
