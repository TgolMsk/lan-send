//! The multicast sockets: one per IPv4 interface address and, when enabled,
//! one per interface with IPv6, following the recipe of the official
//! implementation (ADR-0003, ADR-0009).

use socket2::{Domain, Protocol, Socket, Type};
use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
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
    /// `name (address)` or `name (v6, index n)`, for logs.
    pub description: String,
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

/// The index of the interface called `name` (e.g. `en0`), if any.
pub fn interface_index(name: &str) -> Option<u32> {
    if_addrs::get_if_addrs()
        .ok()?
        .into_iter()
        .find(|interface| interface.name == name)
        .and_then(|interface| interface.index)
}

/// Binds one IPv4 socket per usable interface address and, when `group_v6`
/// is given, one IPv6 socket per interface that has IPv6. Interfaces that
/// fail are skipped; it is an error only when none could be bound.
pub(super) fn bind_all(
    group: Ipv4Addr,
    group_v6: Option<Ipv6Addr>,
    port: u16,
) -> Result<Vec<BoundSocket>, MulticastError> {
    let interfaces = if_addrs::get_if_addrs().map_err(MulticastError::Interfaces)?;
    let mut sockets = Vec::new();
    let mut failures = Vec::new();
    let mut seen_v4 = HashSet::new();
    let mut seen_v6 = HashSet::new();
    for interface in interfaces {
        if interface.is_loopback() {
            continue;
        }
        match interface.ip() {
            IpAddr::V4(ip) => {
                if !seen_v4.insert(ip) {
                    continue;
                }
                match bind_v4(group, port, ip) {
                    Ok(socket) => {
                        tracing::debug!("multicast socket bound on {} ({ip})", interface.name);
                        sockets.push(BoundSocket {
                            socket: Arc::new(socket),
                            description: format!("{} ({ip})", interface.name),
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
            IpAddr::V6(_) => {
                let (Some(group_v6), Some(index)) = (group_v6, interface.index) else {
                    continue;
                };
                if !seen_v6.insert(index) {
                    continue;
                }
                match bind_v6(group_v6, port, index) {
                    Ok(socket) => {
                        tracing::debug!(
                            "IPv6 multicast socket bound on {} (index {index})",
                            interface.name
                        );
                        sockets.push(BoundSocket {
                            socket: Arc::new(socket),
                            description: format!("{} (v6, index {index})", interface.name),
                            target: SocketAddr::V6(SocketAddrV6::new(group_v6, port, 0, index)),
                        });
                    }
                    Err(err) => {
                        tracing::debug!(
                            "could not bind IPv6 multicast socket on {} (index {index}): {err}",
                            interface.name
                        );
                    }
                }
            }
        }
    }
    if sockets.is_empty() {
        return Err(MulticastError::NoInterface {
            port,
            details: if failures.is_empty() {
                "no usable interface".to_string()
            } else {
                failures.join("; ")
            },
        });
    }
    Ok(sockets)
}

fn bind_v4(group: Ipv4Addr, port: u16, interface: Ipv4Addr) -> std::io::Result<UdpSocket> {
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

/// Mirrors [`bind_v4`], joining the group by interface index.
fn bind_v6(group: Ipv6Addr, port: u16, interface: u32) -> std::io::Result<UdpSocket> {
    let socket = Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP))?;
    // A dual-stack socket would clash with the IPv4 sockets on the same port.
    socket.set_only_v6(true)?;
    socket.set_reuse_address(true)?;
    platform::set_reuse_port(&socket)?;
    socket.bind(&SocketAddr::from(SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, port, 0, 0)).into())?;
    socket.join_multicast_v6(&group, interface)?;
    socket.set_multicast_if_v6(interface)?;
    socket.set_multicast_loop_v6(true)?;
    // Discovery is limited to the local link (the group's scope already is).
    socket.set_multicast_hops_v6(1)?;
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
