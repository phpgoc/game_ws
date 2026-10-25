use std::time::Duration;

use ws_common::{
    RuntimeConfig, RuntimeStats, StopSignal, run_game_server_with_cli_default_port,
    run_room_runtime_until_stopped, run_room_runtime_until_stopped_with_ready,
};

use crate::game::NiangniangHandler;

pub const NIANGNIANG_ANDROID_SERVICE_NAME: &str = "niangniang-android";
pub const NIANGNIANG_SERVICE_NAME: &str = "niangniang";
pub const NIANGNIANG_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(20);
pub const NIANGNIANG_IDLE_TIMEOUT: Duration = Duration::from_secs(120);
pub const NIANGNIANG_DEFAULT_PORT: u16 = 9012;

pub fn niangniang_runtime_config(service_name: &'static str, listen_addr: String) -> RuntimeConfig {
    RuntimeConfig {
        service_name,
        listen_addr,
        idle_timeout: NIANGNIANG_IDLE_TIMEOUT,
        heartbeat_interval: NIANGNIANG_HEARTBEAT_INTERVAL,
    }
}

pub async fn run_niangniang_runtime_until_stopped(
    listen_addr: String,
    stop_signal: StopSignal,
) -> anyhow::Result<RuntimeStats> {
    run_room_runtime_until_stopped(
        niangniang_runtime_config(NIANGNIANG_ANDROID_SERVICE_NAME, listen_addr),
        NiangniangHandler::default(),
        stop_signal,
    )
    .await
}

pub async fn run_niangniang_runtime_until_stopped_with_ready(
    listen_addr: String,
    stop_signal: StopSignal,
    ready: std::sync::mpsc::SyncSender<RuntimeStats>,
) -> anyhow::Result<RuntimeStats> {
    run_room_runtime_until_stopped_with_ready(
        niangniang_runtime_config(NIANGNIANG_ANDROID_SERVICE_NAME, listen_addr),
        NiangniangHandler::default(),
        stop_signal,
        ready,
    )
    .await
}

pub async fn run_niangniang_server_with_cli() -> anyhow::Result<()> {
    run_game_server_with_cli_default_port(
        NIANGNIANG_SERVICE_NAME,
        NIANGNIANG_DEFAULT_PORT,
        NIANGNIANG_IDLE_TIMEOUT,
        NiangniangHandler::default(),
    )
    .await
}
