use std::{sync::mpsc::sync_channel, time::Duration};

use ws_common::runtime_stop_channel;

use super::*;

#[test]
fn runtime_config_keeps_the_upgrade_service_contract() {
    let config = upgrade_runtime_config("test-upgrade", "127.0.0.1:9014".to_owned());

    assert_eq!(config.service_name, "test-upgrade");
    assert_eq!(config.listen_addr, "127.0.0.1:9014");
    assert_eq!(config.idle_timeout, UPGRADE_IDLE_TIMEOUT);
    assert_eq!(config.heartbeat_interval, UPGRADE_HEARTBEAT_INTERVAL);
}

#[tokio::test]
async fn ready_runtime_binds_and_honors_an_early_stop() {
    let (stop_handle, stop_signal) = runtime_stop_channel();
    stop_handle.stop();
    let (ready_tx, ready_rx) = sync_channel(1);
    let stats = run_upgrade_runtime_until_stopped_with_ready(
        "127.0.0.1:0".to_owned(),
        stop_signal,
        ready_tx,
    )
    .await
    .expect("stopped upgrade runtime");
    let ready = ready_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("runtime readiness signal");

    assert!(stats.listen_addr().ip().is_loopback());
    assert_eq!(ready.listen_addr(), stats.listen_addr());
    assert_eq!(UPGRADE_SERVICE_NAME, "upgrade");
}
