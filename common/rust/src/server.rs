use std::{future::Future, net::SocketAddr, time::Duration};

use anyhow::Context;
use futures_util::{SinkExt, StreamExt};
use serde::{Serialize, de::DeserializeOwned};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tracing::{error, info, warn};

use crate::{from_message, to_text_message};

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub service_name: &'static str,
    pub listen_addr: String,
    pub idle_timeout: Duration,
}

pub async fn run_ws_server<In, Out, H, Fut>(
    config: ServerConfig,
    handler: H,
) -> anyhow::Result<()>
where
    In: DeserializeOwned + Send + 'static,
    Out: Serialize + Send + 'static,
    H: Fn(In) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = anyhow::Result<Option<Out>>> + Send + 'static,
{
    init_tracing();

    let listener = TcpListener::bind(&config.listen_addr)
        .await
        .with_context(|| format!("bind {} failed", config.listen_addr))?;
    info!(
        service = config.service_name,
        listen = %config.listen_addr,
        "ws server started"
    );

    loop {
        let (stream, peer) = listener.accept().await?;
        let config = config.clone();
        let handler = handler.clone();
        tokio::spawn(async move {
            if let Err(err) = handle_connection::<In, Out, H, Fut>(stream, peer, config, handler).await {
                error!(%peer, ?err, "connection ended with error");
            } else {
                info!(%peer, "connection closed");
            }
        });
    }
}

async fn handle_connection<In, Out, H, Fut>(
    stream: TcpStream,
    peer: SocketAddr,
    config: ServerConfig,
    handler: H,
) -> anyhow::Result<()>
where
    In: DeserializeOwned + Send + 'static,
    Out: Serialize + Send + 'static,
    H: Fn(In) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = anyhow::Result<Option<Out>>> + Send + 'static,
{
    let mut ws = accept_async(stream).await?;

    loop {
        let frame = match tokio::time::timeout(config.idle_timeout, ws.next()).await {
            Ok(Some(frame)) => frame?,
            Ok(None) => break,
            Err(_) => {
                warn!(%peer, "idle timeout, closing connection");
                ws.send(Message::Close(None)).await?;
                break;
            }
        };

        let Some(request) = from_message::<In>(frame)? else {
            continue;
        };

        if let Some(response) = handler(request).await? {
            ws.send(to_text_message(&response)?).await?;
        }
    }

    Ok(())
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .try_init();
}
