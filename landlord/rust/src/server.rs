use std::time::Duration;

use ws_common::{
    RuntimeConfig, RuntimeStats, StopSignal, run_game_server_with_cli,
    run_room_runtime_until_stopped, run_room_runtime_until_stopped_with_ready,
};

use crate::game::LandlordGameHandler;

pub const LANDLORD_SERVICE_NAME: &str = "landlord";
pub const LANDLORD_ANDROID_SERVICE_NAME: &str = "landlord-android";
pub const LANDLORD_IDLE_TIMEOUT: Duration = Duration::from_secs(120);
pub const LANDLORD_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(20);

pub fn landlord_runtime_config(service_name: &'static str, listen_addr: String) -> RuntimeConfig {
    RuntimeConfig {
        service_name,
        listen_addr,
        idle_timeout: LANDLORD_IDLE_TIMEOUT,
        heartbeat_interval: LANDLORD_HEARTBEAT_INTERVAL,
    }
}

pub async fn run_landlord_server_with_cli() -> anyhow::Result<()> {
    run_game_server_with_cli(
        LANDLORD_SERVICE_NAME,
        LANDLORD_IDLE_TIMEOUT,
        LandlordGameHandler::default(),
    )
    .await
}

pub async fn run_landlord_runtime_until_stopped(
    listen_addr: String,
    stop_signal: StopSignal,
) -> anyhow::Result<RuntimeStats> {
    run_room_runtime_until_stopped(
        landlord_runtime_config(LANDLORD_ANDROID_SERVICE_NAME, listen_addr),
        LandlordGameHandler::default(),
        stop_signal,
    )
    .await
}

pub async fn run_landlord_runtime_until_stopped_with_ready(
    listen_addr: String,
    stop_signal: StopSignal,
    ready: std::sync::mpsc::SyncSender<RuntimeStats>,
) -> anyhow::Result<RuntimeStats> {
    run_room_runtime_until_stopped_with_ready(
        landlord_runtime_config(LANDLORD_ANDROID_SERVICE_NAME, listen_addr),
        LandlordGameHandler::default(),
        stop_signal,
        ready,
    )
    .await
}
