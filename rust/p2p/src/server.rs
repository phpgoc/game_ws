use std::{env, sync::mpsc::SyncSender, time::Duration};

use tokio::net::TcpListener;
use tokio::sync::watch;
#[cfg(target_os = "android")]
use ws_common::StopSignal;

use crate::{
    config::P2pServiceConfig,
    runtime::{P2pRuntimeStats, run_p2p_listener, run_p2p_listener_until_stopped},
    turn_server::start_embedded_turn,
};

pub const P2P_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(20);
pub const P2P_IDLE_TIMEOUT: Duration = Duration::from_secs(180);
pub const P2P_SERVICE_NAME: &str = "p2p";
pub const P2P_DEFAULT_PORT: u16 = 9005;

pub async fn run_p2p_server_with_cli() -> anyhow::Result<()> {
    let config = P2pServiceConfig::from_env()?;
    let listen_addr = parse_listen_addr()?;
    run_p2p_server(&listen_addr, config).await
}

/// Run the game-agnostic signaling server and its embedded UDP STUN/TURN
/// service. Android/Kotlin wrappers can construct [`P2pServiceConfig`] and
/// drive this function from the same Rust runtime without starting another
/// native process.
pub async fn run_p2p_server(listen_addr: &str, config: P2pServiceConfig) -> anyhow::Result<()> {
    let listener = TcpListener::bind(&listen_addr).await?;
    let turn_server = start_embedded_turn(&config.turn).await?;
    let ice = config.ice_for_bound_turn(turn_server.listen_addr())?;
    println!(
        "{P2P_SERVICE_NAME} signaling server listening on ws://{}",
        listener.local_addr()?
    );
    println!(
        "{P2P_SERVICE_NAME} embedded STUN/TURN listening on udp://{} (advertised IP {})",
        turn_server.listen_addr(),
        config.turn.public_ip
    );
    let result = run_p2p_listener(listener, ice, P2P_IDLE_TIMEOUT, P2P_HEARTBEAT_INTERVAL).await;
    turn_server.close().await?;
    result
}

pub async fn run_p2p_server_on_listener_until_stopped(
    listener: TcpListener,
    config: P2pServiceConfig,
    stop_signal: watch::Receiver<bool>,
    ready: SyncSender<P2pRuntimeStats>,
) -> anyhow::Result<P2pRuntimeStats> {
    let listen_addr = listener.local_addr()?;
    let turn_server = start_embedded_turn(&config.turn).await?;
    let turn_addr = turn_server.listen_addr();
    let ice = config.ice_for_bound_turn(turn_addr)?;
    println!("{P2P_SERVICE_NAME} signaling server listening on ws://{listen_addr}");
    println!(
        "{P2P_SERVICE_NAME} embedded STUN/TURN listening on udp://{turn_addr} (advertised IP {})",
        config.turn.public_ip
    );

    let result = run_p2p_listener_until_stopped(
        listener,
        ice,
        P2P_IDLE_TIMEOUT,
        P2P_HEARTBEAT_INTERVAL,
        stop_signal,
        Some(ready),
    )
    .await;
    let close_result = turn_server.close().await;
    match result {
        Ok(stats) => {
            close_result?;
            Ok(stats)
        }
        Err(error) => {
            let _ = close_result;
            Err(error)
        }
    }
}

#[cfg(target_os = "android")]
pub async fn run_p2p_android_runtime_until_stopped_with_ready(
    listen_addr: String,
    stop_signal: StopSignal,
    ready: SyncSender<P2pRuntimeStats>,
) -> anyhow::Result<P2pRuntimeStats> {
    let listener = TcpListener::bind(listen_addr).await?;
    let secret = format!(
        "lan-game-android-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos(),
    );
    let config = P2pServiceConfig::for_lan_embedded(0, secret)?;
    run_p2p_server_on_listener_until_stopped(listener, config, stop_signal.into_receiver(), ready)
        .await
}

fn parse_listen_addr() -> anyhow::Result<String> {
    let mut host = "0.0.0.0".to_owned();
    let mut port = P2P_DEFAULT_PORT;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--host" => {
                host = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--host requires a value"))?
            }
            "--port" => {
                port = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--port requires a value"))?
                    .parse::<u16>()?;
            }
            _ => return Err(anyhow::anyhow!("unknown argument: {arg}")),
        }
    }
    Ok(format!("{host}:{port}"))
}
