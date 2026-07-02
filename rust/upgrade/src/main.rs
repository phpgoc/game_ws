use std::process;

use upgrade::server::run_upgrade_server_with_cli;

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("{err}");
        process::exit(2);
    }
}

async fn run() -> anyhow::Result<()> {
    run_upgrade_server_with_cli().await
}
