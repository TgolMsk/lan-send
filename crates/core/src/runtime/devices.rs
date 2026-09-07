//! Device list: discovery merged with the database, liveness probing.

use super::{Inner, RECENT_DEVICE_WINDOW};
use crate::discovery::{Device, DiscoveryEvent};
use crate::protocol::{Fingerprint, ProtocolType};
use crate::runtime::{DeviceView, RuntimeEvent};
use crate::store::KnownDevice;
use crate::transport::Target;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};
use tokio::sync::broadcast;

/// How often online devices are probed.
const LIVENESS_INTERVAL: Duration = Duration::from_secs(30);
/// Failed probes in a row before a device counts as gone.
const LIVENESS_FAILURES: u8 = 2;

impl Inner {
    /// Discovery's live devices merged with the database, minus this device.
    pub(crate) fn device_views(&self) -> Vec<DeviceView> {
        let me = self.identity.fingerprint().as_str();
        let offline = self.offline.lock().clone();
        let known: HashMap<String, KnownDevice> = self
            .db
            .list_devices()
            .unwrap_or_default()
            .into_iter()
            .map(|device| (device.fingerprint.clone(), device))
            .collect();
        let mut views: HashMap<String, DeviceView> = HashMap::new();
        for device in self.discovery.devices() {
            let online = !offline.contains(&device.fingerprint);
            let view = merge(&device, known.get(device.fingerprint.as_str()), online);
            views.insert(view.fingerprint.clone(), view);
        }
        for (fingerprint, device) in &known {
            views
                .entry(fingerprint.clone())
                .or_insert_with(|| DeviceView::from(device));
        }
        views.remove(me);
        let mut views: Vec<DeviceView> = views.into_values().collect();
        views.sort_by(|a, b| {
            b.online
                .cmp(&a.online)
                .then(b.favorite.cmp(&a.favorite))
                .then(b.paired.cmp(&a.paired))
                .then_with(|| {
                    a.display_name
                        .to_lowercase()
                        .cmp(&b.display_name.to_lowercase())
                })
        });
        views
    }

    /// The current view of one device, live or remembered.
    pub(crate) fn device_view(&self, fingerprint: &str) -> Option<DeviceView> {
        let known = self.db.device(fingerprint).ok().flatten();
        match self
            .discovery
            .device_by_fingerprint(&Fingerprint::parse(fingerprint))
        {
            Some(device) => {
                let online = !self.offline.lock().contains(&device.fingerprint);
                Some(merge(&device, known.as_ref(), online))
            }
            None => known.as_ref().map(DeviceView::from),
        }
    }

    /// Emits `device-updated` for a device whose stored attributes changed.
    pub(crate) fn emit_device_changed(&self, fingerprint: &str) {
        if let Some(view) = self.device_view(fingerprint) {
            self.last_views
                .lock()
                .insert(fingerprint.to_string(), view.clone());
            self.emit(RuntimeEvent::DeviceUpdated { device: view });
        }
    }

    /// The name to show for a peer: custom name, remembered alias or the
    /// alias discovery saw.
    pub(crate) fn alias_of(&self, fingerprint: &str) -> Option<String> {
        if let Ok(Some(known)) = self.db.device(fingerprint) {
            return Some(known.display_name().to_string());
        }
        self.discovery
            .device_by_fingerprint(&Fingerprint::parse(fingerprint))
            .map(|device| device.alias)
    }

    pub(crate) fn is_paired(&self, fingerprint: &Fingerprint) -> bool {
        self.db
            .device(fingerprint.as_str())
            .ok()
            .flatten()
            .is_some_and(|known| known.paired)
    }

    /// Addresses worth probing: favourites and recently seen devices.
    pub(crate) fn known_targets(&self) -> Vec<Target> {
        self.db
            .devices_to_probe(RECENT_DEVICE_WINDOW)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|device| {
                Some(Target {
                    host: device.host?,
                    port: device.port?,
                    protocol: match device.protocol.as_deref() {
                        Some("http") => ProtocolType::Http,
                        _ => ProtocolType::Https,
                    },
                })
            })
            .collect()
    }

    /// Announce, probe known addresses, scan when nothing answers.
    pub(crate) fn kick_discovery(self: &Arc<Self>) {
        if !self.active_discovery {
            return;
        }
        let discovery = self.discovery.clone();
        let known = self.known_targets();
        let cancel = self.cancel.clone();
        self.tasks.spawn(async move {
            tokio::select! {
                _ = discovery.discover_staged(known, Duration::from_secs(2)) => {}
                _ = cancel.cancelled() => {}
            }
        });
    }
}

fn merge(device: &Device, known: Option<&KnownDevice>, online: bool) -> DeviceView {
    let last_seen = device
        .last_seen
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    DeviceView {
        fingerprint: device.fingerprint.to_string(),
        alias: device.alias.clone(),
        custom_alias: known.and_then(|k| k.custom_alias.clone()),
        display_name: known
            .and_then(|k| k.custom_alias.clone())
            .unwrap_or_else(|| device.alias.clone()),
        device_type: device.device_type.map(|t| t.as_str().to_string()),
        device_model: device.device_model.clone(),
        version: Some(device.version.clone()),
        host: Some(device.host.clone()),
        port: Some(device.port),
        online,
        favorite: known.is_some_and(|k| k.favorite),
        paired: known.is_some_and(|k| k.paired),
        features: device
            .ext
            .as_ref()
            .map(|ext| ext.features.clone())
            .unwrap_or_default(),
        last_seen: last_seen.max(known.map(|k| k.last_seen).unwrap_or(0)),
    }
}

fn same_except_time(a: &DeviceView, b: &DeviceView) -> bool {
    let mut a = a.clone();
    let mut b = b.clone();
    a.last_seen = 0;
    b.last_seen = 0;
    a == b
}

/// Keeps the database current and forwards discovery results as
/// `device-found` / `device-updated` (only when something changed).
pub(crate) async fn forward_discovery(
    inner: Arc<Inner>,
    mut events: broadcast::Receiver<DiscoveryEvent>,
) {
    loop {
        let event = tokio::select! {
            _ = inner.cancel.cancelled() => break,
            event = events.recv() => event,
        };
        let (device, found) = match event {
            Ok(DiscoveryEvent::Found(device)) => (device, true),
            Ok(DiscoveryEvent::Updated(device)) => (device, false),
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => break,
        };
        if &device.fingerprint == inner.identity.fingerprint() {
            continue;
        }
        if let Err(err) = inner.db.upsert_device(&KnownDevice::from(&device)) {
            tracing::warn!("could not remember {}: {err}", device.alias);
        }
        let was_offline = inner.offline.lock().remove(&device.fingerprint);
        let known = inner.db.device(device.fingerprint.as_str()).ok().flatten();
        let view = merge(&device, known.as_ref(), true);
        let changed = {
            let mut last = inner.last_views.lock();
            let changed = last
                .get(&view.fingerprint)
                .is_none_or(|previous| !same_except_time(previous, &view));
            last.insert(view.fingerprint.clone(), view.clone());
            changed
        };
        if known.as_ref().is_some_and(|k| k.paired) {
            inner.refresh_clipboard_peers();
        }
        if found || was_offline {
            inner.emit(RuntimeEvent::DeviceFound { device: view });
        } else if changed {
            inner.emit(RuntimeEvent::DeviceUpdated { device: view });
        }
    }
}

/// Probes every live device periodically; two misses in a row mean
/// `device-lost`. LocalSend peers do not announce periodically, so this is
/// the only way to notice that one went away.
pub(crate) async fn liveness(inner: Arc<Inner>) {
    let mut failures: HashMap<Fingerprint, u8> = HashMap::new();
    let mut interval = tokio::time::interval(LIVENESS_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    interval.tick().await;
    loop {
        tokio::select! {
            _ = inner.cancel.cancelled() => break,
            _ = interval.tick() => {}
        }
        let me = inner.identity.fingerprint().clone();
        let devices: Vec<Device> = inner
            .discovery
            .devices()
            .into_iter()
            .filter(|device| device.fingerprint != me)
            .collect();
        let probes = devices.iter().map(|device| {
            let discovery = inner.discovery.clone();
            let target = device.target();
            async move { discovery.probe(&target).await.ok().flatten().is_some() }
        });
        let results = futures_util::future::join_all(probes).await;
        let live: HashSet<Fingerprint> = devices.iter().map(|d| d.fingerprint.clone()).collect();
        failures.retain(|fingerprint, _| live.contains(fingerprint));
        for (device, alive) in devices.iter().zip(results) {
            if alive {
                failures.remove(&device.fingerprint);
                continue;
            }
            let count = failures.entry(device.fingerprint.clone()).or_insert(0);
            *count = count.saturating_add(1);
            if *count >= LIVENESS_FAILURES
                && inner.offline.lock().insert(device.fingerprint.clone())
            {
                inner.last_views.lock().remove(device.fingerprint.as_str());
                inner.emit(RuntimeEvent::DeviceLost {
                    fingerprint: device.fingerprint.to_string(),
                });
            }
        }
    }
}
