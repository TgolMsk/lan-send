//! End-to-end test of the resume extension: an upload that breaks halfway
//! is resumed with `Range` after querying the receiver's offset, and a
//! sender without the extension gets plain v2 behaviour.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use bytes::Bytes;
use futures_util::StreamExt;
use lan_send_core::protocol::{
    DeviceInfo, DeviceType, Extensions, FEATURE_RESUME, FileDto, PROTOCOL_VERSION,
    PrepareUploadRequest, ProtocolType,
};
use lan_send_core::transport::server::{self, SaveOutcome};
use lan_send_core::transport::{
    Client, ClientCertPolicy, ClientError, Identity, PrepareUploadOutcome, Resume, ServerConfig,
    ServerEvent, Target, UploadDecision, UploadTarget,
};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

fn info(identity: &Identity, port: u16, resume: bool) -> DeviceInfo {
    DeviceInfo {
        alias: "test".into(),
        version: PROTOCOL_VERSION.into(),
        device_model: None,
        device_type: Some(DeviceType::Headless),
        fingerprint: identity.fingerprint().to_string(),
        port,
        protocol: ProtocolType::Https,
        download: false,
        ext: resume.then(|| Extensions::current([FEATURE_RESUME])),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Starts a receiver that accepts everything into `dir` and reports every
/// upload result on the returned channel.
async fn start_receiver(
    identity: Arc<Identity>,
    dir: PathBuf,
) -> (server::ServerHandle, mpsc::Receiver<(SaveOutcome, u64)>) {
    let (events_tx, mut events_rx) = mpsc::channel(64);
    let handle = server::start(ServerConfig {
        port: 0,
        identity: identity.clone(),
        client_cert_policy: ClientCertPolicy::Required,
        device: info(&identity, 0, true),
        pin: None,
        verify_checksums: true,
        // Short: the test's dying client does not close its connection.
        upload_idle_timeout: std::time::Duration::from_secs(2),
        ipv6: false,
        paired: HashSet::new(),
        events: events_tx,
    })
    .await
    .expect("server starts");
    let (results_tx, results_rx) = mpsc::channel(64);
    tokio::spawn(async move {
        while let Some(event) = events_rx.recv().await {
            match event {
                ServerEvent::PrepareUpload {
                    files, decision, ..
                } => {
                    let _ = decision.send(UploadDecision::Accept(files.keys().cloned().collect()));
                }
                ServerEvent::FileUpload { file, target, .. } => {
                    let _ = target.send(UploadTarget::Path(dir.join(&file.file_name)));
                }
                ServerEvent::FileUploadResult {
                    outcome, received, ..
                } => {
                    let _ = results_tx.send((outcome, received)).await;
                }
                _ => {}
            }
        }
    });
    (handle, results_rx)
}

fn init_logging() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .try_init();
}

#[tokio::test]
async fn interrupted_upload_resumes_with_range() {
    init_logging();
    let dir = tempfile::tempdir().unwrap();
    let receiver_identity = Arc::new(Identity::generate().unwrap());
    let sender_identity = Identity::generate().unwrap();
    let (server, mut results) = start_receiver(receiver_identity.clone(), dir.path().into()).await;
    let target = Target {
        host: "127.0.0.1".into(),
        port: server.port(),
        protocol: ProtocolType::Https,
    };
    let client = Client::new(
        &sender_identity,
        Some(receiver_identity.fingerprint().clone()),
        None,
    )
    .unwrap();

    let content: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    let sha = hex(&Sha256::digest(&content));
    let file = FileDto {
        id: "f1".into(),
        file_name: "resumed.bin".into(),
        size: content.len() as u64,
        file_type: "application/octet-stream".into(),
        sha256: Some(sha),
        preview: None,
        metadata: None,
    };
    let request = PrepareUploadRequest {
        info: info(&sender_identity, 1, true),
        files: HashMap::from([("f1".to_string(), file.clone())]),
    };
    let response = match client
        .prepare_upload(&target, &request, None)
        .await
        .unwrap()
    {
        PrepareUploadOutcome::Accepted(response) => response,
        other => panic!("unexpected {other:?}"),
    };
    let resume_token = response
        .resume_token
        .clone()
        .expect("resume token for a resumable sender");
    let file_token = response.files["f1"].clone();

    // First attempt from a client that dies halfway: its body stream fails
    // and the client is dropped, which closes the connection like a crashed
    // sender would.
    // The first half goes out, then the stream stalls briefly (so hyper
    // flushes what it has instead of failing before sending anything) and
    // fails.
    let half = content.len() / 2;
    let broken = futures_util::stream::iter(vec![
        Ok::<Bytes, std::io::Error>(Bytes::copy_from_slice(&content[..half])),
        Err(std::io::Error::other("network gone")),
    ])
    .then(|item| async move {
        if item.is_err() {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        }
        item
    });
    let dying = Client::new(
        &sender_identity,
        Some(receiver_identity.fingerprint().clone()),
        None,
    )
    .unwrap();
    let first = dying
        .upload(
            &target,
            &response.session_id,
            "f1",
            &file_token,
            reqwest::Body::wrap_stream(broken),
        )
        .await;
    assert!(first.is_err(), "the broken upload must fail");
    drop(dying);
    let (outcome, received) = results.recv().await.unwrap();
    assert!(
        matches!(outcome, SaveOutcome::Interrupted { .. }),
        "got {outcome:?}"
    );
    assert!(
        received > 0 && received <= half as u64,
        "received {received}"
    );

    // The receiver tells us where to continue; a wrong offset is refused.
    let offset = client
        .resume_offset(&target, &response.session_id, "f1", &resume_token)
        .await
        .unwrap();
    assert_eq!(offset, received);
    let wrong = client
        .upload_with(
            &target,
            &response.session_id,
            "f1",
            &file_token,
            reqwest::Body::from(content[offset as usize + 1..].to_vec()),
            Some(Resume {
                offset: offset + 1,
                token: &resume_token,
            }),
        )
        .await;
    assert!(
        matches!(wrong, Err(ClientError::OffsetMismatch(expected)) if expected == offset),
        "got {wrong:?}"
    );
    let _ = results.recv().await.unwrap();

    // Resume from the right offset, streaming from disk like the CLI does.
    let source = dir.path().join("source.bin");
    std::fs::write(&source, &content).unwrap();
    let progress_seen = Arc::new(parking_lot::Mutex::new(Vec::new()));
    let seen = progress_seen.clone();
    client
        .upload_file_from(
            &target,
            &response.session_id,
            "f1",
            &file_token,
            &source,
            Some(Resume {
                offset,
                token: &resume_token,
            }),
            move |sent| seen.lock().push(sent),
        )
        .await
        .unwrap();
    let (outcome, received) = results.recv().await.unwrap();
    assert_eq!(outcome, SaveOutcome::Success);
    assert_eq!(received, content.len() as u64);
    assert_eq!(
        std::fs::read(dir.path().join("resumed.bin")).unwrap(),
        content
    );
    assert!(
        progress_seen
            .lock()
            .first()
            .is_some_and(|first| *first > offset)
    );

    server.stop().await;
}

#[tokio::test]
async fn plain_v2_sender_gets_no_resume_fields() {
    let dir = tempfile::tempdir().unwrap();
    let receiver_identity = Arc::new(Identity::generate().unwrap());
    let sender_identity = Identity::generate().unwrap();
    let (server, mut results) = start_receiver(receiver_identity.clone(), dir.path().into()).await;
    let target = Target {
        host: "127.0.0.1".into(),
        port: server.port(),
        protocol: ProtocolType::Https,
    };
    let client = Client::new(
        &sender_identity,
        Some(receiver_identity.fingerprint().clone()),
        None,
    )
    .unwrap();
    let content = b"plain v2".to_vec();
    let file = FileDto {
        id: "f".into(),
        file_name: "plain.txt".into(),
        size: content.len() as u64,
        file_type: "text/plain".into(),
        sha256: Some(hex(&Sha256::digest(&content))),
        preview: None,
        metadata: None,
    };
    let request = PrepareUploadRequest {
        info: info(&sender_identity, 1, false),
        files: HashMap::from([("f".to_string(), file)]),
    };
    let response = match client
        .prepare_upload(&target, &request, None)
        .await
        .unwrap()
    {
        PrepareUploadOutcome::Accepted(response) => response,
        other => panic!("unexpected {other:?}"),
    };
    assert!(response.resume_token.is_none());
    assert!(response.resume_offsets.is_none());

    // A Range upload without the extension is refused, a plain one works.
    let refused = client
        .upload_with(
            &target,
            &response.session_id,
            "f",
            &response.files["f"],
            reqwest::Body::from(content.clone()),
            Some(Resume {
                offset: 0,
                token: "bogus",
            }),
        )
        .await;
    assert_eq!(refused.err().and_then(|err| err.status()), Some(403));
    client
        .upload(
            &target,
            &response.session_id,
            "f",
            &response.files["f"],
            reqwest::Body::from(content.clone()),
        )
        .await
        .unwrap();
    let (outcome, _) = results.recv().await.unwrap();
    assert_eq!(outcome, SaveOutcome::Success);
    server.stop().await;
}
