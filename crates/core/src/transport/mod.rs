//! HTTPS transport: device identity, TLS policy, the HTTP client and the
//! server implementing the LocalSend upload API.
//!
//! See ADR-0002 (identity) and ADR-0004 (transport).

pub mod client;
pub mod filename;
pub mod identity;
pub mod scoped_host;
pub mod server;
pub mod tls;

pub use client::{Client, ClientError, PrepareUploadOutcome, Registered, Resume, Target};
pub use identity::{CertError, Identity, IdentityError};
pub use server::{
    Peer, ServerConfig, ServerError, ServerEvent, ServerHandle, SessionEndReason, UploadDecision,
    UploadTarget,
};
pub use tls::{ClientCertPolicy, TlsError};
