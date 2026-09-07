//! The synchronisation engine: watches the local clipboard, pushes changes
//! to paired peers, applies what peers send, and prevents loops with a ring
//! of recently written hashes (ADR-0011).

use super::backend::{ClipboardBackend, ClipboardError, with_retry};
use super::model::{ClipboardItem, ClipboardPayload, PayloadKind};
use super::{DEFAULT_IMAGE_LIMIT, DEFAULT_POLL_INTERVAL, DEFAULT_TEXT_LIMIT};
use crate::protocol::Fingerprint;
use crate::transport::{Client, Identity, Target};
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// How many hashes of recently written content are remembered.
const RECENT_HASHES: usize = 10;

/// A paired device to push to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeerTarget {
    pub fingerprint: Fingerprint,
    pub alias: String,
    pub target: Target,
}

pub struct SyncConfig {
    pub backend: Arc<dyn ClipboardBackend>,
    pub identity: Arc<Identity>,
    /// This device's fingerprint, the origin of local items.
    pub origin: Fingerprint,
    pub peers: Vec<PeerTarget>,
    pub text_limit: usize,
    pub image_limit: usize,
    pub poll_interval: Duration,
    pub events: mpsc::Sender<SyncEvent>,
}

impl SyncConfig {
    pub fn new(
        backend: Arc<dyn ClipboardBackend>,
        identity: Arc<Identity>,
        events: mpsc::Sender<SyncEvent>,
    ) -> Self {
        let origin = identity.fingerprint().clone();
        Self {
            backend,
            identity,
            origin,
            peers: Vec::new(),
            text_limit: DEFAULT_TEXT_LIMIT,
            image_limit: DEFAULT_IMAGE_LIMIT,
            poll_interval: DEFAULT_POLL_INTERVAL,
            events,
        }
    }
}

#[derive(Debug)]
pub enum SyncEvent {
    /// The local clipboard changed and the item is being pushed.
    LocalChange(ClipboardItem),
    /// The local clipboard holds files. They are not pushed through the
    /// clipboard endpoint: the application sends them as a file transfer
    /// with the clipboard intent (ADR-0011).
    FilesCopied(ClipboardItem),
    /// A push to one peer finished.
    Pushed {
        item_id: String,
        peer: Fingerprint,
        result: Result<(), String>,
    },
    /// A local change was not synchronised because of its size.
    TooLarge {
        kind: PayloadKind,
        size: usize,
        limit: usize,
    },
    /// A remote item was written to the local clipboard.
    Applied(ClipboardItem),
    /// The clipboard could not be read or written.
    Error(String),
}

/// Ring of the hashes most recently written to the local clipboard, so the
/// watcher does not push them back to their origin.
#[derive(Default)]
pub struct RecentHashes {
    ring: VecDeque<[u8; 32]>,
}

impl RecentHashes {
    pub fn remember(&mut self, hash: [u8; 32]) {
        if self.ring.len() >= RECENT_HASHES {
            self.ring.pop_front();
        }
        self.ring.push_back(hash);
    }

    pub fn contains(&self, hash: &[u8; 32]) -> bool {
        self.ring.contains(hash)
    }
}

struct Inner {
    backend: Arc<dyn ClipboardBackend>,
    identity: Arc<Identity>,
    origin: Fingerprint,
    peers: Mutex<Vec<PeerTarget>>,
    text_limit: usize,
    image_limit: usize,
    events: mpsc::Sender<SyncEvent>,
    recent: Mutex<RecentHashes>,
}

/// A running synchronisation.
#[derive(Clone)]
pub struct ClipboardSync {
    inner: Arc<Inner>,
    cancel: CancellationToken,
}

impl ClipboardSync {
    /// Starts watching the local clipboard.
    pub fn start(config: SyncConfig) -> Self {
        let inner = Arc::new(Inner {
            backend: config.backend,
            identity: config.identity,
            origin: config.origin,
            peers: Mutex::new(config.peers),
            text_limit: config.text_limit,
            image_limit: config.image_limit,
            events: config.events,
            recent: Mutex::new(RecentHashes::default()),
        });
        let cancel = CancellationToken::new();
        tokio::spawn(watch_loop(
            inner.clone(),
            config.poll_interval,
            cancel.clone(),
        ));
        Self { inner, cancel }
    }

    /// Replaces the peers changes are pushed to.
    pub fn set_peers(&self, peers: Vec<PeerTarget>) {
        *self.inner.peers.lock() = peers;
    }

    pub fn peers(&self) -> Vec<PeerTarget> {
        self.inner.peers.lock().clone()
    }

    /// Reads the clipboard now and pushes it regardless of the loop guard
    /// (the user asked for it). Returns the item, or `None` when empty.
    pub async fn push_now(&self) -> Result<Option<ClipboardItem>, ClipboardError> {
        let backend = self.inner.backend.clone();
        let payload = tokio::task::spawn_blocking(move || with_retry(|| backend.read()))
            .await
            .map_err(|err| ClipboardError::Other(err.to_string()))??;
        let Some(payload) = payload else {
            return Ok(None);
        };
        let item = ClipboardItem::new(self.inner.origin.clone(), payload);
        self.inner.push(item.clone()).await;
        Ok(Some(item))
    }

    /// Marks content as written by us, so the watcher ignores it when it
    /// shows up on the local clipboard.
    pub fn remember(&self, hash: [u8; 32]) {
        self.inner.recent.lock().remember(hash);
    }

    /// Writes a remote item to the local clipboard, remembering its hash so
    /// it is not pushed back.
    pub async fn apply_remote(&self, item: ClipboardItem) -> Result<(), ClipboardError> {
        self.inner.recent.lock().remember(item.content_hash);
        let backend = self.inner.backend.clone();
        let payload = item.payload.clone();
        tokio::task::spawn_blocking(move || with_retry(|| backend.write(&payload)))
            .await
            .map_err(|err| ClipboardError::Other(err.to_string()))??;
        let _ = self.inner.events.send(SyncEvent::Applied(item)).await;
        Ok(())
    }

    /// Stops the watcher.
    pub fn stop(&self) {
        self.cancel.cancel();
    }
}

impl Inner {
    /// Pushes `item` to every peer concurrently.
    async fn push(&self, item: ClipboardItem) {
        let peers = self.peers.lock().clone();
        let _ = self.events.send(SyncEvent::LocalChange(item.clone())).await;
        let mut tasks = Vec::new();
        for peer in peers {
            let identity = self.identity.clone();
            let item = item.clone();
            let events = self.events.clone();
            tasks.push(tokio::spawn(async move {
                let result = async {
                    let client = Client::new(&identity, Some(peer.fingerprint.clone()), None)?;
                    client.send_clipboard(&peer.target, &item).await
                }
                .await
                .map_err(|err| err.to_string());
                let _ = events
                    .send(SyncEvent::Pushed {
                        item_id: item.id,
                        peer: peer.fingerprint,
                        result,
                    })
                    .await;
            }));
        }
        for task in tasks {
            let _ = task.await;
        }
    }

    fn limit_for(&self, payload: &ClipboardPayload) -> usize {
        match payload.kind() {
            PayloadKind::Text => self.text_limit,
            PayloadKind::Image => self.image_limit,
            PayloadKind::Files => usize::MAX,
        }
    }
}

async fn watch_loop(inner: Arc<Inner>, poll_interval: Duration, cancel: CancellationToken) {
    loop {
        if cancel.is_cancelled() {
            return;
        }
        let backend = inner.backend.clone();
        let changed =
            tokio::task::spawn_blocking(move || backend.wait_for_change(poll_interval)).await;
        let changed = match changed {
            Ok(Ok(changed)) => changed,
            Ok(Err(err)) => {
                let _ = inner
                    .events
                    .send(SyncEvent::Error(format!("watching the clipboard: {err}")))
                    .await;
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
            Err(_) => return,
        };
        if !changed || cancel.is_cancelled() {
            continue;
        }
        let backend = inner.backend.clone();
        let payload = match tokio::task::spawn_blocking(move || with_retry(|| backend.read())).await
        {
            Ok(Ok(Some(payload))) => payload,
            Ok(Ok(None)) => continue,
            Ok(Err(err)) => {
                let _ = inner
                    .events
                    .send(SyncEvent::Error(format!("reading the clipboard: {err}")))
                    .await;
                continue;
            }
            Err(_) => return,
        };
        let hash = payload.content_hash();
        if inner.recent.lock().contains(&hash) {
            // Something we wrote ourselves came back: do not echo it.
            continue;
        }
        inner.recent.lock().remember(hash);
        let limit = inner.limit_for(&payload);
        if payload.size() > limit {
            let _ = inner
                .events
                .send(SyncEvent::TooLarge {
                    kind: payload.kind(),
                    size: payload.size(),
                    limit,
                })
                .await;
            continue;
        }
        let item = ClipboardItem::new(inner.origin.clone(), payload);
        if matches!(item.payload, ClipboardPayload::Files { .. }) {
            let _ = inner.events.send(SyncEvent::FilesCopied(item)).await;
            continue;
        }
        inner.push(item).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_keeps_the_last_ten() {
        let mut recent = RecentHashes::default();
        for i in 0..12u8 {
            recent.remember([i; 32]);
        }
        assert!(!recent.contains(&[0; 32]));
        assert!(!recent.contains(&[1; 32]));
        assert!(recent.contains(&[2; 32]));
        assert!(recent.contains(&[11; 32]));
    }
}
