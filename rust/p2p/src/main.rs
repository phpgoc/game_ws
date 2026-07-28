use std::process;

use p2p::server::run_p2p_server_with_cli;

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("{err:#}");
        process::exit(2);
    }
}

async fn run() -> anyhow::Result<()> {
    #[cfg(feature = "official")]
    data::init().await?;

    let result = run_p2p_server_with_cli().await;

    #[cfg(feature = "official")]
    data::shutdown().await;

    result
}
