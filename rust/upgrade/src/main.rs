use std::process;

use upgrade::server::run_upgrade_server_with_cli;

#[tokio::main]
async fn main() {
    if let Err(error) = run_upgrade_server_with_cli().await {
        eprintln!("{error}");
        process::exit(2);
    }
}
