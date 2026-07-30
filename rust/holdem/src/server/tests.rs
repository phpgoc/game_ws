use std::{sync::mpsc::sync_channel, time::Duration};

use ws_common::runtime_stop_channel;

use super::{
    HOLDEM_HEARTBEAT_INTERVAL, HOLDEM_IDLE_TIMEOUT, HOLDEM_SERVICE_NAME, holdem_runtime_config,
    run_holdem_runtime_until_stopped_with_ready,
};

#[test]
fn runtime_config_keeps_the_holdem_service_contract() {
    let config = holdem_runtime_config("127.0.0.1:9011".to_owned());

    assert_eq!(config.service_name, HOLDEM_SERVICE_NAME);
    assert_eq!(config.listen_addr, "127.0.0.1:9011");
    assert_eq!(config.idle_timeout, HOLDEM_IDLE_TIMEOUT);
    assert_eq!(config.heartbeat_interval, HOLDEM_HEARTBEAT_INTERVAL);
}

#[tokio::test]
async fn ready_runtime_binds_and_honors_an_early_stop() {
    let (stop_handle, stop_signal) = runtime_stop_channel();
    stop_handle.stop();
    let (ready_tx, ready_rx) = sync_channel(1);
    let stats = run_holdem_runtime_until_stopped_with_ready(
        "127.0.0.1:0".to_owned(),
        stop_signal,
        ready_tx,
    )
    .await
    .expect("stopped holdem runtime");
    let ready = ready_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("runtime readiness signal");
    assert!(stats.listen_addr().ip().is_loopback());
    assert_eq!(ready.listen_addr(), stats.listen_addr());
}
