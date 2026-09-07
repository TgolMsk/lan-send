use crate::app::App;
use crate::ui;
use anyhow::Context;
use indicatif::{MultiProgress, ProgressBar};
use lan_send_core::discovery::Device;
use lan_send_core::protocol::{FEATURE_RESUME, FileDto};
use lan_send_core::store::{
    ConflictPolicy, Direction, KnownDevice, OrganizeRules, PartialUpload, TransferRecord,
    TransferStatus, unix_now,
};
use lan_send_core::transfer::{Destination, Placement};
use lan_send_core::transport::server::{SaveOutcome, part_path};
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

/// One file of the current session, for history entries.
struct SessionFile {
    dto: FileDto,
    started_at: i64,
    /// History entry id; every attempt updates the same entry.
    record_id: String,
}

/// What is known about the current session's sender.
struct SessionInfo {
    session_id: String,
    peer_alias: String,
    peer_fingerprint: String,
    /// The sender announced the resume extension.
    resumable: bool,
    /// Files resumed from an earlier session keep their original path.
    resume_paths: HashMap<String, PathBuf>,
    files: HashMap<String, SessionFile>,
}

pub async fn run(app: App, options: ReceiveOptions) -> anyhow::Result<()> {
    let root = options
        .dir
        .clone()
        .or_else(|| app.paths.download_dir.clone())
        .context("no download directory known; pass --dir")?;
    let destination = Destination::new(&root, options.organize, options.on_conflict)
        .with_context(|| format!("cannot use {}", root.display()))?;
    app.expire_partials();

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
                let device = Device::from_info(&peer.host(), &info, sender.clone());
                discovery.add_confirmed(device.clone());
                let _ = app.db.upsert_device(&KnownDevice::from(&device));
                let resumable = app.settings.resume
                    && info
                        .ext
                        .as_ref()
                        .is_some_and(|ext| ext.supports(FEATURE_RESUME));

                let total: u64 = files.values().map(|file| file.size).sum();
                let mut names: Vec<&str> =
                    files.values().map(|file| file.file_name.as_str()).collect();
                names.sort_unstable();
                println!(
                    "\nRequest from {} ({}) at {}: {} file(s), {}{}",
                    info.alias,
                    sender.short(),
                    peer.addr,
                    files.len(),
                    ui::format_bytes(total),
                    if resumable { " [resumable]" } else { "" }
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
                    let (offsets, resume_paths) = if resumable {
                        resumable_files(&app, &sender.to_string(), &files)
                    } else {
                        (HashMap::new(), HashMap::new())
                    };
                    if !offsets.is_empty() {
                        println!(
                            "Resuming {} file(s) from an earlier session.",
                            offsets.len()
                        );
                    }
                    session = Some(SessionInfo {
                        session_id: session_id.clone(),
                        peer_alias: info.alias.clone(),
                        peer_fingerprint: sender.to_string(),
                        resumable,
                        resume_paths,
                        files: files
                            .iter()
                            .map(|(id, file)| {
                                let entry = SessionFile {
                                    dto: file.clone(),
                                    started_at: unix_now(),
                                    record_id: uuid::Uuid::new_v4().to_string(),
                                };
                                (id.clone(), entry)
                            })
                            .collect(),
                    });
                    let ids: HashSet<String> = files.keys().cloned().collect();
                    if resumable {
                        UploadDecision::AcceptWithResume {
                            files: ids,
                            offsets,
                        }
                    } else {
                        UploadDecision::Accept(ids)
                    }
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
                let (sender_alias, sender_fingerprint, resumable, resumed_path) = match &session {
                    Some(info) => (
                        info.peer_alias.as_str(),
                        info.peer_fingerprint.as_str(),
                        info.resumable,
                        info.resume_paths.get(&file_id).cloned(),
                    ),
                    None => ("unknown", "", false, None),
                };
                let placement = match resumed_path {
                    Some(path) => Ok(Placement::New(path)),
                    None => destination.place(&file, sender_alias, &assigned),
                };
                let answer = match placement {
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
                        if resumable {
                            let _ = app.db.upsert_partial(&PartialUpload {
                                part_path: part_path(&path),
                                final_path: path.clone(),
                                sender_fingerprint: sender_fingerprint.to_string(),
                                file_name: file.file_name.clone(),
                                size: file.size,
                                sha256: file.sha256.clone(),
                                received: 0,
                                updated_at: unix_now(),
                            });
                        }
                        let bar = progress.add(ui::transfer_bar(&file.file_name, file.size));
                        bars.insert(file_id.clone(), bar);
                        UploadTarget::Path(path)
                    }
                    Err(err) => UploadTarget::Reject(err.to_string()),
                };
                if let UploadTarget::Reject(reason) = &answer {
                    println!("Refusing {}: {reason}", file.file_name);
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
                received: on_disk,
            } => {
                if let Some(bar) = bars.remove(&file_id) {
                    bar.finish_and_clear();
                }
                let part = part_path(&path);
                let resumable = session.as_ref().is_some_and(|info| info.resumable);
                match &outcome {
                    SaveOutcome::Success => {
                        received += 1;
                        println!("Received {}", path.display());
                        let _ = app.db.remove_partial(&part);
                    }
                    SaveOutcome::Interrupted { .. } if resumable && on_disk > 0 => {
                        println!(
                            "Interrupted {} after {}; the sender can resume.",
                            path.display(),
                            ui::format_bytes(on_disk)
                        );
                        if let Ok(Some(mut partial)) = app.db.partial_by_path(&part) {
                            partial.received = on_disk;
                            partial.updated_at = unix_now();
                            let _ = app.db.upsert_partial(&partial);
                        }
                    }
                    other => {
                        failed += 1;
                        println!("Failed {}: {other}", path.display());
                        let _ = app.db.remove_partial(&part);
                    }
                }
                if let Some(info) = session.as_ref() {
                    let (file, started_at, record_id) = match info.files.get(&file_id) {
                        Some(entry) => {
                            (entry.dto.clone(), entry.started_at, entry.record_id.clone())
                        }
                        None => (
                            placeholder_file(&file_id, &path),
                            unix_now(),
                            uuid::Uuid::new_v4().to_string(),
                        ),
                    };
                    app.record_transfer(&TransferRecord {
                        id: record_id,
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
                            SaveOutcome::Success => None,
                            other => Some(other.to_string()),
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
                    SessionEndReason::TimedOut => "timed out (the sender went away)",
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

/// Files of a new request that match a partial upload from the same sender:
/// their offsets for the response and the paths to keep writing to.
fn resumable_files(
    app: &App,
    sender_fingerprint: &str,
    files: &HashMap<String, FileDto>,
) -> (HashMap<String, u64>, HashMap<String, PathBuf>) {
    let mut offsets = HashMap::new();
    let mut paths = HashMap::new();
    for (id, file) in files {
        let Some(sha256) = &file.sha256 else {
            continue;
        };
        let Ok(Some(partial)) = app.db.find_partial(sender_fingerprint, sha256, file.size) else {
            continue;
        };
        let on_disk = std::fs::metadata(&partial.part_path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        if on_disk > 0 && on_disk <= file.size && !partial.final_path.exists() {
            offsets.insert(id.clone(), on_disk);
            paths.insert(id.clone(), partial.final_path.clone());
        } else {
            let _ = app.db.remove_partial(&partial.part_path);
            let _ = std::fs::remove_file(&partial.part_path);
        }
    }
    (offsets, paths)
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
