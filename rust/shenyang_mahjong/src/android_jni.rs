use ws_common::RuntimeStats;

ws_common::android_server_jni!(
    stats = RuntimeStats,
    run = crate::server::run_shenyang_mahjong_runtime_until_stopped_with_ready,
    client_count = |stats: RuntimeStats| async move { stats.client_count().await },
    room_count = |stats: RuntimeStats| async move { stats.room_count().await },
);
