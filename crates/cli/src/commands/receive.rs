use crate::app::App;
use crate::ui;
use anyhow::Context;
use indicatif::{MultiProgress, ProgressBar};
use lan_send_core::discovery::Device;
use lan_send_core::protocol::FileDto;
use lan_send_core::store::{
    ConflictPolicy, Direction, KnownDevice, OrganizeRules, TransferRecord, TransferStatus, unix_now,
};
use lan_send_core::transfer::{Destination, Placement};
use lan_send_core::transport::{ServerEvent, SessionEndReason, UploadDecision, UploadTarget};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tokio::sync::mpsc;

pub struct ReceiveOptions {
    pub dir: Option<PathBuf>,
    pub pin: Option<String>,
    pub auto_accept: bool,
    pub verify_checksums: bool,
    pub organize: OrganizeRules,
    pub on_conflict: ConflictPolicy,
}

/// Parses the `--organize` flag: a comma list of device, date, type, or "none".
pub fn parse_organize(rules: &str) -> anyhow::Result<OrganizeRules> {
    let mut organize = OrganizeRules::default();
    for rule in rules
        .split(',')
        .map(str::trim)
        .filter(|rule| !rule.is_empty())
    {
        match rule.to_ascii_lowercase().as_str() {
            "device" => organize.by_device = true,
            "date" => organize.by_date = true,
            "type" => organize.by_type = true,
            "none" => {}
            other => {
                anyhow::bail!("unknown organize rule '{other}' (use device, date, type or none)")
            }
        }
    }
    Ok(organize)
}

/// What is known about the current session's sender, for history entries.
struct SessionInfo {
    session_id: String,
    peer_alias: String,
    peer_fingerprint: String,
    files: HashMap<String, (FileDto, i64)>,
}

pub async fn run(app: App, options: ReceiveOptions) -> anyhow::Result<()> {
    let root = options
        .dir
        .clone()
        .or_else(|| app.paths.download_dir.clone())
        .context("no download directory known; pass --dir")?;
    let destination = Destination::new(&root, options.organize, options.on_conflict)
        .with_context(|| format!("cannot use {}", root.display()))?;

    let (events_tx, mut events_rx) = mpsc::channel(256);
    let server = app
        .start_server(options.pin.clone(), options.verify_checksums, events_tx)
        .await?;
    let discovery = app.start_discovery(server.port());
    if let Some(err) = discovery.multicast_error() {
        eprintln!("multicast unavailable ({err}); peers must find this device by address");
    }
    println!(
        "Receiving as {} ({}) on port {} -> {}{}",
        app.alias,
        app.identity.fingerprint().short(),
        server.port(),
        destination.root().display(),
        if options.pin.is_some() {
            " [PIN required]"
        } else {
            ""
        }
    );
    eprintln!("Press Ctrl+C to stop.");
    {
        let discovery = discovery.clone();
        let known = app.known_targets();
        tokio::spawn(async move {
            tokio::join!(discovery.announce(), discovery.probe_many(known));
        });
    }

    let progress = MultiProgress::new();
    let mut bars: HashMap<String, ProgressBar> = HashMap::new();
    let mut assigned: HashSet<PathBuf> = HashSet::new();
    let mut session: Option<SessionInfo> = None;
    let mut received = 0usize;
    let mut failed = 0usize;

    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);
    loop {
        let event = tokio::select! {
            _ = &mut ctrl_c => {
                eprintln!("Stopping.");
                break;
            }
            event = events_rx.recv() => match event {
                Some(event) => event,
                None => break,
            },
        };
        match event {
            ServerEvent::PrepareUpload {
                session_id,
                peer,
                info,
                files,
                decision,
            } => {
                let sender = peer.identity(&info.fingerprint);
                let device = Device::from_info(peer.addr, &info, sender.clone());
                discovery.add_confirmed(device.clone());
                let _ = app.db.upsert_device(&KnownDevice::from(&device));

                let total: u64 = files.values().map(|file| file.size).sum();
                let mut names: Vec<&str> =
                    files.values().map(|file| file.file_name.as_str()).collect();
                names.sort_unstable();
                println!(
                    "\nRequest from {} ({}) at {}: {} file(s), {}",
                    info.alias,
                    sender.short(),
                    peer.addr,
                    files.len(),
                    ui::format_bytes(total)
                );
                for name in names.iter().take(10) {
                    println!("  {name}");
                }
                if names.len() > 10 {
                    println!("  ... and {} more", names.len() - 10);
                }
                let accept = options.auto_accept
                    || ui::prompt_line("Accept? [y/N] ")
                        .await
                        .map(|answer| matches!(answer.to_ascii_lowercase().as_str(), "y" | "yes"))
                        .unwrap_or(false);
                let answer = if accept {
                    session = Some(SessionInfo {
                        session_id: session_id.clone(),
                        peer_alias: info.alias.clone(),
                        peer_fingerprint: sender.to_string(),
                        files: files
                            .iter()
                            .map(|(id, file)| (id.clone(), (file.clone(), unix_now())))
                            .collect(),
                    });
                    UploadDecision::Accept(files.keys().cloned().collect())
                } else {
                    println!("Declined.");
                    UploadDecision::Decline
                };
                if decision.send(answer).is_err() {
                    println!("The sender withdrew the request {}.", short_id(&session_id));
                    session = None;
                }
                assigned.clear();
            }
            ServerEvent::PrepareUploadAborted { session_id } => {
                println!(
                    "Request {} was withdrawn by the sender.",
                    short_id(&session_id)
                );
                session = None;
            }
            ServerEvent::FileUpload {
                file_id,
                file,
                target,
                ..
            } => {
                let sender_alias = session
                    .as_ref()
                    .map(|info| info.peer_alias.as_str())
                    .unwrap_or("unknown");
                let answer = match destination.place(&file, sender_alias, &assigned) {
                    Ok(placement) => {
                        let path = match placement {
                            Placement::New(path) => path,
                            Placement::Replace(path) => {
                                println!("Replacing {}", path.display());
                                path
                            }
                            Placement::NeedsDecision { existing, renamed } => {
                                decide_conflict(options.auto_accept, existing, renamed).await
                            }
                        };
                        assigned.insert(path.clone());
                        let bar = progress.add(ui::transfer_bar(&file.file_name, file.size));
                        bars.insert(file_id.clone(), bar);
                        UploadTarget::Path(path)
                    }
                    Err(err) => UploadTarget::Reject(err.to_string()),
                };
                if let UploadTarget::Reject(reason) = &answer {
                    println!("Refusing {}: {reason}", file.file_name);
                }
                if let Some(info) = session.as_mut() {
                    info.files
                        .entry(file_id.clone())
                        .or_insert((file, unix_now()))
                        .1 = unix_now();
                }
                let _ = target.send(answer);
            }
            ServerEvent::FileUploadProgress {
                file_id, received, ..
            } => {
                if let Some(bar) = bars.get(&file_id) {
                    bar.set_position(received);
                }
            }
            ServerEvent::FileUploadResult {
                session_id,
                file_id,
                path,
                outcome,
            } => {
                if let Some(bar) = bars.remove(&file_id) {
                    bar.finish_and_clear();
                }
                if outcome.is_success() {
                    received += 1;
                    println!("Received {}", path.display());
                } else {
                    failed += 1;
                    println!("Failed {}: {outcome:?}", path.display());
                }
                if let Some(info) = session.as_ref() {
                    let (file, started_at) = info
                        .files
                        .get(&file_id)
                        .cloned()
                        .unwrap_or_else(|| (placeholder_file(&file_id, &path), unix_now()));
                    app.record_transfer(&TransferRecord {
                        id: uuid::Uuid::new_v4().to_string(),
                        session_id: session_id.clone(),
                        direction: Direction::Receive,
                        peer_fingerprint: info.peer_fingerprint.clone(),
                        peer_alias: info.peer_alias.clone(),
                        file_name: file.file_name.clone(),
                        path: Some(path),
                        size: file.size,
                        mime: file.file_type.clone(),
                        status: if outcome.is_success() {
                            TransferStatus::Finished
                        } else {
                            TransferStatus::Failed
                        },
                        error: match &outcome {
                            lan_send_core::transport::server::SaveOutcome::Success => None,
                            other => Some(format!("{other:?}")),
                        },
                        started_at,
                        finished_at: Some(unix_now()),
                    });
                }
            }
            ServerEvent::SessionEnd { session_id, reason } => {
                let word = match reason {
                    SessionEndReason::Finished => "finished",
                    SessionEndReason::Cancelled => "cancelled by the sender",
                };
                println!("Session {} {word}.", short_id(&session_id));
                if session
                    .as_ref()
                    .is_some_and(|info| info.session_id == session_id)
                {
                    session = None;
                }
            }
            other => app.handle_background_event(&discovery, other),
        }
    }

    println!("{received} file(s) received, {failed} failed.");
    server.stop().await;
    discovery.stop().await;
    Ok(())
}

async fn decide_conflict(auto_accept: bool, existing: PathBuf, renamed: PathBuf) -> PathBuf {
    if auto_accept {
        return renamed;
    }
    let answer = ui::prompt_line(&format!(
        "{} exists. [r]ename to {} / [o]verwrite? ",
        existing.display(),
        renamed
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    ))
    .await
    .unwrap_or_default();
    if matches!(answer.to_ascii_lowercase().as_str(), "o" | "overwrite") {
        println!("Replacing {}", existing.display());
        existing
    } else {
        renamed
    }
}

fn placeholder_file(file_id: &str, path: &std::path::Path) -> FileDto {
    FileDto {
        id: file_id.to_string(),
        file_name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        size: std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
        file_type: "application/octet-stream".into(),
        sha256: None,
        preview: None,
        metadata: None,
    }
}

fn short_id(session_id: &str) -> &str {
    session_id.get(..8).unwrap_or(session_id)
}
