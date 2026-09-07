//! Clipboard sync and history through the runtime (ADR-0011).

use super::{Inner, RuntimeError, SendRequest};
use crate::clipboard::sensitive::looks_sensitive;
use crate::clipboard::{
    ClipboardItem, ClipboardPayload, ClipboardSync, ImageFormat, PeerTarget, SyncConfig, SyncEvent,
    backend::with_retry, platform_backend,
};
use crate::protocol::{Fingerprint, INTENT_CLIPBOARD, ProtocolType};
use crate::runtime::{ClipboardView, RuntimeEvent};
use crate::store::ClipboardRecord;
use crate::transport::{Client, Peer, Target};
use bytes::Bytes;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

impl Inner {
    /// Paired devices as push targets, at their best known addresses.
    pub(crate) fn clipboard_peers(&self) -> Vec<PeerTarget> {
        let offline = self.offline.lock().clone();
        self.db
            .list_devices()
            .unwrap_or_default()
            .into_iter()
            .filter(|device| device.paired)
            .filter_map(|device| {
                let fingerprint = Fingerprint::parse(&device.fingerprint);
                let live = self
                    .discovery
                    .device_by_fingerprint(&fingerprint)
                    .filter(|_| !offline.contains(&fingerprint))
                    .map(|live| live.target());
                let target = live.or_else(|| {
                    Some(Target {
                        host: device.host.clone()?,
                        port: device.port?,
                        protocol: ProtocolType::Https,
                    })
                })?;
                Some(PeerTarget {
                    fingerprint,
                    alias: device.display_name().to_string(),
                    target,
                })
            })
            .collect()
    }

    pub(crate) fn refresh_clipboard_peers(&self) {
        let sync = self.clipboard.lock().clone();
        if let Some(sync) = sync {
            sync.set_peers(self.clipboard_peers());
        }
    }

    fn sync_status(&self, active: bool, message: Option<String>) -> RuntimeEvent {
        RuntimeEvent::ClipboardSync {
            active,
            peers: self
                .clipboard_peers()
                .into_iter()
                .map(|peer| peer.fingerprint.to_string())
                .collect(),
            message,
        }
    }

    /// Starts watching the local clipboard; a no-op without a backend or
    /// when already running.
    pub(crate) fn start_clipboard_sync(self: &Arc<Self>) {
        let Some(backend) = platform_backend() else {
            return;
        };
        let settings = self.settings();
        let (tx, rx) = mpsc::channel(64);
        let mut config = SyncConfig::new(backend, self.identity.clone(), tx);
        config.peers = self.clipboard_peers();
        config.text_limit = settings.clipboard.text_limit;
        config.image_limit = settings.clipboard.image_limit;
        config.poll_interval = Duration::from_millis(settings.clipboard.poll_interval_ms.max(50));
        {
            let mut guard = self.clipboard.lock();
            if guard.is_some() {
                return;
            }
            *guard = Some(ClipboardSync::start(config));
        }
        self.tasks.spawn(sync_events(self.clone(), rx));
        self.emit(self.sync_status(true, None));
    }

    pub(crate) fn stop_clipboard_sync(&self) {
        let sync = self.clipboard.lock().take();
        if let Some(sync) = sync {
            sync.stop();
            self.emit(self.sync_status(false, None));
        }
    }

    /// Keeps a clipboard item in the history unless it is (or looks)
    /// secret. Image bytes go to the cache directory. Returns the stored
    /// record.
    pub(crate) fn record_clipboard(
        &self,
        item: &ClipboardItem,
        sensitive: bool,
    ) -> Option<ClipboardRecord> {
        let settings = self.settings().clipboard;
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
                    return None;
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
                    return None;
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
            return None;
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
        Some(record)
    }

    fn clipboard_view(
        &self,
        item: &ClipboardItem,
        record: Option<&ClipboardRecord>,
    ) -> ClipboardView {
        let me = self.identity.fingerprint().as_str();
        let origin_alias = if item.origin_device.as_str() == me {
            Some(self.alias.clone())
        } else {
            self.alias_of(item.origin_device.as_str())
        };
        match record {
            Some(record) => ClipboardView::from_record(record, me, origin_alias),
            None => ClipboardView::from_item(item, me, origin_alias),
        }
    }

    /// Puts received clipboard files onto the local clipboard and records
    /// them; the sync (when running) is told so it does not push them back.
    pub(crate) fn apply_clipboard_files(&self, paths: &[PathBuf], origin: &str) {
        let payload = ClipboardPayload::Files {
            paths: paths.to_vec(),
        };
        let item = ClipboardItem::new(Fingerprint::parse(origin), payload.clone());
        let Some(backend) = platform_backend() else {
            let view = self.clipboard_view(&item, None);
            self.emit(RuntimeEvent::ClipboardReceived { item: view });
            return;
        };
        let sync = self.clipboard.lock().clone();
        if let Some(sync) = &sync {
            sync.remember(item.content_hash);
        }
        match with_retry(|| backend.write(&payload)) {
            Ok(()) => {
                let record = self.record_clipboard(&item, false);
                let view = self.clipboard_view(&item, record.as_ref());
                self.emit(RuntimeEvent::ClipboardReceived { item: view });
            }
            Err(err) => self.emit_error(
                "clipboard",
                format!("received the files but could not set the clipboard: {err}"),
            ),
        }
    }

    pub(crate) async fn on_clipboard_received(
        &self,
        peer: &Peer,
        item: ClipboardItem,
        sensitive: bool,
    ) {
        let sync = self.clipboard.lock().clone();
        let applied = match &sync {
            Some(sync) => sync.apply_remote(item.clone()).await,
            None => match platform_backend() {
                Some(backend) => {
                    let payload = item.payload.clone();
                    tokio::task::spawn_blocking(move || with_retry(|| backend.write(&payload)))
                        .await
                        .unwrap_or_else(|err| {
                            Err(crate::clipboard::ClipboardError::Other(err.to_string()))
                        })
                }
                None => Err(crate::clipboard::ClipboardError::Unsupported),
            },
        };
        match applied {
            Ok(()) => {
                let record = self.record_clipboard(&item, sensitive);
                let view = self.clipboard_view(&item, record.as_ref());
                self.emit(RuntimeEvent::ClipboardReceived { item: view });
            }
            Err(err) => {
                let from = peer
                    .cert_fingerprint
                    .as_ref()
                    .and_then(|fp| self.alias_of(fp.as_str()))
                    .unwrap_or_else(|| peer.addr.to_string());
                self.emit_error(
                    "clipboard",
                    format!("could not apply the clipboard from {from}: {err}"),
                );
            }
        }
    }

    pub(crate) fn clipboard_history(
        &self,
        limit: usize,
    ) -> Result<Vec<ClipboardView>, RuntimeError> {
        let me = self.identity.fingerprint().as_str();
        Ok(self
            .db
            .list_clipboard(limit)?
            .iter()
            .map(|record| {
                let origin_alias = if record.origin == me {
                    Some(self.alias.clone())
                } else {
                    self.alias_of(&record.origin)
                };
                ClipboardView::from_record(record, me, origin_alias)
            })
            .collect())
    }

    pub(crate) fn clipboard_copy(&self, id: &str) -> Result<(), RuntimeError> {
        let record = self
            .db
            .clipboard_item(id)?
            .ok_or_else(|| RuntimeError::Invalid("no such clipboard entry".into()))?;
        let payload = payload_of(&record)?;
        let backend = platform_backend().ok_or(RuntimeError::NoClipboard)?;
        with_retry(|| backend.write(&payload))?;
        Ok(())
    }

    pub(crate) fn clipboard_delete(&self, id: &str) -> Result<bool, RuntimeError> {
        let removed = self.db.delete_clipboard(id)?;
        if let Some(record) = &removed
            && let Some(path) = &record.image_path
        {
            let _ = std::fs::remove_file(path);
        }
        Ok(removed.is_some())
    }

    pub(crate) fn clipboard_clear(&self) -> Result<usize, RuntimeError> {
        let removed = self.db.clear_clipboard()?;
        for record in &removed {
            if let Some(path) = &record.image_path {
                let _ = std::fs::remove_file(path);
            }
        }
        Ok(removed.len())
    }

    /// Pushes the current clipboard to one paired device or all of them.
    pub(crate) async fn clipboard_push(
        self: &Arc<Self>,
        device: Option<&str>,
    ) -> Result<ClipboardView, RuntimeError> {
        let backend = platform_backend().ok_or(RuntimeError::NoClipboard)?;
        let mut peers = self.clipboard_peers();
        if let Some(query) = device {
            let query = query.trim();
            peers.retain(|peer| {
                peer.fingerprint.as_str().eq_ignore_ascii_case(query)
                    || (query.len() >= 4 && peer.fingerprint.has_prefix(query))
                    || peer.alias.eq_ignore_ascii_case(query)
            });
            if peers.is_empty() {
                return Err(RuntimeError::Invalid(format!(
                    "{query} is not a paired device"
                )));
            }
        }
        if peers.is_empty() {
            return Err(RuntimeError::Invalid("no paired devices".into()));
        }
        let payload = tokio::task::spawn_blocking(move || with_retry(|| backend.read()))
            .await
            .map_err(|err| RuntimeError::Invalid(err.to_string()))??
            .ok_or_else(|| RuntimeError::Invalid("the clipboard is empty".into()))?;
        let item = ClipboardItem::new(self.identity.fingerprint().clone(), payload);
        if let ClipboardPayload::Files { paths } = &item.payload {
            for peer in &peers {
                self.start_send(SendRequest {
                    device: peer.fingerprint.to_string(),
                    paths: paths.clone(),
                    pin: None,
                    intent: Some(INTENT_CLIPBOARD.to_string()),
                })?;
            }
        } else {
            let pushes = peers.iter().map(|peer| {
                let item = item.clone();
                async move {
                    let client = Client::new(
                        &self.identity,
                        Some(peer.fingerprint.clone()),
                        Some(Duration::from_secs(30)),
                    )?;
                    client.send_clipboard(&peer.target, &item).await?;
                    Ok::<(), RuntimeError>(())
                }
            });
            let results = futures_util::future::join_all(pushes).await;
            let mut failures = Vec::new();
            for (peer, result) in peers.iter().zip(results) {
                if let Err(err) = result {
                    failures.push(format!("{}: {err}", peer.alias));
                }
            }
            if failures.len() == peers.len() {
                return Err(RuntimeError::Invalid(failures.join("; ")));
            }
            if !failures.is_empty() {
                self.emit_error("clipboard", failures.join("; "));
            }
        }
        let record = self.record_clipboard(&item, false);
        let view = self.clipboard_view(&item, record.as_ref());
        self.emit(RuntimeEvent::ClipboardLocal { item: view.clone() });
        Ok(view)
    }
}

/// Reacts to the sync engine: history, file transfers, error reporting.
async fn sync_events(inner: Arc<Inner>, mut events: mpsc::Receiver<SyncEvent>) {
    loop {
        let event = tokio::select! {
            _ = inner.cancel.cancelled() => break,
            event = events.recv() => match event {
                Some(event) => event,
                None => break,
            },
        };
        match event {
            SyncEvent::FilesCopied(item) => {
                let ClipboardPayload::Files { paths } = &item.payload else {
                    continue;
                };
                let record = inner.record_clipboard(&item, false);
                let view = inner.clipboard_view(&item, record.as_ref());
                inner.emit(RuntimeEvent::ClipboardLocal { item: view });
                for peer in inner.clipboard_peers() {
                    if let Err(err) = inner.start_send(SendRequest {
                        device: peer.fingerprint.to_string(),
                        paths: paths.clone(),
                        pin: None,
                        intent: Some(INTENT_CLIPBOARD.to_string()),
                    }) {
                        inner.emit_error("clipboard", format!("{}: {err}", peer.alias));
                    }
                }
            }
            SyncEvent::LocalChange(item) => {
                let record = inner.record_clipboard(&item, false);
                let view = inner.clipboard_view(&item, record.as_ref());
                inner.emit(RuntimeEvent::ClipboardLocal { item: view });
            }
            SyncEvent::Pushed { peer, result, .. } => {
                if let Err(err) = result {
                    let alias = inner
                        .alias_of(peer.as_str())
                        .unwrap_or_else(|| peer.short().to_string());
                    inner.emit_error("clipboard", format!("could not push to {alias}: {err}"));
                }
            }
            SyncEvent::TooLarge { kind, size, limit } => {
                let message = format!(
                    "skipped a {kind} of {} (limit {}); send it as a file instead",
                    crate::runtime::format_bytes(size as u64),
                    crate::runtime::format_bytes(limit as u64)
                );
                inner.emit(inner.sync_status(true, Some(message)));
            }
            SyncEvent::Applied(_) => {}
            SyncEvent::Error(err) => inner.emit_error("clipboard", err),
        }
    }
}

fn payload_of(record: &ClipboardRecord) -> Result<ClipboardPayload, RuntimeError> {
    Ok(match record.kind.as_str() {
        "text" => ClipboardPayload::Text {
            plain: record.text.clone().unwrap_or_default(),
            html: record.html.clone(),
            rtf: record.rtf.clone(),
        },
        "image" => {
            let path = record
                .image_path
                .clone()
                .ok_or_else(|| RuntimeError::Invalid("image file missing".into()))?;
            let bytes = std::fs::read(&path)?;
            ClipboardPayload::Image {
                format: match record.image_format.as_deref() {
                    Some("jpg") => ImageFormat::Jpeg,
                    _ => ImageFormat::Png,
                },
                bytes: Bytes::from(bytes),
                width: record.image_width.unwrap_or(0),
                height: record.image_height.unwrap_or(0),
            }
        }
        _ => ClipboardPayload::Files {
            paths: record.file_paths.iter().map(PathBuf::from).collect(),
        },
    })
}
