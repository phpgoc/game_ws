use std::process;

use landlord::server::run_landlord_server_with_cli;

#[tokio::main]
async fn main() {
    #[cfg(feature = "server-runtime")]
    let logging = match runtime_common::init_logging("landlord", env!("CARGO_PKG_NAME")) {
        Ok(logging) => logging,
        Err(error) => {
            eprintln!("failed to initialize server logging: {error}");
            process::exit(2);
        }
    };
    #[cfg(feature = "server-runtime")]
    let _logging_scope = logging.enter();

    if let Err(error) = run().await {
        #[cfg(feature = "server-runtime")]
        tracing::error!(error = %error, "landlord server stopped with an error");
        #[cfg(not(feature = "server-runtime"))]
        eprintln!("{error}");
        process::exit(2);
    }
}

async fn run() -> anyhow::Result<()> {
    #[cfg(feature = "official")]
    data::init().await?;

    run_landlord_server_with_cli().await
}
