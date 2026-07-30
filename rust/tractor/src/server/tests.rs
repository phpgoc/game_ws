use std::{sync::mpsc::sync_channel, time::Duration};

use ws_common::runtime_stop_channel;

use super::{
    TRACTOR_HEARTBEAT_INTERVAL, TRACTOR_IDLE_TIMEOUT, TRACTOR_SERVICE_NAME,
    run_tractor_runtime_until_stopped_with_ready, tractor_runtime_config,
};

#[test]
fn runtime_config_keeps_the_tractor_service_contract() {
    let config = tractor_runtime_config("test-tractor", "127.0.0.1:9013".to_owned());

    assert_eq!(config.service_name, "test-tractor");
    assert_eq!(config.listen_addr, "127.0.0.1:9013");
    assert_eq!(config.idle_timeout, TRACTOR_IDLE_TIMEOUT);
    assert_eq!(config.heartbeat_interval, TRACTOR_HEARTBEAT_INTERVAL);
}

#[tokio::test]
async fn ready_runtime_binds_and_honors_an_early_stop() {
    let (stop_handle, stop_signal) = runtime_stop_channel();
    stop_handle.stop();
    let (ready_tx, ready_rx) = sync_channel(1);
    let stats = run_tractor_runtime_until_stopped_with_ready(
        "127.0.0.1:0".to_owned(),
        stop_signal,
        ready_tx,
    )
    .await
    .expect("stopped tractor runtime");
    let ready = ready_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("runtime readiness signal");
    assert!(stats.listen_addr().ip().is_loopback());
    assert_eq!(ready.listen_addr(), stats.listen_addr());
    assert_eq!(TRACTOR_SERVICE_NAME, "tractor");
}
