use std::time::Duration;

use ws_common::{
    RuntimeConfig, RuntimeStats, StopSignal, run_game_server_with_cli,
    run_room_runtime_until_stopped_with_ready,
};

use crate::game::HoldemGameHandler;

pub const HOLDEM_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(20);
pub const HOLDEM_IDLE_TIMEOUT: Duration = Duration::from_secs(120);
pub const HOLDEM_SERVICE_NAME: &str = "holdem";

pub fn holdem_runtime_config(listen_addr: String) -> RuntimeConfig {
    RuntimeConfig {
        service_name: HOLDEM_SERVICE_NAME,
        listen_addr,
        idle_timeout: HOLDEM_IDLE_TIMEOUT,
        heartbeat_interval: HOLDEM_HEARTBEAT_INTERVAL,
    }
}

pub async fn run_holdem_runtime_until_stopped_with_ready(
    listen_addr: String,
    stop_signal: StopSignal,
    ready: std::sync::mpsc::SyncSender<RuntimeStats>,
) -> anyhow::Result<RuntimeStats> {
    run_room_runtime_until_stopped_with_ready(
        holdem_runtime_config(listen_addr),
        HoldemGameHandler::default(),
        stop_signal,
        ready,
    )
    .await
}

pub async fn run_holdem_server_with_cli() -> anyhow::Result<()> {
    run_game_server_with_cli(
        HOLDEM_SERVICE_NAME,
        HOLDEM_IDLE_TIMEOUT,
        HoldemGameHandler::default(),
    )
    .await
}
