//! The HTTP routes of the v2 upload API plus the resume extension.

use super::save::{self, SaveOptions, SaveOutcome, Timestamps};
use super::state::{AppState, FileStatus, PinCheck, Session, SessionFile, UploadRefusal};
use super::{Peer, ServerEvent, SessionEndReason, UploadDecision, UploadTarget};
use crate::protocol::{
    DeviceInfo, ErrorResponse, FEATURE_RESUME, PAIR_PATH, PairRequest, PairResponse, PeerInfo,
    PrepareUploadRequest, PrepareUploadResponse, RESUME_OFFSET_HEADER, RESUME_PATH,
    RESUME_TOKEN_HEADER, ResumeOffsetResponse, UNPAIR_PATH, verification_code,
};
use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Extension, Query, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::oneshot;
use uuid::Uuid;

/// Largest accepted JSON body (a prepare-upload with many thousand files).
const MAX_JSON_BODY: usize = 32 * 1024 * 1024;
/// Progress events are emitted at most every this many bytes.
const PROGRESS_STEP: u64 = 1024 * 1024;
/// How long a pairing request waits for the user before 408.
const PAIRING_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

type Params = Query<HashMap<String, String>>;

pub(super) fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/localsend/v2/register", post(register))
        .route("/api/localsend/v2/info", get(info))
        .route("/api/localsend/v1/info", get(info))
        .route("/api/localsend/v2/prepare-upload", post(prepare_upload))
        .route(
            "/api/localsend/v2/upload",
            post(upload).layer(DefaultBodyLimit::disable()),
        )
        .route("/api/localsend/v2/cancel", post(cancel))
        .route(RESUME_PATH, get(resume_offset))
        .route(PAIR_PATH, post(pair))
        .route(UNPAIR_PATH, post(unpair))
        .fallback(not_found)
        .layer(DefaultBodyLimit::max(MAX_JSON_BODY))
        .with_state(state)
}

/// An error response with the protocol's `{"message"}` body.
struct ApiError {
    status: StatusCode,
    message: String,
    headers: Vec<(&'static str, String)>,
}

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
            headers: Vec::new(),
        }
    }

    fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    fn internal() -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
    }

    fn invalid_token() -> Self {
        Self::new(StatusCode::FORBIDDEN, "Invalid token or IP address")
    }

    fn with_header(mut self, name: &'static str, value: String) -> Self {
        self.headers.push((name, value));
        self
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut response = (
            self.status,
            Json(ErrorResponse {
                message: self.message,
            }),
        )
            .into_response();
        for (name, value) in self.headers {
            if let Ok(value) = value.parse() {
                response.headers_mut().insert(name, value);
            }
        }
        response
    }
}

async fn not_found() -> StatusCode {
    StatusCode::NOT_FOUND
}

async fn info(State(state): State<Arc<AppState>>) -> Json<PeerInfo> {
    Json(state.peer_info())
}

async fn register(
    State(state): State<Arc<AppState>>,
    Extension(peer): Extension<Peer>,
    body: Bytes,
) -> Result<Json<PeerInfo>, ApiError> {
    let info: DeviceInfo = serde_json::from_slice(&body)
        .map_err(|err| ApiError::bad_request(format!("Invalid body: {err}")))?;

    // Over TLS only registrations whose claimed fingerprint is proven by the
    // client certificate are passed on, so the fingerprint cannot be spoofed.
    let trusted = match &peer.cert_fingerprint {
        Some(fingerprint) => fingerprint.matches(&info.fingerprint),
        None => true,
    };
    if trusted {
        // Not awaited: registrations arrive in bursts (every device answers
        // an announcement) and a dropped one is recoverable.
        if let Err(err) = state.events.try_send(ServerEvent::Register {
            peer: peer.clone(),
            info,
        }) {
            tracing::debug!("dropped a register event: {err}");
        }
    } else {
        tracing::warn!(
            "ignoring register from {}: claimed fingerprint does not match its certificate",
            peer.addr
        );
    }

    Ok(Json(state.peer_info()))
}

async fn prepare_upload(
    State(state): State<Arc<AppState>>,
    Extension(peer): Extension<Peer>,
    Query(query): Params,
    body: Bytes,
) -> Result<Response, ApiError> {
    match state.check_pin(query.get("pin").map(String::as_str), peer.addr) {
        PinCheck::Ok => {}
        PinCheck::Required => return Err(ApiError::new(StatusCode::UNAUTHORIZED, "PIN required")),
        PinCheck::Invalid => return Err(ApiError::new(StatusCode::UNAUTHORIZED, "Invalid PIN")),
        PinCheck::TooManyAttempts => {
            return Err(ApiError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "Too many requests",
            ));
        }
    }

    let request: PrepareUploadRequest = serde_json::from_slice(&body)
        .map_err(|err| ApiError::bad_request(format!("Invalid body: {err}")))?;
    if request.files.is_empty() {
        return Err(ApiError::bad_request("No files provided"));
    }
    let sender_resumes = request
        .info
        .ext
        .as_ref()
        .is_some_and(|ext| ext.supports(FEATURE_RESUME));

    let session_id = Uuid::new_v4().to_string();
    let Some(cancelled) = state.claim_pending(&session_id, peer.addr) else {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "Blocked by another session",
        ));
    };
    // Frees the slot again if this request ends before a session exists.
    let mut pending = PendingGuard::new(state.clone(), session_id.clone());

    let (decision_tx, decision_rx) = oneshot::channel();
    let event = ServerEvent::PrepareUpload {
        session_id: session_id.clone(),
        peer: peer.clone(),
        info: request.info,
        files: request.files.clone(),
        decision: decision_tx,
    };
    if state.events.send(event).await.is_err() {
        return Err(ApiError::internal());
    }

    // The sender may withdraw the request while the application decides.
    let decision = tokio::select! {
        decision = decision_rx => decision.map_err(|_| ApiError::internal())?,
        _ = cancelled.cancelled() => {
            return Err(ApiError::new(StatusCode::FORBIDDEN, "Cancelled by sender"));
        }
    };

    let (accepted, offsets) = match decision {
        UploadDecision::Decline => {
            pending.clear();
            return Err(ApiError::new(StatusCode::FORBIDDEN, "Rejected"));
        }
        UploadDecision::Accept(ids) => (ids, HashMap::new()),
        UploadDecision::AcceptWithResume { files, offsets } => (files, offsets),
    };

    let files: HashMap<String, SessionFile> = request
        .files
        .into_iter()
        .filter(|(id, _)| accepted.contains(id))
        .map(|(id, dto)| {
            let file = SessionFile {
                dto,
                token: Uuid::new_v4().to_string(),
                status: FileStatus::Pending,
                attempts: 0,
                path: None,
            };
            (id, file)
        })
        .collect();

    if files.is_empty() {
        pending.clear();
        return Ok(StatusCode::NO_CONTENT.into_response());
    }

    let tokens: HashMap<String, String> = files
        .iter()
        .map(|(id, file)| (id.clone(), file.token.clone()))
        .collect();
    let resume_token = sender_resumes.then(random_token);
    let resume_offsets: HashMap<String, u64> = offsets
        .into_iter()
        .filter(|(id, offset)| *offset > 0 && files.contains_key(id))
        .collect();
    state.activate(Session {
        session_id: session_id.clone(),
        peer: peer.addr,
        files,
        resume_token: resume_token.clone(),
        last_activity: Instant::now(),
    });
    pending.disarm();
    tracing::info!(
        "upload session {session_id} started with {} file(s){}",
        tokens.len(),
        if sender_resumes { ", resumable" } else { "" }
    );

    Ok(Json(PrepareUploadResponse {
        session_id,
        files: tokens,
        resume_token,
        resume_offsets: (sender_resumes && !resume_offsets.is_empty()).then_some(resume_offsets),
    })
    .into_response())
}

async fn upload(
    State(state): State<Arc<AppState>>,
    Extension(peer): Extension<Peer>,
    Query(query): Params,
    headers: HeaderMap,
    request: Request,
) -> Result<Response, ApiError> {
    let (Some(session_id), Some(file_id), Some(token)) = (
        query.get("sessionId").cloned(),
        query.get("fileId").cloned(),
        query.get("token").cloned(),
    ) else {
        return Err(ApiError::bad_request("Missing parameters"));
    };
    let resume_token = header_string(&headers, RESUME_TOKEN_HEADER);
    let resume_from = parse_range(&headers)?;
    if resume_from.is_some() && resume_token.is_none() {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "Resume token required",
        ));
    }

    let start = state
        .begin_upload(
            &session_id,
            &file_id,
            &token,
            peer.addr,
            resume_token.as_deref(),
        )
        .map_err(|refusal| match refusal {
            UploadRefusal::Invalid => ApiError::invalid_token(),
            UploadRefusal::InProgress => {
                ApiError::new(StatusCode::CONFLICT, "File upload in progress")
            }
        })?;
    let file = start.file;
    // Marks the file as interrupted if this request ends mid-transfer.
    let mut guard = UploadGuard::new(state.clone(), session_id.clone(), file_id.clone());

    // The path of an earlier attempt is reused so its part file is found;
    // otherwise the application decides.
    let path = match start.path {
        Some(path) => path,
        None => {
            let (target_tx, target_rx) = oneshot::channel();
            let event = ServerEvent::FileUpload {
                session_id: session_id.clone(),
                file_id: file_id.clone(),
                file: file.clone(),
                target: target_tx,
            };
            if state.events.send(event).await.is_err() {
                guard.finish(&SaveOutcome::Failed("no application listening".into()));
                return Err(ApiError::internal());
            }
            match target_rx.await {
                Ok(UploadTarget::Path(path)) => path,
                Ok(UploadTarget::Reject(reason)) => {
                    tracing::warn!("file {file_id} rejected by the application: {reason}");
                    guard.finish(&SaveOutcome::Failed(reason));
                    return Err(ApiError::internal());
                }
                Err(_) => {
                    guard.finish(&SaveOutcome::Failed("no save target".into()));
                    return Err(ApiError::internal());
                }
            }
        }
    };
    state.set_file_path(&session_id, &file_id, path.clone());
    guard.path = Some(path.clone());

    let expected_sha256 = match state.verify_checksums {
        true => file.sha256.clone(),
        false => None,
    };
    let timestamps = file
        .metadata
        .as_ref()
        .map(|metadata| Timestamps {
            modified: metadata.modified_time(),
            accessed: metadata.accessed_time(),
        })
        .unwrap_or_default();

    let events = state.events.clone();
    let mut last_reported = resume_from.unwrap_or(0);
    let (progress_session, progress_file) = (session_id.clone(), file_id.clone());
    let outcome = save::save_body(
        request.into_body(),
        &path,
        SaveOptions {
            expected_size: file.size,
            expected_sha256: expected_sha256.as_deref(),
            timestamps,
            resume_from,
            keep_partial: start.resumable,
            idle_timeout: state.upload_idle_timeout,
        },
        |received| {
            if received == file.size || received - last_reported >= PROGRESS_STEP {
                last_reported = received;
                let _ = events.try_send(ServerEvent::FileUploadProgress {
                    session_id: progress_session.clone(),
                    file_id: progress_file.clone(),
                    received,
                });
            }
        },
    )
    .await;

    let received = match &outcome {
        SaveOutcome::Success => file.size,
        other => other.received().unwrap_or(0),
    };
    let _ = state
        .events
        .send(ServerEvent::FileUploadResult {
            session_id: session_id.clone(),
            file_id: file_id.clone(),
            path,
            outcome: outcome.clone(),
            received,
        })
        .await;
    guard.finish(&outcome);

    match outcome {
        SaveOutcome::Success => Ok(StatusCode::OK.into_response()),
        SaveOutcome::HashMismatch => Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Checksum mismatch",
        )),
        SaveOutcome::OffsetMismatch { expected } => Err(ApiError::new(
            StatusCode::RANGE_NOT_SATISFIABLE,
            format!("Expected upload offset {expected}"),
        )
        .with_header(RESUME_OFFSET_HEADER, expected.to_string())),
        SaveOutcome::Interrupted { reason, .. } | SaveOutcome::Failed(reason) => {
            Err(ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, reason))
        }
    }
}

/// Resume extension: how many bytes of a pending file the receiver holds.
async fn resume_offset(
    State(state): State<Arc<AppState>>,
    Extension(peer): Extension<Peer>,
    Query(query): Params,
    headers: HeaderMap,
) -> Result<Json<ResumeOffsetResponse>, ApiError> {
    let (Some(session_id), Some(file_id)) = (query.get("sessionId"), query.get("fileId")) else {
        return Err(ApiError::bad_request("Missing parameters"));
    };
    let token = header_string(&headers, RESUME_TOKEN_HEADER)
        .ok_or_else(|| ApiError::new(StatusCode::FORBIDDEN, "Resume token required"))?;
    let path = state
        .resume_lookup(session_id, file_id, peer.addr, &token)
        .map_err(|refusal| match refusal {
            UploadRefusal::Invalid => ApiError::invalid_token(),
            UploadRefusal::InProgress => {
                ApiError::new(StatusCode::CONFLICT, "File upload in progress")
            }
        })?;
    let offset = match path {
        Some(path) => save::partial_length(&path).await,
        None => 0,
    };
    Ok(Json(ResumeOffsetResponse { offset }))
}

async fn cancel(
    State(state): State<Arc<AppState>>,
    Extension(peer): Extension<Peer>,
    Query(query): Params,
) -> Result<Response, ApiError> {
    let session_id = query.get("sessionId").map(String::as_str);

    // A pending prepare-upload: the sender does not know the session id yet.
    if state.cancel_pending(peer.addr, session_id) {
        tracing::info!("pending upload request cancelled by {}", peer.addr);
        return Ok(StatusCode::OK.into_response());
    }

    if let Some(session_id) = session_id {
        if state.cancel_active(session_id, Some(peer.addr)) {
            tracing::info!("upload session {session_id} cancelled by sender");
            state.emit_session_end(session_id.to_string(), SessionEndReason::Cancelled);
        } else {
            // Not ours: the peer may be cancelling a transfer we are sending.
            let _ = state
                .events
                .send(ServerEvent::CancelReceived {
                    peer: peer.clone(),
                    session_id: session_id.to_string(),
                })
                .await;
        }
    }

    Ok(StatusCode::OK.into_response())
}

/// Pairing (ADR-0010): the user confirms the verification code, then the
/// caller's certificate is trusted for the private endpoints.
async fn pair(
    State(state): State<Arc<AppState>>,
    Extension(peer): Extension<Peer>,
    body: Bytes,
) -> Result<Json<PairResponse>, ApiError> {
    let Some(fingerprint) = peer.cert_fingerprint.clone() else {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "Client certificate required",
        ));
    };
    let request: PairRequest = serde_json::from_slice(&body)
        .map_err(|err| ApiError::bad_request(format!("Invalid body: {err}")))?;
    if !state.begin_pairing() {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "Another pairing request is pending",
        ));
    }
    let _guard = PairingGuard(state.clone());

    let code = verification_code(&state.own_fingerprint(), &fingerprint);
    let (decision_tx, decision_rx) = oneshot::channel();
    let event = ServerEvent::PairRequest {
        peer: peer.clone(),
        alias: request.alias.clone(),
        code,
        decision: decision_tx,
    };
    if state.events.send(event).await.is_err() {
        return Err(ApiError::internal());
    }
    let accepted = match tokio::time::timeout(PAIRING_TIMEOUT, decision_rx).await {
        Ok(Ok(accepted)) => accepted,
        Ok(Err(_)) => return Err(ApiError::internal()),
        Err(_) => {
            return Err(ApiError::new(
                StatusCode::REQUEST_TIMEOUT,
                "Pairing timed out",
            ));
        }
    };
    if !accepted {
        return Err(ApiError::new(StatusCode::FORBIDDEN, "Rejected"));
    }
    state.paired.write().insert(fingerprint);
    tracing::info!("paired with {} ({})", request.alias, peer.addr);
    Ok(Json(PairResponse {
        accepted: true,
        alias: state.device.read().alias.clone(),
    }))
}

/// A paired device withdraws the pairing.
async fn unpair(
    State(state): State<Arc<AppState>>,
    Extension(peer): Extension<Peer>,
) -> Result<Response, ApiError> {
    if let Some(fingerprint) = &peer.cert_fingerprint
        && state.paired.write().remove(fingerprint)
    {
        tracing::info!("unpaired by {}", peer.addr);
        let _ = state
            .events
            .send(ServerEvent::Unpaired { peer: peer.clone() })
            .await;
    }
    Ok(StatusCode::OK.into_response())
}

/// Frees the pairing slot when the request ends, however it ends.
struct PairingGuard(Arc<AppState>);

impl Drop for PairingGuard {
    fn drop(&mut self) {
        self.0.end_pairing();
    }
}

fn header_string(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// `Range: bytes=<offset>-`, the only form the resume extension uses.
fn parse_range(headers: &HeaderMap) -> Result<Option<u64>, ApiError> {
    let Some(range) = header_string(headers, "range") else {
        return Ok(None);
    };
    let offset = range
        .strip_prefix("bytes=")
        .and_then(|rest| rest.strip_suffix('-'))
        .and_then(|start| start.trim().parse::<u64>().ok())
        .ok_or_else(|| ApiError::bad_request("Unsupported Range header"))?;
    Ok(Some(offset))
}

fn random_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

/// Frees a claimed pending slot unless a session was created, also when the
/// handler future is dropped because the sender disconnected.
struct PendingGuard {
    state: Arc<AppState>,
    session_id: String,
    armed: bool,
}

impl PendingGuard {
    fn new(state: Arc<AppState>, session_id: String) -> Self {
        Self {
            state,
            session_id,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }

    fn clear(&mut self) {
        self.armed = false;
        self.state.clear_pending(&self.session_id);
    }
}

impl Drop for PendingGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let session_id = std::mem::take(&mut self.session_id);
        self.state.clear_pending(&session_id);
        let events = self.state.events.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let _ = events
                    .send(ServerEvent::PrepareUploadAborted { session_id })
                    .await;
            });
        }
    }
}

/// Records the outcome of an upload, as an interruption when the handler
/// future is dropped mid-transfer (the sender's connection went away).
struct UploadGuard {
    state: Arc<AppState>,
    session_id: String,
    file_id: String,
    path: Option<PathBuf>,
    armed: bool,
}

impl UploadGuard {
    fn new(state: Arc<AppState>, session_id: String, file_id: String) -> Self {
        Self {
            state,
            session_id,
            file_id,
            path: None,
            armed: true,
        }
    }

    fn finish(&mut self, outcome: &SaveOutcome) {
        self.armed = false;
        if self
            .state
            .finalize_file(&self.session_id, &self.file_id, outcome)
        {
            self.state
                .emit_session_end(self.session_id.clone(), SessionEndReason::Finished);
        }
    }
}

impl Drop for UploadGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let outcome = SaveOutcome::Interrupted {
            received: 0,
            reason: "connection dropped".into(),
        };
        self.finish(&outcome);
        // Tell the application how much is on disk, for its partial records.
        let Some(path) = self.path.take() else {
            return;
        };
        let events = self.state.events.clone();
        let (session_id, file_id) = (self.session_id.clone(), self.file_id.clone());
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let received = save::partial_length(&path).await;
                let _ = events
                    .send(ServerEvent::FileUploadResult {
                        session_id,
                        file_id,
                        path,
                        outcome: SaveOutcome::Interrupted {
                            received,
                            reason: "connection dropped".into(),
                        },
                        received,
                    })
                    .await;
            });
        }
    }
}
