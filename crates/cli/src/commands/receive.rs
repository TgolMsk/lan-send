use crate::app::{App, handle_background_event};
use crate::ui;
use anyhow::Context;
use indicatif::{MultiProgress, ProgressBar};
use lan_send_core::transport::filename::{
    Rules, ensure_within, sanitize_relative_path, unique_path,
};
use lan_send_core::transport::{ServerEvent, SessionEndReason, UploadDecision, UploadTarget};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tokio::sync::mpsc;

pub struct ReceiveOptions {
    pub dir: Option<PathBuf>,
    pub pin: Option<String>,
    pub auto_accept: bool,
    pub verify_checksums: bool,
}

pub async fn run(app: App, options: ReceiveOptions) -> anyhow::Result<()> {
    let destination = options
        .dir
        .clone()
        .or_else(|| app.paths.download_dir.clone())
        .context("no download directory known; pass --dir")?;
    std::fs::create_dir_all(&destination)
        .with_context(|| format!("cannot create {}", destination.display()))?;
    let destination = destination
        .canonicalize()
        .with_context(|| format!("cannot resolve {}", destination.display()))?;

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
        destination.display(),
        if options.pin.is_some() {
            " [PIN required]"
        } else {
            ""
        }
    );
    eprintln!("Press Ctrl+C to stop.");
    {
        let discovery = discovery.clone();
        tokio::spawn(async move { discovery.announce().await });
    }

    let progress = MultiProgress::new();
    let mut bars: HashMap<String, ProgressBar> = HashMap::new();
    let mut assigned: HashSet<PathBuf> = HashSet::new();
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
                    UploadDecision::Accept(files.keys().cloned().collect())
                } else {
                    println!("Declined.");
                    UploadDecision::Decline
                };
                if decision.send(answer).is_err() {
                    println!("The sender withdrew the request {}.", short_id(&session_id));
                }
                assigned.clear();
            }
            ServerEvent::PrepareUploadAborted { session_id } => {
                println!(
                    "Request {} was withdrawn by the sender.",
                    short_id(&session_id)
                );
            }
            ServerEvent::FileUpload {
                file_id,
                file,
                target,
                ..
            } => {
                let answer = match sanitize_relative_path(&file.file_name, Rules::current()) {
                    Ok(relative) => {
                        let path = unique_path(&destination, &relative, &assigned);
                        match ensure_within(&destination, &path) {
                            Ok(()) => {
                                assigned.insert(path.clone());
                                let bar =
                                    progress.add(ui::transfer_bar(&file.file_name, file.size));
                                bars.insert(file_id.clone(), bar);
                                UploadTarget::Path(path)
                            }
                            Err(err) => UploadTarget::Reject(err.to_string()),
                        }
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
                file_id,
                path,
                outcome,
                ..
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
            }
            ServerEvent::SessionEnd { session_id, reason } => {
                let word = match reason {
                    SessionEndReason::Finished => "finished",
                    SessionEndReason::Cancelled => "cancelled by the sender",
                };
                println!("Session {} {word}.", short_id(&session_id));
            }
            other => handle_background_event(&discovery, other),
        }
    }

    println!("{received} file(s) received, {failed} failed.");
    server.stop().await;
    discovery.stop().await;
    Ok(())
}

fn short_id(session_id: &str) -> &str {
    session_id.get(..8).unwrap_or(session_id)
}
