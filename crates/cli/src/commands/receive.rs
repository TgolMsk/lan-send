use crate::app::App;
use crate::commands::incoming::{Incoming, IncomingOptions};
use anyhow::Context;
use lan_send_core::protocol::INTENT_CLIPBOARD;
use lan_send_core::store::{ConflictPolicy, OrganizeRules};
use lan_send_core::transfer::Destination;
use lan_send_core::transport::SessionEndReason;
use std::path::PathBuf;
use tokio::sync::mpsc;

pub struct ReceiveOptions {
    pub dir: Option<PathBuf>,
    pub pin: Option<String>,
    pub auto_accept: bool,
    /// Accept pairing requests without asking (testing only).
    pub accept_pairing: bool,
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

    let mut incoming = Incoming::new(
        &app,
        destination,
        IncomingOptions {
            auto_accept: options.auto_accept,
            accept_pairing: options.accept_pairing,
            accept_clipboard_files: true,
        },
    );

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
        if let Some(session) = incoming.handle(&server, &discovery, event).await
            && session.intent.as_deref() == Some(INTENT_CLIPBOARD)
            && session.reason == SessionEndReason::Finished
            && !session.received.is_empty()
        {
            println!(
                "{} shared {} clipboard file(s){}.",
                session.peer_alias,
                session.received.len(),
                if session.failed > 0 {
                    format!(", {} failed", session.failed)
                } else {
                    String::new()
                }
            );
            app.apply_clipboard_files(&session.received, &session.peer_fingerprint, None);
        }
    }

    println!(
        "{} file(s) received, {} failed.",
        incoming.received_total, incoming.failed_total
    );
    server.stop().await;
    discovery.stop().await;
    Ok(())
}
