use std::{process, time::Duration};

use shenyang_mahjong::game::ShenyangMahjongGameHandler;
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
        "shenyang_mahjong",
        Duration::from_secs(120),
        ShenyangMahjongGameHandler::default(),
    )
    .await
}
