//! End-to-end tests of the streaming-checksum extension (ADR-0017): the
//! sender leaves `sha256` empty in `prepare-upload`, hashes while uploading
//! and confirms afterwards. Covers the happy path, a sender whose digest
//! disagrees, a resumed upload (whose digest must still cover the whole file)
//! and the fallback for peers that do not announce the extension.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use lan_send_core::protocol::{
    DeviceInfo, DeviceType, Extensions, FEATURE_RESUME, FEATURE_STREAM_CHECKSUM, FileDto,
    PROTOCOL_VERSION, PrepareUploadRequest, ProtocolType,
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

fn info(identity: &Identity, features: &[&'static str]) -> DeviceInfo {
    DeviceInfo {
        alias: "test".into(),
        version: PROTOCOL_VERSION.into(),
        device_model: None,
        device_type: Some(DeviceType::Headless),
        fingerprint: identity.fingerprint().to_string(),
        port: 0,
        protocol: ProtocolType::Https,
        download: false,
        ext: Some(Extensions::current(features.iter().copied())),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

async fn start_receiver(
    identity: Arc<Identity>,
    dir: PathBuf,
    verify_checksums: bool,
) -> (server::ServerHandle, mpsc::Receiver<(SaveOutcome, u64)>) {
    let (events_tx, mut events_rx) = mpsc::channel(64);
    let handle = server::start(ServerConfig {
        port: 0,
        identity: identity.clone(),
        client_cert_policy: ClientCertPolicy::Required,
        device: info(&identity, &[FEATURE_RESUME, FEATURE_STREAM_CHECKSUM]),
        pin: None,
        verify_checksums,
        upload_idle_timeout: std::time::Duration::from_secs(2),
        ipv6: false,
        paired: HashSet::new(),
        clipboard_limits: Default::default(),
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

struct Fixture {
    _server: server::ServerHandle,
    client: Client,
    target: Target,
    results: mpsc::Receiver<(SaveOutcome, u64)>,
}

async fn fixture(dir: &std::path::Path) -> Fixture {
    fixture_with(dir, true).await
}

async fn fixture_with(dir: &std::path::Path, verify_checksums: bool) -> Fixture {
    let receiver_identity = Arc::new(Identity::generate().unwrap());
    let sender_identity = Identity::generate().unwrap();
    let (server, results) =
        start_receiver(receiver_identity.clone(), dir.into(), verify_checksums).await;
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
    Fixture {
        _server: server,
        client,
        target,
        results,
    }
}

/// `prepare-upload` announcing the extension and offering one file without a
/// digest. Returns the session id, the file's upload token and the session's
/// resume token when the receiver issued one.
async fn prepare(
    fixture: &Fixture,
    file: FileDto,
    features: &[&'static str],
) -> (String, String, Option<String>) {
    let sender = Identity::generate().unwrap();
    let request = PrepareUploadRequest {
        info: info(&sender, features),
        files: HashMap::from([(file.id.clone(), file.clone())]),
    };
    let response = match fixture
        .client
        .prepare_upload(&fixture.target, &request, None)
        .await
        .expect("prepare-upload")
    {
        PrepareUploadOutcome::Accepted(response) => response,
        PrepareUploadOutcome::NothingToTransfer => panic!("nothing accepted"),
    };
    let token = response.files.get(&file.id).expect("token").clone();
    (response.session_id, token, response.resume_token.clone())
}

fn dto(name: &str, size: u64, sha256: Option<String>) -> FileDto {
    FileDto {
        id: "f1".into(),
        file_name: name.into(),
        size,
        file_type: "application/octet-stream".into(),
        sha256,
        content_id: Some("q:test-content-id".into()),
        preview: None,
        metadata: None,
    }
}

#[tokio::test]
async fn deferred_checksum_lands_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = fixture(dir.path()).await;
    let content: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    let source = dir.path().join("source.bin");
    std::fs::write(&source, &content).unwrap();

    let file = dto("streamed.bin", content.len() as u64, None);
    let (session_id, token, _) =
        prepare(&fixture, file, &[FEATURE_RESUME, FEATURE_STREAM_CHECKSUM]).await;

    fixture
        .client
        .upload_file_from(
            &fixture.target,
            &session_id,
            "f1",
            &token,
            &source,
            None,
            true,
            |_| {},
        )
        .await
        .expect("upload with a streamed checksum");

    let mut results = fixture.results;
    let (outcome, _) = results.recv().await.expect("a result");
    assert_eq!(outcome, SaveOutcome::Success);
    let landed = dir.path().join("streamed.bin");
    assert_eq!(std::fs::read(&landed).unwrap(), content);
    // Nothing is left behind under the part name.
    assert!(!dir.path().join("streamed.bin.lan-send.part").exists());
}

#[tokio::test]
async fn a_wrong_digest_is_refused_and_the_file_is_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = fixture(dir.path()).await;
    let content = b"the bytes that arrive".to_vec();

    let file = dto("bad.bin", content.len() as u64, None);
    let (session_id, token, _) = prepare(&fixture, file, &[FEATURE_STREAM_CHECKSUM]).await;

    // Upload the body, then confirm a digest that does not describe it.
    fixture
        .client
        .upload(
            &fixture.target,
            &session_id,
            "f1",
            &token,
            content.clone().into(),
        )
        .await
        .expect("body accepted");
    let err = fixture
        .client
        .confirm_checksum(
            &fixture.target,
            &session_id,
            "f1",
            &token,
            None,
            &hex(&Sha256::digest(b"something else")),
        )
        .await
        .expect_err("a disagreeing digest is refused");
    assert!(
        matches!(err, ClientError::Status { status: 422, .. }),
        "expected 422, got {err:?}"
    );

    let mut results = fixture.results;
    let (outcome, _) = results.recv().await.expect("a result");
    assert_eq!(outcome, SaveOutcome::HashMismatch);
    assert!(!dir.path().join("bad.bin").exists());
    assert!(!dir.path().join("bad.bin.lan-send.part").exists());
}

#[tokio::test]
async fn a_resumed_upload_hashes_the_part_already_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = fixture(dir.path()).await;
    let content: Vec<u8> = (0..200_000u32).map(|i| (i % 241) as u8).collect();
    let source = dir.path().join("source.bin");
    std::fs::write(&source, &content).unwrap();
    let offset = 80_000u64;

    let file = dto("resumed.bin", content.len() as u64, None);
    let (session_id, token, resume_token) =
        prepare(&fixture, file, &[FEATURE_RESUME, FEATURE_STREAM_CHECKSUM]).await;
    let resume_token = resume_token.expect("the receiver issues a resume token");

    // A first attempt that stops short leaves a prefix in the part file.
    let _ = fixture
        .client
        .upload(
            &fixture.target,
            &session_id,
            "f1",
            &token,
            content[..offset as usize].to_vec().into(),
        )
        .await;
    let mut results = fixture.results;
    let (first, received) = results.recv().await.expect("the short attempt reports");
    assert!(
        matches!(first, SaveOutcome::Interrupted { .. }),
        "expected an interruption, got {first:?}"
    );
    assert_eq!(received, offset);

    // Resuming streams only the tail, but the digest must cover the whole file:
    // a receiver that hashed prefix plus tail would otherwise see a mismatch.
    fixture
        .client
        .upload_file_from(
            &fixture.target,
            &session_id,
            "f1",
            &token,
            &source,
            Some(Resume {
                offset,
                token: &resume_token,
            }),
            true,
            |_| {},
        )
        .await
        .expect("the resumed upload is accepted");

    let (outcome, _) = results.recv().await.expect("a second result");
    assert_eq!(outcome, SaveOutcome::Success);
    assert_eq!(
        std::fs::read(dir.path().join("resumed.bin")).unwrap(),
        content
    );
}

#[tokio::test]
async fn a_peer_without_the_extension_keeps_the_inline_checksum() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = fixture(dir.path()).await;
    let content = b"plain upload".to_vec();
    let source = dir.path().join("plain-source.bin");
    std::fs::write(&source, &content).unwrap();

    // A sender that hashed up front and never announced the extension.
    let file = dto(
        "plain.bin",
        content.len() as u64,
        Some(hex(&Sha256::digest(&content))),
    );
    let (session_id, token, _) = prepare(&fixture, file, &[FEATURE_RESUME]).await;

    fixture
        .client
        .upload_file(&fixture.target, &session_id, "f1", &token, &source, |_| {})
        .await
        .expect("plain upload");

    let mut results = fixture.results;
    let (outcome, _) = results.recv().await.expect("a result");
    assert_eq!(outcome, SaveOutcome::Success);
    assert_eq!(
        std::fs::read(dir.path().join("plain.bin")).unwrap(),
        content
    );
}

#[tokio::test]
async fn a_receiver_that_verifies_nothing_still_accepts_the_confirmation() {
    // The runtime only announces the extension while it verifies, but a peer
    // may have learned the feature before the setting changed. Confirming must
    // then be a no-op rather than an error that costs the sender a retry.
    let dir = tempfile::tempdir().unwrap();
    let fixture = fixture_with(dir.path(), false).await;
    let content: Vec<u8> = (0..120_000u32).map(|i| (i % 233) as u8).collect();
    let source = dir.path().join("source.bin");
    std::fs::write(&source, &content).unwrap();

    let file = dto("unverified.bin", content.len() as u64, None);
    let (session_id, token, _) = prepare(&fixture, file, &[FEATURE_STREAM_CHECKSUM]).await;

    fixture
        .client
        .upload_file_from(
            &fixture.target,
            &session_id,
            "f1",
            &token,
            &source,
            None,
            true,
            |_| {},
        )
        .await
        .expect("the upload and its confirmation both succeed");

    let mut results = fixture.results;
    let (outcome, _) = results.recv().await.expect("a result");
    assert_eq!(outcome, SaveOutcome::Success);
    assert_eq!(
        std::fs::read(dir.path().join("unverified.bin")).unwrap(),
        content
    );
}
