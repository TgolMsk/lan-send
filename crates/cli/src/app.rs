//! Shared setup: paths, settings, database, identity, server and discovery.

use crate::{ClientCerts, Globals};
use anyhow::Context;
use lan_send_core::discovery::{Device, Discovery, DiscoveryConfig, DiscoveryEvent};
use lan_send_core::protocol::{DeviceInfo, DeviceType, Extensions, PROTOCOL_VERSION, ProtocolType};
use lan_send_core::store::{AppPaths, Database, KnownDevice, Settings, TransferRecord};
use lan_send_core::transport::{
    ClientCertPolicy, Identity, ServerConfig, ServerEvent, ServerHandle, Target, UploadDecision,
    server,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

/// Devices seen within this window are probed at startup, like favorites.
const RECENT_DEVICE_WINDOW: Duration = Duration::from_secs(7 * 24 * 3600);

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
            ext: Some(Extensions::current(Vec::<String>::new())),
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
            events,
        })
        .await?;
        Ok(handle)
    }

    /// Starts discovery and keeps the database updated with every device
    /// it confirms.
    pub fn start_discovery(&self, port: u16) -> Discovery {
        let discovery = Discovery::start(DiscoveryConfig::new(
            self.identity.clone(),
            self.device_info(port),
        ));
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
                discovery.add_confirmed(Device::from_info(peer.addr, &info, fingerprint));
            }
            ServerEvent::PrepareUpload { decision, .. } => {
                let _ = decision.send(UploadDecision::Decline);
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
