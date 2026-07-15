use std::{
    env,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use hmac::{Hmac, Mac};
use sha1::Sha1;
use share_type_public::{WsP2pIceConfigEvent, WsP2pIceServer};

type HmacSha1 = Hmac<Sha1>;

const DEFAULT_TURN_PORT: u16 = 3478;
const DEFAULT_TTL_SECONDS: u64 = 3_600;
const MAX_TTL_SECONDS: u64 = 86_400;
const DEFAULT_RELAY_MIN_PORT: u16 = 49_160;
const DEFAULT_RELAY_MAX_PORT: u16 = 49_200;
const DEFAULT_REALM: &str = "lan-game-p2p";

#[derive(Debug, Clone)]
pub struct IceServiceConfig {
    stun_urls: Vec<String>,
    turn_urls: Vec<String>,
    turn_secret: String,
    credential_ttl_seconds: u64,
}

#[derive(Debug, Clone)]
pub struct EmbeddedTurnConfig {
    pub listen_addr: SocketAddr,
    pub public_ip: IpAddr,
    pub relay_bind_ip: IpAddr,
    pub relay_min_port: u16,
    pub relay_max_port: u16,
    pub realm: String,
    pub turn_secret: String,
    pub credential_ttl_seconds: u64,
}

#[derive(Debug, Clone)]
pub struct P2pServiceConfig {
    pub ice: IceServiceConfig,
    pub turn: EmbeddedTurnConfig,
}

impl P2pServiceConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let public_ip = env::var("P2P_TURN_PUBLIC_IP")
            .ok()
            .map(|value| value.parse::<IpAddr>())
            .transpose()
            .context("P2P_TURN_PUBLIC_IP must be an IPv4 or IPv6 address")?
            .or_else(detect_local_ip)
            .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
        let default_bind_ip = if public_ip.is_ipv4() {
            IpAddr::V4(Ipv4Addr::UNSPECIFIED)
        } else {
            IpAddr::V6(Ipv6Addr::UNSPECIFIED)
        };
        let bind_ip = env::var("P2P_TURN_BIND_IP")
            .ok()
            .map(|value| value.parse::<IpAddr>())
            .transpose()
            .context("P2P_TURN_BIND_IP must be an IPv4 or IPv6 address")?
            .unwrap_or(default_bind_ip);
        let relay_bind_ip = env::var("P2P_TURN_RELAY_BIND_IP")
            .ok()
            .map(|value| value.parse::<IpAddr>())
            .transpose()
            .context("P2P_TURN_RELAY_BIND_IP must be an IPv4 or IPv6 address")?
            .unwrap_or(default_bind_ip);
        let port = env_u16("P2P_TURN_PORT", DEFAULT_TURN_PORT)?;
        let relay_min_port = env_u16("P2P_TURN_RELAY_MIN_PORT", DEFAULT_RELAY_MIN_PORT)?;
        let relay_max_port = env_u16("P2P_TURN_RELAY_MAX_PORT", DEFAULT_RELAY_MAX_PORT)?;
        let realm = env::var("P2P_TURN_REALM").unwrap_or_else(|_| DEFAULT_REALM.to_owned());
        let turn_secret = env::var("P2P_TURN_SECRET")
            .context("P2P_TURN_SECRET is required for embedded TURN authentication")?;
        let credential_ttl_seconds = env::var("P2P_TURN_TTL_SECONDS")
            .ok()
            .map(|value| value.parse::<u64>())
            .transpose()
            .context("P2P_TURN_TTL_SECONDS must be an integer")?
            .unwrap_or(DEFAULT_TTL_SECONDS);

        let turn = EmbeddedTurnConfig::new(
            SocketAddr::new(bind_ip, port),
            public_ip,
            relay_bind_ip,
            relay_min_port,
            relay_max_port,
            realm,
            turn_secret.clone(),
            credential_ttl_seconds,
        )?;
        let host = ice_host(public_ip);
        let ice = IceServiceConfig::new(
            vec![format!("stun:{host}:{port}")],
            vec![format!("turn:{host}:{port}?transport=udp")],
            turn_secret,
            credential_ttl_seconds,
        )?;
        Ok(Self { ice, turn })
    }
}

impl EmbeddedTurnConfig {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        listen_addr: SocketAddr,
        public_ip: IpAddr,
        relay_bind_ip: IpAddr,
        relay_min_port: u16,
        relay_max_port: u16,
        realm: String,
        turn_secret: String,
        credential_ttl_seconds: u64,
    ) -> anyhow::Result<Self> {
        validate_secret_and_ttl(&turn_secret, credential_ttl_seconds)?;
        if public_ip.is_unspecified() || public_ip.is_multicast() {
            bail!("TURN public IP must be a concrete unicast address");
        }
        if listen_addr.is_ipv4() != public_ip.is_ipv4()
            || relay_bind_ip.is_ipv4() != public_ip.is_ipv4()
        {
            bail!("TURN listen, relay bind, and public IP addresses must use one IP family");
        }
        if relay_min_port == 0 || relay_max_port < relay_min_port {
            bail!("TURN relay port range is invalid");
        }
        if realm.trim().is_empty() || realm.len() > 128 || realm.chars().any(char::is_control) {
            bail!("TURN realm must be a non-empty printable value up to 128 bytes");
        }
        Ok(Self {
            listen_addr,
            public_ip,
            relay_bind_ip,
            relay_min_port,
            relay_max_port,
            realm,
            turn_secret,
            credential_ttl_seconds,
        })
    }
}

impl IceServiceConfig {
    pub fn new(
        stun_urls: Vec<String>,
        turn_urls: Vec<String>,
        turn_secret: String,
        credential_ttl_seconds: u64,
    ) -> anyhow::Result<Self> {
        if stun_urls.is_empty() {
            bail!("at least one STUN URL is required");
        }
        if turn_urls.is_empty() {
            bail!("at least one TURN URL is required");
        }
        validate_secret_and_ttl(&turn_secret, credential_ttl_seconds)?;
        Ok(Self {
            stun_urls,
            turn_urls,
            turn_secret,
            credential_ttl_seconds,
        })
    }

    pub fn issue_event(
        &self,
        session_id: u64,
        self_position: usize,
    ) -> anyhow::Result<WsP2pIceConfigEvent> {
        let now = unix_time()?;
        self.issue_event_at(session_id, self_position, now)
    }

    pub(crate) fn issue_event_at(
        &self,
        session_id: u64,
        self_position: usize,
        now: u64,
    ) -> anyhow::Result<WsP2pIceConfigEvent> {
        let expires_at = now.saturating_add(self.credential_ttl_seconds);
        let username = format!("{expires_at}:{session_id}");
        let credential = turn_password(&self.turn_secret, &username)?;

        Ok(WsP2pIceConfigEvent {
            self_position: self_position as i32,
            ice_servers: vec![
                WsP2pIceServer {
                    urls: self.stun_urls.clone(),
                    username: None,
                    credential: None,
                },
                WsP2pIceServer {
                    urls: self.turn_urls.clone(),
                    username: Some(username),
                    credential: Some(credential),
                },
            ],
            credential_expires_at: expires_at.to_string(),
        })
    }
}

pub(crate) fn turn_password(secret: &str, username: &str) -> anyhow::Result<String> {
    let mut mac = HmacSha1::new_from_slice(secret.as_bytes())
        .context("invalid embedded TURN shared secret")?;
    mac.update(username.as_bytes());
    Ok(STANDARD.encode(mac.finalize().into_bytes()))
}

pub(crate) fn unix_time() -> anyhow::Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX epoch")?
        .as_secs())
}

fn validate_secret_and_ttl(secret: &str, ttl: u64) -> anyhow::Result<()> {
    if secret.trim().len() < 16 {
        bail!("TURN shared secret must contain at least 16 non-whitespace bytes");
    }
    if !(60..=MAX_TTL_SECONDS).contains(&ttl) {
        bail!("TURN credential TTL must be between 60 and {MAX_TTL_SECONDS} seconds");
    }
    Ok(())
}

fn env_u16(name: &str, default: u16) -> anyhow::Result<u16> {
    env::var(name)
        .ok()
        .map(|value| value.parse::<u16>())
        .transpose()
        .with_context(|| format!("{name} must be an integer between 0 and 65535"))
        .map(|value| value.unwrap_or(default))
}

fn detect_local_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((Ipv4Addr::new(192, 0, 2, 1), 9)).ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_unspecified()).then_some(ip)
}

fn ice_host(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(ip) => ip.to_string(),
        IpAddr::V6(ip) => format!("[{ip}]"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> IceServiceConfig {
        IceServiceConfig::new(
            vec!["stun:stun.example.test:3478".into()],
            vec!["turn:turn.example.test:3478?transport=udp".into()],
            "test-secret-long-enough".into(),
            600,
        )
        .expect("ICE config")
    }

    #[test]
    fn issues_expiring_embedded_turn_credentials() {
        let event = config().issue_event_at(42, 1, 1_000).expect("ICE event");
        assert_eq!(event.self_position, 1);
        assert_eq!(event.credential_expires_at, "1600");
        assert_eq!(event.ice_servers.len(), 2);
        assert_eq!(event.ice_servers[1].username.as_deref(), Some("1600:42"));
        assert!(
            event.ice_servers[1]
                .credential
                .as_deref()
                .is_some_and(|value| !value.is_empty())
        );
    }

    #[test]
    fn rejects_unsafe_turn_configuration() {
        assert!(
            IceServiceConfig::new(
                vec!["stun:x".into()],
                vec!["turn:x".into()],
                "short".into(),
                600
            )
            .is_err()
        );
        assert!(
            EmbeddedTurnConfig::new(
                "0.0.0.0:3478".parse().expect("listen"),
                IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                49_160,
                49_200,
                DEFAULT_REALM.into(),
                "test-secret-long-enough".into(),
                600,
            )
            .is_err()
        );
    }
}
