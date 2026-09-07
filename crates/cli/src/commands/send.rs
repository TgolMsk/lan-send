use crate::app::{App, handle_background_event};
use crate::ui;
use anyhow::{Context, bail};
use futures_util::StreamExt;
use indicatif::MultiProgress;
use lan_send_core::discovery::{Device, Discovery};
use lan_send_core::protocol::{
    DEFAULT_PORT, FileDto, FileMetadata, PrepareUploadRequest, ProtocolType,
};
use lan_send_core::transport::{Client, ClientError, PrepareUploadOutcome, ServerEvent, Target};
use sha2::{Digest, Sha256};
use std::net::{IpAddr, SocketAddr};
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
}

struct Outgoing {
    id: String,
    path: PathBuf,
    dto: FileDto,
}

pub async fn run(app: App, options: SendOptions) -> anyhow::Result<()> {
    let mut files = Vec::new();
    for path in &options.paths {
        let metadata =
            std::fs::metadata(path).with_context(|| format!("cannot read {}", path.display()))?;
        if metadata.is_dir() {
            bail!(
                "{} is a folder; folder transfers arrive in milestone 2",
                path.display()
            );
        }
        if !metadata.is_file() {
            bail!("{} is not a regular file", path.display());
        }
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .with_context(|| format!("{} has no file name", path.display()))?;
        files.push(Outgoing {
            id: uuid::Uuid::new_v4().to_string(),
            path: path.clone(),
            dto: FileDto {
                id: String::new(),
                file_name: name,
                size: metadata.len(),
                file_type: mime_guess::from_path(path)
                    .first_or_octet_stream()
                    .essence_str()
                    .to_string(),
                sha256: None,
                preview: None,
                metadata: FileMetadata::from_path(path),
            },
        });
    }
    for file in &mut files {
        file.dto.id = file.id.clone();
    }

    let (events_tx, events_rx) = mpsc::channel(64);
    let server = app.start_server(None, true, events_tx).await?;
    let discovery = app.start_discovery(server.port());
    let result = send(&app, &discovery, events_rx, options, files).await;
    server.stop().await;
    discovery.stop().await;
    result
}

async fn send(
    app: &App,
    discovery: &Discovery,
    mut events_rx: mpsc::Receiver<ServerEvent>,
    options: SendOptions,
    mut files: Vec<Outgoing>,
) -> anyhow::Result<()> {
    eprintln!("Looking for {}...", options.device);
    let device =
        wait_for_device(discovery, &mut events_rx, &options.device, options.timeout).await?;
    eprintln!(
        "Found {} ({}) at {}:{}",
        device.alias,
        device.fingerprint.short(),
        device.host,
        device.port
    );

    if options.checksum {
        for file in &mut files {
            file.dto.sha256 = Some(sha256_of(&file.path).await?);
        }
    }

    let client = Client::new(&app.identity, Some(device.fingerprint.clone()), None)?;
    // An address given by the user wins over whatever discovery saw last.
    let target = direct_target(&options.device).unwrap_or_else(|| device.target());
    let request = PrepareUploadRequest {
        info: app.device_info(app.port),
        files: files
            .iter()
            .map(|file| (file.id.clone(), file.dto.clone()))
            .collect(),
    };

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

    let session_id = response.session_id;
    let accepted: Vec<&Outgoing> = files
        .iter()
        .filter(|file| response.files.contains_key(&file.id))
        .collect();
    let skipped = files.len() - accepted.len();
    if accepted.is_empty() {
        println!("The receiver accepted no files.");
        return Ok(());
    }
    if skipped > 0 {
        eprintln!("{skipped} file(s) were not accepted.");
    }

    // The receiver may cancel the session through our own server.
    let cancel = CancellationToken::new();
    let watcher = {
        let cancel = cancel.clone();
        let discovery = discovery.clone();
        let session_id = session_id.clone();
        let peer_host = target.host.clone();
        tokio::spawn(async move {
            while let Some(event) = events_rx.recv().await {
                match event {
                    ServerEvent::CancelReceived {
                        peer,
                        session_id: cancelled,
                    } if cancelled == session_id && peer.addr.to_string() == peer_host => {
                        eprintln!("The receiver cancelled the transfer.");
                        cancel.cancel();
                    }
                    other => handle_background_event(&discovery, other),
                }
            }
        })
    };

    let progress = MultiProgress::new();
    let client = Arc::new(client);
    let target = Arc::new(target);
    let session = Arc::new(session_id.clone());
    let results: Vec<(String, Result<(), String>)> = futures_util::stream::iter(accepted)
        .map(|file| {
            let token = response.files.get(&file.id).cloned().unwrap_or_default();
            let bar = progress.add(ui::transfer_bar(&file.dto.file_name, file.dto.size));
            let client = client.clone();
            let target = target.clone();
            let session = session.clone();
            let cancel = cancel.clone();
            let path = file.path.clone();
            let name = file.dto.file_name.clone();
            let id = file.id.clone();
            async move {
                let result = upload_with_retries(
                    &client, &target, &session, &id, &token, &path, &bar, &cancel,
                )
                .await;
                bar.finish_and_clear();
                (name, result)
            }
        })
        .buffer_unordered(options.parallel)
        .collect()
        .await;
    watcher.abort();

    let mut failures = 0;
    for (name, result) in &results {
        match result {
            Ok(()) => println!("Sent {name}"),
            Err(err) => {
                failures += 1;
                println!("Failed {name}: {err}");
            }
        }
    }
    if cancel.is_cancelled() {
        bail!("transfer cancelled by the receiver");
    }
    if failures > 0 {
        let _ = client.cancel(&target, Some(&session_id)).await;
        bail!("{failures} of {} file(s) failed", results.len());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn upload_with_retries(
    client: &Client,
    target: &Target,
    session_id: &str,
    file_id: &str,
    token: &str,
    path: &Path,
    bar: &indicatif::ProgressBar,
    cancel: &CancellationToken,
) -> Result<(), String> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        let bar_for_progress = bar.clone();
        let upload = client.upload_file(target, session_id, file_id, token, path, move |sent| {
            bar_for_progress.set_position(sent);
        });
        let result = tokio::select! {
            result = upload => result,
            _ = cancel.cancelled() => return Err("cancelled".into()),
        };
        match result {
            Ok(()) => return Ok(()),
            Err(ClientError::Status { status: 422, .. }) if attempt < MAX_ATTEMPTS => {
                bar.set_position(0);
                tracing::warn!(
                    "checksum mismatch reported for {}, retrying",
                    path.display()
                );
            }
            Err(err) => return Err(err.to_string()),
        }
    }
}

async fn wait_for_device(
    discovery: &Discovery,
    events_rx: &mut mpsc::Receiver<ServerEvent>,
    query: &str,
    timeout: Duration,
) -> anyhow::Result<Device> {
    let known = direct_target(query).into_iter().collect();
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
                    handle_background_event(discovery, event);
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

/// A query that is an address (`ip` or `ip:port`) is probed directly.
fn direct_target(query: &str) -> Option<Target> {
    if let Ok(addr) = query.parse::<SocketAddr>() {
        return Some(Target {
            host: addr.ip().to_string(),
            port: addr.port(),
            protocol: ProtocolType::Https,
        });
    }
    query.parse::<IpAddr>().ok().map(|ip| Target {
        host: ip.to_string(),
        port: DEFAULT_PORT,
        protocol: ProtocolType::Https,
    })
}

async fn sha256_of(path: &PathBuf) -> anyhow::Result<String> {
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
