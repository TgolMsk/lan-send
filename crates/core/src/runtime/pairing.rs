//! Pairing (ADR-0010) driven by events: incoming requests wait for
//! `respond_pair_request`, outgoing ones for `pair_confirm`.

use super::{ErrorCode, Inner, RuntimeError};
use crate::discovery::Device;
use crate::protocol::{Fingerprint, ProtocolType, verification_code};
use crate::runtime::{PairView, RuntimeEvent};
use crate::store::{KnownDevice, unix_now};
use crate::transport::{Client, ClientError, Peer, Target};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;

/// How long we wait for the other device's user to confirm.
const PAIR_TIMEOUT: Duration = Duration::from_secs(90);

/// Another device asked to pair; the user has not answered yet.
pub(crate) struct PendingPairRequest {
    pub peer: Peer,
    pub alias: String,
    pub decision: oneshot::Sender<bool>,
}

/// We asked a device, it accepted, and our user has to confirm the code.
pub(crate) struct PendingPairConfirm {
    pub device: Device,
    pub target: Target,
    pub alias: String,
}

impl Inner {
    pub(crate) fn on_pair_request(
        &self,
        peer: Peer,
        alias: String,
        code: String,
        decision: oneshot::Sender<bool>,
    ) {
        let Some(fingerprint) = peer.cert_fingerprint.clone() else {
            let _ = decision.send(false);
            return;
        };
        let host = peer.host();
        let previous = self.pending.lock().pair_requests.insert(
            fingerprint.to_string(),
            PendingPairRequest {
                peer,
                alias: alias.clone(),
                decision,
            },
        );
        if let Some(previous) = previous {
            let _ = previous.decision.send(false);
        }
        self.emit(RuntimeEvent::PairRequest {
            fingerprint: fingerprint.to_string(),
            alias,
            host,
            code,
        });
    }

    pub(crate) fn respond_pair_request(
        &self,
        fingerprint: &str,
        accept: bool,
    ) -> Result<(), RuntimeError> {
        let pending = self
            .pending
            .lock()
            .pair_requests
            .remove(fingerprint)
            .ok_or(RuntimeError::NothingPending)?;
        if accept {
            self.apply_pairing(&pending.peer, &pending.alias, true);
        }
        let _ = pending.decision.send(accept);
        self.emit(RuntimeEvent::PairResult {
            fingerprint: fingerprint.to_string(),
            alias: pending.alias,
            paired: accept,
            code: None,
            message: None,
        });
        self.refresh_clipboard_peers();
        self.emit_device_changed(fingerprint);
        Ok(())
    }

    pub(crate) fn on_unpaired(&self, peer: &Peer) {
        let Some(fingerprint) = &peer.cert_fingerprint else {
            return;
        };
        self.apply_pairing(peer, "", false);
        self.emit(RuntimeEvent::PairResult {
            fingerprint: fingerprint.to_string(),
            alias: self
                .alias_of(fingerprint.as_str())
                .unwrap_or_else(|| peer.addr.to_string()),
            paired: false,
            code: Some(ErrorCode::PairWithdrawn),
            message: Some("the other device withdrew the pairing".into()),
        });
        self.refresh_clipboard_peers();
        self.emit_device_changed(fingerprint.as_str());
    }

    /// Persists a pairing decision for `peer`, creating the device record
    /// when it is not known yet.
    pub(crate) fn apply_pairing(&self, peer: &Peer, alias: &str, paired: bool) {
        let Some(fingerprint) = &peer.cert_fingerprint else {
            return;
        };
        let known = self
            .db
            .device(fingerprint.as_str())
            .ok()
            .flatten()
            .is_some();
        if !known {
            let now = unix_now();
            let _ = self.db.upsert_device(&KnownDevice {
                fingerprint: fingerprint.to_string(),
                alias: alias.to_string(),
                custom_alias: None,
                device_type: None,
                device_model: None,
                version: None,
                host: Some(peer.host()),
                port: None,
                protocol: Some("https".into()),
                favorite: false,
                paired: false,
                first_seen: now,
                last_seen: now,
            });
        }
        if let Err(err) = self.db.set_paired(fingerprint.as_str(), paired) {
            tracing::warn!("could not store the pairing: {err}");
        }
    }

    pub(crate) async fn pair_start(
        self: &Arc<Self>,
        query: &str,
    ) -> Result<PairView, RuntimeError> {
        let (device, target) = self.resolve_device(query).await?;
        let code = verification_code(self.identity.fingerprint(), &device.fingerprint);
        let fingerprint = device.fingerprint.to_string();
        let view = PairView {
            fingerprint: fingerprint.clone(),
            alias: self
                .alias_of(&fingerprint)
                .unwrap_or_else(|| device.alias.clone()),
            code: code.clone(),
        };
        let this = self.clone();
        let alias = self.alias.clone();
        self.tasks.spawn(async move {
            let client = match Client::new(
                &this.identity,
                Some(device.fingerprint.clone()),
                Some(PAIR_TIMEOUT),
            ) {
                Ok(client) => client,
                Err(err) => {
                    this.emit(RuntimeEvent::PairResult {
                        fingerprint,
                        alias: device.alias,
                        paired: false,
                        code: Some(ErrorCode::from_client(&err)),
                        message: Some(err.to_string()),
                    });
                    return;
                }
            };
            match client.pair(&target, &alias).await {
                Ok(response) => {
                    let alias = if response.alias.trim().is_empty() {
                        device.alias.clone()
                    } else {
                        response.alias.clone()
                    };
                    this.pending.lock().pair_confirms.insert(
                        fingerprint.clone(),
                        PendingPairConfirm {
                            device,
                            target,
                            alias: alias.clone(),
                        },
                    );
                    this.emit(RuntimeEvent::PairResponse {
                        fingerprint,
                        alias,
                        code,
                    });
                }
                Err(err) => {
                    let (error_code, message) = match &err {
                        ClientError::Status { status: 403, .. } => (
                            ErrorCode::PairDeclined,
                            "the other device declined".to_string(),
                        ),
                        ClientError::Status { status: 408, .. } => (
                            ErrorCode::PairTimeout,
                            "the other device did not answer in time".to_string(),
                        ),
                        ClientError::Status { status: 409, .. } => (
                            ErrorCode::PairBusy,
                            "the other device is busy with another pairing".to_string(),
                        ),
                        ClientError::Status { status: 404, .. } => (
                            ErrorCode::PairUnsupported,
                            "the other device does not support pairing (official LocalSend?)"
                                .to_string(),
                        ),
                        other => (ErrorCode::from_client(other), other.to_string()),
                    };
                    this.emit(RuntimeEvent::PairResult {
                        fingerprint,
                        alias: device.alias,
                        paired: false,
                        code: Some(error_code),
                        message: Some(message),
                    });
                }
            }
        });
        Ok(view)
    }

    pub(crate) async fn pair_confirm(
        &self,
        fingerprint: &str,
        matches: bool,
    ) -> Result<(), RuntimeError> {
        let pending = self
            .pending
            .lock()
            .pair_confirms
            .remove(fingerprint)
            .ok_or(RuntimeError::NothingPending)?;
        if matches {
            let mut known = KnownDevice::from(&pending.device);
            known.host = Some(pending.target.host.clone());
            known.port = Some(pending.target.port);
            self.db.upsert_device(&known)?;
            self.db.set_paired(fingerprint, true)?;
            self.server.add_paired(pending.device.fingerprint.clone());
            self.refresh_clipboard_peers();
            self.emit(RuntimeEvent::PairResult {
                fingerprint: fingerprint.to_string(),
                alias: pending.alias,
                paired: true,
                code: None,
                message: None,
            });
        } else {
            if let Ok(client) = Client::new(
                &self.identity,
                Some(pending.device.fingerprint.clone()),
                Some(Duration::from_secs(10)),
            ) {
                let _ = client.unpair(&pending.target).await;
            }
            self.emit(RuntimeEvent::PairResult {
                fingerprint: fingerprint.to_string(),
                alias: pending.alias,
                paired: false,
                code: Some(ErrorCode::PairCodeMismatch),
                message: Some("the codes did not match".into()),
            });
        }
        self.emit_device_changed(fingerprint);
        Ok(())
    }

    pub(crate) async fn unpair(&self, fingerprint: &str) -> Result<(), RuntimeError> {
        let known = self.db.device(fingerprint)?;
        let was_paired = known.as_ref().is_some_and(|known| known.paired);
        let parsed = Fingerprint::parse(fingerprint);
        self.db.set_paired(fingerprint, false)?;
        self.server.remove_paired(&parsed);
        self.refresh_clipboard_peers();
        if was_paired {
            let target = self
                .discovery
                .device_by_fingerprint(&parsed)
                .map(|device| device.target())
                .or_else(|| {
                    let known = known.as_ref()?;
                    Some(Target {
                        host: known.host.clone()?,
                        port: known.port?,
                        protocol: ProtocolType::Https,
                    })
                });
            if let Some(target) = target
                && let Ok(client) = Client::new(
                    &self.identity,
                    Some(parsed.clone()),
                    Some(Duration::from_secs(10)),
                )
            {
                let _ = client.unpair(&target).await;
            }
            self.emit(RuntimeEvent::PairResult {
                fingerprint: fingerprint.to_string(),
                alias: self.alias_of(fingerprint).unwrap_or_default(),
                paired: false,
                code: None,
                message: None,
            });
        }
        self.emit_device_changed(fingerprint);
        Ok(())
    }
}
