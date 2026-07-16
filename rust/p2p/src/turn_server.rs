use std::{net::SocketAddr, sync::Arc, time::Duration};

use anyhow::Context;
use tokio::net::UdpSocket;
use turn::{
    Error,
    auth::{AuthHandler, generate_auth_key},
    relay::relay_range::RelayAddressGeneratorRanges,
    server::{
        Server,
        config::{ConnConfig, ServerConfig},
    },
};
use util::vnet::net::Net;

use crate::config::{EmbeddedTurnConfig, turn_password, unix_time};

pub struct EmbeddedTurnServer {
    server: Server,
    listen_addr: SocketAddr,
}

struct ExpiringAuthHandler {
    secret: String,
    maximum_ttl_seconds: u64,
}

pub async fn start_embedded_turn(
    config: &EmbeddedTurnConfig,
) -> anyhow::Result<EmbeddedTurnServer> {
    let socket = Arc::new(
        UdpSocket::bind(config.listen_addr)
            .await
            .with_context(|| format!("bind embedded STUN/TURN at {}", config.listen_addr))?,
    );
    let listen_addr = socket.local_addr()?;
    let server = Server::new(ServerConfig {
        conn_configs: vec![ConnConfig {
            conn: socket,
            relay_addr_generator: Box::new(RelayAddressGeneratorRanges {
                relay_address: config.public_ip,
                min_port: config.relay_min_port,
                max_port: config.relay_max_port,
                max_retries: 32,
                address: config.relay_bind_ip.to_string(),
                net: Arc::new(Net::new(None)),
            }),
        }],
        realm: config.realm.clone(),
        auth_handler: Arc::new(ExpiringAuthHandler {
            secret: config.turn_secret.clone(),
            maximum_ttl_seconds: config.credential_ttl_seconds,
        }),
        channel_bind_timeout: Duration::from_secs(0),
        alloc_close_notify: None,
    })
    .await
    .context("start embedded STUN/TURN server")?;
    Ok(EmbeddedTurnServer {
        server,
        listen_addr,
    })
}

impl EmbeddedTurnServer {
    pub async fn close(self) -> anyhow::Result<()> {
        self.server
            .close()
            .await
            .context("close embedded TURN server")
    }

    pub fn listen_addr(&self) -> SocketAddr {
        self.listen_addr
    }
}

impl AuthHandler for ExpiringAuthHandler {
    fn auth_handle(
        &self,
        username: &str,
        realm: &str,
        _src_addr: SocketAddr,
    ) -> Result<Vec<u8>, Error> {
        let (expires_at, session_id) = username
            .split_once(':')
            .ok_or_else(|| Error::Other("malformed time-windowed TURN username".into()))?;
        let expires_at = expires_at
            .parse::<u64>()
            .map_err(|_| Error::Other("malformed TURN credential expiry".into()))?;
        let session_id = session_id
            .parse::<u64>()
            .map_err(|_| Error::Other("malformed TURN session id".into()))?;
        let now = unix_time().map_err(|error| Error::Other(error.to_string()))?;
        if session_id == 0
            || expires_at < now
            || expires_at.saturating_sub(now) > self.maximum_ttl_seconds
        {
            return Err(Error::Other("expired or invalid TURN credential".into()));
        }
        let password = turn_password(&self.secret, username)
            .map_err(|error| Error::Other(error.to_string()))?;
        Ok(generate_auth_key(username, realm, &password))
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use turn::client::{Client, ClientConfig};
    use util::Conn;

    use super::*;
    use crate::config::IceServiceConfig;

    fn embedded_config() -> EmbeddedTurnConfig {
        EmbeddedTurnConfig::new(
            "127.0.0.1:0".parse().expect("listen address"),
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            55_000,
            60_000,
            "p2p-test".into(),
            "embedded-turn-test-secret".into(),
            600,
        )
        .expect("embedded TURN config")
    }

    #[tokio::test]
    async fn serves_stun_binding_and_authenticated_turn_allocation() {
        let config = embedded_config();
        let server = start_embedded_turn(&config).await.expect("TURN server");
        let address = server.listen_addr();
        let ice = IceServiceConfig::new(
            vec![format!("stun:{address}")],
            vec![format!("turn:{address}?transport=udp")],
            config.turn_secret.clone(),
            config.credential_ttl_seconds,
        )
        .expect("ICE config");
        let event = ice.issue_event(7, 0).expect("credentials");
        let turn_ice = &event.ice_servers[1];
        let socket = UdpSocket::bind("127.0.0.1:0").await.expect("client socket");
        let client = Client::new(ClientConfig {
            stun_serv_addr: address.to_string(),
            turn_serv_addr: address.to_string(),
            username: turn_ice.username.clone().expect("username"),
            password: turn_ice.credential.clone().expect("credential"),
            realm: config.realm.clone(),
            software: "p2p-test".into(),
            rto_in_ms: 0,
            conn: Arc::new(socket),
            vnet: None,
        })
        .await
        .expect("TURN client");
        client.listen().await.expect("client listener");

        let mapped = client
            .send_binding_request()
            .await
            .expect("STUN binding response");
        assert_eq!(mapped.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
        let relay = client.allocate().await.expect("TURN allocation");
        let relay_addr = relay.local_addr().expect("relay address");
        assert_eq!(relay_addr.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert!((55_000..=60_000).contains(&relay_addr.port()));

        drop(relay);
        client.close().await.expect("close client");
        server.close().await.expect("close server");
    }
}
