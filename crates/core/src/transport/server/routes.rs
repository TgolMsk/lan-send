//! The HTTP routes of the v2 upload API.

use super::save::{self, SaveOutcome, Timestamps};
use super::state::{AppState, FileStatus, PinCheck, Session, SessionFile};
use super::{Peer, ServerEvent, SessionEndReason, UploadDecision, UploadTarget};
use crate::protocol::{
    DeviceInfo, ErrorResponse, PeerInfo, PrepareUploadRequest, PrepareUploadResponse,
};
use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Extension, Query, Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::oneshot;
use uuid::Uuid;

/// Largest accepted JSON body (a prepare-upload with many thousand files).
const MAX_JSON_BODY: usize = 32 * 1024 * 1024;
/// Progress events are emitted at most every this many bytes.
const PROGRESS_STEP: u64 = 1024 * 1024;

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
        .fallback(not_found)
        .layer(DefaultBodyLimit::max(MAX_JSON_BODY))
        .with_state(state)
}

/// An error response with the protocol's `{"message"}` body.
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    fn internal() -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorResponse {
                message: self.message,
            }),
        )
            .into_response()
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

    let accepted = match decision {
        UploadDecision::Decline => {
            pending.clear();
            return Err(ApiError::new(StatusCode::FORBIDDEN, "Rejected"));
        }
        UploadDecision::Accept(ids) => ids,
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
    state.activate(Session {
        session_id: session_id.clone(),
        peer: peer.addr,
        files,
    });
    pending.disarm();
    tracing::info!(
        "upload session {session_id} started with {} file(s)",
        tokens.len()
    );

    Ok(Json(PrepareUploadResponse {
        session_id,
        files: tokens,
    })
    .into_response())
}

async fn upload(
    State(state): State<Arc<AppState>>,
    Extension(peer): Extension<Peer>,
    Query(query): Params,
    request: Request,
) -> Result<Response, ApiError> {
    let (Some(session_id), Some(file_id), Some(token)) = (
        query.get("sessionId").cloned(),
        query.get("fileId").cloned(),
        query.get("token").cloned(),
    ) else {
        return Err(ApiError::bad_request("Missing parameters"));
    };

    let file = state
        .begin_upload(&session_id, &file_id, &token, peer.addr)
        .ok_or_else(|| ApiError::new(StatusCode::FORBIDDEN, "Invalid token or IP address"))?;
    // Marks the file as failed if this request ends mid-transfer.
    let mut guard = UploadGuard::new(state.clone(), session_id.clone(), file_id.clone());

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
    let path = match target_rx.await {
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
    };

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
    let mut last_reported = 0u64;
    let (progress_session, progress_file) = (session_id.clone(), file_id.clone());
    let outcome = save::save_body(
        request.into_body(),
        &path,
        file.size,
        expected_sha256.as_deref(),
        timestamps,
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

    let _ = state
        .events
        .send(ServerEvent::FileUploadResult {
            session_id: session_id.clone(),
            file_id: file_id.clone(),
            path,
            outcome: outcome.clone(),
        })
        .await;
    guard.finish(&outcome);

    match outcome {
        SaveOutcome::Success => Ok(StatusCode::OK.into_response()),
        SaveOutcome::HashMismatch => Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Checksum mismatch",
        )),
        SaveOutcome::Failed(message) => {
            Err(ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, message))
        }
    }
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

/// Records the outcome of an upload, as a failure when the handler future is
/// dropped mid-transfer.
struct UploadGuard {
    state: Arc<AppState>,
    session_id: String,
    file_id: String,
    armed: bool,
}

impl UploadGuard {
    fn new(state: Arc<AppState>, session_id: String, file_id: String) -> Self {
        Self {
            state,
            session_id,
            file_id,
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
        if self.armed {
            self.finish(&SaveOutcome::Failed("upload aborted".into()));
        }
    }
}
