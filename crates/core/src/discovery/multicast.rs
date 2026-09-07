//! The multicast sockets: one per IPv4 interface address, following the
//! recipe of the official implementation (see ADR-0003).

use socket2::{Domain, Protocol, Socket, Type};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use tokio::net::UdpSocket;

#[derive(Debug, thiserror::Error)]
pub enum MulticastError {
    #[error("could not enumerate network interfaces: {0}")]
    Interfaces(std::io::Error),

    #[error("no network interface could be used for multicast on port {port}: {details}")]
    NoInterface { port: u16, details: String },
}

/// A socket bound to one interface, with the group address it sends to.
pub(super) struct BoundSocket {
    pub socket: Arc<UdpSocket>,
    pub interface: Ipv4Addr,
    pub target: SocketAddr,
}

/// The non-loopback IPv4 addresses of this machine.
pub(super) fn local_ipv4_addresses() -> Vec<Ipv4Addr> {
    let Ok(interfaces) = if_addrs::get_if_addrs() else {
        return Vec::new();
    };
    let mut addresses: Vec<Ipv4Addr> = interfaces
        .into_iter()
        .filter(|interface| !interface.is_loopback())
        .filter_map(|interface| match interface.ip() {
            IpAddr::V4(ip) => Some(ip),
            IpAddr::V6(_) => None,
        })
        .collect();
    addresses.sort();
    addresses.dedup();
    addresses
}

/// Binds one socket per usable interface. Interfaces that fail are skipped;
/// it is an error only when none could be bound.
pub(super) fn bind_all(group: Ipv4Addr, port: u16) -> Result<Vec<BoundSocket>, MulticastError> {
    let interfaces = if_addrs::get_if_addrs().map_err(MulticastError::Interfaces)?;
    let mut sockets = Vec::new();
    let mut failures = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for interface in interfaces {
        if interface.is_loopback() {
            continue;
        }
        let IpAddr::V4(ip) = interface.ip() else {
            continue;
        };
        if !seen.insert(ip) {
            continue;
        }
        match bind_one(group, port, ip) {
            Ok(socket) => {
                tracing::debug!("multicast socket bound on {} ({ip})", interface.name);
                sockets.push(BoundSocket {
                    socket: Arc::new(socket),
                    interface: ip,
                    target: SocketAddr::new(group.into(), port),
                });
            }
            Err(err) => {
                tracing::warn!(
                    "could not bind multicast socket on {} ({ip}): {err}",
                    interface.name
                );
                failures.push(format!("{} ({ip}): {err}", interface.name));
            }
        }
    }
    if sockets.is_empty() {
        return Err(MulticastError::NoInterface {
            port,
            details: if failures.is_empty() {
                "no IPv4 interface".to_string()
            } else {
                failures.join("; ")
            },
        });
    }
    Ok(sockets)
}

fn bind_one(group: Ipv4Addr, port: u16, interface: Ipv4Addr) -> std::io::Result<UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    // All sockets (and other LocalSend instances on this host) share the port.
    socket.set_reuse_address(true)?;
    platform::set_reuse_port(&socket)?;
    // Bind the wildcard address: binding the interface address stops some
    // platforms from delivering multicast datagrams.
    socket.bind(&SocketAddr::from(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port)).into())?;
    socket.join_multicast_v4(&group, &interface)?;
    // Pin outgoing datagrams to this interface; otherwise the routing table
    // decides and every socket would announce on the same one.
    socket.set_multicast_if_v4(&interface)?;
    // Keep loopback on so that instances on the same host see each other;
    // own datagrams are filtered by fingerprint.
    socket.set_multicast_loop_v4(true)?;
    socket.set_multicast_ttl_v4(1)?;
    socket.set_nonblocking(true)?;
    UdpSocket::from_std(socket.into())
}

/// `SO_REUSEPORT` exists on Unix only; Windows shares the port through
/// `SO_REUSEADDR` alone.
mod platform {
    use socket2::Socket;

    #[cfg(all(unix, not(any(target_os = "solaris", target_os = "illumos"))))]
    pub(super) fn set_reuse_port(socket: &Socket) -> std::io::Result<()> {
        socket.set_reuse_port(true)
    }

    #[cfg(not(all(unix, not(any(target_os = "solaris", target_os = "illumos")))))]
    pub(super) fn set_reuse_port(_socket: &Socket) -> std::io::Result<()> {
        Ok(())
    }
}
