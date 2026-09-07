//! Protocol state of the server: the single session slot and PIN attempts.

use super::save::SaveOutcome;
use super::{ServerEvent, SessionEndReason};
use crate::protocol::{DeviceInfo, FileDto, Fingerprint, PeerInfo};
use parking_lot::{Mutex, RwLock};
use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Failed PIN attempts per IP before requests are blocked with 429.
pub(super) const MAX_PIN_ATTEMPTS: u32 = 3;
/// How often the same file may be uploaded (retries after a checksum
/// mismatch or, with the resume extension, after a dropped connection).
pub(super) const MAX_UPLOAD_ATTEMPTS: u8 = 3;
/// Bound on the PIN attempt table.
const PIN_TABLE_CAPACITY: usize = 200;
/// A session without any request for this long is dropped.
pub(super) const SESSION_IDLE_TIMEOUT: Duration = Duration::from_secs(10 * 60);

pub(super) struct AppState {
    pub device: RwLock<DeviceInfo>,
    pub pin: Option<String>,
    pub verify_checksums: bool,
    pub upload_idle_timeout: Duration,
    pub events: mpsc::Sender<ServerEvent>,
    pub session: Mutex<Option<SessionState>>,
    pub pin_attempts: Mutex<HashMap<IpAddr, u32>>,
    /// Fingerprints of paired devices; the private extension endpoints
    /// require the caller's certificate to be one of them.
    pub paired: RwLock<HashSet<Fingerprint>>,
    /// Whether a pairing request is waiting for the user.
    pub pairing_pending: Mutex<bool>,
}

pub(super) enum SessionState {
    Pending(PendingSession),
    Active(Session),
}

pub(super) struct PendingSession {
    pub session_id: String,
    pub peer: IpAddr,
    pub cancel: CancellationToken,
}

pub(super) struct Session {
    pub session_id: String,
    pub peer: IpAddr,
    pub files: HashMap<String, SessionFile>,
    /// Resume extension: the token `Range` uploads must carry. `None` for
    /// senders that did not announce the extension.
    pub resume_token: Option<String>,
    pub last_activity: Instant,
}

pub(super) struct SessionFile {
    pub dto: FileDto,
    pub token: String,
    pub status: FileStatus,
    pub attempts: u8,
    /// Where the file is being written, once the application decided.
    pub path: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FileStatus {
    Pending,
    InProgress,
    Finished,
    Failed,
}

impl Session {
    fn is_complete(&self) -> bool {
        self.files
            .values()
            .all(|file| matches!(file.status, FileStatus::Finished | FileStatus::Failed))
    }

    pub fn resumable(&self) -> bool {
        self.resume_token.is_some()
    }
}

pub(super) enum PinCheck {
    Ok,
    Required,
    Invalid,
    TooManyAttempts,
}

/// Why an upload request was refused.
pub(super) enum UploadRefusal {
    /// 403: no such session/file/token, wrong address or wrong state.
    Invalid,
    /// 409: the file is currently being uploaded.
    InProgress,
}

/// A validated upload request.
pub(super) struct UploadStart {
    pub file: FileDto,
    /// The path chosen for an earlier attempt, when there was one.
    pub path: Option<PathBuf>,
    pub resumable: bool,
}

impl AppState {
    pub fn new(
        device: DeviceInfo,
        pin: Option<String>,
        verify_checksums: bool,
        upload_idle_timeout: Duration,
        paired: HashSet<Fingerprint>,
        events: mpsc::Sender<ServerEvent>,
    ) -> Self {
        Self {
            device: RwLock::new(device),
            pin,
            verify_checksums,
            upload_idle_timeout,
            events,
            session: Mutex::new(None),
            pin_attempts: Mutex::new(HashMap::new()),
            paired: RwLock::new(paired),
            pairing_pending: Mutex::new(false),
        }
    }

    /// This device's own fingerprint.
    pub fn own_fingerprint(&self) -> Fingerprint {
        Fingerprint::parse(&self.device.read().fingerprint)
    }

    /// Whether the peer presented a certificate of a paired device. Used by
    /// the private endpoints (clipboard, milestone 3).
    #[allow(dead_code)]
    pub fn is_paired(&self, fingerprint: Option<&Fingerprint>) -> bool {
        fingerprint.is_some_and(|fingerprint| self.paired.read().contains(fingerprint))
    }

    /// Claims the single pairing slot; `false` when one is pending.
    pub fn begin_pairing(&self) -> bool {
        let mut pending = self.pairing_pending.lock();
        if *pending {
            return false;
        }
        *pending = true;
        true
    }

    pub fn end_pairing(&self) {
        *self.pairing_pending.lock() = false;
    }

    pub fn peer_info(&self) -> PeerInfo {
        PeerInfo::from(&*self.device.read())
    }

    pub fn check_pin(&self, given: Option<&str>, ip: IpAddr) -> PinCheck {
        let Some(required) = &self.pin else {
            return PinCheck::Ok;
        };
        let mut attempts = self.pin_attempts.lock();
        let count = attempts.get(&ip).copied().unwrap_or(0);
        if count >= MAX_PIN_ATTEMPTS {
            return PinCheck::TooManyAttempts;
        }
        match given {
            Some(pin) if pin == required => {
                attempts.remove(&ip);
                PinCheck::Ok
            }
            Some(_) => {
                if attempts.len() >= PIN_TABLE_CAPACITY && !attempts.contains_key(&ip) {
                    attempts.clear();
                }
                attempts.insert(ip, count + 1);
                PinCheck::Invalid
            }
            None => PinCheck::Required,
        }
    }

    /// Claims the session slot for a pending request. `None` when busy.
    pub fn claim_pending(&self, session_id: &str, peer: IpAddr) -> Option<CancellationToken> {
        let mut slot = self.session.lock();
        if slot.is_some() {
            return None;
        }
        let cancel = CancellationToken::new();
        *slot = Some(SessionState::Pending(PendingSession {
            session_id: session_id.to_string(),
            peer,
            cancel: cancel.clone(),
        }));
        Some(cancel)
    }

    /// Replaces the pending slot with an active session.
    pub fn activate(&self, session: Session) {
        *self.session.lock() = Some(SessionState::Active(session));
    }

    /// Frees the slot if it holds the pending request `session_id`.
    pub fn clear_pending(&self, session_id: &str) {
        let mut slot = self.session.lock();
        if matches!(&*slot, Some(SessionState::Pending(pending)) if pending.session_id == session_id)
        {
            *slot = None;
        }
    }

    /// Cancels a pending request from `peer` (optionally checked against
    /// `session_id`). The waiting handler frees the slot.
    pub fn cancel_pending(&self, peer: IpAddr, session_id: Option<&str>) -> bool {
        let slot = self.session.lock();
        match &*slot {
            Some(SessionState::Pending(pending))
                if pending.peer == peer && session_id.is_none_or(|id| id == pending.session_id) =>
            {
                pending.cancel.cancel();
                true
            }
            _ => false,
        }
    }

    /// Removes the active session `session_id`, optionally only when it
    /// belongs to `peer`. Returns whether a session was removed.
    pub fn cancel_active(&self, session_id: &str, peer: Option<IpAddr>) -> bool {
        let mut slot = self.session.lock();
        match &*slot {
            Some(SessionState::Active(session))
                if session.session_id == session_id
                    && peer.is_none_or(|peer| peer == session.peer) =>
            {
                *slot = None;
                true
            }
            _ => false,
        }
    }

    /// Drops the active session when it has been idle for too long.
    /// Returns its id when that happened.
    pub fn reap_idle(&self) -> Option<String> {
        let mut slot = self.session.lock();
        match &*slot {
            Some(SessionState::Active(session))
                if session.last_activity.elapsed() >= SESSION_IDLE_TIMEOUT
                    && !session
                        .files
                        .values()
                        .any(|file| file.status == FileStatus::InProgress) =>
            {
                let session_id = session.session_id.clone();
                *slot = None;
                Some(session_id)
            }
            _ => None,
        }
    }

    /// Validates an upload request and marks the file in progress. With
    /// `resume_token` given, it must match the session's token.
    pub fn begin_upload(
        &self,
        session_id: &str,
        file_id: &str,
        token: &str,
        peer: IpAddr,
        resume_token: Option<&str>,
    ) -> Result<UploadStart, UploadRefusal> {
        let mut slot = self.session.lock();
        let Some(SessionState::Active(session)) = slot.as_mut() else {
            return Err(UploadRefusal::Invalid);
        };
        if session.session_id != session_id || session.peer != peer {
            return Err(UploadRefusal::Invalid);
        }
        if let Some(given) = resume_token
            && session.resume_token.as_deref() != Some(given)
        {
            return Err(UploadRefusal::Invalid);
        }
        let resumable = session.resumable();
        session.last_activity = Instant::now();
        let file = session
            .files
            .get_mut(file_id)
            .ok_or(UploadRefusal::Invalid)?;
        if file.token != token {
            return Err(UploadRefusal::Invalid);
        }
        match file.status {
            FileStatus::Pending => {}
            FileStatus::InProgress => return Err(UploadRefusal::InProgress),
            FileStatus::Finished | FileStatus::Failed => return Err(UploadRefusal::Invalid),
        }
        file.status = FileStatus::InProgress;
        file.attempts = file.attempts.saturating_add(1);
        Ok(UploadStart {
            file: file.dto.clone(),
            path: file.path.clone(),
            resumable,
        })
    }

    /// Remembers where a file is written, for later attempts and queries.
    pub fn set_file_path(&self, session_id: &str, file_id: &str, path: PathBuf) {
        let mut slot = self.session.lock();
        if let Some(SessionState::Active(session)) = slot.as_mut()
            && session.session_id == session_id
            && let Some(file) = session.files.get_mut(file_id)
        {
            file.path = Some(path);
        }
    }

    /// The resume query: the path of a pending file of `peer`'s session,
    /// validated against the resume token.
    pub fn resume_lookup(
        &self,
        session_id: &str,
        file_id: &str,
        peer: IpAddr,
        resume_token: &str,
    ) -> Result<Option<PathBuf>, UploadRefusal> {
        let mut slot = self.session.lock();
        let Some(SessionState::Active(session)) = slot.as_mut() else {
            return Err(UploadRefusal::Invalid);
        };
        if session.session_id != session_id
            || session.peer != peer
            || session.resume_token.as_deref() != Some(resume_token)
        {
            return Err(UploadRefusal::Invalid);
        }
        session.last_activity = Instant::now();
        let file = session.files.get(file_id).ok_or(UploadRefusal::Invalid)?;
        match file.status {
            FileStatus::InProgress => Err(UploadRefusal::InProgress),
            FileStatus::Pending => Ok(file.path.clone()),
            FileStatus::Finished | FileStatus::Failed => Err(UploadRefusal::Invalid),
        }
    }

    /// Records the outcome of an upload. A checksum mismatch, or a dropped
    /// connection with the resume extension, puts the file back to pending
    /// (until the attempt limit) so the sender can retry with the same
    /// token. Returns `true` when the session ended.
    pub fn finalize_file(&self, session_id: &str, file_id: &str, outcome: &SaveOutcome) -> bool {
        let mut slot = self.session.lock();
        let Some(SessionState::Active(session)) = slot.as_mut() else {
            return false;
        };
        if session.session_id != session_id {
            return false;
        }
        session.last_activity = Instant::now();
        let resumable = session.resumable();
        if let Some(file) = session.files.get_mut(file_id)
            && file.status == FileStatus::InProgress
        {
            let retry_allowed = file.attempts < MAX_UPLOAD_ATTEMPTS;
            file.status = match outcome {
                SaveOutcome::Success => FileStatus::Finished,
                SaveOutcome::HashMismatch if retry_allowed => FileStatus::Pending,
                SaveOutcome::Interrupted { .. } if resumable && retry_allowed => {
                    FileStatus::Pending
                }
                SaveOutcome::OffsetMismatch { .. } if retry_allowed => FileStatus::Pending,
                _ => FileStatus::Failed,
            };
        }
        if session.is_complete() {
            *slot = None;
            true
        } else {
            false
        }
    }

    /// Emits `SessionEnd` without blocking the caller.
    pub fn emit_session_end(&self, session_id: String, reason: SessionEndReason) {
        let events = self.events.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let _ = events
                    .send(ServerEvent::SessionEnd { session_id, reason })
                    .await;
            });
        }
    }
}
