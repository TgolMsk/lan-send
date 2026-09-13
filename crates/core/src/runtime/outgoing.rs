//! Sending: device lookup, prepare-upload with the PIN dance, parallel
//! uploads with retries and resume, history entries.

use super::{ErrorCode, Inner, RuntimeError};
use crate::discovery::{Device, parse_host_port};
use crate::protocol::{
    DEFAULT_PORT, FEATURE_RESUME, Fingerprint, INTENT_CLIPBOARD, PrepareUploadRequest, ProtocolType,
};
use crate::runtime::{FileState, RuntimeEvent, TransferFileView, TransferState, TransferView};
use crate::store::{Direction, TransferRecord, TransferStatus, unix_now};
use crate::transfer::{CollectOptions, OutgoingFile, collect};
use crate::transport::{Client, ClientError, PrepareUploadOutcome, Resume, Target};
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

/// Attempts per file (the receiver allows the same number).
const MAX_ATTEMPTS: u32 = 3;

/// What to send and to whom.
#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SendRequest {
    /// Fingerprint (or prefix), name, or `host[:port]`.
    pub device: String,
    pub paths: Vec<PathBuf>,
    /// Known PIN; otherwise a `transfer-needs-pin` event asks for it.
    pub pin: Option<String>,
    /// `x-lanext.intent`, e.g. the clipboard intent. Not set by interfaces.
    #[serde(skip)]
    pub intent: Option<String>,
}

impl Inner {
    /// Registers the transfer and starts it in the background.
    pub(crate) fn start_send(
        self: &Arc<Self>,
        request: SendRequest,
    ) -> Result<String, RuntimeError> {
        if request.paths.is_empty() {
            return Err(RuntimeError::Invalid("nothing to send".into()));
        }
        if request.device.trim().is_empty() {
            return Err(RuntimeError::Invalid("no device given".into()));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let peer_alias = self
            .alias_of(request.device.trim())
            .unwrap_or_else(|| request.device.trim().to_string());
        let view = TransferView {
            id: id.clone(),
            direction: Direction::Send,
            session_id: None,
            peer_fingerprint: String::new(),
            peer_alias,
            files: Vec::new(),
            total_size: 0,
            done_size: 0,
            state: TransferState::Preparing,
            clipboard_intent: request.intent.as_deref() == Some(INTENT_CLIPBOARD),
            error: None,
            error_code: None,
            started_at: unix_now(),
            finished_at: None,
        };
        let cancel = self.insert_transfer(view, None);
        let this = self.clone();
        let transfer_id = id.clone();
        self.tasks.spawn(async move {
            if let Err(err) = run_send(&this, &transfer_id, request, cancel).await {
                this.fail_transfer(&transfer_id, err.code(), err.to_string());
            }
        });
        Ok(id)
    }

    /// Finds the device behind `query` and the address to use for it.
    pub(crate) async fn resolve_device(
        &self,
        query: &str,
    ) -> Result<(Device, Target), RuntimeError> {
        let query = query.trim();
        if let Some((host, port)) = parse_host_port(query) {
            let target = Target {
                host,
                port: port.unwrap_or(DEFAULT_PORT),
                protocol: ProtocolType::Https,
            };
            let device = self
                .discovery
                .probe(&target)
                .await?
                .ok_or_else(|| RuntimeError::DeviceNotFound(query.to_string()))?;
            return Ok((device, target));
        }
        if let Some(device) = self.discovery.find(query)
            && !self.offline.lock().contains(&device.fingerprint)
        {
            let target = device.target();
            return Ok((device, target));
        }
        let known = self.db.list_devices()?.into_iter().find(|known| {
            known.fingerprint.eq_ignore_ascii_case(query)
                || (query.len() >= 4 && Fingerprint::parse(&known.fingerprint).has_prefix(query))
                || known.display_name().eq_ignore_ascii_case(query)
                || known.alias.eq_ignore_ascii_case(query)
        });
        if let Some(known) = known
            && let (Some(host), Some(port)) = (known.host.clone(), known.port)
        {
            let target = Target {
                host,
                port,
                protocol: ProtocolType::Https,
            };
            if let Ok(Some(device)) = self.discovery.probe(&target).await {
                self.offline.lock().remove(&device.fingerprint);
                return Ok((device, target));
            }
        }
        Err(RuntimeError::DeviceNotFound(query.to_string()))
    }
}

async fn run_send(
    inner: &Arc<Inner>,
    transfer_id: &str,
    request: SendRequest,
    cancel: CancellationToken,
) -> Result<(), RuntimeError> {
    // 1. Collect the files.
    let skip_hidden = inner.settings().skip_hidden_files;
    let paths = request.paths.clone();
    let outgoing =
        tokio::task::spawn_blocking(move || collect(&paths, &CollectOptions { skip_hidden }))
            .await
            .map_err(|err| RuntimeError::Invalid(err.to_string()))??;
    if outgoing.files.is_empty() {
        return Err(RuntimeError::Invalid(
            "no files to send (empty folders?)".into(),
        ));
    }
    inner.update_transfer(transfer_id, |transfer| {
        transfer.view.files = outgoing
            .files
            .iter()
            .map(|file| TransferFileView {
                id: file.id.clone(),
                name: file.name.clone(),
                size: file.size,
                mime: file.mime.clone(),
                done: 0,
                state: FileState::Pending,
                path: Some(file.path.clone()),
                error: None,
            })
            .collect();
        transfer.view.recompute_totals();
    });
    inner.emit_transfer_updated(transfer_id);

    // 2. Find the device.
    let (device, target) = tokio::select! {
        result = inner.resolve_device(&request.device) => result?,
        _ = cancel.cancelled() => {
            inner.complete_transfer(transfer_id, TransferState::Cancelled, None, None);
            return Ok(());
        }
    };
    inner.update_transfer(transfer_id, |transfer| {
        transfer.view.peer_fingerprint = device.fingerprint.to_string();
        transfer.view.peer_alias = inner
            .alias_of(device.fingerprint.as_str())
            .unwrap_or_else(|| device.alias.clone());
        transfer.peer_host = Some(target.host.clone());
    });
    inner.emit_transfer_updated(transfer_id);

    // 3. Checksums.
    let mut checksums: HashMap<String, String> = HashMap::new();
    if inner.settings().create_checksums {
        for file in &outgoing.files {
            if cancel.is_cancelled() {
                inner.complete_transfer(transfer_id, TransferState::Cancelled, None, None);
                return Ok(());
            }
            checksums.insert(file.id.clone(), sha256_of(&file.path).await?);
        }
    }

    // 4. prepare-upload, asking for a PIN when the receiver wants one.
    let client = Client::new(&inner.identity, Some(device.fingerprint.clone()), None)?;
    let mut info = inner.device_info(inner.port);
    if let (Some(ext), Some(intent)) = (info.ext.as_mut(), &request.intent) {
        ext.intent = Some(intent.clone());
    }
    let prepare = PrepareUploadRequest {
        info,
        files: outgoing
            .files
            .iter()
            .map(|file| {
                (
                    file.id.clone(),
                    file.to_dto(checksums.get(&file.id).cloned()),
                )
            })
            .collect(),
    };
    let peer_supports_resume = device.supports(FEATURE_RESUME);
    let started_at = unix_now();
    let mut pin = request.pin.clone();
    let response = loop {
        let attempt = tokio::select! {
            result = client.prepare_upload(&target, &prepare, pin.as_deref()) => result,
            _ = cancel.cancelled() => {
                inner.complete_transfer(transfer_id, TransferState::Cancelled, None, None);
                return Ok(());
            }
        };
        match attempt {
            Ok(PrepareUploadOutcome::Accepted(response)) => break response,
            Ok(PrepareUploadOutcome::NothingToTransfer) => {
                inner.complete_transfer(
                    transfer_id,
                    TransferState::Declined,
                    Some("the receiver accepted nothing".into()),
                    Some(ErrorCode::NothingAccepted),
                );
                return Ok(());
            }
            Err(ClientError::Status {
                status: 401,
                message,
            }) => {
                let (tx, rx) = oneshot::channel();
                inner
                    .pending
                    .lock()
                    .pins
                    .insert(transfer_id.to_string(), tx);
                inner.set_transfer_state(transfer_id, TransferState::WaitingPin);
                inner.emit(RuntimeEvent::TransferNeedsPin {
                    transfer_id: transfer_id.to_string(),
                    message,
                });
                let answer = tokio::select! {
                    answer = rx => answer.ok().flatten(),
                    _ = cancel.cancelled() => None,
                };
                inner.pending.lock().pins.remove(transfer_id);
                match answer {
                    Some(entered) => {
                        pin = Some(entered);
                        inner.set_transfer_state(transfer_id, TransferState::Preparing);
                    }
                    None => {
                        inner.complete_transfer(transfer_id, TransferState::Cancelled, None, None);
                        return Ok(());
                    }
                }
            }
            Err(ClientError::Status {
                status: 403,
                message,
            }) => {
                inner.complete_transfer(
                    transfer_id,
                    TransferState::Declined,
                    Some(message),
                    Some(ErrorCode::Declined),
                );
                return Ok(());
            }
            Err(ClientError::Status { status: 409, .. }) => {
                inner.fail_transfer(
                    transfer_id,
                    ErrorCode::Busy,
                    "the receiver is busy with another transfer",
                );
                return Ok(());
            }
            Err(err) => return Err(err.into()),
        }
    };

    // 5. Uploads.
    let session_id = response.session_id.clone();
    let resume_token: Option<Arc<str>> = match (&response.resume_token, inner.settings().resume) {
        (Some(token), true) if peer_supports_resume => Some(Arc::from(token.as_str())),
        _ => None,
    };
    let (accepted, skipped): (Vec<OutgoingFile>, Vec<OutgoingFile>) = outgoing
        .files
        .into_iter()
        .partition(|file| response.files.contains_key(&file.id));
    for file in &skipped {
        inner.finish_file(transfer_id, &file.id, FileState::Skipped, None, None);
        inner.record_transfer(&transfer_record(
            &session_id,
            &device,
            file,
            TransferStatus::Skipped,
            None,
            started_at,
        ));
    }
    inner.update_transfer(transfer_id, |transfer| {
        transfer.view.session_id = Some(session_id.clone());
        transfer.view.state = TransferState::Active;
    });
    inner.emit_transfer_updated(transfer_id);
    if accepted.is_empty() {
        inner.complete_transfer(
            transfer_id,
            TransferState::Declined,
            Some("the receiver accepted no files".into()),
            Some(ErrorCode::NothingAccepted),
        );
        return Ok(());
    }

    let parallel = inner.settings().parallel_uploads.max(1);
    let client = Arc::new(client);
    let target = Arc::new(target.clone());
    let session = Arc::new(session_id.clone());
    let results: Vec<(OutgoingFile, Result<(), String>)> = futures_util::stream::iter(accepted)
        .map(|file: OutgoingFile| {
            let token = response.files.get(&file.id).cloned().unwrap_or_default();
            let client = client.clone();
            let target = target.clone();
            let session = session.clone();
            let cancel = cancel.clone();
            let resume_token = resume_token.clone();
            let offset = response.resume_offset(&file.id);
            let inner = inner.clone();
            let transfer_id = transfer_id.to_string();
            async move {
                let plan = UploadPlan {
                    session_id: &session,
                    file_id: &file.id,
                    token: &token,
                    path: &file.path,
                    resume_token: resume_token.as_deref(),
                    offset,
                };
                let result =
                    upload_with_retries(&inner, &transfer_id, &client, &target, plan, &cancel)
                        .await;
                (file, result)
            }
        })
        .buffer_unordered(parallel)
        .collect()
        .await;

    // 6. Outcome.
    let mut sent = 0;
    let mut failed = 0;
    for (file, result) in &results {
        let (status, state, error) = match result {
            Ok(()) => {
                sent += 1;
                (TransferStatus::Finished, FileState::Finished, None)
            }
            Err(err) if cancel.is_cancelled() => (
                TransferStatus::Cancelled,
                FileState::Cancelled,
                Some(err.clone()),
            ),
            Err(err) => {
                failed += 1;
                (TransferStatus::Failed, FileState::Failed, Some(err.clone()))
            }
        };
        inner.finish_file(transfer_id, &file.id, state, None, error.clone());
        inner.record_transfer(&transfer_record(
            &session_id,
            &device,
            file,
            status,
            error,
            started_at,
        ));
    }
    let cancelled = cancel.is_cancelled();

    // Remember the device at the address that actually worked.
    if let Ok(Some(mut known)) = inner.db.device(device.fingerprint.as_str()) {
        known.host = Some(target.host.clone());
        known.port = Some(target.port);
        let _ = inner.db.upsert_device(&known);
    }
    if cancelled {
        let _ = client.cancel(&target, Some(&session_id)).await;
        inner.complete_transfer(transfer_id, TransferState::Cancelled, None, None);
    } else if failed > 0 {
        let _ = client.cancel(&target, Some(&session_id)).await;
        inner.complete_transfer(
            transfer_id,
            TransferState::Failed,
            Some(format!("{failed} of {} file(s) failed", sent + failed)),
            Some(ErrorCode::PartialFailure),
        );
    } else {
        inner.complete_transfer(transfer_id, TransferState::Finished, None, None);
    }
    Ok(())
}

fn transfer_record(
    session_id: &str,
    device: &Device,
    file: &OutgoingFile,
    status: TransferStatus,
    error: Option<String>,
    started_at: i64,
) -> TransferRecord {
    TransferRecord {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        direction: Direction::Send,
        peer_fingerprint: device.fingerprint.to_string(),
        peer_alias: device.alias.clone(),
        file_name: file.name.clone(),
        path: Some(file.path.clone()),
        size: file.size,
        mime: file.mime.clone(),
        status,
        error,
        started_at,
        finished_at: Some(unix_now()),
    }
}

/// Everything one file upload needs.
struct UploadPlan<'a> {
    session_id: &'a str,
    file_id: &'a str,
    token: &'a str,
    path: &'a Path,
    /// Present when both sides support the resume extension.
    resume_token: Option<&'a str>,
    /// Bytes the receiver already holds (from an earlier session).
    offset: u64,
}

async fn upload_with_retries(
    inner: &Arc<Inner>,
    transfer_id: &str,
    client: &Client,
    target: &Target,
    plan: UploadPlan<'_>,
    cancel: &CancellationToken,
) -> Result<(), String> {
    let mut attempt = 0;
    let mut offset = if plan.resume_token.is_some() {
        plan.offset
    } else {
        0
    };
    loop {
        attempt += 1;
        inner.progress(transfer_id, plan.file_id, offset);
        let resume = plan
            .resume_token
            .filter(|_| offset > 0)
            .map(|token| Resume { offset, token });
        let progress_inner = inner.clone();
        let progress_transfer = transfer_id.to_string();
        let progress_file = plan.file_id.to_string();
        let upload = client.upload_file_from(
            target,
            plan.session_id,
            plan.file_id,
            plan.token,
            plan.path,
            resume,
            move |sent| progress_inner.progress(&progress_transfer, &progress_file, sent),
        );
        let result = tokio::select! {
            result = upload => result,
            _ = cancel.cancelled() => return Err("cancelled".into()),
        };
        match result {
            Ok(()) => return Ok(()),
            Err(_) if attempt >= MAX_ATTEMPTS => {
                return Err(format!("gave up after {attempt} attempts"));
            }
            Err(ClientError::Status { status: 422, .. }) => {
                tracing::warn!(
                    "checksum mismatch reported for {}, retrying",
                    plan.path.display()
                );
                offset = 0;
            }
            Err(ClientError::OffsetMismatch(expected)) => {
                offset = expected;
            }
            Err(err) if err.is_transport() && plan.resume_token.is_some() => {
                let Some(token) = plan.resume_token else {
                    return Err(err.to_string());
                };
                tokio::time::sleep(Duration::from_millis(500)).await;
                match client
                    .resume_offset(target, plan.session_id, plan.file_id, token)
                    .await
                {
                    Ok(held) => offset = held,
                    Err(query_err) => {
                        return Err(format!(
                            "{err}; could not query the resume offset: {query_err}"
                        ));
                    }
                }
            }
            Err(err) => return Err(err.to_string()),
        }
    }
}

async fn sha256_of(path: &Path) -> Result<String, RuntimeError> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 512 * 1024];
    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}
