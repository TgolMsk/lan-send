//! Two runtimes in one process talk to each other through events and
//! answers: send with PIN, pairing, clipboard push (ADR-0013).

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use lan_send_core::protocol::DeviceType;
use lan_send_core::runtime::{Runtime, RuntimeConfig, RuntimeEvent, SendRequest, TransferState};
use lan_send_core::store::{AppPaths, Settings};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;

struct Node {
    runtime: Runtime,
    events: mpsc::Receiver<RuntimeEvent>,
    receive_dir: PathBuf,
    _dir: tempfile::TempDir,
}

async fn node(alias: &str, pin: Option<&str>) -> Node {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::under(dir.path());
    paths.ensure_dirs().unwrap();
    let receive_dir = dir.path().join("received");
    let mut settings = Settings {
        receive_dir: Some(receive_dir.clone()),
        pin: pin.map(String::from),
        ..Settings::default()
    };
    settings.clipboard.sync_enabled = false;
    settings.save(&paths.settings_file()).unwrap();
    let (tx, events) = mpsc::channel(256);
    let mut config = RuntimeConfig::new(paths, DeviceType::Headless, tx);
    config.alias = Some(alias.to_string());
    config.port = Some(0);
    config.active_discovery = false;
    let runtime = Runtime::start(config).await.expect("runtime starts");
    Node {
        runtime,
        events,
        receive_dir,
        _dir: dir,
    }
}

async fn wait_for<T>(
    events: &mut mpsc::Receiver<RuntimeEvent>,
    what: &str,
    mut pick: impl FnMut(&RuntimeEvent) -> Option<T>,
) -> T {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let event = tokio::time::timeout(remaining, events.recv())
            .await
            .unwrap_or_else(|_| panic!("timed out waiting for {what}"))
            .expect("event channel open");
        if let Some(value) = pick(&event) {
            return value;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn send_with_pin_accept_and_history() {
    let mut a = node("alpha", None).await;
    let mut b = node("bravo", Some("2468")).await;
    let source = a._dir.path().join("hello.txt");
    std::fs::write(&source, b"hello over the runtime").unwrap();
    let target = format!("127.0.0.1:{}", b.runtime.identity().port);

    let transfer_id = a
        .runtime
        .send(SendRequest {
            device: target,
            paths: vec![source],
            pin: None,
            intent: None,
        })
        .unwrap();

    // The receiver wants a PIN first.
    let asked = wait_for(&mut a.events, "pin request", |event| match event {
        RuntimeEvent::TransferNeedsPin {
            transfer_id: id, ..
        } if id == &transfer_id => Some(()),
        _ => None,
    })
    .await;
    assert_eq!(asked, ());
    assert!(matches!(
        a.runtime.transfers().first().map(|t| t.state),
        Some(TransferState::WaitingPin)
    ));
    a.runtime
        .provide_pin(&transfer_id, Some("2468".into()))
        .unwrap();

    // Now the request reaches bravo's user.
    let request = wait_for(&mut b.events, "incoming request", |event| match event {
        RuntimeEvent::IncomingRequest { request } => Some(request.clone()),
        _ => None,
    })
    .await;
    assert_eq!(request.peer_alias, "alpha");
    assert_eq!(request.files.len(), 1);
    assert!(!request.auto_accepted);
    b.runtime
        .respond_incoming(&request.session_id, true)
        .unwrap();

    let sent = wait_for(&mut a.events, "sender completion", |event| match event {
        RuntimeEvent::TransferCompleted { transfer } if transfer.id == transfer_id => {
            Some(transfer.clone())
        }
        _ => None,
    })
    .await;
    assert_eq!(sent.state, TransferState::Finished, "{sent:?}");
    assert_eq!(sent.done_size, sent.total_size);

    let received = wait_for(&mut b.events, "receiver completion", |event| match event {
        RuntimeEvent::TransferCompleted { transfer } if transfer.id == request.session_id => {
            Some(transfer.clone())
        }
        _ => None,
    })
    .await;
    assert_eq!(received.state, TransferState::Finished, "{received:?}");
    let saved = b.receive_dir.join("hello.txt");
    assert_eq!(std::fs::read(&saved).unwrap(), b"hello over the runtime");

    assert_eq!(a.runtime.history(10).unwrap().len(), 1);
    assert_eq!(b.runtime.history(10).unwrap().len(), 1);
    assert!(a.runtime.devices().iter().any(|d| d.alias == "bravo"));

    a.runtime.stop().await;
    b.runtime.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn decline_and_pairing_round_trip() {
    let mut a = node("alpha", None).await;
    let mut b = node("bravo", None).await;
    let source = a._dir.path().join("secret.bin");
    std::fs::write(&source, vec![7u8; 4096]).unwrap();
    let target = format!("127.0.0.1:{}", b.runtime.identity().port);

    // Declined transfer.
    let transfer_id = a
        .runtime
        .send(SendRequest {
            device: target.clone(),
            paths: vec![source],
            pin: None,
            intent: None,
        })
        .unwrap();
    let request = wait_for(&mut b.events, "incoming request", |event| match event {
        RuntimeEvent::IncomingRequest { request } => Some(request.clone()),
        _ => None,
    })
    .await;
    b.runtime
        .respond_incoming(&request.session_id, false)
        .unwrap();
    let outcome = wait_for(&mut a.events, "declined", |event| match event {
        RuntimeEvent::TransferCompleted { transfer } if transfer.id == transfer_id => {
            Some(transfer.state)
        }
        _ => None,
    })
    .await;
    assert_eq!(outcome, TransferState::Declined);
    assert!(!b.receive_dir.join("secret.bin").exists());

    // Pairing: alpha asks, bravo confirms, alpha confirms.
    let view = a.runtime.pair_start(&target).await.unwrap();
    let request = wait_for(&mut b.events, "pair request", |event| match event {
        RuntimeEvent::PairRequest {
            fingerprint,
            alias,
            code,
            ..
        } => Some((fingerprint.clone(), alias.clone(), code.clone())),
        _ => None,
    })
    .await;
    assert_eq!(request.1, "alpha");
    assert_eq!(request.2, view.code, "both sides derive the same code");
    b.runtime.respond_pair_request(&request.0, true).unwrap();
    let accepted = wait_for(&mut b.events, "bravo's pair result", |event| match event {
        RuntimeEvent::PairResult { paired, .. } => Some(*paired),
        _ => None,
    })
    .await;
    assert!(accepted);
    let response = wait_for(&mut a.events, "pair response", |event| match event {
        RuntimeEvent::PairResponse {
            fingerprint, code, ..
        } => Some((fingerprint.clone(), code.clone())),
        _ => None,
    })
    .await;
    assert_eq!(response.0, view.fingerprint);
    a.runtime
        .pair_confirm(&view.fingerprint, true)
        .await
        .unwrap();
    let paired = wait_for(&mut a.events, "pair result", |event| match event {
        RuntimeEvent::PairResult { paired, .. } => Some(*paired),
        _ => None,
    })
    .await;
    assert!(paired);
    assert!(
        a.runtime
            .devices()
            .iter()
            .any(|d| d.alias == "bravo" && d.paired)
    );
    assert!(
        b.runtime
            .devices()
            .iter()
            .any(|d| d.alias == "alpha" && d.paired)
    );

    // Unpair from alpha; bravo learns about it.
    a.runtime.unpair(&view.fingerprint).await.unwrap();
    let withdrawn = wait_for(&mut b.events, "unpaired", |event| match event {
        RuntimeEvent::PairResult { paired, .. } => Some(*paired),
        _ => None,
    })
    .await;
    assert!(!withdrawn);
    assert!(b.runtime.devices().iter().all(|d| !d.paired));

    a.runtime.stop().await;
    b.runtime.stop().await;
}
