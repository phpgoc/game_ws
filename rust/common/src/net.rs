use std::net::{Ipv4Addr, SocketAddrV4, TcpListener};

fn find_bindable_port(host: Ipv4Addr) -> Option<u16> {
    (9001..=u16::MAX).find(|port| is_bindable(host, *port))
}

fn is_bindable(host: Ipv4Addr, port: u16) -> bool {
    let addr = SocketAddrV4::new(host, port);
    TcpListener::bind(addr).is_ok()
}

pub(crate) fn resolve_host(host: Option<String>) -> anyhow::Result<Ipv4Addr> {
    match host {
        Some(host) => host
            .parse::<Ipv4Addr>()
            .map_err(|_| anyhow::anyhow!("invalid host: {host}")),
        None => select_private_ipv4().ok_or_else(|| anyhow::anyhow!("no private ipv4 found")),
    }
}

pub(crate) fn resolve_port(host: Ipv4Addr, port: Option<u16>) -> anyhow::Result<u16> {
    match port {
        Some(port) => {
            if port <= 9000 {
                return Err(anyhow::anyhow!("port must be > 9000, got {port}"));
            }
            if !is_bindable(host, port) {
                return Err(anyhow::anyhow!("port is not bindable: {host}:{port}"));
            }
            Ok(port)
        }
        None => find_bindable_port(host)
            .ok_or_else(|| anyhow::anyhow!("no bindable port found above 9000 for host {host}")),
    }
}

fn select_private_ipv4() -> Option<Ipv4Addr> {
    let mut candidates = if_addrs::get_if_addrs().ok()?;
    candidates.sort_by(|a, b| a.name.cmp(&b.name));

    candidates.into_iter().find_map(|iface| match iface.ip() {
        std::net::IpAddr::V4(v4) if v4.is_private() => Some(v4),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use std::{
        net::{Ipv4Addr, TcpListener},
        str::FromStr,
    };

    use super::{resolve_host, resolve_port};

    #[test]
    fn resolve_host_accepts_ipv4_literal() {
        assert_eq!(
            resolve_host(Some("127.0.0.1".to_string())).unwrap(),
            Ipv4Addr::LOCALHOST
        );
    }

    #[test]
    fn resolve_host_rejects_invalid_literal() {
        let error = resolve_host(Some("localhost".to_string())).unwrap_err();
        assert_eq!(error.to_string(), "invalid host: localhost");
    }

    #[test]
    fn resolve_port_rejects_address_already_in_use() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(port > 9000);

        let error = resolve_port(Ipv4Addr::from_str("127.0.0.1").unwrap(), Some(port)).unwrap_err();
        assert_eq!(
            error.to_string(),
            format!("port is not bindable: 127.0.0.1:{port}")
        );
    }

    #[test]
    fn resolve_port_rejects_reserved_range() {
        let error = resolve_port(Ipv4Addr::LOCALHOST, Some(9000)).unwrap_err();
        assert_eq!(error.to_string(), "port must be > 9000, got 9000");
    }
}
