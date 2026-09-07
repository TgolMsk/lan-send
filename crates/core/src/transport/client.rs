//! HTTP client for the LocalSend v2 API (ADR-0004).

use crate::protocol::{
    API_PREFIX_V2, DeviceInfo, ErrorResponse, Fingerprint, PeerInfo, PrepareUploadRequest,
    PrepareUploadResponse, ProtocolType,
};
use crate::transport::identity::Identity;
use crate::transport::tls::{self, TlsError};
use futures_util::StreamExt;
use reqwest::StatusCode;
use std::path::Path;
use std::time::Duration;

/// Where a request goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    /// IP address (or host name) of the peer.
    pub host: String,
    pub port: u16,
    pub protocol: ProtocolType,
}

impl Target {
    /// Full URL of an API path such as `/register`.
    pub fn url(&self, path: &str) -> String {
        let host = if self.host.contains(':') && !self.host.starts_with('[') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        format!(
            "{}://{host}:{}{API_PREFIX_V2}{path}",
            self.protocol.scheme(),
            self.port
        )
    }
}

impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}://{}:{}",
            self.protocol.scheme(),
            self.host,
            self.port
        )
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// The peer answered with an error status; `message` is its
    /// `{"message"}` body or the raw text, possibly empty.
    #[error("HTTP {status}{}", format_message(.message))]
    Status { status: u16, message: String },

    #[error("request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error(transparent)]
    Tls(#[from] TlsError),

    #[error("invalid response body: {0}")]
    Json(#[from] serde_json::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("cancelled")]
    Cancelled,
}

fn format_message(message: &str) -> String {
    if message.is_empty() {
        String::new()
    } else {
        format!(": {message}")
    }
}

impl ClientError {
    /// The HTTP status code, when the peer answered at all.
    pub fn status(&self) -> Option<u16> {
        match self {
            Self::Status { status, .. } => Some(*status),
            _ => None,
        }
    }
}

/// A successful `register` or `info` exchange.
#[derive(Clone, Debug)]
pub struct Registered {
    pub response: PeerInfo,
    /// The fingerprint of the certificate the peer presented; `None` over
    /// plain HTTP. In HTTPS mode this, not `response.fingerprint`, is the
    /// peer's identity.
    pub peer_fingerprint: Option<Fingerprint>,
}

impl Registered {
    /// The peer's identity: the certificate fingerprint when there is one,
    /// otherwise the fingerprint claimed in the body.
    pub fn identity(&self) -> Fingerprint {
        self.peer_fingerprint
            .clone()
            .unwrap_or_else(|| Fingerprint::parse(&self.response.fingerprint))
    }
}

/// Outcome of `prepare-upload`.
#[derive(Clone, Debug)]
pub enum PrepareUploadOutcome {
    /// 200: a session with tokens for the accepted files.
    Accepted(PrepareUploadResponse),
    /// 204: the receiver accepted nothing (e.g. it read a text message).
    NothingToTransfer,
}

/// HTTP client presenting this device's certificate.
///
/// Build one with a pinned fingerprint for anything that carries data; the
/// unpinned form is only for discovery, where the peer is not known yet.
#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
}

impl Client {
    /// `timeout` bounds each whole request; leave it `None` for transfers.
    pub fn new(
        identity: &Identity,
        pinned: Option<Fingerprint>,
        timeout: Option<Duration>,
    ) -> Result<Self, ClientError> {
        let tls_config = tls::client_config(identity, pinned)?;
        let mut builder = reqwest::Client::builder()
            .tls_backend_preconfigured(tls_config)
            .tls_info(true)
            // Peers are on the LAN: never dial them through a proxy, and
            // never follow a redirect to a host whose certificate was not
            // the one verified.
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5));
        if let Some(timeout) = timeout {
            builder = builder.timeout(timeout);
        }
        Ok(Self {
            http: builder.build()?,
        })
    }

    /// `POST /register`: introduces this device and learns about the peer.
    pub async fn register(
        &self,
        target: &Target,
        info: &DeviceInfo,
    ) -> Result<Registered, ClientError> {
        let response = self
            .http
            .post(target.url("/register"))
            .json(info)
            .send()
            .await?;
        let response = ok_or_error(response).await?;
        let peer_fingerprint = fingerprint_of(&response);
        Ok(Registered {
            response: response.json().await?,
            peer_fingerprint,
        })
    }

    /// `GET /info`: the peer's device information (debugging, old clients).
    pub async fn info(&self, target: &Target) -> Result<Registered, ClientError> {
        let response = self.http.get(target.url("/info")).send().await?;
        let response = ok_or_error(response).await?;
        let peer_fingerprint = fingerprint_of(&response);
        Ok(Registered {
            response: response.json().await?,
            peer_fingerprint,
        })
    }

    /// `POST /prepare-upload`. Blocks until the receiver decided; use
    /// `tokio::select!` with a cancellation token to give up early (closing
    /// the connection tells the receiver the request is withdrawn).
    pub async fn prepare_upload(
        &self,
        target: &Target,
        request: &PrepareUploadRequest,
        pin: Option<&str>,
    ) -> Result<PrepareUploadOutcome, ClientError> {
        let mut builder = self.http.post(target.url("/prepare-upload")).json(request);
        if let Some(pin) = pin {
            builder = builder.query(&[("pin", pin)]);
        }
        let response = ok_or_error(builder.send().await?).await?;
        if response.status() == StatusCode::NO_CONTENT {
            return Ok(PrepareUploadOutcome::NothingToTransfer);
        }
        Ok(PrepareUploadOutcome::Accepted(response.json().await?))
    }

    /// `POST /upload` with an arbitrary body.
    pub async fn upload(
        &self,
        target: &Target,
        session_id: &str,
        file_id: &str,
        token: &str,
        body: reqwest::Body,
    ) -> Result<(), ClientError> {
        let response = self
            .http
            .post(target.url("/upload"))
            .query(&[
                ("sessionId", session_id),
                ("fileId", file_id),
                ("token", token),
            ])
            .body(body)
            .send()
            .await?;
        ok_or_error(response).await?;
        Ok(())
    }

    /// `POST /upload` streaming the file at `path`; `progress` receives the
    /// cumulative number of bytes handed to the network.
    pub async fn upload_file(
        &self,
        target: &Target,
        session_id: &str,
        file_id: &str,
        token: &str,
        path: &Path,
        progress: impl Fn(u64) + Send + Sync + 'static,
    ) -> Result<(), ClientError> {
        let file = tokio::fs::File::open(path).await?;
        let mut sent = 0u64;
        let stream =
            tokio_util::io::ReaderStream::with_capacity(file, 512 * 1024).map(move |chunk| {
                if let Ok(bytes) = &chunk {
                    sent += bytes.len() as u64;
                    progress(sent);
                }
                chunk
            });
        self.upload(
            target,
            session_id,
            file_id,
            token,
            reqwest::Body::wrap_stream(stream),
        )
        .await
    }

    /// `POST /cancel`. Without a session id it withdraws a pending
    /// prepare-upload from this address.
    pub async fn cancel(
        &self,
        target: &Target,
        session_id: Option<&str>,
    ) -> Result<(), ClientError> {
        let mut builder = self.http.post(target.url("/cancel"));
        if let Some(session_id) = session_id {
            builder = builder.query(&[("sessionId", session_id)]);
        }
        builder.send().await?;
        Ok(())
    }
}

async fn ok_or_error(response: reqwest::Response) -> Result<reqwest::Response, ClientError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let text = response.text().await.unwrap_or_default();
    let message = serde_json::from_str::<ErrorResponse>(&text)
        .map(|error| error.message)
        .unwrap_or(text);
    Err(ClientError::Status {
        status: status.as_u16(),
        message,
    })
}

fn fingerprint_of(response: &reqwest::Response) -> Option<Fingerprint> {
    response
        .extensions()
        .get::<reqwest::tls::TlsInfo>()?
        .peer_certificate()
        .map(Fingerprint::from_der)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_urls() {
        let target = Target {
            host: "192.168.1.5".into(),
            port: 53317,
            protocol: ProtocolType::Https,
        };
        assert_eq!(
            target.url("/register"),
            "https://192.168.1.5:53317/api/localsend/v2/register"
        );
        let v6 = Target {
            host: "fe80::1".into(),
            port: 1,
            protocol: ProtocolType::Http,
        };
        assert_eq!(v6.url("/info"), "http://[fe80::1]:1/api/localsend/v2/info");
    }
}
