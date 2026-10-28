use std::process;

use upgrade::server::run_upgrade_server_with_cli;

#[tokio::main]
async fn main() {
    #[cfg(feature = "official")]
    if let Err(error) = data::init().await {
        eprintln!("{error}");
        process::exit(2);
    }
    let result = run_upgrade_server_with_cli().await;
    #[cfg(feature = "official")]
    data::shutdown().await;
    if let Err(error) = result {
        eprintln!("{error}");
        process::exit(2);
    }
}
