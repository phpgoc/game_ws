use std::time::Duration;

use ws_common::{RuntimeConfig, run_game_server_with_cli};

use crate::game::TractorGameHandler;

pub const TRACTOR_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(20);
pub const TRACTOR_IDLE_TIMEOUT: Duration = Duration::from_secs(120);
pub const TRACTOR_SERVICE_NAME: &str = "tractor";

pub async fn run_tractor_server_with_cli() -> anyhow::Result<()> {
    run_game_server_with_cli(
        TRACTOR_SERVICE_NAME,
        TRACTOR_IDLE_TIMEOUT,
        TractorGameHandler::default(),
    )
    .await
}

pub fn tractor_runtime_config(service_name: &'static str, listen_addr: String) -> RuntimeConfig {
    RuntimeConfig {
        service_name,
        listen_addr,
        idle_timeout: TRACTOR_IDLE_TIMEOUT,
        heartbeat_interval: TRACTOR_HEARTBEAT_INTERVAL,
    }
}
