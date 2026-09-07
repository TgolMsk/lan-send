use crate::app::App;
use crate::ui;
use anyhow::{Context, bail};
use futures_util::StreamExt;
use indicatif::MultiProgress;
use lan_send_core::discovery::{Device, Discovery};
use lan_send_core::protocol::{DEFAULT_PORT, FEATURE_RESUME, PrepareUploadRequest, ProtocolType};
use lan_send_core::store::{Direction, TransferRecord, TransferStatus, unix_now};
use lan_send_core::transfer::{CollectOptions, OutgoingFile, collect};
use lan_send_core::transport::{
    Client, ClientError, PrepareUploadOutcome, Resume, ServerEvent, Target,
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Attempts per file (the receiver allows the same number).
const MAX_ATTEMPTS: u32 = 3;

pub struct SendOptions {
    pub device: String,
    pub paths: Vec<PathBuf>,
    pub pin: Option<String>,
    pub timeout: Duration,
    pub parallel: usize,
    pub checksum: bool,
    pub skip_hidden: bool,
}

pub async fn run(app: App, options: SendOptions) -> anyhow::Result<()> {
    let outgoing = collect(
        &options.paths,
        &CollectOptions {
            skip_hidden: options.skip_hidden,
        },
    )?;
    for skipped in &outgoing.skipped {
        eprintln!("skipping {} ({})", skipped.path.display(), skipped.reason);
    }
    eprintln!(
        "{} file(s), {}{}",
        outgoing.files.len(),
        ui::format_bytes(outgoing.total_size),
        if outgoing.skipped.is_empty() {
            String::new()
        } else {
            format!(", {} skipped", outgoing.skipped.len())
        }
    );

    let (events_tx, events_rx) = mpsc::channel(64);
    let server = app.start_server(None, true, events_tx).await?;
    let discovery = app.start_discovery(server.port());
    let result = send(&app, &discovery, events_rx, options, outgoing.files).await;
    server.stop().await;
    discovery.stop().await;
    result
}

async fn send(
    app: &App,
    discovery: &Discovery,
    mut events_rx: mpsc::Receiver<ServerEvent>,
    options: SendOptions,
    files: Vec<OutgoingFile>,
) -> anyhow::Result<()> {
    eprintln!("Looking for {}...", options.device);
    let device = wait_for_device(
        app,
        discovery,
        &mut events_rx,
        &options.device,
        options.timeout,
    )
    .await?;
    // An address given by the user wins over whatever discovery saw last.
    let target = direct_target(&options.device).unwrap_or_else(|| device.target());
    eprintln!(
        "Found {} ({}) at {}:{}",
        device.alias,
        device.fingerprint.short(),
        target.host,
        target.port
    );

    let mut checksums: HashMap<String, String> = HashMap::new();
    if options.checksum {
        let bar = indicatif::ProgressBar::new(files.len() as u64)
            .with_message("hashing")
            .with_style(
                indicatif::ProgressStyle::with_template("{msg} {pos}/{len}")
                    .unwrap_or_else(|_| indicatif::ProgressStyle::default_bar()),
            );
        for file in &files {
            checksums.insert(file.id.clone(), sha256_of(&file.path).await?);
            bar.inc(1);
        }
        bar.finish_and_clear();
    }

    let client = Client::new(&app.identity, Some(device.fingerprint.clone()), None)?;
    let request = PrepareUploadRequest {
        info: app.device_info(app.port),
        files: files
            .iter()
            .map(|file| {
                (
                    file.id.clone(),
                    file.to_dto(checksums.get(&file.id).cloned()),
                )
            })
            .collect(),
    };

    let started_at = unix_now();
    let mut pin = options.pin.clone();
    let response = loop {
        match client
            .prepare_upload(&target, &request, pin.as_deref())
            .await
        {
            Ok(PrepareUploadOutcome::Accepted(response)) => break response,
            Ok(PrepareUploadOutcome::NothingToTransfer) => {
                println!("The receiver accepted nothing.");
                return Ok(());
            }
            Err(ClientError::Status {
                status: 401,
                message,
            }) => {
                eprintln!("{message}");
                let entered = ui::prompt_line("PIN: ").await?;
                if entered.is_empty() {
                    bail!("no PIN entered");
                }
                pin = Some(entered);
            }
            Err(ClientError::Status {
                status: 403,
                message,
            }) => bail!("rejected: {message}"),
            Err(ClientError::Status { status: 409, .. }) => {
                bail!("the receiver is busy with another session")
            }
            Err(err) => return Err(err).context("prepare-upload failed"),
        }
    };

    let session_id = response.session_id.clone();
    let resume_token: Option<Arc<str>> = match (&response.resume_token, app.settings.resume) {
        (Some(token), true) if device.supports(FEATURE_RESUME) => Some(Arc::from(token.as_str())),
        _ => None,
    };
    let resumed: Vec<&str> = files
        .iter()
        .filter(|file| response.resume_offset(&file.id) > 0)
        .map(|file| file.name.as_str())
        .collect();
    if resume_token.is_some() && !resumed.is_empty() {
        eprintln!("Resuming {} file(s): {}", resumed.len(), resumed.join(", "));
    }
    let (accepted, skipped): (Vec<&OutgoingFile>, Vec<&OutgoingFile>) = files
        .iter()
        .partition(|file| response.files.contains_key(&file.id));
    for file in &skipped {
        app.record_transfer(&transfer_record(
            app,
            &session_id,
            &device,
            file,
            TransferStatus::Skipped,
            None,
            started_at,
        ));
    }
    if accepted.is_empty() {
        println!("The receiver accepted no files.");
        return Ok(());
    }
    if !skipped.is_empty() {
        eprintln!("{} file(s) were not accepted.", skipped.len());
    }

    // The receiver may cancel the session through our own server.
    let cancel = CancellationToken::new();
    let watcher = {
        let cancel = cancel.clone();
        let discovery = discovery.clone();
        let session_id = session_id.clone();
        let peer_host = target.host.clone();
        let db = app.db.clone();
        tokio::spawn(async move {
            while let Some(event) = events_rx.recv().await {
                match event {
                    ServerEvent::CancelReceived {
                        peer,
                        session_id: cancelled,
                    } if cancelled == session_id
                        && peer.addr.to_string()
                            == peer_host.split('%').next().unwrap_or(&peer_host) =>
                    {
                        eprintln!("The receiver cancelled the transfer.");
                        cancel.cancel();
                    }
                    ServerEvent::Register { peer, info } => {
                        let fingerprint = peer.identity(&info.fingerprint);
                        let device = Device::from_info(&peer.host(), &info, fingerprint);
                        let _ = db.upsert_device(&lan_send_core::store::KnownDevice::from(&device));
                        discovery.add_confirmed(device);
                    }
                    ServerEvent::PrepareUpload { decision, .. } => {
                        let _ = decision.send(lan_send_core::transport::UploadDecision::Decline);
                    }
                    _ => {}
                }
            }
        })
    };

    let progress = MultiProgress::new();
    let client = Arc::new(client);
    let target = Arc::new(target);
    let session = Arc::new(session_id.clone());
    let results: Vec<(&OutgoingFile, Result<(), String>)> = futures_util::stream::iter(accepted)
        .map(|file| {
            let token = response.files.get(&file.id).cloned().unwrap_or_default();
            let bar = progress.add(ui::transfer_bar(&file.name, file.size));
            let client = client.clone();
            let target = target.clone();
            let session = session.clone();
            let cancel = cancel.clone();
            let resume_token = resume_token.clone();
            let offset = response.resume_offset(&file.id);
            async move {
                let plan = UploadPlan {
                    session_id: &session,
                    file_id: &file.id,
                    token: &token,
                    path: &file.path,
                    resume_token: resume_token.as_deref(),
                    offset,
                };
                let result = upload_with_retries(&client, &target, plan, &bar, &cancel).await;
                bar.finish_and_clear();
                (file, result)
            }
        })
        .buffer_unordered(options.parallel)
        .collect()
        .await;
    watcher.abort();

    let mut failures = 0;
    for (file, result) in &results {
        let (status, error) = match result {
            Ok(()) => {
                println!("Sent {}", file.name);
                (TransferStatus::Finished, None)
            }
            Err(err) if cancel.is_cancelled() => {
                println!("Cancelled {}", file.name);
                (TransferStatus::Cancelled, Some(err.clone()))
            }
            Err(err) => {
                failures += 1;
                println!("Failed {}: {err}", file.name);
                (TransferStatus::Failed, Some(err.clone()))
            }
        };
        app.record_transfer(&transfer_record(
            app,
            &session_id,
            &device,
            file,
            status,
            error,
            started_at,
        ));
    }
    // Remember the device at the address that actually worked.
    let mut known = lan_send_core::store::KnownDevice::from(&device);
    known.host = Some(target.host.clone());
    known.port = Some(target.port);
    let _ = app.db.upsert_device(&known);

    if cancel.is_cancelled() {
        bail!("transfer cancelled by the receiver");
    }
    if failures > 0 {
        let _ = client.cancel(&target, Some(&session_id)).await;
        bail!("{failures} of {} file(s) failed", results.len());
    }
    Ok(())
}

fn transfer_record(
    app: &App,
    session_id: &str,
    device: &Device,
    file: &OutgoingFile,
    status: TransferStatus,
    error: Option<String>,
    started_at: i64,
) -> TransferRecord {
    let _ = app;
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
    client: &Client,
    target: &Target,
    plan: UploadPlan<'_>,
    bar: &indicatif::ProgressBar,
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
        bar.set_position(offset);
        let bar_for_progress = bar.clone();
        let resume = plan
            .resume_token
            .filter(|_| offset > 0)
            .map(|token| Resume { offset, token });
        let upload = client.upload_file_from(
            target,
            plan.session_id,
            plan.file_id,
            plan.token,
            plan.path,
            resume,
            move |sent| bar_for_progress.set_position(sent),
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
                tracing::info!(
                    "receiver holds {expected} bytes of {}, resuming there",
                    plan.path.display()
                );
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
                    Ok(held) => {
                        tracing::info!("resuming {} at byte {held}", plan.path.display());
                        offset = held;
                    }
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

pub(crate) async fn wait_for_device(
    app: &App,
    discovery: &Discovery,
    events_rx: &mut mpsc::Receiver<ServerEvent>,
    query: &str,
    timeout: Duration,
) -> anyhow::Result<Device> {
    let mut known = app.known_targets();
    if let Some(direct) = direct_target(query) {
        known.insert(0, direct);
    }
    let staged = {
        let discovery = discovery.clone();
        tokio::spawn(async move {
            discovery
                .discover_staged(known, Duration::from_secs(1))
                .await;
        })
    };
    let deadline = Instant::now() + timeout;
    let found = loop {
        if let Some(device) = discovery.find(query) {
            break Some(device);
        }
        if Instant::now() >= deadline {
            break None;
        }
        tokio::select! {
            event = events_rx.recv() => {
                if let Some(event) = event {
                    app.handle_background_event(discovery, event);
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(200)) => {}
        }
    };
    staged.abort();
    found.with_context(|| {
        let known: Vec<String> = discovery
            .devices()
            .iter()
            .map(|device| format!("{} ({})", device.alias, device.fingerprint.short()))
            .collect();
        format!(
            "device '{query}' not found within {:.0}s; seen: {}",
            timeout.as_secs_f64(),
            if known.is_empty() {
                "nothing".to_string()
            } else {
                known.join(", ")
            }
        )
    })
}

/// A query that is an address (`ip`, `ip:port`, `[v6%scope]:port`) is
/// probed directly.
pub(crate) fn direct_target(query: &str) -> Option<Target> {
    let (host, port) = lan_send_core::discovery::parse_host_port(query)?;
    Some(Target {
        host,
        port: port.unwrap_or(DEFAULT_PORT),
        protocol: ProtocolType::Https,
    })
}

async fn sha256_of(path: &Path) -> anyhow::Result<String> {
    let mut file = tokio::fs::File::open(path)
        .await
        .with_context(|| format!("cannot open {}", path.display()))?;
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
