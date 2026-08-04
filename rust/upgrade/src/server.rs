use std::time::Duration;

use ws_common::{
    RuntimeConfig, RuntimeStats, StopSignal, run_game_server_with_cli,
    run_room_runtime_until_stopped_with_ready,
};

use crate::game::UpgradeGameHandler;

pub const UPGRADE_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(20);
pub const UPGRADE_IDLE_TIMEOUT: Duration = Duration::from_secs(120);

/// 独立升级 WebSocket 服务使用的稳定服务名。
pub const UPGRADE_SERVICE_NAME: &str = "upgrade";

pub async fn run_upgrade_server_with_cli() -> anyhow::Result<()> {
    run_game_server_with_cli(
        UPGRADE_SERVICE_NAME,
        UPGRADE_IDLE_TIMEOUT,
        UpgradeGameHandler::default(),
    )
    .await
}

pub fn upgrade_runtime_config(service_name: &'static str, listen_addr: String) -> RuntimeConfig {
    RuntimeConfig {
        service_name,
        listen_addr,
        idle_timeout: UPGRADE_IDLE_TIMEOUT,
        heartbeat_interval: UPGRADE_HEARTBEAT_INTERVAL,
    }
}

pub async fn run_upgrade_runtime_until_stopped_with_ready(
    listen_addr: String,
    stop_signal: StopSignal,
    ready: std::sync::mpsc::SyncSender<RuntimeStats>,
) -> anyhow::Result<RuntimeStats> {
    run_room_runtime_until_stopped_with_ready(
        upgrade_runtime_config(UPGRADE_SERVICE_NAME, listen_addr),
        UpgradeGameHandler::default(),
        stop_signal,
        ready,
    )
    .await
}

#[cfg(test)]
#[path = "server/tests.rs"]
mod tests;
