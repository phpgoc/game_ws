use std::process;

use dominoes::server::run_dominoes_server_with_cli;

#[tokio::main]
async fn main() {
    #[cfg(feature = "official")]
    let logging = match runtime_common::init_logging("dominoes", env!("CARGO_PKG_NAME")) {
        Ok(logging) => logging,
        Err(error) => {
            eprintln!("failed to initialize server logging: {error}");
            process::exit(2);
        }
    };
    #[cfg(feature = "official")]
    let _logging_scope = logging.enter();

    if let Err(error) = run_dominoes_server_with_cli().await {
        #[cfg(feature = "official")]
        tracing::error!(error = %error, "dominoes server stopped with an error");
        #[cfg(not(feature = "official"))]
        eprintln!("{error}");
        process::exit(2);
    }
}
