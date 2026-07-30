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
fn resolve_host_uses_a_private_interface_when_one_is_available() {
    match resolve_host(None) {
        Ok(host) => assert!(host.is_private()),
        Err(error) => assert_eq!(error.to_string(), "no private ipv4 found"),
    }
}

#[test]
fn resolve_port_accepts_explicit_and_automatically_selected_ports() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    assert!(port > 9000);

    let error = resolve_port(Ipv4Addr::from_str("127.0.0.1").unwrap(), Some(port)).unwrap_err();
    assert_eq!(
        error.to_string(),
        format!("port is not bindable: 127.0.0.1:{port}")
    );
    drop(listener);

    assert_eq!(resolve_port(Ipv4Addr::LOCALHOST, Some(port)).unwrap(), port);
    let selected = resolve_port(Ipv4Addr::LOCALHOST, None).unwrap();
    assert!(selected > 9000);
}

#[test]
fn resolve_port_rejects_reserved_range() {
    let error = resolve_port(Ipv4Addr::LOCALHOST, Some(9000)).unwrap_err();
    assert_eq!(error.to_string(), "port must be > 9000, got 9000");
}
