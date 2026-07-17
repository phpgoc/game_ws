use crate::runtime::P2pRuntimeStats;

ws_common::android_server_jni!(
    stats = P2pRuntimeStats,
    run = crate::server::run_p2p_android_runtime_until_stopped_with_ready,
    client_count = |stats: P2pRuntimeStats| async move { stats.client_count() },
    room_count = |stats: P2pRuntimeStats| async move { stats.room_count().await },
);
