//! `lan-send clip watch / push / history` (ADR-0011).

use crate::app::App;
use crate::ui;
use anyhow::{Context, bail};
use bytes::Bytes;
use lan_send_core::clipboard::{
    ClipboardItem, ClipboardPayload, ClipboardSync, ImageFormat, SyncConfig, SyncEvent,
    platform_backend,
};
use lan_send_core::discovery::DiscoveryEvent;
use lan_send_core::protocol::Fingerprint;
use lan_send_core::store::ClipboardRecord;
use lan_send_core::transport::{Client, ServerEvent};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;

/// Bidirectional sync with the given (or all) paired devices until Ctrl+C.
pub async fn watch(app: App, devices: Vec<String>) -> anyhow::Result<()> {
    let backend = platform_backend().context("the clipboard is not supported on this platform")?;
    let peers = app.clipboard_peers(&devices)?;
    if peers.is_empty() {
        bail!("no paired devices; run `lan-send pair <device>` first");
    }

    let (server_tx, mut server_rx) = mpsc::channel(256);
    let server = app
        .start_server(app.settings.pin.clone(), true, server_tx)
        .await?;
    let discovery = app.start_discovery(server.port());
    let mut discovery_events = discovery.subscribe();
    {
        // Refresh the peers' addresses: announce and probe what we know.
        let discovery = discovery.clone();
        let known = app.known_targets();
        tokio::spawn(async move {
            tokio::join!(discovery.announce(), discovery.probe_many(known));
        });
    }

    let (sync_tx, mut sync_rx) = mpsc::channel(64);
    let mut config = SyncConfig::new(backend, app.identity.clone(), sync_tx);
    config.peers = peers.clone();
    config.text_limit = app.settings.clipboard.text_limit;
    config.image_limit = app.settings.clipboard.image_limit;
    config.poll_interval = Duration::from_millis(app.settings.clipboard.poll_interval_ms.max(50));
    let sync = ClipboardSync::start(config);

    println!(
        "Syncing the clipboard as {} ({}) with: {}",
        app.alias,
        app.identity.fingerprint().short(),
        peers
            .iter()
            .map(|peer| format!("{} ({})", peer.alias, peer.fingerprint.short()))
            .collect::<Vec<_>>()
            .join(", ")
    );
    eprintln!("Press Ctrl+C to stop.");

    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);
    loop {
        tokio::select! {
            _ = &mut ctrl_c => {
                eprintln!("Stopping.");
                break;
            }
            event = server_rx.recv() => {
                let Some(event) = event else { break };
                match event {
                    ServerEvent::ClipboardReceived { peer, item, sensitive } => {
                        let from = peer
                            .cert_fingerprint
                            .as_ref()
                            .and_then(|fingerprint| sync.peers().into_iter().find(|p| &p.fingerprint == fingerprint))
                            .map(|p| p.alias)
                            .unwrap_or_else(|| peer.addr.to_string());
                        let description = item.payload.describe();
                        match sync.apply_remote(item.clone()).await {
                            Ok(()) => {
                                let stored = app.record_clipboard(&item, sensitive);
                                println!(
                                    "{} {from}: {description}{}",
                                    ui::format_time(item.created_at / 1000),
                                    if stored { "" } else { " (not kept in history)" }
                                );
                            }
                            Err(err) => println!("Could not apply the clipboard from {from}: {err}"),
                        }
                    }
                    ServerEvent::PairRequest { peer, alias, code, decision } => {
                        app.answer_pair_request(&server, false, &peer, &alias, &code, decision).await;
                        if let Ok(peers) = app.clipboard_peers(&devices) {
                            sync.set_peers(peers);
                        }
                    }
                    other => app.handle_background_event(&discovery, other),
                }
            }
            event = sync_rx.recv() => {
                let Some(event) = event else { break };
                match event {
                    SyncEvent::LocalChange(item) => {
                        let stored = app.record_clipboard(&item, false);
                        println!(
                            "{} copied here: {}{}",
                            ui::format_time(item.created_at / 1000),
                            item.payload.describe(),
                            if stored { "" } else { " (not kept in history)" }
                        );
                    }
                    SyncEvent::Pushed { peer, result, .. } => {
                        let alias = sync
                            .peers()
                            .into_iter()
                            .find(|p| p.fingerprint == peer)
                            .map(|p| p.alias)
                            .unwrap_or_else(|| peer.short().to_string());
                        match result {
                            Ok(()) => println!("  -> {alias}: ok"),
                            Err(err) => println!("  -> {alias}: {err}"),
                        }
                    }
                    SyncEvent::TooLarge { kind, size, limit } => println!(
                        "Skipped a {kind} of {} (limit {}); send it as a file with `lan-send send`.",
                        ui::format_bytes(size as u64),
                        ui::format_bytes(limit as u64)
                    ),
                    SyncEvent::Applied(_) => {}
                    SyncEvent::Error(err) => eprintln!("clipboard: {err}"),
                }
            }
            event = discovery_events.recv() => {
                // Paired devices may show up at new addresses.
                if let Ok(DiscoveryEvent::Found(device) | DiscoveryEvent::Updated(device)) = event {
                    let mut peers = sync.peers();
                    if let Some(peer) = peers.iter_mut().find(|p| p.fingerprint == device.fingerprint) {
                        peer.target = device.target();
                        sync.set_peers(peers);
                    }
                }
            }
        }
    }

    sync.stop();
    server.stop().await;
    discovery.stop().await;
    Ok(())
}

/// Sends the current clipboard once to one paired device.
pub async fn push(app: App, device: String) -> anyhow::Result<()> {
    let backend = platform_backend().context("the clipboard is not supported on this platform")?;
    let peers = app.clipboard_peers(std::slice::from_ref(&device))?;
    let peer = peers.into_iter().next().context("no such paired device")?;

    let payload = tokio::task::spawn_blocking(move || {
        lan_send_core::clipboard::backend::with_retry(|| backend.read())
    })
    .await??;
    let Some(payload) = payload else {
        bail!("the clipboard is empty");
    };
    if let ClipboardPayload::Files { .. } = payload {
        bail!(
            "file lists are sent with `lan-send send` (clipboard file sync arrives later in milestone 3)"
        );
    }
    let item = ClipboardItem::new(app.identity.fingerprint().clone(), payload);
    let client = Client::new(
        &app.identity,
        Some(peer.fingerprint.clone()),
        Some(Duration::from_secs(30)),
    )?;
    client
        .send_clipboard(&peer.target, &item)
        .await
        .with_context(|| format!("could not push to {}", peer.alias))?;
    let stored = app.record_clipboard(&item, false);
    println!(
        "Pushed {} to {}{}",
        item.payload.describe(),
        peer.alias,
        if stored { "" } else { " (not kept in history)" }
    );
    Ok(())
}

/// Lists, restores, deletes or clears the clipboard history.
pub fn history(
    app: &App,
    limit: usize,
    copy: Option<String>,
    delete: Option<String>,
    clear: bool,
) -> anyhow::Result<()> {
    if clear {
        let removed = app.db.clear_clipboard()?;
        for record in &removed {
            if let Some(path) = &record.image_path {
                let _ = std::fs::remove_file(path);
            }
        }
        println!("Removed {} clipboard entries.", removed.len());
        return Ok(());
    }
    if let Some(id) = delete {
        let full = resolve_id(app, &id)?;
        if let Some(record) = app.db.delete_clipboard(&full)?
            && let Some(path) = record.image_path
        {
            let _ = std::fs::remove_file(path);
        }
        println!("Deleted {full}.");
        return Ok(());
    }
    if let Some(id) = copy {
        let full = resolve_id(app, &id)?;
        let record = app.db.clipboard_item(&full)?.context("no such entry")?;
        let payload = payload_of(&record)?;
        let backend =
            platform_backend().context("the clipboard is not supported on this platform")?;
        lan_send_core::clipboard::backend::with_retry(|| backend.write(&payload))?;
        println!("Copied {} back to the clipboard.", payload.describe());
        return Ok(());
    }
    let records = app.db.list_clipboard(limit)?;
    if records.is_empty() {
        println!("No clipboard history yet.");
        return Ok(());
    }
    println!(
        "{:<8} {:<16} {:<10} {:<6} CONTENT",
        "ID", "WHEN", "FROM", "KIND"
    );
    for record in records {
        let from = if Fingerprint::parse(&record.origin) == *app.identity.fingerprint() {
            "here".to_string()
        } else {
            record.origin.get(..8).unwrap_or(&record.origin).to_string()
        };
        let content = match record.kind.as_str() {
            "text" => {
                let text = record.text.clone().unwrap_or_default();
                let mut line: String = text.lines().next().unwrap_or("").chars().take(60).collect();
                if text.chars().count() > line.chars().count() {
                    line.push('…');
                }
                line
            }
            "image" => format!(
                "{}x{} {} image, {}",
                record.image_width.unwrap_or(0),
                record.image_height.unwrap_or(0),
                record.image_format.clone().unwrap_or_default(),
                ui::format_bytes(record.size)
            ),
            _ => format!("{} file(s)", record.file_paths.len()),
        };
        println!(
            "{:<8} {:<16} {:<10} {:<6} {content}",
            record.id.get(..8).unwrap_or(&record.id),
            ui::format_time(record.created_at / 1000),
            from,
            record.kind
        );
    }
    Ok(())
}

fn resolve_id(app: &App, prefix: &str) -> anyhow::Result<String> {
    let matching: Vec<String> = app
        .db
        .list_clipboard(usize::MAX)?
        .into_iter()
        .filter(|record| record.id.starts_with(prefix))
        .map(|record| record.id)
        .collect();
    match matching.as_slice() {
        [one] => Ok(one.clone()),
        [] => bail!("no clipboard entry starts with {prefix}"),
        _ => bail!(
            "{prefix} matches {} entries; give more characters",
            matching.len()
        ),
    }
}

fn payload_of(record: &ClipboardRecord) -> anyhow::Result<ClipboardPayload> {
    Ok(match record.kind.as_str() {
        "text" => ClipboardPayload::Text {
            plain: record.text.clone().unwrap_or_default(),
            html: record.html.clone(),
            rtf: record.rtf.clone(),
        },
        "image" => {
            let path = record.image_path.clone().context("image file missing")?;
            let bytes =
                std::fs::read(&path).with_context(|| format!("cannot read {}", path.display()))?;
            ClipboardPayload::Image {
                format: match record.image_format.as_deref() {
                    Some("jpg") => ImageFormat::Jpeg,
                    _ => ImageFormat::Png,
                },
                bytes: Bytes::from(bytes),
                width: record.image_width.unwrap_or(0),
                height: record.image_height.unwrap_or(0),
            }
        }
        _ => ClipboardPayload::Files {
            paths: record.file_paths.iter().map(PathBuf::from).collect(),
        },
    })
}
