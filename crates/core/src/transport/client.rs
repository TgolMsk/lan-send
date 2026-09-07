//! HTTP client for the LocalSend v2 API and the resume extension (ADR-0004,
//! ADR-0008).

use crate::protocol::{
    API_PREFIX_V2, DeviceInfo, ErrorResponse, Fingerprint, PAIR_PATH, PairRequest, PairResponse,
    PeerInfo, PrepareUploadRequest, PrepareUploadResponse, ProtocolType, RESUME_OFFSET_HEADER,
    RESUME_PATH, RESUME_TOKEN_HEADER, ResumeOffsetResponse, UNPAIR_PATH,
};
use crate::transport::identity::Identity;
use crate::transport::scoped_host;
use crate::transport::tls::{self, TlsError};
use futures_util::StreamExt;
use reqwest::StatusCode;
use std::path::Path;
use std::sync::Arc;
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
        format!("{}{API_PREFIX_V2}{path}", self.origin())
    }

    /// `scheme://host:port`, with IPv6 hosts bracketed and scoped
    /// link-local addresses encoded for the client's resolver.
    pub fn origin(&self) -> String {
        format!(
            "{}://{}:{}",
            self.protocol.scheme(),
            scoped_host::url_host(&self.host),
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

    /// A `Range` upload started at the wrong offset; the receiver expects
    /// this one (resume extension).
    #[error("the receiver expects the upload to resume at byte {0}")]
    OffsetMismatch(u64),

    /// The request could not be built from its input.
    #[error("invalid request: {0}")]
    Invalid(String),
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

    /// Whether the request itself failed (connection, timeout, body
    /// stream) rather than being refused by the peer.
    pub fn is_transport(&self) -> bool {
        matches!(self, Self::Http(_) | Self::Io(_))
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

/// Resume parameters of an upload (resume extension).
#[derive(Clone, Copy, Debug)]
pub struct Resume<'a> {
    /// Bytes the receiver already holds; the body starts there.
    pub offset: u64,
    /// The session's resume token from `prepare-upload`.
    pub token: &'a str,
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
            .dns_resolver(Arc::new(ScopedHostResolver))
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
        self.upload_with(target, session_id, file_id, token, body, None)
            .await
    }

    /// `POST /upload`, optionally resuming: with `resume` given, the body
    /// starts at `resume.offset` and the request carries `Range` and the
    /// resume token. A 416 answer becomes [`ClientError::OffsetMismatch`].
    pub async fn upload_with(
        &self,
        target: &Target,
        session_id: &str,
        file_id: &str,
        token: &str,
        body: reqwest::Body,
        resume: Option<Resume<'_>>,
    ) -> Result<(), ClientError> {
        let mut request = self
            .http
            .post(target.url("/upload"))
            .query(&[
                ("sessionId", session_id),
                ("fileId", file_id),
                ("token", token),
            ])
            .body(body);
        if let Some(resume) = resume {
            request = request
                .header("Range", format!("bytes={}-", resume.offset))
                .header(RESUME_TOKEN_HEADER, resume.token);
        }
        let response = request.send().await?;
        if response.status() == StatusCode::RANGE_NOT_SATISFIABLE
            && let Some(expected) = response
                .headers()
                .get(RESUME_OFFSET_HEADER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok())
        {
            return Err(ClientError::OffsetMismatch(expected));
        }
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
        self.upload_file_from(target, session_id, file_id, token, path, None, progress)
            .await
    }

    /// Like [`Client::upload_file`], resuming at `resume.offset` when given;
    /// `progress` then starts from that offset.
    #[allow(clippy::too_many_arguments)]
    pub async fn upload_file_from(
        &self,
        target: &Target,
        session_id: &str,
        file_id: &str,
        token: &str,
        path: &Path,
        resume: Option<Resume<'_>>,
        progress: impl Fn(u64) + Send + Sync + 'static,
    ) -> Result<(), ClientError> {
        use tokio::io::AsyncSeekExt;

        let mut file = tokio::fs::File::open(path).await?;
        let offset = resume.as_ref().map(|resume| resume.offset).unwrap_or(0);
        if offset > 0 {
            file.seek(std::io::SeekFrom::Start(offset)).await?;
        }
        let mut sent = offset;
        let stream =
            tokio_util::io::ReaderStream::with_capacity(file, 512 * 1024).map(move |chunk| {
                if let Ok(bytes) = &chunk {
                    sent += bytes.len() as u64;
                    progress(sent);
                }
                chunk
            });
        self.upload_with(
            target,
            session_id,
            file_id,
            token,
            reqwest::Body::wrap_stream(stream),
            resume,
        )
        .await
    }

    /// Resume extension: how many bytes of a pending file the receiver
    /// already holds.
    pub async fn resume_offset(
        &self,
        target: &Target,
        session_id: &str,
        file_id: &str,
        resume_token: &str,
    ) -> Result<u64, ClientError> {
        let response = self
            .http
            .get(format!("{}{RESUME_PATH}", target.origin()))
            .query(&[("sessionId", session_id), ("fileId", file_id)])
            .header(RESUME_TOKEN_HEADER, resume_token)
            .send()
            .await?;
        let response = ok_or_error(response).await?;
        Ok(response.json::<ResumeOffsetResponse>().await?.offset)
    }

    /// Clipboard sync (ADR-0011): pushes an item to a paired peer. Images
    /// above the multipart threshold travel as a binary part.
    pub async fn send_clipboard(
        &self,
        target: &Target,
        item: &crate::clipboard::ClipboardItem,
    ) -> Result<(), ClientError> {
        use crate::clipboard::wire::{Encoded, encode};

        let url = format!("{}{}", target.origin(), crate::clipboard::CLIPBOARD_PATH);
        let encoded = encode(item, crate::clipboard::MULTIPART_THRESHOLD)
            .map_err(|err| ClientError::Invalid(err.to_string()))?;
        let request = match encoded {
            Encoded::Json(dto) => self.http.post(url).json(&dto),
            Encoded::Multipart { item, image, mime } => {
                let form = reqwest::multipart::Form::new()
                    .text("item", serde_json::to_string(&item)?)
                    .part(
                        "image",
                        reqwest::multipart::Part::stream(reqwest::Body::from(image))
                            .mime_str(mime)
                            .map_err(ClientError::Http)?
                            .file_name("clipboard"),
                    );
                self.http.post(url).multipart(form)
            }
        };
        ok_or_error(request.send().await?).await?;
        Ok(())
    }

    /// Pairing (ADR-0010): asks the peer to pair; blocks until its user
    /// decided. A refusal surfaces as HTTP 403, no answer as 408.
    pub async fn pair(&self, target: &Target, alias: &str) -> Result<PairResponse, ClientError> {
        let response = self
            .http
            .post(format!("{}{PAIR_PATH}", target.origin()))
            .json(&PairRequest {
                alias: alias.to_string(),
            })
            .send()
            .await?;
        let response = ok_or_error(response).await?;
        Ok(response.json().await?)
    }

    /// Tells a paired peer that the pairing is withdrawn (best effort).
    pub async fn unpair(&self, target: &Target) -> Result<(), ClientError> {
        let response = self
            .http
            .post(format!("{}{UNPAIR_PATH}", target.origin()))
            .send()
            .await?;
        ok_or_error(response).await?;
        Ok(())
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

/// Resolves the synthetic names of scoped IPv6 peers (see
/// [`scoped_host`]); everything else goes to the system resolver.
struct ScopedHostResolver;

impl reqwest::dns::Resolve for ScopedHostResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        Box::pin(async move {
            // The port is a placeholder; reqwest substitutes the URL's.
            if let Some(addr) = scoped_host::decode(name.as_str(), 0) {
                return Ok(Box::new(std::iter::once(addr)) as reqwest::dns::Addrs);
            }
            let addrs = tokio::net::lookup_host((name.as_str(), 0))
                .await?
                .collect::<Vec<_>>();
            Ok(Box::new(addrs.into_iter()) as reqwest::dns::Addrs)
        })
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
        assert_eq!(v6.origin(), "http://[fe80::1]:1");
        let scoped = Target {
            host: "fe80::1%3".into(),
            port: 2,
            protocol: ProtocolType::Https,
        };
        assert_eq!(
            scoped.origin(),
            "https://fe80--1s3.scoped.lan-send.internal:2"
        );
    }
}
