use std::time::Duration;

use ws_common::{
    RuntimeConfig, RuntimeStats, StopSignal, run_game_server_with_cli_default_port,
    run_room_runtime_until_stopped, run_room_runtime_until_stopped_with_ready,
};

use crate::game::WhoIsLandlordHandler;

pub const WHO_IS_LANDLORD_ANDROID_SERVICE_NAME: &str = "who_is_landlord-android";
pub const WHO_IS_LANDLORD_SERVICE_NAME: &str = "who_is_landlord";
pub const WHO_IS_LANDLORD_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(20);
pub const WHO_IS_LANDLORD_IDLE_TIMEOUT: Duration = Duration::from_secs(120);
pub const WHO_IS_LANDLORD_DEFAULT_PORT: u16 = 9011;

pub fn who_is_landlord_runtime_config(
    service_name: &'static str,
    listen_addr: String,
) -> RuntimeConfig {
    RuntimeConfig {
        service_name,
        listen_addr,
        idle_timeout: WHO_IS_LANDLORD_IDLE_TIMEOUT,
        heartbeat_interval: WHO_IS_LANDLORD_HEARTBEAT_INTERVAL,
    }
}

pub async fn run_who_is_landlord_runtime_until_stopped(
    listen_addr: String,
    stop_signal: StopSignal,
) -> anyhow::Result<RuntimeStats> {
    run_room_runtime_until_stopped(
        who_is_landlord_runtime_config(WHO_IS_LANDLORD_ANDROID_SERVICE_NAME, listen_addr),
        WhoIsLandlordHandler::default(),
        stop_signal,
    )
    .await
}

pub async fn run_who_is_landlord_runtime_until_stopped_with_ready(
    listen_addr: String,
    stop_signal: StopSignal,
    ready: std::sync::mpsc::SyncSender<RuntimeStats>,
) -> anyhow::Result<RuntimeStats> {
    run_room_runtime_until_stopped_with_ready(
        who_is_landlord_runtime_config(WHO_IS_LANDLORD_ANDROID_SERVICE_NAME, listen_addr),
        WhoIsLandlordHandler::default(),
        stop_signal,
        ready,
    )
    .await
}

pub async fn run_who_is_landlord_server_with_cli() -> anyhow::Result<()> {
    run_game_server_with_cli_default_port(
        WHO_IS_LANDLORD_SERVICE_NAME,
        WHO_IS_LANDLORD_DEFAULT_PORT,
        WHO_IS_LANDLORD_IDLE_TIMEOUT,
        WhoIsLandlordHandler::default(),
    )
    .await
}
