#[cfg(not(any(
    feature = "landlord",
    feature = "shenyang_mahjong",
    feature = "holdem",
    feature = "tractor",
    feature = "upgrade",
    feature = "p2p",
    feature = "dominoes",
)))]
compile_error!(
    "enable exactly one Android server feature: landlord, shenyang_mahjong, holdem, tractor, upgrade, p2p, or dominoes"
);

#[cfg(any(
    all(feature = "landlord", feature = "shenyang_mahjong"),
    all(feature = "landlord", feature = "holdem"),
    all(feature = "landlord", feature = "tractor"),
    all(feature = "landlord", feature = "upgrade"),
    all(feature = "landlord", feature = "p2p"),
    all(feature = "shenyang_mahjong", feature = "holdem"),
    all(feature = "shenyang_mahjong", feature = "tractor"),
    all(feature = "shenyang_mahjong", feature = "upgrade"),
    all(feature = "shenyang_mahjong", feature = "p2p"),
    all(feature = "holdem", feature = "tractor"),
    all(feature = "holdem", feature = "upgrade"),
    all(feature = "holdem", feature = "p2p"),
    all(feature = "tractor", feature = "upgrade"),
    all(feature = "tractor", feature = "p2p"),
    all(feature = "upgrade", feature = "p2p"),
    all(feature = "dominoes", feature = "landlord"),
    all(feature = "dominoes", feature = "shenyang_mahjong"),
    all(feature = "dominoes", feature = "holdem"),
    all(feature = "dominoes", feature = "tractor"),
    all(feature = "dominoes", feature = "upgrade"),
    all(feature = "dominoes", feature = "p2p"),
))]
compile_error!("enable only one Android server feature");

#[cfg(feature = "landlord")]
use ws_common::RuntimeStats;

#[cfg(feature = "landlord")]
ws_common::android_server_jni!(
    stats = RuntimeStats,
    run = landlord::server::run_landlord_runtime_until_stopped_with_ready,
    client_count = |stats: RuntimeStats| async move { stats.client_count().await },
    room_count = |stats: RuntimeStats| async move { stats.room_count().await },
);

#[cfg(feature = "shenyang_mahjong")]
use ws_common::RuntimeStats;

#[cfg(feature = "shenyang_mahjong")]
ws_common::android_server_jni!(
    stats = RuntimeStats,
    run = shenyang_mahjong::server::run_shenyang_mahjong_runtime_until_stopped_with_ready,
    client_count = |stats: RuntimeStats| async move { stats.client_count().await },
    room_count = |stats: RuntimeStats| async move { stats.room_count().await },
);

#[cfg(feature = "holdem")]
use ws_common::RuntimeStats;

#[cfg(feature = "holdem")]
ws_common::android_server_jni!(
    stats = RuntimeStats,
    run = holdem::server::run_holdem_runtime_until_stopped_with_ready,
    client_count = |stats: RuntimeStats| async move { stats.client_count().await },
    room_count = |stats: RuntimeStats| async move { stats.room_count().await },
);

#[cfg(feature = "tractor")]
use ws_common::RuntimeStats;

#[cfg(feature = "tractor")]
ws_common::android_server_jni!(
    stats = RuntimeStats,
    run = tractor::server::run_tractor_runtime_until_stopped_with_ready,
    client_count = |stats: RuntimeStats| async move { stats.client_count().await },
    room_count = |stats: RuntimeStats| async move { stats.room_count().await },
);

#[cfg(feature = "dominoes")]
use ws_common::RuntimeStats;

#[cfg(feature = "dominoes")]
ws_common::android_server_jni!(
    stats = RuntimeStats,
    run = dominoes::server::run_dominoes_runtime_until_stopped_with_ready,
    client_count = |stats: RuntimeStats| async move { stats.client_count().await },
    room_count = |stats: RuntimeStats| async move { stats.room_count().await },
);

#[cfg(feature = "upgrade")]
use ws_common::RuntimeStats;

#[cfg(feature = "upgrade")]
ws_common::android_server_jni!(
    stats = RuntimeStats,
    run = upgrade::server::run_upgrade_runtime_until_stopped_with_ready,
    client_count = |stats: RuntimeStats| async move { stats.client_count().await },
    room_count = |stats: RuntimeStats| async move { stats.room_count().await },
);

#[cfg(all(feature = "p2p", not(target_os = "android")))]
compile_error!("the p2p Android server bridge must be built for an Android target");

#[cfg(all(feature = "p2p", target_os = "android"))]
use p2p::runtime::P2pRuntimeStats;

#[cfg(all(feature = "p2p", target_os = "android"))]
ws_common::android_server_jni!(
    stats = P2pRuntimeStats,
    run = p2p::server::run_p2p_android_runtime_until_stopped_with_ready,
    client_count = |stats: P2pRuntimeStats| async move { stats.client_count() },
    room_count = |stats: P2pRuntimeStats| async move { stats.room_count().await },
);
