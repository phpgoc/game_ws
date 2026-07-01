use std::process;

use landlord::server::run_landlord_server_with_cli;

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("{err}");
        process::exit(2);
    }
}

async fn run() -> anyhow::Result<()> {
    run_landlord_server_with_cli().await
}
