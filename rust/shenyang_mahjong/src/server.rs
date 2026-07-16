use std::time::Duration;

use ws_common::{
    RuntimeConfig, RuntimeStats, StopSignal, run_game_server_with_cli,
    run_room_runtime_until_stopped_with_ready,
};

use crate::game::ShenyangMahjongGameHandler;

pub const SHENYANG_MAHJONG_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(20);
pub const SHENYANG_MAHJONG_IDLE_TIMEOUT: Duration = Duration::from_secs(120);
pub const SHENYANG_MAHJONG_SERVICE_NAME: &str = "shenyang_mahjong";

pub fn shenyang_mahjong_runtime_config(
    service_name: &'static str,
    listen_addr: String,
) -> RuntimeConfig {
    RuntimeConfig {
        service_name,
        listen_addr,
        idle_timeout: SHENYANG_MAHJONG_IDLE_TIMEOUT,
        heartbeat_interval: SHENYANG_MAHJONG_HEARTBEAT_INTERVAL,
    }
}

pub async fn run_shenyang_mahjong_runtime_until_stopped_with_ready(
    listen_addr: String,
    stop_signal: StopSignal,
    ready: std::sync::mpsc::SyncSender<RuntimeStats>,
) -> anyhow::Result<RuntimeStats> {
    run_room_runtime_until_stopped_with_ready(
        shenyang_mahjong_runtime_config(SHENYANG_MAHJONG_SERVICE_NAME, listen_addr),
        ShenyangMahjongGameHandler::default(),
        stop_signal,
        ready,
    )
    .await
}

pub async fn run_shenyang_mahjong_server_with_cli() -> anyhow::Result<()> {
    run_game_server_with_cli(
        SHENYANG_MAHJONG_SERVICE_NAME,
        SHENYANG_MAHJONG_IDLE_TIMEOUT,
        ShenyangMahjongGameHandler::default(),
    )
    .await
}
