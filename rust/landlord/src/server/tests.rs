use std::{sync::mpsc::sync_channel, time::Duration};

use ws_common::runtime_stop_channel;

use super::{
    LANDLORD_ANDROID_SERVICE_NAME, LANDLORD_HEARTBEAT_INTERVAL, LANDLORD_IDLE_TIMEOUT,
    landlord_runtime_config, run_landlord_runtime_until_stopped,
    run_landlord_runtime_until_stopped_with_ready,
};

#[test]
fn runtime_config_keeps_the_landlord_service_contract() {
    let config = landlord_runtime_config("custom-landlord", "127.0.0.1:9010".to_owned());

    assert_eq!(config.service_name, "custom-landlord");
    assert_eq!(config.listen_addr, "127.0.0.1:9010");
    assert_eq!(config.idle_timeout, LANDLORD_IDLE_TIMEOUT);
    assert_eq!(config.heartbeat_interval, LANDLORD_HEARTBEAT_INTERVAL);
}

#[tokio::test]
async fn runtime_helpers_bind_report_ready_and_honor_an_early_stop() {
    let (stop_handle, stop_signal) = runtime_stop_channel();
    stop_handle.stop();
    let stats = run_landlord_runtime_until_stopped("127.0.0.1:0".to_owned(), stop_signal)
        .await
        .expect("stopped landlord runtime");
    assert!(stats.listen_addr().ip().is_loopback());

    let (stop_handle, stop_signal) = runtime_stop_channel();
    stop_handle.stop();
    let (ready_tx, ready_rx) = sync_channel(1);
    let stats = run_landlord_runtime_until_stopped_with_ready(
        "127.0.0.1:0".to_owned(),
        stop_signal,
        ready_tx,
    )
    .await
    .expect("ready landlord runtime");
    let ready = ready_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("runtime readiness signal");
    assert_eq!(ready.listen_addr(), stats.listen_addr());
    assert_eq!(LANDLORD_ANDROID_SERVICE_NAME, "landlord-android");
}
