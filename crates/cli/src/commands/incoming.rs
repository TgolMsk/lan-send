//! Handling of incoming transfers, shared by `receive` and `clip watch`:
//! accepting requests, placing files, recording history, tracking sessions.

use crate::app::App;
use crate::ui;
use indicatif::{MultiProgress, ProgressBar};
use lan_send_core::discovery::{Device, Discovery};
use lan_send_core::protocol::{FEATURE_RESUME, FileDto, INTENT_CLIPBOARD};
use lan_send_core::store::{
    Direction, KnownDevice, PartialUpload, TransferRecord, TransferStatus, unix_now,
};
use lan_send_core::transfer::{Destination, Placement};
use lan_send_core::transport::server::{SaveOutcome, part_path};
use lan_send_core::transport::{
    ServerEvent, ServerHandle, SessionEndReason, UploadDecision, UploadTarget,
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

pub struct IncomingOptions {
    /// Accept every transfer request without asking.
    pub auto_accept: bool,
    /// Accept pairing requests without asking (testing only).
    pub accept_pairing: bool,
    /// Accept clipboard-intent transfers from paired devices without asking.
    pub accept_clipboard_files: bool,
}

/// One file of the current session, for history entries.
struct SessionFile {
    dto: FileDto,
    started_at: i64,
    /// History entry id; every attempt updates the same entry.
    record_id: String,
}

struct SessionInfo {
    session_id: String,
    peer_alias: String,
    peer_fingerprint: String,
    intent: Option<String>,
    resumable: bool,
    resume_paths: HashMap<String, PathBuf>,
    files: HashMap<String, SessionFile>,
    received: Vec<PathBuf>,
    failed: usize,
}

/// A session that reached its end.
pub struct CompletedSession {
    pub peer_alias: String,
    pub peer_fingerprint: String,
    pub intent: Option<String>,
    pub received: Vec<PathBuf>,
    pub failed: usize,
    pub reason: SessionEndReason,
}

pub struct Incoming<'a> {
    app: &'a App,
    destination: Destination,
    options: IncomingOptions,
    progress: MultiProgress,
    bars: HashMap<String, ProgressBar>,
    assigned: HashSet<PathBuf>,
    session: Option<SessionInfo>,
    pub received_total: usize,
    pub failed_total: usize,
}

impl<'a> Incoming<'a> {
    pub fn new(app: &'a App, destination: Destination, options: IncomingOptions) -> Self {
        Self {
            app,
            destination,
            options,
            progress: MultiProgress::new(),
            bars: HashMap::new(),
            assigned: HashSet::new(),
            session: None,
            received_total: 0,
            failed_total: 0,
        }
    }

    /// Handles one server event. Returns the session when it just ended.
    pub async fn handle(
        &mut self,
        server: &ServerHandle,
        discovery: &Discovery,
        event: ServerEvent,
    ) -> Option<CompletedSession> {
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
                let _ = self.app.db.upsert_device(&KnownDevice::from(&device));
                let resumable = self.app.settings.resume
                    && info
                        .ext
                        .as_ref()
                        .is_some_and(|ext| ext.supports(FEATURE_RESUME));
                let intent = info.ext.as_ref().and_then(|ext| ext.intent.clone());
                let paired = self
                    .app
                    .db
                    .device(sender.as_str())
                    .ok()
                    .flatten()
                    .is_some_and(|known| known.paired);
                let clipboard_files = intent.as_deref() == Some(INTENT_CLIPBOARD) && paired;

                let total: u64 = files.values().map(|file| file.size).sum();
                let mut names: Vec<&str> =
                    files.values().map(|file| file.file_name.as_str()).collect();
                names.sort_unstable();
                println!(
                    "\n{} from {} ({}) at {}: {} file(s), {}{}",
                    if clipboard_files {
                        "Clipboard files"
                    } else {
                        "Request"
                    },
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
                let accept = self.options.auto_accept
                    || (clipboard_files && self.options.accept_clipboard_files)
                    || ui::prompt_line("Accept? [y/N] ")
                        .await
                        .map(|answer| matches!(answer.to_ascii_lowercase().as_str(), "y" | "yes"))
                        .unwrap_or(false);
                let answer = if accept {
                    let (offsets, resume_paths) = if resumable {
                        resumable_files(self.app, &sender.to_string(), &files)
                    } else {
                        (HashMap::new(), HashMap::new())
                    };
                    if !offsets.is_empty() {
                        println!(
                            "Resuming {} file(s) from an earlier session.",
                            offsets.len()
                        );
                    }
                    self.session = Some(SessionInfo {
                        session_id: session_id.clone(),
                        peer_alias: info.alias.clone(),
                        peer_fingerprint: sender.to_string(),
                        intent,
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
                        received: Vec::new(),
                        failed: 0,
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
                    self.session = None;
                }
                self.assigned.clear();
                None
            }
            ServerEvent::PrepareUploadAborted { session_id } => {
                println!(
                    "Request {} was withdrawn by the sender.",
                    short_id(&session_id)
                );
                self.session = None;
                None
            }
            ServerEvent::FileUpload {
                file_id,
                file,
                target,
                ..
            } => {
                let (sender_alias, sender_fingerprint, resumable, resumed_path) =
                    match &self.session {
                        Some(info) => (
                            info.peer_alias.clone(),
                            info.peer_fingerprint.clone(),
                            info.resumable,
                            info.resume_paths.get(&file_id).cloned(),
                        ),
                        None => ("unknown".to_string(), String::new(), false, None),
                    };
                let placement = match resumed_path {
                    Some(path) => Ok(Placement::New(path)),
                    None => self.destination.place(&file, &sender_alias, &self.assigned),
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
                                decide_conflict(self.options.auto_accept, existing, renamed).await
                            }
                        };
                        self.assigned.insert(path.clone());
                        if resumable {
                            let _ = self.app.db.upsert_partial(&PartialUpload {
                                part_path: part_path(&path),
                                final_path: path.clone(),
                                sender_fingerprint,
                                file_name: file.file_name.clone(),
                                size: file.size,
                                sha256: file.sha256.clone(),
                                received: 0,
                                updated_at: unix_now(),
                            });
                        }
                        let bar = self
                            .progress
                            .add(ui::transfer_bar(&file.file_name, file.size));
                        self.bars.insert(file_id.clone(), bar);
                        UploadTarget::Path(path)
                    }
                    Err(err) => UploadTarget::Reject(err.to_string()),
                };
                if let UploadTarget::Reject(reason) = &answer {
                    println!("Refusing {}: {reason}", file.file_name);
                }
                let _ = target.send(answer);
                None
            }
            ServerEvent::FileUploadProgress {
                file_id, received, ..
            } => {
                if let Some(bar) = self.bars.get(&file_id) {
                    bar.set_position(received);
                }
                None
            }
            ServerEvent::FileUploadResult {
                session_id,
                file_id,
                path,
                outcome,
                received: on_disk,
            } => {
                if let Some(bar) = self.bars.remove(&file_id) {
                    bar.finish_and_clear();
                }
                let part = part_path(&path);
                let resumable = self.session.as_ref().is_some_and(|info| info.resumable);
                match &outcome {
                    SaveOutcome::Success => {
                        self.received_total += 1;
                        println!("Received {}", path.display());
                        let _ = self.app.db.remove_partial(&part);
                        if let Some(info) = self.session.as_mut() {
                            info.received.push(path.clone());
                        }
                    }
                    SaveOutcome::Interrupted { .. } if resumable && on_disk > 0 => {
                        println!(
                            "Interrupted {} after {}; the sender can resume.",
                            path.display(),
                            ui::format_bytes(on_disk)
                        );
                        if let Ok(Some(mut partial)) = self.app.db.partial_by_path(&part) {
                            partial.received = on_disk;
                            partial.updated_at = unix_now();
                            let _ = self.app.db.upsert_partial(&partial);
                        }
                    }
                    other => {
                        self.failed_total += 1;
                        if let Some(info) = self.session.as_mut() {
                            info.failed += 1;
                        }
                        println!("Failed {}: {other}", path.display());
                        let _ = self.app.db.remove_partial(&part);
                    }
                }
                if let Some(info) = self.session.as_ref() {
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
                    self.app.record_transfer(&TransferRecord {
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
                None
            }
            ServerEvent::SessionEnd { session_id, reason } => {
                let word = match reason {
                    SessionEndReason::Finished => "finished",
                    SessionEndReason::Cancelled => "cancelled by the sender",
                    SessionEndReason::TimedOut => "timed out (the sender went away)",
                };
                println!("Session {} {word}.", short_id(&session_id));
                if self
                    .session
                    .as_ref()
                    .is_some_and(|info| info.session_id == session_id)
                {
                    let info = self.session.take()?;
                    return Some(CompletedSession {
                        peer_alias: info.peer_alias,
                        peer_fingerprint: info.peer_fingerprint,
                        intent: info.intent,
                        received: info.received,
                        failed: info.failed,
                        reason,
                    });
                }
                None
            }
            ServerEvent::PairRequest {
                peer,
                alias,
                code,
                decision,
            } => {
                self.app
                    .answer_pair_request(
                        server,
                        self.options.accept_pairing,
                        &peer,
                        &alias,
                        &code,
                        decision,
                    )
                    .await;
                None
            }
            other => {
                self.app.handle_background_event(discovery, other);
                None
            }
        }
    }
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
