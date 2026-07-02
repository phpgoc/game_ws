use std::time::Duration;

use ws_common::{RuntimeConfig, run_game_server_with_cli};

use crate::game::UpgradeGameHandler;

pub const UPGRADE_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(20);
pub const UPGRADE_IDLE_TIMEOUT: Duration = Duration::from_secs(120);
pub const UPGRADE_SERVICE_NAME: &str = "upgrade";

pub fn upgrade_runtime_config(service_name: &'static str, listen_addr: String) -> RuntimeConfig {
    RuntimeConfig {
        service_name,
        listen_addr,
        idle_timeout: UPGRADE_IDLE_TIMEOUT,
        heartbeat_interval: UPGRADE_HEARTBEAT_INTERVAL,
    }
}

pub async fn run_upgrade_server_with_cli() -> anyhow::Result<()> {
    run_game_server_with_cli(
        UPGRADE_SERVICE_NAME,
        UPGRADE_IDLE_TIMEOUT,
        UpgradeGameHandler::default(),
    )
    .await
}
