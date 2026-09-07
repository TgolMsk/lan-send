//! The clipboard endpoint over a real TLS server: paired senders get their
//! items delivered (JSON and multipart), unpaired ones are refused, and
//! oversized items are rejected.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use bytes::Bytes;
use lan_send_core::clipboard::{ClipboardItem, ClipboardPayload, ImageFormat};
use lan_send_core::protocol::{DeviceInfo, DeviceType, PROTOCOL_VERSION, ProtocolType};
use lan_send_core::transport::server::{self, ClipboardLimits, DEFAULT_UPLOAD_IDLE_TIMEOUT};
use lan_send_core::transport::{
    Client, ClientCertPolicy, Identity, ServerConfig, ServerEvent, Target,
};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::mpsc;

fn info(identity: &Identity) -> DeviceInfo {
    DeviceInfo {
        alias: "receiver".into(),
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

async fn start_receiver(
    identity: Arc<Identity>,
    paired: HashSet<lan_send_core::protocol::Fingerprint>,
) -> (server::ServerHandle, mpsc::Receiver<(ClipboardItem, bool)>) {
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
        paired,
        clipboard_limits: ClipboardLimits {
            text: 64,
            image: 2 * 1024 * 1024,
        },
        events: events_tx,
    })
    .await
    .expect("server starts");
    let (items_tx, items_rx) = mpsc::channel(16);
    tokio::spawn(async move {
        while let Some(event) = events_rx.recv().await {
            if let ServerEvent::ClipboardReceived {
                item, sensitive, ..
            } = event
            {
                let _ = items_tx.send((item, sensitive)).await;
            }
        }
    });
    (handle, items_rx)
}

#[tokio::test]
async fn paired_sender_delivers_text_and_images() {
    let receiver = Arc::new(Identity::generate().unwrap());
    let sender = Identity::generate().unwrap();
    let (server, mut items) = start_receiver(
        receiver.clone(),
        HashSet::from([sender.fingerprint().clone()]),
    )
    .await;
    let target = Target {
        host: "127.0.0.1".into(),
        port: server.port(),
        protocol: ProtocolType::Https,
    };
    let client = Client::new(&sender, Some(receiver.fingerprint().clone()), None).unwrap();

    let text = ClipboardItem::new(
        sender.fingerprint().clone(),
        ClipboardPayload::Text {
            plain: "hello from the other side".into(),
            html: Some("<b>hello</b>".into()),
            rtf: None,
        },
    );
    client.send_clipboard(&target, &text).await.unwrap();
    let (received, sensitive) = items.recv().await.unwrap();
    assert_eq!(received, text);
    assert!(!sensitive);

    let secret = ClipboardItem::new(
        sender.fingerprint().clone(),
        ClipboardPayload::Text {
            plain: "sk-live-9fA3kQz8LmP2xR7vT1wY5bN0cH4dJ6e".into(),
            html: None,
            rtf: None,
        },
    );
    client.send_clipboard(&target, &secret).await.unwrap();
    let (_, sensitive) = items.recv().await.unwrap();
    assert!(sensitive, "secret-looking text must be flagged");

    // Above the multipart threshold: travels as a binary part.
    let pixels: Vec<u8> = (0..700_000u32).map(|i| (i % 253) as u8).collect();
    let image = ClipboardItem::new(
        sender.fingerprint().clone(),
        ClipboardPayload::Image {
            format: ImageFormat::Png,
            bytes: Bytes::from(pixels),
            width: 1000,
            height: 700,
        },
    );
    client.send_clipboard(&target, &image).await.unwrap();
    let (received, _) = items.recv().await.unwrap();
    assert_eq!(received, image);

    let too_long = ClipboardItem::new(
        sender.fingerprint().clone(),
        ClipboardPayload::Text {
            plain: "x".repeat(65),
            html: None,
            rtf: None,
        },
    );
    let err = client.send_clipboard(&target, &too_long).await.unwrap_err();
    assert_eq!(err.status(), Some(413));

    server.stop().await;
}

#[tokio::test]
async fn unpaired_sender_is_refused() {
    let receiver = Arc::new(Identity::generate().unwrap());
    let sender = Identity::generate().unwrap();
    let (server, _items) = start_receiver(receiver.clone(), HashSet::new()).await;
    let target = Target {
        host: "127.0.0.1".into(),
        port: server.port(),
        protocol: ProtocolType::Https,
    };
    let client = Client::new(&sender, Some(receiver.fingerprint().clone()), None).unwrap();
    let item = ClipboardItem::new(
        sender.fingerprint().clone(),
        ClipboardPayload::Text {
            plain: "nope".into(),
            html: None,
            rtf: None,
        },
    );
    let err = client.send_clipboard(&target, &item).await.unwrap_err();
    assert_eq!(err.status(), Some(403));
    server.stop().await;
}
