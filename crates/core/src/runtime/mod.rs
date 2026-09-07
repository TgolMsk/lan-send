//! Application runtime (ADR-0013): everything a user interface needs, as
//! commands in and [`RuntimeEvent`]s out. It owns the HTTPS server,
//! discovery, the optional clipboard sync and the database, and turns each
//! decision a user has to make (accept a transfer, confirm a pairing code,
//! enter a PIN, resolve a file conflict) into an event plus an answering
//! method. Nothing here prints or depends on a terminal or a window system.

pub mod events;

mod clipboard;
mod devices;
mod incoming;
mod outgoing;
mod pairing;

pub use events::*;
pub use outgoing::SendRequest;

use crate::clipboard::{ClipboardError, ClipboardSync, platform_backend};
use crate::discovery::{Discovery, DiscoveryConfig};
use crate::protocol::{
    DeviceInfo, DeviceType, Extensions, FEATURE_CLIPBOARD, FEATURE_PAIRING, FEATURE_RESUME,
    Fingerprint, PROTOCOL_VERSION, ProtocolType,
};
use crate::store::{
    AppPaths, Database, KnownDevice, Settings, StoreError, TransferRecord, unix_now,
};
use crate::transfer::CollectError;
use crate::transport::server::{ClipboardLimits, DEFAULT_UPLOAD_IDLE_TIMEOUT};
use crate::transport::{
    ClientCertPolicy, ClientError, Identity, IdentityError, ServerConfig, ServerError,
    ServerHandle, server,
};
use parking_lot::{Mutex, RwLock};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

/// Partial uploads older than this are discarded (ADR-0008).
pub const RESUME_WINDOW: Duration = Duration::from_secs(24 * 3600);
/// Devices seen within this window are probed at startup, like favourites.
const RECENT_DEVICE_WINDOW: Duration = Duration::from_secs(7 * 24 * 3600);
/// Progress events per file are spaced at least this far apart.
pub(crate) const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);
/// Finished transfers stay in [`Runtime::transfers`] this long.
const FINISHED_RETENTION: Duration = Duration::from_secs(3600);
/// How many finished transfers are kept in the list at most.
const FINISHED_KEPT: usize = 50;

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Identity(#[from] IdentityError),
    #[error(transparent)]
    Server(#[from] ServerError),
    #[error(transparent)]
    Client(#[from] ClientError),
    #[error(transparent)]
    Collect(#[from] CollectError),
    #[error(transparent)]
    Clipboard(#[from] ClipboardError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("device not found: {0}")]
    DeviceNotFound(String),
    #[error("{0}")]
    Invalid(String),
    #[error("nothing is waiting for this answer")]
    NothingPending,
    #[error("the clipboard is not supported on this platform")]
    NoClipboard,
}

/// How to start a [`Runtime`].
pub struct RuntimeConfig {
    pub paths: AppPaths,
    /// `Desktop` / `Mobile` for the app, `Headless` for the CLI.
    pub device_type: DeviceType,
    /// Overrides `settings.alias`.
    pub alias: Option<String>,
    /// Overrides `settings.port`.
    pub port: Option<u16>,
    /// Overrides `settings.require_client_certs`.
    pub client_cert_policy: Option<ClientCertPolicy>,
    /// Where events go. A slow consumer drops progress events first.
    pub events: mpsc::Sender<RuntimeEvent>,
    /// Probe known addresses and scan the subnets at start and on
    /// `refresh_devices`; tests turn it off.
    pub active_discovery: bool,
}

impl RuntimeConfig {
    pub fn new(
        paths: AppPaths,
        device_type: DeviceType,
        events: mpsc::Sender<RuntimeEvent>,
    ) -> Self {
        Self {
            paths,
            device_type,
            alias: None,
            port: None,
            client_cert_policy: None,
            events,
            active_discovery: true,
        }
    }
}

/// A transfer the runtime is tracking, in either direction.
pub(crate) struct ActiveTransfer {
    pub view: TransferView,
    pub cancel: CancellationToken,
    /// The peer's address, to match `CancelReceived` events.
    pub peer_host: Option<String>,
    last_emit: HashMap<String, Instant>,
    finished: Option<Instant>,
}

impl ActiveTransfer {
    fn new(view: TransferView, peer_host: Option<String>) -> Self {
        Self {
            view,
            cancel: CancellationToken::new(),
            peer_host,
            last_emit: HashMap::new(),
            finished: None,
        }
    }
}

/// Decisions waiting for the user.
#[derive(Default)]
pub(crate) struct Pending {
    pub incoming: Option<incoming::PendingIncoming>,
    pub conflicts: HashMap<(String, String), oneshot::Sender<bool>>,
    pub pins: HashMap<String, oneshot::Sender<Option<String>>>,
    pub pair_requests: HashMap<String, pairing::PendingPairRequest>,
    pub pair_confirms: HashMap<String, pairing::PendingPairConfirm>,
}

pub(crate) struct Inner {
    pub paths: AppPaths,
    pub settings: RwLock<Settings>,
    pub db: Arc<Database>,
    pub identity: Arc<Identity>,
    pub alias: String,
    pub device_type: DeviceType,
    /// The port the server actually listens on.
    pub port: u16,
    pub server: ServerHandle,
    pub discovery: Discovery,
    events: mpsc::Sender<RuntimeEvent>,
    pub transfers: Mutex<HashMap<String, ActiveTransfer>>,
    pub pending: Mutex<Pending>,
    pub incoming: Mutex<Option<incoming::SessionInfo>>,
    /// Devices discovery still lists but that stopped answering probes.
    pub offline: Mutex<HashSet<Fingerprint>>,
    pub clipboard: Mutex<Option<ClipboardSync>>,
    pub last_views: Mutex<HashMap<String, DeviceView>>,
    pub active_discovery: bool,
    pub cancel: CancellationToken,
    pub tasks: TaskTracker,
}

/// A running application. Cheap to clone; all clones share one state.
#[derive(Clone)]
pub struct Runtime {
    inner: Arc<Inner>,
}

impl Runtime {
    /// Loads settings, database and identity, starts the server, discovery
    /// and (when enabled) the clipboard sync.
    pub async fn start(config: RuntimeConfig) -> Result<Self, RuntimeError> {
        let RuntimeConfig {
            paths,
            device_type,
            alias,
            port,
            client_cert_policy,
            events,
            active_discovery,
        } = config;
        paths.ensure_dirs()?;
        let settings = Settings::load_or_create(&paths.settings_file())?;
        let db = Arc::new(Database::open(&paths.database_file())?);
        let identity = Arc::new(Identity::load_or_generate(&paths.identity_file())?);
        let alias = alias
            .or_else(|| settings.alias.clone())
            .map(|alias| alias.trim().to_string())
            .filter(|alias| !alias.is_empty())
            .unwrap_or_else(default_alias);
        let client_cert_policy = client_cert_policy.unwrap_or(if settings.require_client_certs {
            ClientCertPolicy::Required
        } else {
            ClientCertPolicy::Optional
        });
        let port = port.unwrap_or(settings.port);
        let features = features_for(&settings);
        let (server_tx, server_rx) = mpsc::channel(256);
        let server = server::start(ServerConfig {
            port,
            identity: identity.clone(),
            client_cert_policy,
            device: device_info(&alias, &identity, device_type, port, &features),
            pin: settings.pin.clone(),
            verify_checksums: settings.verify_checksums,
            upload_idle_timeout: DEFAULT_UPLOAD_IDLE_TIMEOUT,
            ipv6: settings.ipv6,
            paired: paired_fingerprints(&db),
            clipboard_limits: ClipboardLimits {
                text: settings.clipboard.text_limit,
                image: settings.clipboard.image_limit,
            },
            events: server_tx,
        })
        .await?;
        let port = server.port();
        let mut discovery_config = DiscoveryConfig::new(
            identity.clone(),
            device_info(&alias, &identity, device_type, port, &features),
        );
        if !settings.ipv6 {
            discovery_config.group_v6 = None;
        }
        let discovery = Discovery::start(discovery_config);
        let discovery_events = discovery.subscribe();
        let sync_enabled = settings.clipboard.sync_enabled;

        let inner = Arc::new(Inner {
            paths,
            settings: RwLock::new(settings),
            db,
            identity,
            alias,
            device_type,
            port,
            server,
            discovery,
            events,
            transfers: Mutex::new(HashMap::new()),
            pending: Mutex::new(Pending::default()),
            incoming: Mutex::new(None),
            offline: Mutex::new(HashSet::new()),
            clipboard: Mutex::new(None),
            last_views: Mutex::new(HashMap::new()),
            active_discovery,
            cancel: CancellationToken::new(),
            tasks: TaskTracker::new(),
        });
        inner.tasks.spawn(incoming::run(inner.clone(), server_rx));
        inner
            .tasks
            .spawn(devices::forward_discovery(inner.clone(), discovery_events));
        inner.tasks.spawn(devices::liveness(inner.clone()));
        inner.expire_partials();
        inner.kick_discovery();
        if sync_enabled {
            inner.start_clipboard_sync();
        }
        Ok(Self { inner })
    }

    /// Stops everything. Transfers in progress are cancelled.
    pub async fn stop(&self) {
        let inner = &self.inner;
        inner.cancel.cancel();
        for transfer in inner.transfers.lock().values() {
            transfer.cancel.cancel();
        }
        inner.stop_clipboard_sync();
        inner.server.stop().await;
        inner.discovery.stop().await;
        inner.tasks.close();
        let _ = tokio::time::timeout(Duration::from_secs(5), inner.tasks.wait()).await;
    }

    // ----- identity and settings -------------------------------------------------

    pub fn identity(&self) -> IdentityView {
        let inner = &self.inner;
        let settings = inner.settings();
        IdentityView {
            alias: inner.alias.clone(),
            fingerprint: inner.identity.fingerprint().to_string(),
            port: inner.port,
            device_type: inner.device_type.as_str().to_string(),
            device_model: os_name().to_string(),
            config_dir: inner.paths.config_dir.clone(),
            receive_dir: settings
                .receive_dir
                .clone()
                .or_else(|| inner.paths.download_dir.clone()),
            clipboard_supported: platform_backend().is_some(),
            features: inner.features().into_iter().map(String::from).collect(),
            multicast_error: inner.discovery.multicast_error().map(String::from),
        }
    }

    pub fn settings(&self) -> Settings {
        self.inner.settings()
    }

    /// Saves `settings` and applies what can be applied live. Returns
    /// whether the runtime must be restarted for the rest (port, alias,
    /// PIN, certificate policy, IPv6, resume).
    pub fn update_settings(&self, settings: Settings) -> Result<bool, RuntimeError> {
        let inner = &self.inner;
        settings.save(&inner.paths.settings_file())?;
        let previous = inner.settings();
        let restart = previous.network_differs(&settings);
        *inner.settings.write() = settings.clone();
        if previous.clipboard != settings.clipboard {
            inner.stop_clipboard_sync();
            if settings.clipboard.sync_enabled {
                inner.start_clipboard_sync();
            }
        }
        if previous.history_limit != settings.history_limit {
            let _ = inner.db.prune_transfers(settings.history_limit);
        }
        Ok(restart)
    }

    // ----- devices ---------------------------------------------------------------

    /// Every device seen live or remembered, excluding this one.
    pub fn devices(&self) -> Vec<DeviceView> {
        self.inner.device_views()
    }

    /// Announces this device again and probes known addresses; when that
    /// yields nothing, scans the local subnets.
    pub fn refresh_devices(&self) {
        self.inner.kick_discovery();
    }

    pub fn set_favorite(&self, fingerprint: &str, favorite: bool) -> Result<(), RuntimeError> {
        self.inner.db.set_favorite(fingerprint, favorite)?;
        self.inner.emit_device_changed(fingerprint);
        Ok(())
    }

    pub fn set_custom_alias(
        &self,
        fingerprint: &str,
        alias: Option<String>,
    ) -> Result<(), RuntimeError> {
        let alias = alias
            .map(|a| a.trim().to_string())
            .filter(|a| !a.is_empty());
        self.inner
            .db
            .set_custom_alias(fingerprint, alias.as_deref())?;
        self.inner.emit_device_changed(fingerprint);
        Ok(())
    }

    /// Forgets a device: unpairs it and removes it from the database. It
    /// reappears (unpaired) when discovery sees it again.
    pub async fn forget_device(&self, fingerprint: &str) -> Result<(), RuntimeError> {
        let _ = self.inner.unpair(fingerprint).await;
        self.inner.db.remove_device(fingerprint)?;
        self.inner.emit(RuntimeEvent::DeviceLost {
            fingerprint: fingerprint.to_string(),
        });
        Ok(())
    }

    // ----- transfers -------------------------------------------------------------

    /// Starts sending files. Progress and the outcome arrive as events for
    /// the returned transfer id.
    pub fn send(&self, request: SendRequest) -> Result<String, RuntimeError> {
        self.inner.start_send(request)
    }

    /// Transfers in progress plus recently finished ones.
    pub fn transfers(&self) -> Vec<TransferView> {
        let mut views: Vec<TransferView> = self
            .inner
            .transfers
            .lock()
            .values()
            .map(|transfer| transfer.view.clone())
            .collect();
        views.sort_by_key(|view| std::cmp::Reverse(view.started_at));
        views
    }

    /// Cancels a transfer in either direction.
    pub async fn cancel_transfer(&self, transfer_id: &str) -> Result<(), RuntimeError> {
        self.inner.cancel_transfer(transfer_id).await
    }

    /// Answers a `transfer-needs-pin` event; `None` gives up.
    pub fn provide_pin(&self, transfer_id: &str, pin: Option<String>) -> Result<(), RuntimeError> {
        let sender = self
            .inner
            .pending
            .lock()
            .pins
            .remove(transfer_id)
            .ok_or(RuntimeError::NothingPending)?;
        let _ = sender.send(pin.map(|p| p.trim().to_string()).filter(|p| !p.is_empty()));
        Ok(())
    }

    /// Drops a finished transfer from the list.
    pub fn dismiss_transfer(&self, transfer_id: &str) -> bool {
        let mut transfers = self.inner.transfers.lock();
        match transfers.get(transfer_id) {
            Some(transfer) if transfer.view.state.is_final() => {
                transfers.remove(transfer_id);
                true
            }
            _ => false,
        }
    }

    /// Answers an `incoming-request` event.
    pub fn respond_incoming(&self, session_id: &str, accept: bool) -> Result<(), RuntimeError> {
        self.inner.respond_incoming(session_id, accept)
    }

    /// Answers an `incoming-conflict` event.
    pub fn respond_conflict(
        &self,
        session_id: &str,
        file_id: &str,
        overwrite: bool,
    ) -> Result<(), RuntimeError> {
        let sender = self
            .inner
            .pending
            .lock()
            .conflicts
            .remove(&(session_id.to_string(), file_id.to_string()))
            .ok_or(RuntimeError::NothingPending)?;
        let _ = sender.send(overwrite);
        Ok(())
    }

    // ----- pairing ---------------------------------------------------------------

    /// Asks `device` (fingerprint, name or address) to pair. Both sides show
    /// the returned code; a `pair-response` event follows when the other
    /// side accepted, then `pair_confirm` finishes it.
    pub async fn pair_start(&self, device: &str) -> Result<PairView, RuntimeError> {
        self.inner.pair_start(device).await
    }

    /// Confirms (or not) that the other device shows the same code.
    pub async fn pair_confirm(&self, fingerprint: &str, matches: bool) -> Result<(), RuntimeError> {
        self.inner.pair_confirm(fingerprint, matches).await
    }

    /// Answers a `pair-request` event.
    pub fn respond_pair_request(
        &self,
        fingerprint: &str,
        accept: bool,
    ) -> Result<(), RuntimeError> {
        self.inner.respond_pair_request(fingerprint, accept)
    }

    pub async fn unpair(&self, fingerprint: &str) -> Result<(), RuntimeError> {
        self.inner.unpair(fingerprint).await
    }

    // ----- history ---------------------------------------------------------------

    pub fn history(&self, limit: usize) -> Result<Vec<TransferRecord>, RuntimeError> {
        Ok(self.inner.db.list_transfers(limit)?)
    }

    pub fn delete_history(&self, id: &str) -> Result<bool, RuntimeError> {
        Ok(self.inner.db.delete_transfer(id)?)
    }

    pub fn clear_history(&self) -> Result<usize, RuntimeError> {
        Ok(self.inner.db.clear_transfers()?)
    }

    // ----- clipboard -------------------------------------------------------------

    pub fn clipboard_history(&self, limit: usize) -> Result<Vec<ClipboardView>, RuntimeError> {
        self.inner.clipboard_history(limit)
    }

    /// Puts a history entry back onto the clipboard.
    pub fn clipboard_copy(&self, id: &str) -> Result<(), RuntimeError> {
        self.inner.clipboard_copy(id)
    }

    pub fn clipboard_delete(&self, id: &str) -> Result<bool, RuntimeError> {
        self.inner.clipboard_delete(id)
    }

    pub fn clipboard_clear(&self) -> Result<usize, RuntimeError> {
        self.inner.clipboard_clear()
    }

    /// Pushes the current clipboard to one paired device, or to all of them.
    pub async fn clipboard_push(
        &self,
        device: Option<&str>,
    ) -> Result<ClipboardView, RuntimeError> {
        self.inner.clipboard_push(device).await
    }

    pub fn clipboard_sync_active(&self) -> bool {
        self.inner.clipboard.lock().is_some()
    }

    /// Turns the continuous sync on or off and remembers the choice.
    pub fn set_clipboard_sync(&self, enabled: bool) -> Result<(), RuntimeError> {
        let mut settings = self.inner.settings();
        settings.clipboard.sync_enabled = enabled;
        self.update_settings(settings)?;
        Ok(())
    }
}

impl Inner {
    pub(crate) fn settings(&self) -> Settings {
        self.settings.read().clone()
    }

    /// Sends an event; when the consumer is behind, waits for it in the
    /// background rather than blocking the caller.
    pub(crate) fn emit(&self, event: RuntimeEvent) {
        match self.events.try_send(event) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(event)) => {
                if matches!(event, RuntimeEvent::TransferProgress { .. }) {
                    return;
                }
                let sender = self.events.clone();
                tokio::spawn(async move {
                    let _ = sender.send(event).await;
                });
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {}
        }
    }

    pub(crate) fn emit_error(&self, scope: &str, message: impl Into<String>) {
        self.emit(RuntimeEvent::Error {
            scope: scope.to_string(),
            message: message.into(),
        });
    }

    /// Extension features this device announces.
    pub(crate) fn features(&self) -> Vec<&'static str> {
        features_for(&self.settings.read())
    }

    /// This device as announced to peers.
    pub(crate) fn device_info(&self, port: u16) -> DeviceInfo {
        device_info(
            &self.alias,
            &self.identity,
            self.device_type,
            port,
            &self.features(),
        )
    }

    // ----- transfer bookkeeping ---------------------------------------------------

    pub(crate) fn insert_transfer(
        &self,
        view: TransferView,
        peer_host: Option<String>,
    ) -> CancellationToken {
        let mut transfers = self.transfers.lock();
        prune_finished(&mut transfers);
        let transfer = ActiveTransfer::new(view.clone(), peer_host);
        let cancel = transfer.cancel.clone();
        transfers.insert(view.id.clone(), transfer);
        drop(transfers);
        self.emit(RuntimeEvent::TransferUpdated { transfer: view });
        cancel
    }

    pub(crate) fn update_transfer<R>(
        &self,
        id: &str,
        update: impl FnOnce(&mut ActiveTransfer) -> R,
    ) -> Option<R> {
        self.transfers.lock().get_mut(id).map(update)
    }

    pub(crate) fn transfer_view(&self, id: &str) -> Option<TransferView> {
        self.transfers
            .lock()
            .get(id)
            .map(|transfer| transfer.view.clone())
    }

    /// Changes the state and tells the interface.
    pub(crate) fn set_transfer_state(&self, id: &str, state: TransferState) {
        let view = self.update_transfer(id, |transfer| {
            transfer.view.state = state;
            transfer.view.clone()
        });
        if let Some(view) = view {
            self.emit(RuntimeEvent::TransferUpdated { transfer: view });
        }
    }

    pub(crate) fn emit_transfer_updated(&self, id: &str) {
        if let Some(view) = self.transfer_view(id) {
            self.emit(RuntimeEvent::TransferUpdated { transfer: view });
        }
    }

    /// Byte progress of one file; throttled per file except at the end.
    pub(crate) fn progress(&self, transfer_id: &str, file_id: &str, done: u64) {
        let event = {
            let mut transfers = self.transfers.lock();
            let Some(transfer) = transfers.get_mut(transfer_id) else {
                return;
            };
            let Some(file) = transfer.view.file_mut(file_id) else {
                return;
            };
            file.done = done;
            if file.state == FileState::Pending {
                file.state = FileState::Active;
            }
            let size = file.size;
            let complete = done >= size;
            let now = Instant::now();
            let due = transfer
                .last_emit
                .get(file_id)
                .is_none_or(|last| now.duration_since(*last) >= PROGRESS_INTERVAL);
            if !due && !complete {
                return;
            }
            transfer.last_emit.insert(file_id.to_string(), now);
            transfer.view.recompute_totals();
            RuntimeEvent::TransferProgress {
                transfer_id: transfer_id.to_string(),
                file_id: file_id.to_string(),
                done,
                size,
                total_done: transfer.view.done_size,
                total_size: transfer.view.total_size,
            }
        };
        self.emit(event);
    }

    /// Marks one file final and tells the interface.
    pub(crate) fn finish_file(
        &self,
        transfer_id: &str,
        file_id: &str,
        state: FileState,
        path: Option<PathBuf>,
        error: Option<String>,
    ) {
        let file = self.update_transfer(transfer_id, |transfer| {
            let file = transfer.view.file_mut(file_id)?;
            file.state = state;
            if state == FileState::Finished {
                file.done = file.size;
            }
            if path.is_some() {
                file.path = path;
            }
            file.error = error;
            let file = file.clone();
            transfer.view.recompute_totals();
            Some(file)
        });
        if let Some(Some(file)) = file {
            self.emit(RuntimeEvent::TransferFileDone {
                transfer_id: transfer_id.to_string(),
                file,
            });
        }
    }

    /// Ends a transfer: files still open get a matching final state, the
    /// interface gets `transfer-completed`.
    pub(crate) fn complete_transfer(&self, id: &str, state: TransferState, error: Option<String>) {
        let view = self.update_transfer(id, |transfer| {
            if transfer.view.state.is_final() {
                return None;
            }
            transfer.view.state = state;
            transfer.view.error = error;
            transfer.view.finished_at = Some(unix_now());
            transfer.finished = Some(Instant::now());
            let file_state = match state {
                TransferState::Finished => FileState::Finished,
                TransferState::Cancelled => FileState::Cancelled,
                TransferState::Declined => FileState::Skipped,
                _ => FileState::Failed,
            };
            for file in &mut transfer.view.files {
                if matches!(file.state, FileState::Pending | FileState::Active) {
                    file.state = file_state;
                }
            }
            transfer.view.recompute_totals();
            Some(transfer.view.clone())
        });
        if let Some(Some(view)) = view {
            self.emit(RuntimeEvent::TransferCompleted { transfer: view });
        }
    }

    pub(crate) fn fail_transfer(&self, id: &str, message: impl Into<String>) {
        self.complete_transfer(id, TransferState::Failed, Some(message.into()));
    }

    pub(crate) async fn cancel_transfer(&self, id: &str) -> Result<(), RuntimeError> {
        let (direction, session_id, token) = {
            let transfers = self.transfers.lock();
            let transfer = transfers
                .get(id)
                .ok_or_else(|| RuntimeError::Invalid(format!("no transfer {id}")))?;
            (
                transfer.view.direction,
                transfer.view.session_id.clone(),
                transfer.cancel.clone(),
            )
        };
        token.cancel();
        match direction {
            crate::store::Direction::Receive => {
                if let Some(session_id) = &session_id {
                    self.server.cancel_session(session_id);
                }
                let pending = self.pending.lock().incoming.take_if(|p| p.session_id == id);
                if let Some(pending) = pending {
                    self.decline_incoming(pending);
                } else {
                    self.complete_transfer(id, TransferState::Cancelled, None);
                }
            }
            crate::store::Direction::Send => {
                // The sending task notices the token and tells the receiver.
                if let Some(sender) = self.pending.lock().pins.remove(id) {
                    let _ = sender.send(None);
                }
            }
        }
        Ok(())
    }

    /// Writes a history entry and keeps the history within its limit.
    pub(crate) fn record_transfer(&self, record: &TransferRecord) {
        if let Err(err) = self.db.record_transfer(record) {
            tracing::warn!("could not record transfer of {}: {err}", record.file_name);
        }
        let limit = self.settings.read().history_limit;
        if let Err(err) = self.db.prune_transfers(limit) {
            tracing::warn!("could not prune the history: {err}");
        }
    }

    /// Removes partial uploads older than the resume window, files included.
    pub(crate) fn expire_partials(&self) {
        match self.db.expire_partials(RESUME_WINDOW) {
            Ok(expired) => {
                for partial in expired {
                    let _ = std::fs::remove_file(&partial.part_path);
                }
            }
            Err(err) => tracing::warn!("could not expire partial uploads: {err}"),
        }
    }
}

fn prune_finished(transfers: &mut HashMap<String, ActiveTransfer>) {
    let now = Instant::now();
    transfers.retain(|_, transfer| {
        transfer
            .finished
            .is_none_or(|at| now.duration_since(at) < FINISHED_RETENTION)
    });
    let finished = transfers
        .values()
        .filter(|transfer| transfer.finished.is_some())
        .count();
    if finished > FINISHED_KEPT {
        let mut oldest: Vec<(String, Instant)> = transfers
            .iter()
            .filter_map(|(id, transfer)| transfer.finished.map(|at| (id.clone(), at)))
            .collect();
        oldest.sort_by_key(|(_, at)| *at);
        for (id, _) in oldest.into_iter().take(finished - FINISHED_KEPT) {
            transfers.remove(&id);
        }
    }
}

fn features_for(settings: &Settings) -> Vec<&'static str> {
    let mut features = vec![FEATURE_PAIRING];
    if settings.resume {
        features.push(FEATURE_RESUME);
    }
    if platform_backend().is_some() {
        features.push(FEATURE_CLIPBOARD);
    }
    features
}

fn device_info(
    alias: &str,
    identity: &Identity,
    device_type: DeviceType,
    port: u16,
    features: &[&'static str],
) -> DeviceInfo {
    DeviceInfo {
        alias: alias.to_string(),
        version: PROTOCOL_VERSION.to_string(),
        device_model: Some(os_name().to_string()),
        device_type: Some(device_type),
        fingerprint: identity.fingerprint().to_string(),
        port,
        protocol: ProtocolType::Https,
        download: false,
        ext: Some(Extensions::current(features.iter().copied())),
    }
}

fn paired_fingerprints(db: &Database) -> HashSet<Fingerprint> {
    db.list_devices()
        .unwrap_or_default()
        .into_iter()
        .filter(|device| device.paired)
        .map(|device| Fingerprint::parse(&device.fingerprint))
        .collect()
}

/// The host name without a `.local` suffix, or `lan-send`.
pub fn default_alias() -> String {
    let host = gethostname::gethostname().to_string_lossy().into_owned();
    let host = host.trim_end_matches(".local").trim();
    if host.is_empty() {
        "lan-send".to_string()
    } else {
        host.to_string()
    }
}

/// The operating system's display name, announced as `deviceModel`.
pub fn os_name() -> &'static str {
    match std::env::consts::OS {
        "macos" => "macOS",
        "windows" => "Windows",
        "ios" => "iOS",
        "linux" => "Linux",
        other => other,
    }
}

impl From<&KnownDevice> for DeviceView {
    fn from(known: &KnownDevice) -> Self {
        Self {
            fingerprint: known.fingerprint.clone(),
            alias: known.alias.clone(),
            custom_alias: known.custom_alias.clone(),
            display_name: known.display_name().to_string(),
            device_type: known.device_type.clone(),
            device_model: known.device_model.clone(),
            version: known.version.clone(),
            host: known.host.clone(),
            port: known.port,
            online: false,
            favorite: known.favorite,
            paired: known.paired,
            features: Vec::new(),
            last_seen: known.last_seen,
        }
    }
}
