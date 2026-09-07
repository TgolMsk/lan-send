//! Pairing over a real TLS server: the responder's user accepts or declines,
//! and only accepted certificates count as paired.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use lan_send_core::protocol::{
    DeviceInfo, DeviceType, PROTOCOL_VERSION, ProtocolType, verification_code,
};
use lan_send_core::transport::server::{self, DEFAULT_UPLOAD_IDLE_TIMEOUT};
use lan_send_core::transport::{
    Client, ClientCertPolicy, Identity, ServerConfig, ServerEvent, Target,
};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::mpsc;

fn info(identity: &Identity) -> DeviceInfo {
    DeviceInfo {
        alias: "responder".into(),
        version: PROTOCOL_VERSION.into(),
        device_model: None,
        device_type: Some(DeviceType::Headless),
        fingerprint: identity.fingerprint().to_string(),
        port: 0,
        protocol: ProtocolType::Https,
        download: false,
        ext: None,
    }
}

/// Starts a responder whose user answers every pairing request with
/// `accept` and reports the code it was shown.
async fn start_responder(
    identity: Arc<Identity>,
    accept: bool,
) -> (server::ServerHandle, mpsc::Receiver<(String, String)>) {
    let (events_tx, mut events_rx) = mpsc::channel(16);
    let handle = server::start(ServerConfig {
        port: 0,
        identity: identity.clone(),
        client_cert_policy: ClientCertPolicy::Required,
        device: info(&identity),
        pin: None,
        verify_checksums: true,
        upload_idle_timeout: DEFAULT_UPLOAD_IDLE_TIMEOUT,
        ipv6: false,
        paired: HashSet::new(),
        clipboard_limits: Default::default(),
        events: events_tx,
    })
    .await
    .expect("server starts");
    let (seen_tx, seen_rx) = mpsc::channel(16);
    tokio::spawn(async move {
        while let Some(event) = events_rx.recv().await {
            if let ServerEvent::PairRequest {
                alias,
                code,
                decision,
                ..
            } = event
            {
                let _ = seen_tx.send((alias, code)).await;
                let _ = decision.send(accept);
            }
        }
    });
    (handle, seen_rx)
}

#[tokio::test]
async fn accepted_pairing_trusts_the_certificate() {
    let responder = Arc::new(Identity::generate().unwrap());
    let initiator = Identity::generate().unwrap();
    let (server, mut seen) = start_responder(responder.clone(), true).await;
    let target = Target {
        host: "127.0.0.1".into(),
        port: server.port(),
        protocol: ProtocolType::Https,
    };
    let client = Client::new(&initiator, Some(responder.fingerprint().clone()), None).unwrap();

    assert!(!server.is_paired(initiator.fingerprint()));
    let response = client.pair(&target, "initiator").await.unwrap();
    assert!(response.accepted);
    assert_eq!(response.alias, "responder");
    let (alias, code) = seen.recv().await.unwrap();
    assert_eq!(alias, "initiator");
    assert_eq!(
        code,
        verification_code(initiator.fingerprint(), responder.fingerprint())
    );
    assert!(server.is_paired(initiator.fingerprint()));

    client.unpair(&target).await.unwrap();
    assert!(!server.is_paired(initiator.fingerprint()));
    server.stop().await;
}

#[tokio::test]
async fn declined_pairing_is_refused() {
    let responder = Arc::new(Identity::generate().unwrap());
    let initiator = Identity::generate().unwrap();
    let (server, _seen) = start_responder(responder.clone(), false).await;
    let target = Target {
        host: "127.0.0.1".into(),
        port: server.port(),
        protocol: ProtocolType::Https,
    };
    let client = Client::new(&initiator, Some(responder.fingerprint().clone()), None).unwrap();
    let err = client.pair(&target, "initiator").await.unwrap_err();
    assert_eq!(err.status(), Some(403));
    assert!(!server.is_paired(initiator.fingerprint()));
    server.stop().await;
}
