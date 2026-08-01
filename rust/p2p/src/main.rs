use std::process;

use p2p::server::run_p2p_server_with_cli;

#[tokio::main]
async fn main() {
    #[cfg(feature = "server-runtime")]
    let logging = match runtime_common::init_logging("p2p", env!("CARGO_PKG_NAME")) {
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
        tracing::error!(error = %error, "p2p server stopped with an error");
        #[cfg(not(feature = "server-runtime"))]
        eprintln!("{error:#}");
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
