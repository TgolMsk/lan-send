//! The devices discovery has confirmed, keyed by fingerprint.

use crate::protocol::{DeviceInfo, DeviceType, Extensions, Fingerprint, PeerInfo, ProtocolType};
use crate::store::{KnownDevice, unix_now};
use crate::transport::Target;
use parking_lot::RwLock;
use std::net::IpAddr;
use std::time::SystemTime;

/// A device that answered us at least once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub fingerprint: Fingerprint,
    pub alias: String,
    pub version: String,
    pub device_model: Option<String>,
    pub device_type: Option<DeviceType>,
    /// The address it was last reached at.
    pub host: String,
    pub port: u16,
    pub protocol: ProtocolType,
    pub download: bool,
    /// Its `x-lanext` block, absent for official clients.
    pub ext: Option<Extensions>,
    pub last_seen: SystemTime,
    /// Every address the device was confirmed at, most recent first.
    pub addresses: Vec<Target>,
}

impl Device {
    /// Where to reach the device.
    pub fn target(&self) -> Target {
        Target {
            host: self.host.clone(),
            port: self.port,
            protocol: self.protocol,
        }
    }

    /// Whether the device announced the extension feature.
    pub fn supports(&self, feature: &str) -> bool {
        self.ext.as_ref().is_some_and(|ext| ext.supports(feature))
    }

    /// A device that registered with us from `host`.
    pub fn from_info(host: IpAddr, info: &DeviceInfo, fingerprint: Fingerprint) -> Self {
        Self {
            fingerprint,
            alias: info.alias.clone(),
            version: info.version.clone(),
            device_model: info.device_model.clone(),
            device_type: info.device_type,
            host: host.to_string(),
            port: info.port,
            protocol: info.protocol,
            download: info.download,
            ext: info.ext.clone(),
            last_seen: SystemTime::now(),
            addresses: vec![Target {
                host: host.to_string(),
                port: info.port,
                protocol: info.protocol,
            }],
        }
    }

    /// A device that answered our register request at `target`.
    pub fn from_peer_info(target: &Target, info: &PeerInfo, fingerprint: Fingerprint) -> Self {
        Self {
            fingerprint,
            alias: info.alias.clone(),
            version: info.version.clone(),
            device_model: info.device_model.clone(),
            device_type: info.device_type,
            host: target.host.clone(),
            port: target.port,
            protocol: target.protocol,
            download: info.download,
            ext: info.ext.clone(),
            last_seen: SystemTime::now(),
            addresses: vec![target.clone()],
        }
    }

    /// Whether the device was confirmed at `host` (and `port`, when given).
    pub fn reachable_at(&self, host: &str, port: Option<u16>) -> bool {
        self.addresses
            .iter()
            .any(|address| address.host == host && port.is_none_or(|port| address.port == port))
    }
}

impl From<&Device> for KnownDevice {
    /// The persisted form of a discovered device, seen now.
    fn from(device: &Device) -> Self {
        let now = unix_now();
        Self {
            fingerprint: device.fingerprint.to_string(),
            alias: device.alias.clone(),
            custom_alias: None,
            device_type: device.device_type.map(|kind| kind.to_string()),
            device_model: device.device_model.clone(),
            version: Some(device.version.clone()),
            host: Some(device.host.clone()),
            port: Some(device.port),
            protocol: Some(device.protocol.to_string()),
            favorite: false,
            paired: false,
            first_seen: now,
            last_seen: now,
        }
    }
}

/// How many addresses are remembered per device.
const MAX_ADDRESSES: usize = 8;

#[derive(Default)]
pub(super) struct DeviceStore {
    devices: RwLock<Vec<Device>>,
}

impl DeviceStore {
    /// Inserts or refreshes a device. Returns whether it is new and the
    /// stored state. Addresses are merged, most recent first.
    pub fn upsert(&self, mut device: Device) -> (bool, Device) {
        let mut devices = self.devices.write();
        match devices
            .iter_mut()
            .find(|known| known.fingerprint == device.fingerprint)
        {
            Some(known) => {
                for address in known.addresses.drain(..) {
                    if !device.addresses.contains(&address) {
                        device.addresses.push(address);
                    }
                }
                device.addresses.truncate(MAX_ADDRESSES);
                *known = device;
                (false, known.clone())
            }
            None => {
                devices.push(device.clone());
                (true, device)
            }
        }
    }

    pub fn devices(&self) -> Vec<Device> {
        self.devices.read().clone()
    }

    pub fn by_fingerprint(&self, fingerprint: &Fingerprint) -> Option<Device> {
        self.devices
            .read()
            .iter()
            .find(|device| &device.fingerprint == fingerprint)
            .cloned()
    }
}
