use std::{
    ffi::c_void,
    os::raw::{c_int, c_uchar},
    sync::{Mutex, OnceLock},
    thread::{self, JoinHandle},
    time::Duration,
};

use ws_common::{RuntimeStats, RuntimeStopHandle, runtime_stop_channel};

use crate::server::run_landlord_runtime_until_stopped_with_ready;

static SERVER: OnceLock<Mutex<Option<AndroidServer>>> = OnceLock::new();

struct AndroidServer {
    stop: RuntimeStopHandle,
    join: Option<JoinHandle<()>>,
    stats: RuntimeStats,
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_landlordserver_rust_LandlordNativeServer_nativeClientCount(
    _env: *mut c_void,
    _class: *mut c_void,
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
pub extern "system" fn Java_com_example_landlordserver_rust_LandlordNativeServer_nativeRoomCount(
    _env: *mut c_void,
    _class: *mut c_void,
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
pub extern "system" fn Java_com_example_landlordserver_rust_LandlordNativeServer_nativeStart(
    _env: *mut c_void,
    _class: *mut c_void,
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
            let _ =
                run_landlord_runtime_until_stopped_with_ready(listen_addr, stop_signal, stats_tx)
                    .await;
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

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_example_landlordserver_rust_LandlordNativeServer_nativeStop(
    _env: *mut c_void,
    _class: *mut c_void,
) {
    let server = server_slot().lock().ok().and_then(|mut guard| guard.take());
    if let Some(mut server) = server {
        server.stop.stop();
        if let Some(join) = server.join.take() {
            let _ = join.join();
        }
    }
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

fn server_slot() -> &'static Mutex<Option<AndroidServer>> {
    SERVER.get_or_init(|| Mutex::new(None))
}
