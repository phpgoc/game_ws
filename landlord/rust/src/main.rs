use std::{process, time::Duration};

use landlord_ws_server::game::LandlordGameHandler;
use ws_common::run_game_server_with_cli;

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("{err}");
        process::exit(2);
    }
}

async fn run() -> anyhow::Result<()> {
    run_game_server_with_cli(
        "landlord",
        Duration::from_secs(120),
        LandlordGameHandler::default(),
    )
    .await
}
