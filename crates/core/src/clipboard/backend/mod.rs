//! The platform clipboard behind one trait. Implementations live in
//! `macos.rs` and `windows.rs`; other platforms have no backend.

use super::model::ClipboardPayload;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum ClipboardError {
    /// Another process holds the clipboard; retried by the engine.
    #[error("clipboard is busy")]
    Busy,

    #[error("clipboard is not available on this platform")]
    Unsupported,

    #[error("clipboard error: {0}")]
    Other(String),
}

/// Access to the system clipboard.
pub trait ClipboardBackend: Send + Sync {
    /// Blocks up to `timeout` and reports whether the clipboard changed
    /// since the previous call. Polling backends sleep and compare a change
    /// counter; event-driven backends wait for the notification.
    fn wait_for_change(&self, timeout: Duration) -> Result<bool, ClipboardError>;

    /// The current content, or `None` when it is empty or of an unsupported
    /// type.
    fn read(&self) -> Result<Option<ClipboardPayload>, ClipboardError>;

    fn write(&self, payload: &ClipboardPayload) -> Result<(), ClipboardError>;
}

/// The backend of the platform this binary runs on, when there is one.
pub fn platform_backend() -> Option<Arc<dyn ClipboardBackend>> {
    platform::backend()
}

/// Retries `operation` on [`ClipboardError::Busy`] with exponential
/// backoff (50, 100, 200 ms), then gives up.
pub fn with_retry<T>(
    mut operation: impl FnMut() -> Result<T, ClipboardError>,
) -> Result<T, ClipboardError> {
    let mut delay = Duration::from_millis(50);
    for attempt in 0..4 {
        match operation() {
            Err(ClipboardError::Busy) if attempt < 3 => {
                std::thread::sleep(delay);
                delay *= 2;
            }
            result => return result,
        }
    }
    Err(ClipboardError::Busy)
}

/// An in-memory clipboard for tests and headless environments.
#[derive(Default)]
pub struct MemoryClipboard {
    inner: parking_lot::Mutex<MemoryState>,
}

#[derive(Default)]
struct MemoryState {
    content: Option<ClipboardPayload>,
    change_count: u64,
    seen_count: u64,
}

impl MemoryClipboard {
    /// Simulates the user copying something.
    pub fn set(&self, payload: ClipboardPayload) {
        let mut inner = self.inner.lock();
        inner.content = Some(payload);
        inner.change_count += 1;
    }

    pub fn get(&self) -> Option<ClipboardPayload> {
        self.inner.lock().content.clone()
    }
}

impl ClipboardBackend for MemoryClipboard {
    fn wait_for_change(&self, timeout: Duration) -> Result<bool, ClipboardError> {
        std::thread::sleep(timeout.min(Duration::from_millis(20)));
        let mut inner = self.inner.lock();
        let changed = inner.change_count != inner.seen_count;
        inner.seen_count = inner.change_count;
        Ok(changed)
    }

    fn read(&self) -> Result<Option<ClipboardPayload>, ClipboardError> {
        Ok(self.inner.lock().content.clone())
    }

    fn write(&self, payload: &ClipboardPayload) -> Result<(), ClipboardError> {
        let mut inner = self.inner.lock();
        inner.content = Some(payload.clone());
        inner.change_count += 1;
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

mod platform {
    use super::ClipboardBackend;
    use std::sync::Arc;

    #[cfg(target_os = "macos")]
    pub(super) fn backend() -> Option<Arc<dyn ClipboardBackend>> {
        Some(Arc::new(super::macos::MacClipboard::new()))
    }

    #[cfg(target_os = "windows")]
    pub(super) fn backend() -> Option<Arc<dyn ClipboardBackend>> {
        super::windows::WindowsClipboard::new()
            .ok()
            .map(|backend| Arc::new(backend) as Arc<dyn ClipboardBackend>)
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    pub(super) fn backend() -> Option<Arc<dyn ClipboardBackend>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn retries_busy_then_gives_up() {
        let calls = Cell::new(0);
        let result: Result<(), _> = with_retry(|| {
            calls.set(calls.get() + 1);
            Err(ClipboardError::Busy)
        });
        assert!(matches!(result, Err(ClipboardError::Busy)));
        assert_eq!(calls.get(), 4);

        let calls = Cell::new(0);
        let result = with_retry(|| {
            calls.set(calls.get() + 1);
            if calls.get() < 2 {
                Err(ClipboardError::Busy)
            } else {
                Ok(42)
            }
        });
        assert_eq!(result.unwrap(), 42);
    }
}
