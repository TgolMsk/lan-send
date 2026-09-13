//! The server event loop: incoming transfer sessions, cancel requests,
//! pairing requests and clipboard pushes.

use super::{ErrorCode, Inner, RuntimeError};
use crate::discovery::Device;
use crate::protocol::{DeviceInfo, FEATURE_RESUME, FileDto, Fingerprint, INTENT_CLIPBOARD};
use crate::runtime::{
    FileState, FileView, IncomingRequestView, RuntimeEvent, TransferFileView, TransferState,
    TransferView,
};
use crate::store::{
    Direction, KnownDevice, PartialUpload, TransferRecord, TransferStatus, unix_now,
};
use crate::transfer::{Destination, Placement};
use crate::transport::server::{SaveOutcome, part_path};
use crate::transport::{Peer, ServerEvent, SessionEndReason, UploadDecision, UploadTarget};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

/// How long an `incoming-conflict` question waits before renaming.
const CONFLICT_TIMEOUT: Duration = Duration::from_secs(30);

/// A request the user has not answered yet.
pub(crate) struct PendingIncoming {
    pub session_id: String,
    pub info: DeviceInfo,
    pub sender: Fingerprint,
    pub files: HashMap<String, FileDto>,
    pub resumable: bool,
    pub intent: Option<String>,
    pub decision: oneshot::Sender<UploadDecision>,
}

struct SessionFile {
    dto: FileDto,
    started_at: i64,
    /// History entry id; every attempt updates the same entry.
    record_id: String,
}

/// The accepted session being received.
pub(crate) struct SessionInfo {
    session_id: String,
    peer_alias: String,
    peer_fingerprint: String,
    intent: Option<String>,
    resumable: bool,
    resume_paths: HashMap<String, PathBuf>,
    files: HashMap<String, SessionFile>,
    received: Vec<PathBuf>,
    failed: usize,
    destination: Destination,
    assigned: HashSet<PathBuf>,
}

pub(crate) async fn run(inner: Arc<Inner>, mut events: mpsc::Receiver<ServerEvent>) {
    loop {
        let event = tokio::select! {
            _ = inner.cancel.cancelled() => break,
            event = events.recv() => match event {
                Some(event) => event,
                None => break,
            },
        };
        inner.handle_server_event(event).await;
    }
}

impl Inner {
    async fn handle_server_event(self: &Arc<Self>, event: ServerEvent) {
        match event {
            ServerEvent::Register { peer, info } => {
                self.confirm_peer(&peer, &info);
            }
            ServerEvent::PrepareUpload {
                session_id,
                peer,
                info,
                files,
                decision,
            } => self.on_prepare_upload(session_id, peer, info, files, decision),
            ServerEvent::PrepareUploadAborted { session_id } => {
                let pending = self
                    .pending
                    .lock()
                    .incoming
                    .take_if(|pending| pending.session_id == session_id);
                if pending.is_some() {
                    self.transfers.lock().remove(&session_id);
                }
                self.emit(RuntimeEvent::IncomingWithdrawn { session_id });
            }
            ServerEvent::FileUpload {
                session_id,
                file_id,
                file,
                target,
            } => self.on_file_upload(session_id, file_id, file, target),
            ServerEvent::FileUploadProgress {
                session_id,
                file_id,
                received,
            } => self.progress(&session_id, &file_id, received),
            ServerEvent::FileUploadResult {
                session_id,
                file_id,
                path,
                outcome,
                received,
            } => self.on_file_result(&session_id, &file_id, path, outcome, received),
            ServerEvent::SessionEnd { session_id, reason } => {
                self.on_session_end(&session_id, reason);
            }
            ServerEvent::CancelReceived { peer, session_id } => {
                self.on_cancel_received(&peer, &session_id);
            }
            ServerEvent::PairRequest {
                peer,
                alias,
                code,
                decision,
            } => self.on_pair_request(peer, alias, code, decision),
            ServerEvent::Unpaired { peer } => self.on_unpaired(&peer),
            ServerEvent::ClipboardReceived {
                peer,
                item,
                sensitive,
            } => self.on_clipboard_received(&peer, item, sensitive).await,
        }
    }

    /// A peer proved reachable: remember it and let discovery list it.
    pub(crate) fn confirm_peer(&self, peer: &Peer, info: &DeviceInfo) -> Fingerprint {
        let fingerprint = peer.identity(&info.fingerprint);
        if &fingerprint == self.identity.fingerprint() {
            return fingerprint;
        }
        let device = Device::from_info(&peer.host(), info, fingerprint.clone());
        if let Err(err) = self.db.upsert_device(&KnownDevice::from(&device)) {
            tracing::warn!("could not remember {}: {err}", device.alias);
        }
        self.discovery.add_confirmed(device);
        fingerprint
    }

    fn on_prepare_upload(
        self: &Arc<Self>,
        session_id: String,
        peer: Peer,
        info: DeviceInfo,
        files: HashMap<String, FileDto>,
        decision: oneshot::Sender<UploadDecision>,
    ) {
        let sender = self.confirm_peer(&peer, &info);
        let settings = self.settings();
        let resumable = settings.resume
            && info
                .ext
                .as_ref()
                .is_some_and(|ext| ext.supports(FEATURE_RESUME));
        let intent = info.ext.as_ref().and_then(|ext| ext.intent.clone());
        let paired = self.is_paired(&sender);
        let clipboard_intent = intent.as_deref() == Some(INTENT_CLIPBOARD);
        let auto_accepted = paired
            && (settings.app.auto_accept_paired
                || (clipboard_intent && settings.clipboard.sync_enabled));

        let mut file_views: Vec<TransferFileView> = files
            .values()
            .map(|file| TransferFileView {
                id: file.id.clone(),
                name: file.file_name.clone(),
                size: file.size,
                mime: file.file_type.clone(),
                done: 0,
                state: FileState::Pending,
                path: None,
                error: None,
            })
            .collect();
        file_views.sort_by(|a, b| a.name.cmp(&b.name));
        let peer_alias = self
            .alias_of(sender.as_str())
            .unwrap_or_else(|| info.alias.clone());
        let mut view = TransferView {
            id: session_id.clone(),
            direction: Direction::Receive,
            session_id: Some(session_id.clone()),
            peer_fingerprint: sender.to_string(),
            peer_alias: peer_alias.clone(),
            files: file_views,
            total_size: 0,
            done_size: 0,
            state: if auto_accepted {
                TransferState::Active
            } else {
                TransferState::WaitingAccept
            },
            clipboard_intent,
            error: None,
            error_code: None,
            started_at: unix_now(),
            finished_at: None,
        };
        view.recompute_totals();
        let request = IncomingRequestView {
            session_id: session_id.clone(),
            peer_fingerprint: sender.to_string(),
            peer_alias,
            peer_host: peer.host(),
            files: view
                .files
                .iter()
                .map(|file| FileView {
                    id: file.id.clone(),
                    name: file.name.clone(),
                    size: file.size,
                    mime: file.mime.clone(),
                })
                .collect(),
            total_size: view.total_size,
            resumable,
            clipboard_intent,
            auto_accepted,
        };
        self.insert_transfer(view, Some(peer.host()));

        let pending = PendingIncoming {
            session_id,
            info,
            sender,
            files,
            resumable,
            intent,
            decision,
        };
        // Register the decision *before* announcing it. A consumer that answers
        // as soon as it sees `IncomingRequest` would otherwise be able to call
        // `respond_incoming` before the request is stored and get
        // `NothingPending` (pairing registers first for the same reason).
        let mut auto = None;
        let previous = if auto_accepted {
            auto = Some(pending);
            None
        } else {
            self.pending.lock().incoming.replace(pending)
        };

        self.emit(RuntimeEvent::IncomingRequest { request });

        if let Some(pending) = auto {
            self.accept_incoming(pending);
        }
        if let Some(previous) = previous {
            self.decline_incoming(previous);
        }
    }

    pub(crate) fn respond_incoming(
        self: &Arc<Self>,
        session_id: &str,
        accept: bool,
    ) -> Result<(), RuntimeError> {
        let pending = self
            .pending
            .lock()
            .incoming
            .take_if(|pending| pending.session_id == session_id)
            .ok_or(RuntimeError::NothingPending)?;
        if accept {
            self.accept_incoming(pending);
        } else {
            self.decline_incoming(pending);
        }
        Ok(())
    }

    fn accept_incoming(self: &Arc<Self>, pending: PendingIncoming) {
        let settings = self.settings();
        let root = settings
            .receive_dir
            .clone()
            .or_else(|| self.paths.download_dir.clone());
        let destination = root
            .ok_or_else(|| "no receive directory is configured".to_string())
            .and_then(|root| {
                Destination::new(&root, settings.organize, settings.on_conflict)
                    .map_err(|err| format!("cannot use {}: {err}", root.display()))
            });
        let destination = match destination {
            Ok(destination) => destination,
            Err(message) => {
                let _ = pending.decision.send(UploadDecision::Decline);
                self.fail_transfer(
                    &pending.session_id,
                    ErrorCode::NoReceiveDir,
                    message.clone(),
                );
                self.emit_error("receive", message);
                return;
            }
        };
        let (offsets, resume_paths) = if pending.resumable {
            self.resumable_files(pending.sender.as_str(), &pending.files)
        } else {
            (HashMap::new(), HashMap::new())
        };
        let peer_alias = self
            .alias_of(pending.sender.as_str())
            .unwrap_or_else(|| pending.info.alias.clone());
        let info = SessionInfo {
            session_id: pending.session_id.clone(),
            peer_alias,
            peer_fingerprint: pending.sender.to_string(),
            intent: pending.intent,
            resumable: pending.resumable,
            resume_paths,
            files: pending
                .files
                .iter()
                .map(|(id, file)| {
                    let entry = SessionFile {
                        dto: file.clone(),
                        started_at: unix_now(),
                        record_id: uuid::Uuid::new_v4().to_string(),
                    };
                    (id.clone(), entry)
                })
                .collect(),
            received: Vec::new(),
            failed: 0,
            destination,
            assigned: HashSet::new(),
        };
        *self.incoming.lock() = Some(info);
        let ids: HashSet<String> = pending.files.keys().cloned().collect();
        let decision = if pending.resumable {
            UploadDecision::AcceptWithResume {
                files: ids,
                offsets,
            }
        } else {
            UploadDecision::Accept(ids)
        };
        if pending.decision.send(decision).is_err() {
            *self.incoming.lock() = None;
            self.transfers.lock().remove(&pending.session_id);
            self.emit(RuntimeEvent::IncomingWithdrawn {
                session_id: pending.session_id,
            });
        } else {
            self.set_transfer_state(&pending.session_id, TransferState::Active);
        }
    }

    pub(crate) fn decline_incoming(&self, pending: PendingIncoming) {
        let _ = pending.decision.send(UploadDecision::Decline);
        self.complete_transfer(&pending.session_id, TransferState::Declined, None, None);
    }

    /// Files of a new request that match a partial upload from the same
    /// sender: their offsets for the response and the paths to continue.
    fn resumable_files(
        &self,
        sender_fingerprint: &str,
        files: &HashMap<String, FileDto>,
    ) -> (HashMap<String, u64>, HashMap<String, PathBuf>) {
        let mut offsets = HashMap::new();
        let mut paths = HashMap::new();
        for (id, file) in files {
            let Some(sha256) = &file.sha256 else {
                continue;
            };
            let Ok(Some(partial)) = self.db.find_partial(sender_fingerprint, sha256, file.size)
            else {
                continue;
            };
            let on_disk = std::fs::metadata(&partial.part_path)
                .map(|metadata| metadata.len())
                .unwrap_or(0);
            if on_disk > 0 && on_disk <= file.size && !partial.final_path.exists() {
                offsets.insert(id.clone(), on_disk);
                paths.insert(id.clone(), partial.final_path.clone());
            } else {
                let _ = self.db.remove_partial(&partial.part_path);
                let _ = std::fs::remove_file(&partial.part_path);
            }
        }
        (offsets, paths)
    }

    fn on_file_upload(
        self: &Arc<Self>,
        session_id: String,
        file_id: String,
        file: FileDto,
        target: oneshot::Sender<UploadTarget>,
    ) {
        let placement = {
            let guard = self.incoming.lock();
            match guard.as_ref() {
                Some(info) if info.session_id == session_id => {
                    match info.resume_paths.get(&file_id).cloned() {
                        Some(path) => Ok(Placement::New(path)),
                        None => info
                            .destination
                            .place(&file, &info.peer_alias, &info.assigned)
                            .map_err(|err| err.to_string()),
                    }
                }
                _ => Err("no session is being received".to_string()),
            }
        };
        match placement {
            Err(reason) => {
                self.finish_file(
                    &session_id,
                    &file_id,
                    FileState::Failed,
                    None,
                    Some(reason.clone()),
                );
                let _ = target.send(UploadTarget::Reject(reason));
            }
            Ok(Placement::New(path)) | Ok(Placement::Replace(path)) => {
                self.assign_upload(&session_id, &file_id, &file, path, target);
            }
            Ok(Placement::NeedsDecision { existing, renamed }) => {
                let (tx, rx) = oneshot::channel();
                let key = (session_id.clone(), file_id.clone());
                self.pending.lock().conflicts.insert(key.clone(), tx);
                self.emit(RuntimeEvent::IncomingConflict {
                    session_id: session_id.clone(),
                    file_id: file_id.clone(),
                    existing: existing.clone(),
                    renamed: renamed.clone(),
                });
                let this = self.clone();
                self.tasks.spawn(async move {
                    let overwrite = matches!(
                        tokio::time::timeout(CONFLICT_TIMEOUT, rx).await,
                        Ok(Ok(true))
                    );
                    this.pending.lock().conflicts.remove(&key);
                    let path = if overwrite { existing } else { renamed };
                    this.assign_upload(&session_id, &file_id, &file, path, target);
                });
            }
        }
    }

    fn assign_upload(
        &self,
        session_id: &str,
        file_id: &str,
        file: &FileDto,
        path: PathBuf,
        target: oneshot::Sender<UploadTarget>,
    ) {
        let resumable = {
            let mut guard = self.incoming.lock();
            match guard.as_mut() {
                Some(info) if info.session_id == session_id => {
                    info.assigned.insert(path.clone());
                    info.resumable
                }
                _ => false,
            }
        };
        if resumable {
            let sender = self
                .incoming
                .lock()
                .as_ref()
                .map(|info| info.peer_fingerprint.clone())
                .unwrap_or_default();
            let _ = self.db.upsert_partial(&PartialUpload {
                part_path: part_path(&path),
                final_path: path.clone(),
                sender_fingerprint: sender,
                file_name: file.file_name.clone(),
                size: file.size,
                sha256: file.sha256.clone(),
                received: 0,
                updated_at: unix_now(),
            });
        }
        self.update_transfer(session_id, |transfer| {
            if let Some(view) = transfer.view.file_mut(file_id) {
                view.path = Some(path.clone());
                view.state = FileState::Active;
            }
        });
        let _ = target.send(UploadTarget::Path(path));
    }

    fn on_file_result(
        &self,
        session_id: &str,
        file_id: &str,
        path: PathBuf,
        outcome: SaveOutcome,
        on_disk: u64,
    ) {
        let part = part_path(&path);
        let resumable = self
            .incoming
            .lock()
            .as_ref()
            .is_some_and(|info| info.session_id == session_id && info.resumable);
        let (state, error) = match &outcome {
            SaveOutcome::Success => {
                let _ = self.db.remove_partial(&part);
                if let Some(info) = self.incoming.lock().as_mut() {
                    info.received.push(path.clone());
                }
                (FileState::Finished, None)
            }
            SaveOutcome::Interrupted { .. } if resumable && on_disk > 0 => {
                if let Ok(Some(mut partial)) = self.db.partial_by_path(&part) {
                    partial.received = on_disk;
                    partial.updated_at = unix_now();
                    let _ = self.db.upsert_partial(&partial);
                }
                // Still pending: the sender may retry within the session.
                (FileState::Pending, Some(outcome.to_string()))
            }
            other => {
                if let Some(info) = self.incoming.lock().as_mut() {
                    info.failed += 1;
                }
                let _ = self.db.remove_partial(&part);
                (FileState::Failed, Some(other.to_string()))
            }
        };
        if state == FileState::Pending {
            self.update_transfer(session_id, |transfer| {
                if let Some(view) = transfer.view.file_mut(file_id) {
                    view.done = on_disk;
                    view.error = error.clone();
                }
            });
        } else {
            self.finish_file(
                session_id,
                file_id,
                state,
                Some(path.clone()),
                error.clone(),
            );
        }

        let record = {
            let guard = self.incoming.lock();
            guard
                .as_ref()
                .filter(|info| info.session_id == session_id)
                .map(|info| {
                    let (dto, started_at, record_id) = match info.files.get(file_id) {
                        Some(entry) => {
                            (entry.dto.clone(), entry.started_at, entry.record_id.clone())
                        }
                        None => (
                            placeholder_file(file_id, &path),
                            unix_now(),
                            uuid::Uuid::new_v4().to_string(),
                        ),
                    };
                    TransferRecord {
                        id: record_id,
                        session_id: session_id.to_string(),
                        direction: Direction::Receive,
                        peer_fingerprint: info.peer_fingerprint.clone(),
                        peer_alias: info.peer_alias.clone(),
                        file_name: dto.file_name.clone(),
                        path: Some(path.clone()),
                        size: dto.size,
                        mime: dto.file_type.clone(),
                        status: if outcome.is_success() {
                            TransferStatus::Finished
                        } else {
                            TransferStatus::Failed
                        },
                        error,
                        started_at,
                        finished_at: Some(unix_now()),
                    }
                })
        };
        if let Some(mut record) = record {
            if crate::media::is_generic_mime(&record.mime) {
                if let Some(path) = record.path.as_deref() {
                    record.mime = crate::media::sniff_mime(path);
                }
            }
            self.record_transfer(&record);
        }
    }

    fn on_session_end(&self, session_id: &str, reason: SessionEndReason) {
        let info = self
            .incoming
            .lock()
            .take_if(|info| info.session_id == session_id);
        let Some(info) = info else {
            return;
        };
        let state = match reason {
            SessionEndReason::Finished if info.failed == 0 => TransferState::Finished,
            SessionEndReason::Finished => TransferState::Failed,
            SessionEndReason::Cancelled | SessionEndReason::TimedOut => TransferState::Cancelled,
        };
        let (error, code) = match (reason, info.failed) {
            (SessionEndReason::Finished, 0) => (None, None),
            (SessionEndReason::Finished, failed) => (
                Some(format!("{failed} file(s) failed")),
                Some(ErrorCode::PartialFailure),
            ),
            (SessionEndReason::Cancelled, _) => (
                Some("cancelled by the sender".to_string()),
                Some(ErrorCode::Cancelled),
            ),
            (SessionEndReason::TimedOut, _) => (
                Some("the sender went away".to_string()),
                Some(ErrorCode::PeerGone),
            ),
        };
        self.complete_transfer(session_id, state, error, code);
        if info.intent.as_deref() == Some(INTENT_CLIPBOARD)
            && state == TransferState::Finished
            && !info.received.is_empty()
        {
            self.apply_clipboard_files(&info.received, &info.peer_fingerprint);
        }
    }

    /// The peer cancels a transfer we are sending to it.
    fn on_cancel_received(&self, peer: &Peer, session_id: &str) {
        let peer_addr = peer.addr.to_string();
        let transfers = self.transfers.lock();
        for transfer in transfers.values() {
            let same_session = transfer.view.session_id.as_deref() == Some(session_id);
            let same_peer = transfer
                .peer_host
                .as_deref()
                .is_some_and(|host| host.split('%').next().unwrap_or(host) == peer_addr);
            if transfer.view.direction == Direction::Send && same_session && same_peer {
                transfer.cancel.cancel();
            }
        }
    }
}

fn placeholder_file(file_id: &str, path: &std::path::Path) -> FileDto {
    FileDto {
        id: file_id.to_string(),
        file_name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        size: std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
        file_type: "application/octet-stream".into(),
        sha256: None,
        preview: None,
        metadata: None,
    }
}
