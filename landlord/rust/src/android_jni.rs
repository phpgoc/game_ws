use std::{
    os::raw::{c_int, c_uchar},
    sync::{Mutex, OnceLock},
    thread::{self, JoinHandle},
    time::Duration,
};

use ws_common::{
    RuntimeConfig, RuntimeStats, RuntimeStopHandle, run_room_runtime_until_stopped,
    runtime_stop_channel,
};

use crate::game::LandlordGameHandler;

static SERVER: OnceLock<Mutex<Option<AndroidServer>>> = OnceLock::new();

struct AndroidServer {
    stop: RuntimeStopHandle,
    join: Option<JoinHandle<()>>,
    stats: RuntimeStats,
}

fn block_on_count<F>(future: F) -> c_int
where
    F: std::future::Future<Output = usize>,
{
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map(|runtime| runtime.block_on(future) as c_int)
        .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn java_com_example_landlord_server_rust_landlord_server_native_client_count(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
) -> c_int {
    let stats = server_slot()
        .lock()
        .ok()
        .and_then(|guard| guard.as_ref().map(|server| server.stats.clone()));
    stats
        .map(|stats| block_on_count(async move { stats.client_count().await }))
        .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn java_com_example_landlord_server_rust_landlord_server_native_room_count(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
) -> c_int {
    let stats = server_slot()
        .lock()
        .ok()
        .and_then(|guard| guard.as_ref().map(|server| server.stats.clone()));
    stats
        .map(|stats| block_on_count(async move { stats.room_count().await }))
        .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn java_com_example_landlord_server_rust_landlord_server_native_stop(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
) {
    let server = server_slot().lock().ok().and_then(|mut guard| guard.take());
    if let Some(mut server) = server {
        server.stop.stop();
        if let Some(join) = server.join.take() {
            let _ = join.join();
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn java_com_example_landlordserver_rust_landlord_server_native_start(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    port: c_int,
) -> c_uchar {
    let slot = server_slot();
    let mut guard = match slot.lock() {
        Ok(guard) => guard,
        Err(_) => return 0,
    };
    if guard.is_some() {
        return 1;
    }

    let (stop, stop_signal) = runtime_stop_channel();
    let (stats_tx, stats_rx) = std::sync::mpsc::sync_channel(1);
    let listen_addr = format!("0.0.0.0:{}", port.max(0));
    let join = thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(_) => return,
        };
        runtime.block_on(async move {
            let result = run_room_runtime_until_stopped(
                RuntimeConfig {
                    service_name: "landlord-android",
                    listen_addr,
                    idle_timeout: Duration::from_secs(120),
                    heartbeat_interval: Duration::from_secs(20),
                },
                LandlordGameHandler::default(),
                stop_signal,
            )
            .await;
            if let Ok(stats) = result {
                let _ = stats_tx.send(stats);
            }
        });
    });

    let stats = match stats_rx.recv_timeout(Duration::from_secs(3)) {
        Ok(stats) => stats,
        Err(_) => {
            let _ = join.join();
            return 0;
        }
    };

    *guard = Some(AndroidServer {
        stop,
        join: Some(join),
        stats,
    });
    1
}

fn server_slot() -> &'static Mutex<Option<AndroidServer>> {
    SERVER.get_or_init(|| Mutex::new(None))
}
