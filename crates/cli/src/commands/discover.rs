use crate::app::{App, handle_background_event};
use crate::ui;
use std::time::Duration;
use tokio::sync::mpsc;

pub async fn run(app: App, timeout: Duration) -> anyhow::Result<()> {
    let (events_tx, mut events_rx) = mpsc::channel(64);
    let server = app.start_server(None, true, events_tx).await?;
    let discovery = app.start_discovery(server.port());
    if let Some(err) = discovery.multicast_error() {
        eprintln!("multicast unavailable ({err}); relying on subnet scan");
    }
    eprintln!(
        "Discovering as {} ({}) for {:.1}s...",
        app.alias,
        app.identity.fingerprint().short(),
        timeout.as_secs_f64()
    );

    let staged = {
        let discovery = discovery.clone();
        tokio::spawn(async move {
            discovery
                .discover_staged(Vec::new(), Duration::from_secs(1))
                .await;
        })
    };

    let deadline = tokio::time::sleep(timeout);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            _ = &mut deadline => break,
            event = events_rx.recv() => match event {
                Some(event) => handle_background_event(&discovery, event),
                None => break,
            },
        }
    }
    staged.abort();

    ui::print_device_table(&discovery.devices());
    server.stop().await;
    discovery.stop().await;
    Ok(())
}
