//! Device discovery: UDP multicast announcements answered over HTTP, probing
//! of known addresses, and the `/24` subnet scan fallback (ADR-0003).
//!
//! [`Discovery::start`] binds the multicast sockets and answers other
//! devices' announcements. Nothing is announced until
//! [`Discovery::announce`] is called. Devices that register with *our*
//! HTTP server are fed back through [`Discovery::add_confirmed`].

mod multicast;
mod store;

pub use store::Device;

use crate::protocol::{DeviceInfo, Fingerprint, MulticastAnnouncement, ProtocolType};
use crate::protocol::{MULTICAST_GROUP_V4, MULTICAST_PORT};
use crate::transport::{Client, ClientError, Identity, Target};
use futures_util::StreamExt;
use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

/// Timeout of the register requests sent by discovery. LAN peers answer
/// quickly or not at all, and it bounds how long a subnet scan takes.
pub const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_millis(500);
/// Hosts probed at once during a subnet scan.
const SCAN_CONCURRENCY: usize = 50;
/// Delays before each datagram of an announcement burst: a single datagram
/// is easily lost and a device that just joined may not be ready yet.
const ANNOUNCE_DELAYS: [Duration; 3] = [
    Duration::from_millis(100),
    Duration::from_millis(500),
    Duration::from_millis(2000),
];
/// Announcements of the same peer within this window are answered once
/// (its burst, and duplicates delivered to several of our sockets).
const ANSWER_DEDUP_WINDOW: Duration = Duration::from_secs(3);
const RECEIVE_BUFFER: usize = 65536;
const MAX_CONSECUTIVE_RECEIVE_ERRORS: u32 = 10;
const EVENT_CAPACITY: usize = 64;

/// How discovery is started.
pub struct DiscoveryConfig {
    pub identity: Arc<Identity>,
    /// This device as announced and as sent in register requests.
    pub device: DeviceInfo,
    pub group: Ipv4Addr,
    pub multicast_port: u16,
    pub probe_timeout: Duration,
    /// Whether announcements of other devices are answered. Turn off when
    /// the HTTP server is not running.
    pub answer_announcements: bool,
}

impl DiscoveryConfig {
    /// Protocol defaults for the given identity and device.
    pub fn new(identity: Arc<Identity>, device: DeviceInfo) -> Self {
        Self {
            identity,
            device,
            group: MULTICAST_GROUP_V4,
            multicast_port: MULTICAST_PORT,
            probe_timeout: DEFAULT_PROBE_TIMEOUT,
            answer_announcements: true,
        }
    }
}

#[derive(Clone, Debug)]
pub enum DiscoveryEvent {
    /// A device was confirmed reachable for the first time.
    Found(Device),
    /// A known device was confirmed again, possibly at a new address.
    Updated(Device),
}

struct Inner {
    identity: Arc<Identity>,
    device: DeviceInfo,
    probe_timeout: Duration,
    store: store::DeviceStore,
    events: broadcast::Sender<DiscoveryEvent>,
    sockets: Vec<multicast::BoundSocket>,
    multicast_error: Option<String>,
    answering: AtomicBool,
    scanning: parking_lot::Mutex<HashSet<Ipv4Addr>>,
    recently_answered: parking_lot::Mutex<HashMap<String, Instant>>,
    confirmations: AtomicU64,
}

/// A running discovery. Cheap to clone; all clones share the same state.
#[derive(Clone)]
pub struct Discovery {
    inner: Arc<Inner>,
    cancel: CancellationToken,
    tasks: TaskTracker,
}

impl Discovery {
    /// Binds the multicast sockets and starts listening. Never fails: when
    /// multicast is unavailable the reason is kept in
    /// [`Discovery::multicast_error`] and HTTP-based discovery still works.
    pub fn start(config: DiscoveryConfig) -> Self {
        let (sockets, multicast_error) =
            match multicast::bind_all(config.group, config.multicast_port) {
                Ok(sockets) => (sockets, None),
                Err(err) => {
                    tracing::warn!("multicast discovery unavailable: {err}");
                    (Vec::new(), Some(err.to_string()))
                }
            };
        let (events, _) = broadcast::channel(EVENT_CAPACITY);
        let inner = Arc::new(Inner {
            identity: config.identity,
            device: config.device,
            probe_timeout: config.probe_timeout,
            store: store::DeviceStore::default(),
            events,
            sockets,
            multicast_error,
            answering: AtomicBool::new(config.answer_announcements),
            scanning: parking_lot::Mutex::new(HashSet::new()),
            recently_answered: parking_lot::Mutex::new(HashMap::new()),
            confirmations: AtomicU64::new(0),
        });
        let cancel = CancellationToken::new();
        let tasks = TaskTracker::new();
        for socket in &inner.sockets {
            tasks.spawn(receive_loop(
                socket.socket.clone(),
                socket.interface,
                inner.clone(),
                cancel.clone(),
            ));
        }
        Self {
            inner,
            cancel,
            tasks,
        }
    }

    /// Why multicast could not be started, when it could not.
    pub fn multicast_error(&self) -> Option<&str> {
        self.inner.multicast_error.as_deref()
    }

    /// Interfaces the multicast sockets are bound to.
    pub fn multicast_interfaces(&self) -> Vec<Ipv4Addr> {
        self.inner
            .sockets
            .iter()
            .map(|socket| socket.interface)
            .collect()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<DiscoveryEvent> {
        self.inner.events.subscribe()
    }

    /// All confirmed devices, in discovery order.
    pub fn devices(&self) -> Vec<Device> {
        self.inner.store.devices()
    }

    pub fn device_by_fingerprint(&self, fingerprint: &Fingerprint) -> Option<Device> {
        self.inner.store.by_fingerprint(fingerprint)
    }

    /// Finds a device by exact alias (case-insensitive), fingerprint prefix
    /// (at least 4 characters), IP address or `ip:port`.
    pub fn find(&self, query: &str) -> Option<Device> {
        let query = query.trim();
        let devices = self.devices();
        let by_address = |device: &Device| match query.parse::<std::net::SocketAddr>() {
            Ok(addr) => device.reachable_at(&addr.ip().to_string(), Some(addr.port())),
            Err(_) => match query.parse::<IpAddr>() {
                Ok(ip) => device.reachable_at(&ip.to_string(), None),
                Err(_) => false,
            },
        };
        devices
            .iter()
            .find(|device| device.alias.eq_ignore_ascii_case(query))
            .or_else(|| {
                devices
                    .iter()
                    .find(|device| query.len() >= 4 && device.fingerprint.has_prefix(query))
            })
            .or_else(|| devices.iter().find(|device| by_address(device)))
            .cloned()
    }

    /// Records a device confirmed outside of discovery, e.g. one that
    /// registered with our HTTP server. Returns whether it is new.
    pub fn add_confirmed(&self, device: Device) -> bool {
        self.inner.confirm(device).0
    }

    pub fn set_answer_announcements(&self, answer: bool) {
        self.inner.answering.store(answer, Ordering::Relaxed);
    }

    /// Announces this device: every other device on the network registers
    /// with our HTTP server in response. Returns after the whole burst
    /// (a few seconds) or immediately when multicast is unavailable.
    pub async fn announce(&self) {
        if self.inner.sockets.is_empty() {
            return;
        }
        let announcement = MulticastAnnouncement {
            info: self.inner.device.clone(),
            announce: true,
        };
        let payload = match serde_json::to_vec(&announcement) {
            Ok(payload) => payload,
            Err(err) => {
                tracing::error!("could not serialise the announcement: {err}");
                return;
            }
        };
        for delay in ANNOUNCE_DELAYS {
            tokio::select! {
                _ = self.cancel.cancelled() => return,
                _ = tokio::time::sleep(delay) => {}
            }
            tracing::debug!("announcing via multicast");
            for socket in &self.inner.sockets {
                if let Err(err) = socket.socket.send_to(&payload, socket.target).await {
                    tracing::warn!(
                        "could not announce on interface {}: {err}",
                        socket.interface
                    );
                }
            }
        }
    }

    /// Registers with a known address. `Ok(None)` when the address answered
    /// with our own fingerprint.
    pub async fn probe(&self, target: &Target) -> Result<Option<Device>, ClientError> {
        let client = self.inner.unpinned_client()?;
        self.inner.probe_with(&client, target).await
    }

    /// Probes many addresses concurrently; unreachable ones are skipped.
    pub async fn probe_many(&self, targets: Vec<Target>) -> Vec<Device> {
        let Ok(client) = self.inner.unpinned_client() else {
            return Vec::new();
        };
        let inner = &self.inner;
        let client = &client;
        futures_util::stream::iter(targets)
            .map(|target| async move { inner.probe_with(client, &target).await.ok().flatten() })
            .buffer_unordered(SCAN_CONCURRENCY)
            .filter_map(std::future::ready)
            .collect()
            .await
    }

    /// Sends a register request to every other host of the `/24` subnet of
    /// `interface`, for networks without multicast. At most one scan runs
    /// per interface at a time.
    pub async fn scan_subnet(
        &self,
        interface: Ipv4Addr,
        port: u16,
        protocol: ProtocolType,
    ) -> Vec<Device> {
        if !self.inner.scanning.lock().insert(interface) {
            return Vec::new();
        }
        let _guard = ScanGuard {
            inner: &self.inner,
            interface,
        };
        let base = interface.octets();
        let targets: Vec<Target> = (0..=255u8)
            .map(|host| Ipv4Addr::new(base[0], base[1], base[2], host))
            .filter(|ip| *ip != interface)
            .map(|ip| Target {
                host: ip.to_string(),
                port,
                protocol,
            })
            .collect();
        tracing::info!("scanning subnet of {interface}/24 on port {port}");
        self.probe_many(targets).await
    }

    /// Cheapest first: announce and probe `known` addresses; when nothing
    /// was confirmed within `grace` afterwards, scan the local `/24`
    /// subnets on the protocol's default port and, when different, on our
    /// own port. Returns once every stage finished.
    pub async fn discover_staged(&self, known: Vec<Target>, grace: Duration) {
        let before = self.inner.confirmations.load(Ordering::Relaxed);
        let mut ports = vec![crate::protocol::DEFAULT_PORT];
        if self.inner.device.port != crate::protocol::DEFAULT_PORT {
            ports.push(self.inner.device.port);
        }
        let escalate = async {
            self.probe_many(known).await;
            tokio::time::sleep(grace).await;
            if self.inner.confirmations.load(Ordering::Relaxed) == before {
                for interface in multicast::local_ipv4_addresses() {
                    for port in &ports {
                        self.scan_subnet(interface, *port, ProtocolType::Https)
                            .await;
                    }
                }
            }
        };
        tokio::join!(self.announce(), escalate);
    }

    /// The non-loopback IPv4 addresses of this machine.
    pub fn local_ipv4_addresses() -> Vec<Ipv4Addr> {
        multicast::local_ipv4_addresses()
    }

    /// Stops listening and releases the sockets.
    pub async fn stop(&self) {
        self.cancel.cancel();
        self.tasks.close();
        self.tasks.wait().await;
    }
}

struct ScanGuard<'a> {
    inner: &'a Inner,
    interface: Ipv4Addr,
}

impl Drop for ScanGuard<'_> {
    fn drop(&mut self) {
        self.inner.scanning.lock().remove(&self.interface);
    }
}

impl Inner {
    /// A client accepting any valid certificate, for peers whose fingerprint
    /// is not known yet. The identity is read off the handshake.
    fn unpinned_client(&self) -> Result<Client, ClientError> {
        Client::new(&self.identity, None, Some(self.probe_timeout))
    }

    async fn probe_with(
        &self,
        client: &Client,
        target: &Target,
    ) -> Result<Option<Device>, ClientError> {
        let registered = client.register(target, &self.device).await?;
        let fingerprint = registered.identity();
        if fingerprint.matches(&self.device.fingerprint) {
            return Ok(None);
        }
        let device = Device::from_peer_info(target, &registered.response, fingerprint);
        Ok(Some(self.confirm(device).1))
    }

    fn confirm(&self, device: Device) -> (bool, Device) {
        self.confirmations.fetch_add(1, Ordering::Relaxed);
        let (is_new, merged) = self.store.upsert(device);
        let event = if is_new {
            DiscoveryEvent::Found(merged.clone())
        } else {
            DiscoveryEvent::Updated(merged.clone())
        };
        // No receivers is fine: the store is the source of truth.
        let _ = self.events.send(event);
        (is_new, merged)
    }

    /// Whether an announcement should be answered now, remembering it.
    fn should_answer(&self, key: String) -> bool {
        let now = Instant::now();
        let mut recent = self.recently_answered.lock();
        recent.retain(|_, at| now.duration_since(*at) < ANSWER_DEDUP_WINDOW);
        if recent.contains_key(&key) {
            return false;
        }
        recent.insert(key, now);
        true
    }
}

async fn receive_loop(
    socket: Arc<UdpSocket>,
    interface: Ipv4Addr,
    inner: Arc<Inner>,
    cancel: CancellationToken,
) {
    let mut buffer = vec![0u8; RECEIVE_BUFFER];
    let mut errors = 0u32;
    loop {
        let received = tokio::select! {
            _ = cancel.cancelled() => return,
            received = socket.recv_from(&mut buffer) => received,
        };
        let (len, source) = match received {
            Ok(received) => {
                errors = 0;
                received
            }
            Err(err) => {
                errors += 1;
                tracing::warn!("multicast receive error on {interface}: {err}");
                if errors >= MAX_CONSECUTIVE_RECEIVE_ERRORS {
                    tracing::error!("giving up on multicast interface {interface}");
                    return;
                }
                continue;
            }
        };
        let announcement: MulticastAnnouncement = match serde_json::from_slice(&buffer[..len]) {
            Ok(announcement) => announcement,
            Err(err) => {
                tracing::debug!("ignoring unparsable multicast datagram from {source}: {err}");
                continue;
            }
        };
        // Loopback is on, so our own announcements come back as well.
        if inner
            .device
            .fingerprint
            .eq_ignore_ascii_case(&announcement.info.fingerprint)
        {
            continue;
        }
        if !inner.answering.load(Ordering::Relaxed) {
            continue;
        }
        let key = format!(
            "{}|{}|{}",
            announcement.info.fingerprint.to_ascii_uppercase(),
            source.ip(),
            announcement.info.port
        );
        if !inner.should_answer(key) {
            continue;
        }
        tokio::spawn(answer_announcement(
            inner.clone(),
            source.ip(),
            announcement,
        ));
    }
}

/// Answers an announcement with a register request, as the protocol
/// requires. The device enters the store only once that succeeded.
async fn answer_announcement(
    inner: Arc<Inner>,
    source: IpAddr,
    announcement: MulticastAnnouncement,
) {
    let info = announcement.info;
    let target = Target {
        host: source.to_string(),
        port: info.port,
        protocol: info.protocol,
    };
    // Pin the claimed fingerprint so nothing is sent to a device that does
    // not hold the matching certificate.
    let pinned = match info.protocol {
        ProtocolType::Https => Some(Fingerprint::parse(&info.fingerprint)),
        ProtocolType::Http => None,
    };
    let client = match Client::new(&inner.identity, pinned, Some(inner.probe_timeout)) {
        Ok(client) => client,
        Err(err) => {
            tracing::error!("could not create the client to answer {target}: {err}");
            return;
        }
    };
    match client.register(&target, &inner.device).await {
        Ok(registered) => {
            let device = Device::from_peer_info(
                &target,
                &registered.response,
                Fingerprint::parse(&info.fingerprint),
            );
            inner.confirm(device);
        }
        Err(err) => {
            tracing::debug!(
                "could not register with announcing device {} at {target}: {err}",
                info.alias
            );
        }
    }
}
