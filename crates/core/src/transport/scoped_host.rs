//! Link-local IPv6 peers must be dialled with their scope (`fe80::1%3`),
//! which URLs cannot carry. The scoped address is encoded into a synthetic
//! host name that a custom DNS resolver turns back into the socket address.

use std::net::{Ipv6Addr, SocketAddr, SocketAddrV6};

/// Suffix of the synthetic host names.
const SUFFIX: &str = ".scoped.lan-send.internal";

/// A host string as stored for devices: `ip`, or `ip%scope` for link-local
/// IPv6 addresses.
pub fn parse_scoped(host: &str) -> Option<(Ipv6Addr, u32)> {
    let (ip, scope) = host.split_once('%')?;
    Some((ip.parse().ok()?, scope.parse().ok()?))
}

/// The host name to put into a URL for a scoped address:
/// `fe80::1%3` becomes `fe80--1s3.scoped.lan-send.internal`.
pub fn encode(ip: Ipv6Addr, scope: u32) -> String {
    format!("{}s{scope}{SUFFIX}", ip.to_string().replace(':', "-"))
}

/// The socket address a synthetic host name stands for. `port` is the
/// placeholder the resolver receives; callers substitute the URL's port.
pub fn decode(name: &str, port: u16) -> Option<SocketAddr> {
    let encoded = name.strip_suffix(SUFFIX)?;
    let (ip, scope) = encoded.rsplit_once('s')?;
    let ip: Ipv6Addr = ip.replace('-', ":").parse().ok()?;
    let scope: u32 = scope.parse().ok()?;
    Some(SocketAddr::V6(SocketAddrV6::new(ip, port, 0, scope)))
}

/// The host part of a URL for `host`: scoped IPv6 addresses are encoded,
/// other IPv6 addresses bracketed, everything else passed through.
pub fn url_host(host: &str) -> String {
    if let Some((ip, scope)) = parse_scoped(host) {
        return encode(ip, scope);
    }
    if host.contains(':') && !host.starts_with('[') {
        return format!("[{host}]");
    }
    host.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let ip: Ipv6Addr = "fe80::1c2a:3b4c".parse().unwrap();
        let name = encode(ip, 7);
        assert_eq!(name, "fe80--1c2a-3b4cs7.scoped.lan-send.internal");
        assert_eq!(
            decode(&name, 53317),
            Some(SocketAddr::V6(SocketAddrV6::new(ip, 53317, 0, 7)))
        );
        assert_eq!(decode("example.com", 1), None);
        assert_eq!(url_host("fe80::1%3"), "fe80--1s3.scoped.lan-send.internal");
        assert_eq!(url_host("fd00::1"), "[fd00::1]");
        assert_eq!(url_host("192.168.1.2"), "192.168.1.2");
        assert_eq!(
            parse_scoped("fe80::1%3"),
            Some(("fe80::1".parse().unwrap(), 3))
        );
        assert_eq!(parse_scoped("fe80::1"), None);
    }
}
