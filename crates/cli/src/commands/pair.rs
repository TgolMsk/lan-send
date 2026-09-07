use crate::app::App;
use crate::commands::send::{direct_target, wait_for_device};
use crate::ui;
use anyhow::{Context, bail};
use lan_send_core::protocol::verification_code;
use lan_send_core::store::KnownDevice;
use lan_send_core::transport::{Client, ClientError};
use std::time::Duration;
use tokio::sync::mpsc;

/// `lan-send pair <device>`: both devices show the verification code and
/// their users confirm it (ADR-0010).
pub async fn run(app: App, query: String, timeout: Duration, yes: bool) -> anyhow::Result<()> {
    let (events_tx, mut events_rx) = mpsc::channel(64);
    let server = app.start_server(None, true, events_tx).await?;
    let discovery = app.start_discovery(server.port());

    eprintln!("Looking for {query}...");
    let result = async {
        let device = wait_for_device(&app, &discovery, &mut events_rx, &query, timeout).await?;
        let target = direct_target(&query).unwrap_or_else(|| device.target());
        let code = verification_code(app.identity.fingerprint(), &device.fingerprint);
        println!(
            "Pairing with {} ({}) at {}:{}",
            device.alias,
            device.fingerprint.short(),
            target.host,
            target.port
        );
        println!("Verification code: {code}");
        println!(
            "Confirm the same code on {}; waiting for it...",
            device.alias
        );

        let client = Client::new(&app.identity, Some(device.fingerprint.clone()), None)?;
        match client.pair(&target, &app.alias).await {
            Ok(response) => {
                let confirmed = yes
                    || ui::prompt_line(&format!(
                        "Does {} show the same code {code}? [y/N] ",
                        response.alias
                    ))
                    .await
                    .map(|answer| matches!(answer.to_ascii_lowercase().as_str(), "y" | "yes"))
                    .unwrap_or(false);
                if !confirmed {
                    let _ = client.unpair(&target).await;
                    bail!("codes did not match; not paired");
                }
                let mut known = KnownDevice::from(&device);
                known.host = Some(target.host.clone());
                known.port = Some(target.port);
                app.db.upsert_device(&known)?;
                app.db.set_paired(known.fingerprint.as_str(), true)?;
                server.add_paired(device.fingerprint.clone());
                println!(
                    "Paired with {} ({}).",
                    response.alias,
                    device.fingerprint.short()
                );
                Ok(())
            }
            Err(ClientError::Status { status: 403, .. }) => {
                bail!("{} declined the pairing", device.alias)
            }
            Err(ClientError::Status { status: 408, .. }) => {
                bail!("{} did not answer in time", device.alias)
            }
            Err(ClientError::Status { status: 409, .. }) => {
                bail!("{} is busy with another pairing request", device.alias)
            }
            Err(ClientError::Status { status: 404, .. }) => {
                bail!(
                    "{} does not support pairing (official LocalSend?)",
                    device.alias
                )
            }
            Err(err) => Err(err).context("pairing request failed"),
        }
    }
    .await;

    server.stop().await;
    discovery.stop().await;
    result
}
