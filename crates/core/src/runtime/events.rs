//! Events the runtime emits and the views it hands to user interfaces.
//! Everything here is serialisable so a shell (Tauri, tests) can forward it
//! unchanged. Clipboard text never appears in logs; views carry it only
//! when the item is kept in the history.

use crate::clipboard::{ClipboardItem, ClipboardPayload};
use crate::store::{ClipboardRecord, Direction};
use serde::Serialize;
use std::path::PathBuf;

/// A device as shown in lists: discovery state merged with what the
/// database remembers (favourite, pairing, custom name).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceView {
    pub fingerprint: String,
    pub alias: String,
    pub custom_alias: Option<String>,
    /// `custom_alias` when set, otherwise `alias`.
    pub display_name: String,
    pub device_type: Option<String>,
    pub device_model: Option<String>,
    pub version: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub online: bool,
    pub favorite: bool,
    pub paired: bool,
    /// Extension features the device announced (`resume`, `pairing`, …).
    pub features: Vec<String>,
    /// Unix seconds.
    pub last_seen: i64,
}

/// This device.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityView {
    pub alias: String,
    pub fingerprint: String,
    pub port: u16,
    pub device_type: String,
    pub device_model: String,
    pub config_dir: PathBuf,
    pub receive_dir: Option<PathBuf>,
    pub clipboard_supported: bool,
    pub features: Vec<String>,
    pub multicast_error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileView {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub mime: String,
}

/// A transfer request waiting for the user's answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IncomingRequestView {
    pub session_id: String,
    pub peer_fingerprint: String,
    pub peer_alias: String,
    pub peer_host: String,
    pub files: Vec<FileView>,
    pub total_size: u64,
    pub resumable: bool,
    /// The sender copied these files; a paired sender's clipboard files are
    /// accepted without asking.
    pub clipboard_intent: bool,
    pub auto_accepted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FileState {
    Pending,
    Active,
    Finished,
    Failed,
    Cancelled,
    Skipped,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferFileView {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub mime: String,
    pub done: u64,
    pub state: FileState,
    /// Where it was saved, or the source that is sent.
    pub path: Option<PathBuf>,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TransferState {
    /// Collecting files, hashing, looking up the device.
    Preparing,
    /// The receiver wants a PIN; answer with `provide_pin`.
    WaitingPin,
    /// Incoming: waiting for the local user's answer.
    WaitingAccept,
    Active,
    Finished,
    Failed,
    Cancelled,
    /// The other side (or the local user) declined.
    Declined,
}

impl TransferState {
    pub fn is_final(self) -> bool {
        matches!(
            self,
            Self::Finished | Self::Failed | Self::Cancelled | Self::Declined
        )
    }
}

/// A transfer in progress or just finished.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferView {
    pub id: String,
    pub direction: Direction,
    pub session_id: Option<String>,
    pub peer_fingerprint: String,
    pub peer_alias: String,
    pub files: Vec<TransferFileView>,
    pub total_size: u64,
    pub done_size: u64,
    pub state: TransferState,
    pub clipboard_intent: bool,
    pub error: Option<String>,
    /// Unix seconds.
    pub started_at: i64,
    pub finished_at: Option<i64>,
}

impl TransferView {
    pub(crate) fn recompute_totals(&mut self) {
        self.total_size = self.files.iter().map(|file| file.size).sum();
        self.done_size = self.files.iter().map(|file| file.done).sum();
    }

    pub(crate) fn file_mut(&mut self, id: &str) -> Option<&mut TransferFileView> {
        self.files.iter_mut().find(|file| file.id == id)
    }
}

/// A pairing in progress: both devices show `code`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairView {
    pub fingerprint: String,
    pub alias: String,
    pub code: String,
}

/// A clipboard item for the user interface: a history entry, or a just
/// received item that is not kept (then `text` is absent).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardView {
    pub id: String,
    pub origin: String,
    pub origin_alias: Option<String>,
    pub from_self: bool,
    /// Unix milliseconds.
    pub created_at: i64,
    /// `text`, `image` or `files`.
    pub kind: String,
    pub size: u64,
    pub text: Option<String>,
    pub image_path: Option<PathBuf>,
    pub image_width: Option<u32>,
    pub image_height: Option<u32>,
    pub file_paths: Vec<String>,
    /// Kept in the history (false for secrets and when history is off).
    pub stored: bool,
    /// Human description without content, e.g. `text, 1.2 KB`.
    pub description: String,
}

impl ClipboardView {
    pub(crate) fn from_record(
        record: &ClipboardRecord,
        self_fingerprint: &str,
        origin_alias: Option<String>,
    ) -> Self {
        Self {
            id: record.id.clone(),
            origin: record.origin.clone(),
            origin_alias,
            from_self: record.origin == self_fingerprint,
            created_at: record.created_at,
            kind: record.kind.clone(),
            size: record.size,
            text: record.text.clone(),
            image_path: record.image_path.clone(),
            image_width: record.image_width,
            image_height: record.image_height,
            file_paths: record.file_paths.clone(),
            stored: true,
            description: describe(&record.kind, record.size, record.file_paths.len()),
        }
    }

    /// A view of an item that was not stored: content stays out except for
    /// file paths (which are not secret).
    pub(crate) fn from_item(
        item: &ClipboardItem,
        self_fingerprint: &str,
        origin_alias: Option<String>,
    ) -> Self {
        let kind = item.payload.kind().to_string();
        let (image_width, image_height, file_paths) = match &item.payload {
            ClipboardPayload::Image { width, height, .. } => (Some(*width), Some(*height), vec![]),
            ClipboardPayload::Files { paths } => (
                None,
                None,
                paths
                    .iter()
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect(),
            ),
            ClipboardPayload::Text { .. } => (None, None, vec![]),
        };
        let size = item.payload.size() as u64;
        Self {
            id: item.id.clone(),
            origin: item.origin_device.to_string(),
            origin_alias,
            from_self: item.origin_device.as_str() == self_fingerprint,
            created_at: item.created_at,
            description: describe(&kind, size, file_paths.len()),
            kind,
            size,
            text: None,
            image_path: None,
            image_width,
            image_height,
            file_paths,
            stored: false,
        }
    }
}

fn describe(kind: &str, size: u64, files: usize) -> String {
    match kind {
        "files" => format!("{files} file(s)"),
        other => format!("{other}, {}", format_bytes(size)),
    }
}

/// `1.2 MB` style sizes for descriptions.
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// What the runtime tells its user interface. Tauri forwards each variant
/// as `event:<name>` (see [`RuntimeEvent::name`]).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum RuntimeEvent {
    DeviceFound {
        device: DeviceView,
    },
    DeviceUpdated {
        device: DeviceView,
    },
    DeviceLost {
        fingerprint: String,
    },

    /// Answer with `respond_incoming` unless `auto_accepted`.
    IncomingRequest {
        request: IncomingRequestView,
    },
    IncomingWithdrawn {
        session_id: String,
    },
    /// A received file exists already and the conflict policy is `ask`;
    /// answer with `respond_conflict` within 30 s or it is renamed.
    IncomingConflict {
        session_id: String,
        file_id: String,
        existing: PathBuf,
        renamed: PathBuf,
    },

    /// Throttled to one per file per 100 ms.
    TransferProgress {
        transfer_id: String,
        file_id: String,
        done: u64,
        size: u64,
        total_done: u64,
        total_size: u64,
    },
    TransferFileDone {
        transfer_id: String,
        file: TransferFileView,
    },
    /// State changes other than progress (preparing → active, …).
    TransferUpdated {
        transfer: TransferView,
    },
    TransferCompleted {
        transfer: TransferView,
    },
    /// The receiver asks for a PIN; answer with `provide_pin`.
    TransferNeedsPin {
        transfer_id: String,
        message: String,
    },

    /// Another device wants to pair; answer with `respond_pair_request`.
    PairRequest {
        fingerprint: String,
        alias: String,
        host: String,
        code: String,
    },
    /// The device we asked accepted; confirm the code with `pair_confirm`.
    PairResponse {
        fingerprint: String,
        alias: String,
        code: String,
    },
    PairResult {
        fingerprint: String,
        alias: String,
        paired: bool,
        message: Option<String>,
    },

    ClipboardReceived {
        item: ClipboardView,
    },
    /// Copied on this device (and pushed when sync is on).
    ClipboardLocal {
        item: ClipboardView,
    },
    /// Sync started, stopped, or skipped something.
    ClipboardSync {
        active: bool,
        peers: Vec<String>,
        message: Option<String>,
    },

    Error {
        scope: String,
        message: String,
    },
}

impl RuntimeEvent {
    /// The kebab-case name, e.g. `transfer-progress`.
    pub fn name(&self) -> &'static str {
        match self {
            Self::DeviceFound { .. } => "device-found",
            Self::DeviceUpdated { .. } => "device-updated",
            Self::DeviceLost { .. } => "device-lost",
            Self::IncomingRequest { .. } => "incoming-request",
            Self::IncomingWithdrawn { .. } => "incoming-withdrawn",
            Self::IncomingConflict { .. } => "incoming-conflict",
            Self::TransferProgress { .. } => "transfer-progress",
            Self::TransferFileDone { .. } => "transfer-file-done",
            Self::TransferUpdated { .. } => "transfer-updated",
            Self::TransferCompleted { .. } => "transfer-completed",
            Self::TransferNeedsPin { .. } => "transfer-needs-pin",
            Self::PairRequest { .. } => "pair-request",
            Self::PairResponse { .. } => "pair-response",
            Self::PairResult { .. } => "pair-result",
            Self::ClipboardReceived { .. } => "clipboard-received",
            Self::ClipboardLocal { .. } => "clipboard-local",
            Self::ClipboardSync { .. } => "clipboard-sync",
            Self::Error { .. } => "error",
        }
    }
}
