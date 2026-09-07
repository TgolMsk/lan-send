//! Protocol state of the server: the single session slot and PIN attempts.

use super::save::SaveOutcome;
use super::{ServerEvent, SessionEndReason};
use crate::protocol::{DeviceInfo, FileDto, PeerInfo};
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::net::IpAddr;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Failed PIN attempts per IP before requests are blocked with 429.
pub(super) const MAX_PIN_ATTEMPTS: u32 = 3;
/// How often the same file may be uploaded (retries after a checksum mismatch).
pub(super) const MAX_UPLOAD_ATTEMPTS: u8 = 3;
/// Bound on the PIN attempt table.
const PIN_TABLE_CAPACITY: usize = 200;

pub(super) struct AppState {
    pub device: RwLock<DeviceInfo>,
    pub pin: Option<String>,
    pub verify_checksums: bool,
    pub events: mpsc::Sender<ServerEvent>,
    pub session: Mutex<Option<SessionState>>,
    pub pin_attempts: Mutex<HashMap<IpAddr, u32>>,
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
}

pub(super) struct SessionFile {
    pub dto: FileDto,
    pub token: String,
    pub status: FileStatus,
    pub attempts: u8,
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
}

pub(super) enum PinCheck {
    Ok,
    Required,
    Invalid,
    TooManyAttempts,
}

impl AppState {
    pub fn new(
        device: DeviceInfo,
        pin: Option<String>,
        verify_checksums: bool,
        events: mpsc::Sender<ServerEvent>,
    ) -> Self {
        Self {
            device: RwLock::new(device),
            pin,
            verify_checksums,
            events,
            session: Mutex::new(None),
            pin_attempts: Mutex::new(HashMap::new()),
        }
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

    /// Validates an upload request and marks the file in progress.
    pub fn begin_upload(
        &self,
        session_id: &str,
        file_id: &str,
        token: &str,
        peer: IpAddr,
    ) -> Option<FileDto> {
        let mut slot = self.session.lock();
        let Some(SessionState::Active(session)) = slot.as_mut() else {
            return None;
        };
        if session.session_id != session_id || session.peer != peer {
            return None;
        }
        let file = session.files.get_mut(file_id)?;
        if file.token != token || file.status != FileStatus::Pending {
            return None;
        }
        file.status = FileStatus::InProgress;
        file.attempts = file.attempts.saturating_add(1);
        Some(file.dto.clone())
    }

    /// Records the outcome of an upload. A checksum mismatch puts the file
    /// back to pending (until the attempt limit) so the sender can retry
    /// with the same token. Returns `true` when the session ended.
    pub fn finalize_file(&self, session_id: &str, file_id: &str, outcome: &SaveOutcome) -> bool {
        let mut slot = self.session.lock();
        let Some(SessionState::Active(session)) = slot.as_mut() else {
            return false;
        };
        if session.session_id != session_id {
            return false;
        }
        if let Some(file) = session.files.get_mut(file_id) {
            if file.status == FileStatus::InProgress {
                file.status = match outcome {
                    SaveOutcome::Success => FileStatus::Finished,
                    SaveOutcome::HashMismatch if file.attempts < MAX_UPLOAD_ATTEMPTS => {
                        FileStatus::Pending
                    }
                    SaveOutcome::HashMismatch | SaveOutcome::Failed(_) => FileStatus::Failed,
                };
            }
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
