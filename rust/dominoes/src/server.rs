use std::time::Duration;

use ws_common::{
    RuntimeConfig, RuntimeStats, StopSignal, run_game_server_with_cli,
    run_room_runtime_until_stopped, run_room_runtime_until_stopped_with_ready,
};

use crate::game::DominoesGameHandler;

pub const DOMINOES_ANDROID_SERVICE_NAME: &str = "dominoes-android";
pub const DOMINOES_SERVICE_NAME: &str = "dominoes";
pub const DOMINOES_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(20);
pub const DOMINOES_IDLE_TIMEOUT: Duration = Duration::from_secs(120);

pub fn dominoes_runtime_config(service_name: &'static str, listen_addr: String) -> RuntimeConfig {
    RuntimeConfig {
        service_name,
        listen_addr,
        idle_timeout: DOMINOES_IDLE_TIMEOUT,
        heartbeat_interval: DOMINOES_HEARTBEAT_INTERVAL,
    }
}

pub async fn run_dominoes_runtime_until_stopped(
    listen_addr: String,
    stop_signal: StopSignal,
) -> anyhow::Result<RuntimeStats> {
    run_room_runtime_until_stopped(
        dominoes_runtime_config(DOMINOES_ANDROID_SERVICE_NAME, listen_addr),
        DominoesGameHandler::default(),
        stop_signal,
    )
    .await
}

pub async fn run_dominoes_runtime_until_stopped_with_ready(
    listen_addr: String,
    stop_signal: StopSignal,
    ready: std::sync::mpsc::SyncSender<RuntimeStats>,
) -> anyhow::Result<RuntimeStats> {
    run_room_runtime_until_stopped_with_ready(
        dominoes_runtime_config(DOMINOES_ANDROID_SERVICE_NAME, listen_addr),
        DominoesGameHandler::default(),
        stop_signal,
        ready,
    )
    .await
}

pub async fn run_dominoes_server_with_cli() -> anyhow::Result<()> {
    run_game_server_with_cli(
        DOMINOES_SERVICE_NAME,
        DOMINOES_IDLE_TIMEOUT,
        DominoesGameHandler::default(),
    )
    .await
}
