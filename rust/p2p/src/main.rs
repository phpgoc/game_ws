use std::process;

use p2p::server::run_p2p_server_with_cli;

#[tokio::main]
async fn main() {
    if let Err(err) = run_p2p_server_with_cli().await {
        eprintln!("{err:#}");
        process::exit(2);
    }
}
