use std::process;

use who_is_landlord::server::run_who_is_landlord_server_with_cli;

#[tokio::main]
async fn main() {
    #[cfg(feature = "official")]
    let logging = match runtime_common::init_logging("who_is_landlord", env!("CARGO_PKG_NAME")) {
        Ok(logging) => logging,
        Err(error) => {
            eprintln!("failed to initialize server logging: {error}");
            process::exit(2);
        }
    };
    #[cfg(feature = "official")]
    let _logging_scope = logging.enter();

    if let Err(error) = run().await {
        #[cfg(feature = "official")]
        tracing::error!(error = %error, "who_is_landlord server stopped with an error");
        #[cfg(not(feature = "official"))]
        eprintln!("{error}");
        process::exit(2);
    }
}

async fn run() -> anyhow::Result<()> {
    #[cfg(feature = "official")]
    data::init().await?;

    run_who_is_landlord_server_with_cli().await
}
