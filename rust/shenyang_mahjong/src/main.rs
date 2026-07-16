use std::process;

use shenyang_mahjong::server::run_shenyang_mahjong_server_with_cli;

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("{err}");
        process::exit(2);
    }
}

async fn run() -> anyhow::Result<()> {
    #[cfg(feature = "official")]
    data::init().await?;

    run_shenyang_mahjong_server_with_cli().await
}
