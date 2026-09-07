//! Shared setup: paths, settings, database, identity, server and discovery.

use crate::{ClientCerts, Globals};
use anyhow::Context;
use lan_send_core::clipboard::ClipboardItem;
use lan_send_core::clipboard::PeerTarget;
use lan_send_core::discovery::{Device, Discovery, DiscoveryConfig, DiscoveryEvent};
use lan_send_core::protocol::{
    DeviceInfo, DeviceType, Extensions, FEATURE_CLIPBOARD, FEATURE_PAIRING, FEATURE_RESUME,
    Fingerprint, PROTOCOL_VERSION, ProtocolType,
};
use lan_send_core::store::{
    AppPaths, ClipboardRecord, Database, KnownDevice, Settings, TransferRecord,
};
use lan_send_core::transport::{
    ClientCertPolicy, Identity, Peer, ServerConfig, ServerEvent, ServerHandle, Target,
    UploadDecision, server,
};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

/// Devices seen within this window are probed at startup, like favorites.
const RECENT_DEVICE_WINDOW: Duration = Duration::from_secs(7 * 24 * 3600);
/// Partial uploads older than this are discarded (ADR-0008).
pub const RESUME_WINDOW: Duration = Duration::from_secs(24 * 3600);

pub struct App {
    pub paths: AppPaths,
    pub settings: Settings,
    pub db: Arc<Database>,
    pub identity: Arc<Identity>,
    pub alias: String,
    pub port: u16,
    pub client_cert_policy: ClientCertPolicy,
}

impl App {
    pub fn load(globals: &Globals) -> anyhow::Result<Self> {
        let paths = match &globals.config_dir {
            Some(dir) => AppPaths::under(dir),
            None => AppPaths::resolve()?,
        };
        paths.ensure_dirs()?;
        let settings = Settings::load_or_create(&paths.settings_file())?;
        let db = Arc::new(
            Database::open(&paths.database_file())
                .with_context(|| format!("cannot open {}", paths.database_file().display()))?,
        );
        let identity = Arc::new(Identity::load_or_generate(&paths.identity_file())?);
        let alias = globals
            .alias
            .clone()
            .or_else(|| settings.alias.clone())
            .map(|alias| alias.trim().to_string())
            .filter(|alias| !alias.is_empty())
            .unwrap_or_else(default_alias);
        let client_cert_policy = match globals.client_certs {
            Some(ClientCerts::Required) => ClientCertPolicy::Required,
            Some(ClientCerts::Optional) => ClientCertPolicy::Optional,
            None if settings.require_client_certs => ClientCertPolicy::Required,
            None => ClientCertPolicy::Optional,
        };
        Ok(Self {
            port: globals.port.unwrap_or(settings.port),
            paths,
            settings,
            db,
            identity,
            alias,
            client_cert_policy,
        })
    }

    /// This device as announced to peers, listening on `port`.
    pub fn device_info(&self, port: u16) -> DeviceInfo {
        DeviceInfo {
            alias: self.alias.clone(),
            version: PROTOCOL_VERSION.to_string(),
            device_model: Some(os_name().to_string()),
            device_type: Some(DeviceType::Headless),
            fingerprint: self.identity.fingerprint().to_string(),
            port,
            protocol: ProtocolType::Https,
            download: false,
            ext: Some(Extensions::current(self.features())),
        }
    }

    /// Extension features this device announces.
    pub fn features(&self) -> Vec<&'static str> {
        let mut features = vec![FEATURE_PAIRING];
        if self.settings.resume {
            features.push(FEATURE_RESUME);
        }
        if lan_send_core::clipboard::platform_backend().is_some() {
            features.push(FEATURE_CLIPBOARD);
        }
        features
    }

    /// Paired devices as push targets, at their last known addresses.
    pub fn clipboard_peers(&self, only: &[String]) -> anyhow::Result<Vec<PeerTarget>> {
        let devices = self.db.list_devices()?;
        let mut peers = Vec::new();
        for device in devices.iter().filter(|device| device.paired) {
            let (Some(host), Some(port)) = (device.host.clone(), device.port) else {
                continue;
            };
            peers.push(PeerTarget {
                fingerprint: Fingerprint::parse(&device.fingerprint),
                alias: device.display_name().to_string(),
                target: Target {
                    host,
                    port,
                    protocol: ProtocolType::Https,
                },
            });
        }
        if !only.is_empty() {
            let mut selected = Vec::new();
            for query in only {
                let query = query.trim();
                let found = peers.iter().find(|peer| {
                    peer.alias.eq_ignore_ascii_case(query)
                        || (query.len() >= 4 && peer.fingerprint.has_prefix(query))
                });
                match found {
                    Some(peer) => selected.push(peer.clone()),
                    None => anyhow::bail!(
                        "'{query}' is not a paired device (run `lan-send pair` first)"
                    ),
                }
            }
            peers = selected;
        }
        Ok(peers)
    }

    /// Fingerprints of the devices paired so far.
    pub fn paired_fingerprints(&self) -> HashSet<Fingerprint> {
        self.db
            .list_devices()
            .unwrap_or_default()
            .into_iter()
            .filter(|device| device.paired)
            .map(|device| Fingerprint::parse(&device.fingerprint))
            .collect()
    }

    /// Persists a pairing decision for `peer`, creating the device record
    /// when it is not known yet.
    pub fn apply_pairing(&self, peer: &Peer, alias: &str, paired: bool) {
        let Some(fingerprint) = &peer.cert_fingerprint else {
            return;
        };
        let known = self
            .db
            .device(fingerprint.as_str())
            .ok()
            .flatten()
            .is_some();
        if !known {
            let now = lan_send_core::store::unix_now();
            let _ = self.db.upsert_device(&KnownDevice {
                fingerprint: fingerprint.to_string(),
                alias: alias.to_string(),
                custom_alias: None,
                device_type: None,
                device_model: None,
                version: None,
                host: Some(peer.host()),
                port: None,
                protocol: Some("https".into()),
                favorite: false,
                paired: false,
                first_seen: now,
                last_seen: now,
            });
        }
        if let Err(err) = self.db.set_paired(fingerprint.as_str(), paired) {
            tracing::warn!("could not store the pairing: {err}");
        }
    }

    /// Answers a pairing request: asks the user (or accepts when
    /// `auto_accept`), persists the result and updates the server's set.
    pub async fn answer_pair_request(
        &self,
        server: &ServerHandle,
        auto_accept: bool,
        peer: &Peer,
        alias: &str,
        code: &str,
        decision: tokio::sync::oneshot::Sender<bool>,
    ) {
        let short = peer
            .cert_fingerprint
            .as_ref()
            .map(|fingerprint| fingerprint.short().to_string())
            .unwrap_or_default();
        println!("\nPairing request from {alias} ({short}) at {}.", peer.addr);
        println!("Verification code: {code}");
        let accept = auto_accept
            || crate::ui::prompt_line("Does the other device show the same code? [y/N] ")
                .await
                .map(|answer| matches!(answer.to_ascii_lowercase().as_str(), "y" | "yes"))
                .unwrap_or(false);
        if accept {
            self.apply_pairing(peer, alias, true);
            if let Some(fingerprint) = &peer.cert_fingerprint {
                server.add_paired(fingerprint.clone());
            }
            println!("Paired with {alias}.");
        } else {
            println!("Pairing with {alias} declined.");
        }
        let _ = decision.send(accept);
    }

    /// Keeps a clipboard item in the history unless it is (or looks) secret.
    /// Image bytes go to the cache directory. Returns whether it was stored.
    pub fn record_clipboard(&self, item: &ClipboardItem, sensitive: bool) -> bool {
        use lan_send_core::clipboard::{ClipboardPayload, sensitive::looks_sensitive};

        let settings = &self.settings.clipboard;
        let mut record = ClipboardRecord {
            id: item.id.clone(),
            origin: item.origin_device.to_string(),
            created_at: item.created_at,
            kind: item.payload.kind().to_string(),
            content_hash: item.hash_hex(),
            size: item.payload.size() as u64,
            text: None,
            html: None,
            rtf: None,
            image_format: None,
            image_width: None,
            image_height: None,
            image_path: None,
            file_paths: Vec::new(),
        };
        match &item.payload {
            ClipboardPayload::Text { plain, html, rtf } => {
                if sensitive || settings.never_store_text || looks_sensitive(plain) {
                    return false;
                }
                record.text = Some(plain.clone());
                record.html = html.clone();
                record.rtf = rtf.clone();
            }
            ClipboardPayload::Image {
                format,
                bytes,
                width,
                height,
            } => {
                let dir = self.paths.cache_dir.join("clipboard");
                let path = dir.join(format!("{}.{}", item.id, format.extension()));
                if let Err(err) =
                    std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, bytes))
                {
                    tracing::warn!("could not store the clipboard image: {err}");
                    return false;
                }
                record.image_format = Some(format.extension().to_string());
                record.image_width = Some(*width);
                record.image_height = Some(*height);
                record.image_path = Some(path);
            }
            ClipboardPayload::Files { paths } => {
                record.file_paths = paths
                    .iter()
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect();
            }
        }
        if let Err(err) = self.db.record_clipboard(&record) {
            tracing::warn!("could not record the clipboard item: {err}");
            return false;
        }
        match self.db.prune_clipboard(settings.history_limit) {
            Ok(pruned) => {
                for old in pruned {
                    if let Some(path) = old.image_path {
                        let _ = std::fs::remove_file(path);
                    }
                }
            }
            Err(err) => tracing::warn!("could not prune the clipboard history: {err}"),
        }
        true
    }

    /// Removes partial uploads older than the resume window, files included.
    pub fn expire_partials(&self) {
        match self.db.expire_partials(RESUME_WINDOW) {
            Ok(expired) => {
                for partial in expired {
                    let _ = std::fs::remove_file(&partial.part_path);
                }
            }
            Err(err) => tracing::warn!("could not expire partial uploads: {err}"),
        }
    }

    pub async fn start_server(
        &self,
        pin: Option<String>,
        verify_checksums: bool,
        events: mpsc::Sender<ServerEvent>,
    ) -> anyhow::Result<ServerHandle> {
        let handle = server::start(ServerConfig {
            port: self.port,
            identity: self.identity.clone(),
            client_cert_policy: self.client_cert_policy,
            device: self.device_info(self.port),
            pin,
            verify_checksums,
            upload_idle_timeout: server::DEFAULT_UPLOAD_IDLE_TIMEOUT,
            ipv6: self.settings.ipv6,
            paired: self.paired_fingerprints(),
            clipboard_limits: server::ClipboardLimits {
                text: self.settings.clipboard.text_limit,
                image: self.settings.clipboard.image_limit,
            },
            events,
        })
        .await?;
        Ok(handle)
    }

    /// Starts discovery and keeps the database updated with every device
    /// it confirms.
    pub fn start_discovery(&self, port: u16) -> Discovery {
        let mut config = DiscoveryConfig::new(self.identity.clone(), self.device_info(port));
        if !self.settings.ipv6 {
            config.group_v6 = None;
        }
        let discovery = Discovery::start(config);
        let mut events = discovery.subscribe();
        let db = self.db.clone();
        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(DiscoveryEvent::Found(device) | DiscoveryEvent::Updated(device)) => {
                        if let Err(err) = db.upsert_device(&KnownDevice::from(&device)) {
                            tracing::warn!("could not remember {}: {err}", device.alias);
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        discovery
    }

    /// Addresses worth probing at startup: favorites and recently seen
    /// devices.
    pub fn known_targets(&self) -> Vec<Target> {
        self.db
            .devices_to_probe(RECENT_DEVICE_WINDOW)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|device| {
                Some(Target {
                    host: device.host?,
                    port: device.port?,
                    protocol: match device.protocol.as_deref() {
                        Some("http") => ProtocolType::Http,
                        _ => ProtocolType::Https,
                    },
                })
            })
            .collect()
    }

    /// Writes a history entry and keeps the history within its limit.
    pub fn record_transfer(&self, record: &TransferRecord) {
        if let Err(err) = self.db.record_transfer(record) {
            tracing::warn!("could not record transfer of {}: {err}", record.file_name);
        }
        if let Err(err) = self.db.prune_transfers(self.settings.history_limit) {
            tracing::warn!("could not prune the history: {err}");
        }
    }

    /// Feeds registrations into discovery and declines transfer requests;
    /// what every command does with server events it is not interested in.
    pub fn handle_background_event(&self, discovery: &Discovery, event: ServerEvent) {
        match event {
            ServerEvent::Register { peer, info } => {
                let fingerprint = peer.identity(&info.fingerprint);
                discovery.add_confirmed(Device::from_info(&peer.host(), &info, fingerprint));
            }
            ServerEvent::PrepareUpload { decision, .. } => {
                let _ = decision.send(UploadDecision::Decline);
            }
            ServerEvent::PairRequest {
                decision, alias, ..
            } => {
                eprintln!("Pairing request from {alias} declined (not in receive mode).");
                let _ = decision.send(false);
            }
            ServerEvent::Unpaired { peer } => {
                self.apply_pairing(&peer, "", false);
                eprintln!("{} withdrew the pairing.", peer.addr);
            }
            _ => {}
        }
    }
}

pub fn default_alias() -> String {
    let host = gethostname::gethostname().to_string_lossy().into_owned();
    let host = host.trim_end_matches(".local").trim();
    if host.is_empty() {
        "lan-send".to_string()
    } else {
        host.to_string()
    }
}

pub fn os_name() -> &'static str {
    match std::env::consts::OS {
        "macos" => "macOS",
        "windows" => "Windows",
        "ios" => "iOS",
        "linux" => "Linux",
        other => other,
    }
}

pub fn init_logging(verbose: u8) {
    let default = match verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}
