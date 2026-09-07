//! Shared setup: paths, identity, device information, server and discovery.

use crate::{ClientCerts, Globals};
use lan_send_core::discovery::{Device, Discovery, DiscoveryConfig};
use lan_send_core::protocol::{
    DEFAULT_PORT, DeviceInfo, DeviceType, Extensions, PROTOCOL_VERSION, ProtocolType,
};
use lan_send_core::store::AppPaths;
use lan_send_core::transport::{
    ClientCertPolicy, Identity, ServerConfig, ServerEvent, ServerHandle, UploadDecision, server,
};
use std::sync::Arc;
use tokio::sync::mpsc;

pub struct App {
    pub paths: AppPaths,
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
        let identity = Arc::new(Identity::load_or_generate(&paths.identity_file())?);
        let alias = globals
            .alias
            .clone()
            .map(|alias| alias.trim().to_string())
            .filter(|alias| !alias.is_empty())
            .unwrap_or_else(default_alias);
        Ok(Self {
            paths,
            identity,
            alias,
            port: globals.port.unwrap_or(DEFAULT_PORT),
            client_cert_policy: match globals.client_certs {
                ClientCerts::Required => ClientCertPolicy::Required,
                ClientCerts::Optional => ClientCertPolicy::Optional,
            },
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

    pub fn start_discovery(&self, port: u16) -> Discovery {
        Discovery::start(DiscoveryConfig::new(
            self.identity.clone(),
            self.device_info(port),
        ))
    }
}

/// Feeds registrations into discovery and declines transfer requests; what
/// every command does with server events it is not interested in.
pub fn handle_background_event(discovery: &Discovery, event: ServerEvent) {
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
