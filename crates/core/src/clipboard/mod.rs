//! Clipboard synchronisation (ADR-0011): the platform-independent model,
//! the wire format of `/api/ext/v1/clipboard`, the sync engine and the
//! platform backends.

pub mod backend;
pub mod model;
pub mod sensitive;
pub mod sync;
pub mod wire;

pub use backend::{ClipboardBackend, ClipboardError, platform_backend};
pub use model::{ClipboardItem, ClipboardPayload, ImageFormat, PayloadKind};
pub use sync::{ClipboardSync, PeerTarget, SyncConfig, SyncEvent};

/// Default size limit for synchronised text (1 MB).
pub const DEFAULT_TEXT_LIMIT: usize = 1024 * 1024;
/// Default size limit for synchronised images (10 MB).
pub const DEFAULT_IMAGE_LIMIT: usize = 10 * 1024 * 1024;
/// Images above this size are sent as multipart instead of base64 JSON.
pub const MULTIPART_THRESHOLD: usize = 512 * 1024;
/// Default poll interval of the macOS change-count watcher.
pub const DEFAULT_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(300);
/// Path of the clipboard endpoint.
pub const CLIPBOARD_PATH: &str = "/api/ext/v1/clipboard";
