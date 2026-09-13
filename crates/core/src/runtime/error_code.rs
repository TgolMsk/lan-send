//! Stable, language-neutral codes for the errors an interface shows. The
//! English messages stay as detail; a translated interface maps the code
//! to its own wording and falls back to the message for `Unknown`.

use super::RuntimeError;
use crate::transfer::CollectError;
use crate::transport::{ClientError, ServerError};
use serde::{Deserialize, Serialize};
use std::io;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCode {
    /// No route, connection refused or reset.
    Unreachable,
    Timeout,
    /// The receiver declined the transfer.
    Declined,
    /// The receiver is busy with another transfer.
    Busy,
    PinRequired,
    PinRejected,
    Cancelled,
    /// The other device disappeared mid-transfer.
    PeerGone,
    DiskFull,
    PermissionDenied,
    NotFound,
    ChecksumMismatch,
    NotPaired,
    RuntimeStopped,
    PortInUse,
    InvalidAddress,
    /// Handshake or certificate problem.
    Tls,
    Unsupported,
    /// The receiver accepted none of the offered files.
    NothingAccepted,
    /// Some files failed; the per-file states say which.
    PartialFailure,
    DeviceNotFound,
    NoReceiveDir,
    Invalid,
    PairDeclined,
    PairTimeout,
    PairBusy,
    PairUnsupported,
    PairWithdrawn,
    PairCodeMismatch,
    Unknown,
}

impl ErrorCode {
    pub fn from_io(err: &io::Error) -> Self {
        use io::ErrorKind as K;
        match err.kind() {
            K::NotFound => Self::NotFound,
            K::PermissionDenied => Self::PermissionDenied,
            K::ConnectionRefused
            | K::ConnectionReset
            | K::ConnectionAborted
            | K::HostUnreachable
            | K::NetworkUnreachable
            | K::NetworkDown => Self::Unreachable,
            K::TimedOut => Self::Timeout,
            K::AddrInUse => Self::PortInUse,
            K::StorageFull => Self::DiskFull,
            K::Unsupported => Self::Unsupported,
            _ => match err.raw_os_error() {
                // ENOSPC / ERROR_DISK_FULL on platforms whose kind mapping lags.
                Some(28) | Some(112) => Self::DiskFull,
                _ => Self::Unknown,
            },
        }
    }

    pub fn from_client(err: &ClientError) -> Self {
        match err {
            ClientError::Status { status: 401, .. } => Self::PinRequired,
            ClientError::Status { status: 403, .. } => Self::Declined,
            ClientError::Status { status: 409, .. } => Self::Busy,
            ClientError::Status { status: 400, .. } => Self::Invalid,
            ClientError::Status { .. } => Self::Unknown,
            ClientError::Http(http) => {
                let mut source: Option<&(dyn std::error::Error + 'static)> = Some(http);
                while let Some(err) = source {
                    if err.is::<rustls::Error>() {
                        return Self::Tls;
                    }
                    if let Some(io) = err.downcast_ref::<io::Error>() {
                        let code = Self::from_io(io);
                        if code != Self::Unknown {
                            return code;
                        }
                    }
                    source = err.source();
                }
                if http.is_timeout() {
                    Self::Timeout
                } else if http.is_connect() {
                    Self::Unreachable
                } else {
                    Self::Unknown
                }
            }
            ClientError::Tls(_) => Self::Tls,
            ClientError::Io(io) => Self::from_io(io),
            ClientError::Cancelled => Self::Cancelled,
            ClientError::Invalid(_) => Self::Invalid,
            ClientError::Json(_) | ClientError::OffsetMismatch(_) => Self::Unknown,
        }
    }

    pub fn from_runtime(err: &RuntimeError) -> Self {
        match err {
            RuntimeError::Client(client) => Self::from_client(client),
            RuntimeError::Io(io) => Self::from_io(io),
            RuntimeError::Server(ServerError::Bind { .. }) => Self::PortInUse,
            RuntimeError::Server(ServerError::Tls(_)) => Self::Tls,
            RuntimeError::Server(ServerError::Io(io)) => Self::from_io(io),
            RuntimeError::Collect(CollectError::Missing(_)) => Self::NotFound,
            RuntimeError::Collect(CollectError::Io { source, .. }) => Self::from_io(source),
            RuntimeError::Collect(_) => Self::Invalid,
            RuntimeError::DeviceNotFound(_) => Self::DeviceNotFound,
            RuntimeError::Invalid(_) => Self::Invalid,
            RuntimeError::NoClipboard => Self::Unsupported,
            RuntimeError::Identity(_) => Self::Tls,
            RuntimeError::Store(_) | RuntimeError::Clipboard(_) | RuntimeError::NothingPending => {
                Self::Unknown
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialises_as_kebab_case() {
        assert_eq!(
            serde_json::to_string(&ErrorCode::PinRequired)
                .ok()
                .as_deref(),
            Some("\"pin-required\"")
        );
    }

    #[test]
    fn classifies_io_errors() {
        let refused = io::Error::from(io::ErrorKind::ConnectionRefused);
        assert_eq!(ErrorCode::from_io(&refused), ErrorCode::Unreachable);
        let enospc = io::Error::from_raw_os_error(28);
        assert_eq!(ErrorCode::from_io(&enospc), ErrorCode::DiskFull);
    }
}
