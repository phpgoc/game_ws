use std::{env, time::Duration};

use tokio::net::TcpListener;

use crate::{
    config::P2pServiceConfig, runtime::run_p2p_listener, turn_server::start_embedded_turn,
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
    println!("{P2P_SERVICE_NAME} signaling server listening on ws://{listen_addr}");
    println!(
        "{P2P_SERVICE_NAME} embedded STUN/TURN listening on udp://{} (advertised IP {})",
        turn_server.listen_addr(),
        config.turn.public_ip
    );
    let result = run_p2p_listener(
        listener,
        config.ice,
        P2P_IDLE_TIMEOUT,
        P2P_HEARTBEAT_INTERVAL,
    )
    .await;
    turn_server.close().await?;
    result
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
