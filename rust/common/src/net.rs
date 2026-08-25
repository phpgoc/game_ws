//! 自建服务启动时的本地地址和端口选择。

use std::net::{Ipv4Addr, SocketAddrV4, TcpListener};

fn find_bindable_port(host: Ipv4Addr) -> Option<u16> {
    (9001..=u16::MAX).find(|port| is_bindable(host, *port))
}

fn is_bindable(host: Ipv4Addr, port: u16) -> bool {
    let addr = SocketAddrV4::new(host, port);
    TcpListener::bind(addr).is_ok()
}

pub(crate) fn resolve_host(host: Option<String>) -> anyhow::Result<Ipv4Addr> {
    // 未指定地址时优先选择私网 IPv4，方便局域网联机；显式地址必须能解析
    // 成 IPv4，避免等到 bind 阶段才暴露配置错误。
    match host {
        Some(host) => host
            .parse::<Ipv4Addr>()
            .map_err(|_| anyhow::anyhow!("invalid host: {host}")),
        None => select_private_ipv4().ok_or_else(|| anyhow::anyhow!("no private ipv4 found")),
    }
}

pub(crate) fn resolve_port(host: Ipv4Addr, port: Option<u16>) -> anyhow::Result<u16> {
    // 端口必须高于系统保留范围；未指定时从 9001 开始寻找当前可绑定端口。
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
#[path = "net/tests.rs"]
mod tests;
