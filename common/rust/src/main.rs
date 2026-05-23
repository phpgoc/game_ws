use std::{
    net::{Ipv4Addr, SocketAddrV4, TcpListener},
    process,
    time::Duration,
};

use clap::Parser;
use ws_common::{ServerConfig, run_ws_server};

#[derive(Debug, Parser)]
#[command(name = "ws-common-server")]
#[command(about = "Common WS bootstrap for game servers")]
struct Cli {
    #[arg(long)]
    host: Option<String>,
    #[arg(long)]
    port: Option<u16>,
}

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("{err}");
        process::exit(2);
    }
}

async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let host = resolve_host(cli.host)?;
    let port = resolve_port(host, cli.port)?;
    let listen_addr = format!("{host}:{port}");

    run_ws_server::<
        landlord_ws_server::protocol::ClientEvent,
        landlord_ws_server::protocol::ServerEvent,
        _,
        _,
    >(
        ServerConfig {
            service_name: "landlord",
            listen_addr,
            idle_timeout: Duration::from_secs(45),
        },
        landlord_ws_server::game::handle_event,
    )
    .await
}

fn resolve_host(host: Option<String>) -> anyhow::Result<Ipv4Addr> {
    match host {
        Some(host) => host
            .parse::<Ipv4Addr>()
            .map_err(|_| anyhow::anyhow!("invalid host: {host}")),
        None => select_private_ipv4().ok_or_else(|| anyhow::anyhow!("no private ipv4 found")),
    }
}

fn resolve_port(host: Ipv4Addr, port: Option<u16>) -> anyhow::Result<u16> {
    match port {
        Some(port) => {
            if port <= 9000 {
                return Err(anyhow::anyhow!("port must be > 9000, got {port}"));
            }
            if !is_bindable(host, port) {
                return Err(anyhow::anyhow!("port is not bindable: {host}:{port}"));
            }
            Ok(port)
        }
        None => find_bindable_port(host)
            .ok_or_else(|| anyhow::anyhow!("no bindable port found above 9000 for host {host}")),
    }
}

fn select_private_ipv4() -> Option<Ipv4Addr> {
    let mut candidates = if_addrs::get_if_addrs().ok()?;
    candidates.sort_by(|a, b| a.name.cmp(&b.name));

    candidates.into_iter().find_map(|iface| match iface.ip() {
        std::net::IpAddr::V4(v4) if is_private_range(v4) => Some(v4),
        _ => None,
    })
}

fn is_private_range(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    octets[0] == 10
        || (octets[0] == 172 && (16..=31).contains(&octets[1]))
        || (octets[0] == 192 && octets[1] == 168)
}

fn find_bindable_port(host: Ipv4Addr) -> Option<u16> {
    (9001..=u16::MAX).find(|port| is_bindable(host, *port))
}

fn is_bindable(host: Ipv4Addr, port: u16) -> bool {
    let addr = SocketAddrV4::new(host, port);
    TcpListener::bind(addr).is_ok()
}
