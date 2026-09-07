//! HTTPS server implementing the LocalSend v2 upload API (ADR-0004).
//!
//! The server owns the protocol state (one upload session at a time, PIN
//! attempts, tokens) and streams file content to disk. Everything that needs
//! a decision — accept a request, where to save a file — is asked from the
//! application through [`ServerEvent`]s carrying one-shot responders.

mod routes;
mod save;
mod state;

pub use save::{SaveOutcome, part_path, partial_length};

use crate::protocol::{DeviceInfo, FileDto, Fingerprint};
use crate::transport::identity::Identity;
use crate::transport::tls::{self, ClientCertPolicy, TlsError};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use hyper_util::service::TowerToHyperService;
use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot};
use tokio_rustls::TlsAcceptor;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

/// How the server is started.
pub struct ServerConfig {
    /// Port to bind on all interfaces; `0` lets the OS pick one.
    pub port: u16,
    /// Also listen on IPv6 (`[::]`, v6-only). Failure to bind it is logged
    /// and does not prevent the IPv4 listener.
    pub ipv6: bool,
    pub identity: Arc<Identity>,
    pub client_cert_policy: ClientCertPolicy,
    /// This device as reported to peers. `port` is replaced by the bound port.
    pub device: DeviceInfo,
    /// PIN senders must supply with `prepare-upload`.
    pub pin: Option<String>,
    /// Verify sender-provided SHA-256 checksums after receiving.
    pub verify_checksums: bool,
    /// An upload whose body sends nothing for this long is treated as
    /// interrupted (a sender that vanished without closing the connection).
    /// [`DEFAULT_UPLOAD_IDLE_TIMEOUT`] is a sensible value.
    pub upload_idle_timeout: Duration,
    /// Where events for the application go.
    pub events: mpsc::Sender<ServerEvent>,
}

/// Thirty seconds without data on an upload counts as an interruption.
pub const DEFAULT_UPLOAD_IDLE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error(transparent)]
    Tls(#[from] TlsError),

    #[error("could not bind port {port}: {source}")]
    Bind {
        port: u16,
        #[source]
        source: std::io::Error,
    },

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// The remote end of a connection.
#[derive(Clone, Debug)]
pub struct Peer {
    pub addr: IpAddr,
    /// The IPv6 scope (interface index) the connection arrived with; set for
    /// link-local peers, which cannot be dialled back without it.
    pub scope_id: Option<u32>,
    /// Fingerprint of the client certificate verified during the handshake.
    /// `None` when the peer presented none (only possible with
    /// [`ClientCertPolicy::Optional`]).
    pub cert_fingerprint: Option<Fingerprint>,
}

impl Peer {
    fn from_remote(addr: SocketAddr, cert_fingerprint: Option<Fingerprint>) -> Self {
        let scope_id = match addr {
            SocketAddr::V6(v6) if v6.scope_id() != 0 => Some(v6.scope_id()),
            _ => None,
        };
        Self {
            addr: addr.ip(),
            scope_id,
            cert_fingerprint,
        }
    }

    /// The address to dial the peer back at: `ip`, or `ip%scope` for
    /// link-local IPv6 peers.
    pub fn host(&self) -> String {
        match self.scope_id {
            Some(scope) => format!("{}%{scope}", self.addr),
            None => self.addr.to_string(),
        }
    }

    /// The peer's identity: the certificate fingerprint when there is one,
    /// otherwise the fingerprint it claimed in the request body.
    pub fn identity(&self, claimed: &str) -> Fingerprint {
        self.cert_fingerprint
            .clone()
            .unwrap_or_else(|| Fingerprint::parse(claimed))
    }
}

/// Events the application must handle.
#[derive(Debug)]
pub enum ServerEvent {
    /// A peer registered via `POST /register`. Over TLS this is only emitted
    /// when the claimed fingerprint matches the client certificate.
    Register { peer: Peer, info: DeviceInfo },

    /// A sender wants to upload files. Answer on `decision`; dropping it
    /// fails the request with 500.
    PrepareUpload {
        session_id: String,
        peer: Peer,
        info: DeviceInfo,
        files: HashMap<String, FileDto>,
        decision: oneshot::Sender<UploadDecision>,
    },

    /// The sender withdrew a request before it was answered; the matching
    /// `decision` responder is dead.
    PrepareUploadAborted { session_id: String },

    /// An accepted file is being uploaded. Answer on `target` with where to
    /// write it; dropping it fails the upload with 500.
    FileUpload {
        session_id: String,
        file_id: String,
        file: FileDto,
        target: oneshot::Sender<UploadTarget>,
    },

    /// Bytes written so far for a file (throttled).
    FileUploadProgress {
        session_id: String,
        file_id: String,
        received: u64,
    },

    /// A file upload finished, successfully or not. `received` is what the
    /// (part) file holds afterwards; with the resume extension an
    /// interrupted upload keeps its part file and stays pending.
    FileUploadResult {
        session_id: String,
        file_id: String,
        path: PathBuf,
        outcome: SaveOutcome,
        received: u64,
    },

    /// All accepted files reached a final state, or the sender cancelled.
    SessionEnd {
        session_id: String,
        reason: SessionEndReason,
    },

    /// `POST /cancel` for a session this server does not own: the peer is
    /// cancelling a transfer this application is *sending* to it. Verify
    /// that `peer` is the target of that send session before acting.
    CancelReceived { peer: Peer, session_id: String },
}

/// The application's answer to a `prepare-upload` request.
#[derive(Debug)]
pub enum UploadDecision {
    /// Accept these file ids (any subset; empty answers 204).
    Accept(HashSet<String>),
    /// Accept these file ids and, for files whose part file the application
    /// still holds, tell the sender how many bytes to skip (resume
    /// extension; only used when the sender announced `resume`).
    AcceptWithResume {
        files: HashSet<String>,
        offsets: HashMap<String, u64>,
    },
    /// Reject the whole request (403).
    Decline,
}

/// Where an accepted file's content goes.
#[derive(Debug)]
pub enum UploadTarget {
    /// Write to this path. The server writes to a `.lan-send.part` file
    /// next to it first and renames on success.
    Path(PathBuf),
    /// Refuse this file (500 to the sender), e.g. because no safe path exists.
    Reject(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionEndReason {
    Finished,
    Cancelled,
    /// No request for ten minutes; the slot was freed.
    TimedOut,
}

/// A running server.
pub struct ServerHandle {
    port: u16,
    state: Arc<state::AppState>,
    cancel: CancellationToken,
    connections: TaskTracker,
    task: parking_lot::Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl ServerHandle {
    /// The bound port.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Updates the device information reported to peers (e.g. a new alias).
    pub fn set_device(&self, mut device: DeviceInfo) {
        device.port = self.port;
        *self.state.device.write() = device;
    }

    /// Cancels the active upload session from the receiving side. Uploads
    /// in progress still finish; new ones are refused. No `SessionEnd` is
    /// emitted since the application initiated it. Returns whether a
    /// session was cancelled.
    pub fn cancel_session(&self, session_id: &str) -> bool {
        self.state.cancel_active(session_id, None)
    }

    /// Stops accepting, drops every connection and waits for the tasks.
    pub async fn stop(&self) {
        self.cancel.cancel();
        self.connections.close();
        self.connections.wait().await;
        let task = self.task.lock().take();
        if let Some(task) = task {
            let _ = task.await;
        }
    }
}

/// Binds the port and starts serving.
pub async fn start(config: ServerConfig) -> Result<ServerHandle, ServerError> {
    let tls_config = tls::server_config(&config.identity, config.client_cert_policy)?;
    let addr = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), config.port);
    let listener = TcpListener::bind(addr)
        .await
        .map_err(|source| ServerError::Bind {
            port: config.port,
            source,
        })?;
    let port = listener.local_addr()?.port();
    let ipv6_listener = match config.ipv6 {
        true => match bind_ipv6_only(SocketAddr::new(Ipv6Addr::UNSPECIFIED.into(), port)) {
            Ok(listener) => Some(listener),
            Err(err) => {
                tracing::warn!("could not listen on [::]:{port}: {err}");
                None
            }
        },
        false => None,
    };

    let mut device = config.device;
    device.port = port;
    let state = Arc::new(state::AppState::new(
        device,
        config.pin,
        config.verify_checksums,
        config.upload_idle_timeout,
        config.events,
    ));
    let router = routes::router(state.clone());

    let cancel = CancellationToken::new();
    let connections = TaskTracker::new();
    let acceptor = TlsAcceptor::from(tls_config);

    tracing::info!(
        "server listening on {addr}{} (port {port}, TLS)",
        if ipv6_listener.is_some() {
            " and [::]"
        } else {
            ""
        }
    );
    if let Some(ipv6_listener) = ipv6_listener {
        connections.spawn(accept_loop(
            ipv6_listener,
            acceptor.clone(),
            router.clone(),
            cancel.clone(),
            connections.clone(),
        ));
    }
    let task = tokio::spawn(accept_loop(
        listener,
        acceptor,
        router,
        cancel.clone(),
        connections.clone(),
    ));
    connections.spawn(reap_idle_sessions(state.clone(), cancel.clone()));

    Ok(ServerHandle {
        port,
        state,
        cancel,
        connections,
        task: parking_lot::Mutex::new(Some(task)),
    })
}

/// Binds an IPv6 listener with `IPV6_V6ONLY`: without it some systems
/// (macOS) bind IPv6 wildcard sockets dual-stack, which conflicts with the
/// separate IPv4 listener on the same port.
fn bind_ipv6_only(addr: SocketAddr) -> std::io::Result<TcpListener> {
    let socket = socket2::Socket::new(
        socket2::Domain::IPV6,
        socket2::Type::STREAM,
        Some(socket2::Protocol::TCP),
    )?;
    socket.set_only_v6(true)?;
    socket.set_reuse_address(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&addr.into())?;
    socket.listen(1024)?;
    TcpListener::from_std(socket.into())
}

/// Frees the session slot when a sender disappears without cancelling.
async fn reap_idle_sessions(state: Arc<state::AppState>, cancel: CancellationToken) {
    let mut ticker = tokio::time::interval(Duration::from_secs(60));
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return,
            _ = ticker.tick() => {}
        }
        if let Some(session_id) = state.reap_idle() {
            tracing::info!("upload session {session_id} timed out");
            state.emit_session_end(session_id, SessionEndReason::TimedOut);
        }
    }
}

async fn accept_loop(
    listener: TcpListener,
    acceptor: TlsAcceptor,
    router: axum::Router,
    cancel: CancellationToken,
    connections: TaskTracker,
) {
    loop {
        let accepted = tokio::select! {
            _ = cancel.cancelled() => break,
            accepted = listener.accept() => accepted,
        };
        match accepted {
            Ok((stream, addr)) => {
                let _ = stream.set_nodelay(true);
                connections.spawn(serve_connection(
                    stream,
                    addr,
                    acceptor.clone(),
                    router.clone(),
                    cancel.clone(),
                ));
            }
            Err(err) => {
                // A subnet scan opens hundreds of sockets at once; wait for
                // descriptors to be released instead of spinning.
                tracing::warn!("could not accept a connection: {err}");
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
}

async fn serve_connection(
    stream: TcpStream,
    addr: SocketAddr,
    acceptor: TlsAcceptor,
    router: axum::Router,
    cancel: CancellationToken,
) {
    let tls_stream = match acceptor.accept(stream).await {
        Ok(tls_stream) => tls_stream,
        Err(err) => {
            tracing::debug!("TLS handshake with {addr} failed: {err}");
            return;
        }
    };
    let cert_fingerprint = {
        let (_, connection) = tls_stream.get_ref();
        tls::peer_fingerprint(connection.peer_certificates())
    };
    let peer = Peer::from_remote(addr, cert_fingerprint);

    let service = TowerToHyperService::new(router.layer(axum::Extension(peer)));
    let builder = Builder::new(TokioExecutor::new());
    let connection = builder.serve_connection(TokioIo::new(tls_stream), service);
    tokio::select! {
        result = connection => {
            if let Err(err) = result {
                tracing::debug!("connection with {addr} ended: {err}");
            }
        }
        _ = cancel.cancelled() => {}
    }
}
