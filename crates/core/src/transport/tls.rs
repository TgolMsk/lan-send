//! TLS policy: any self-consistent certificate is accepted, the identity is
//! its fingerprint. Client certificates are required by default (ADR-0004).

use crate::protocol::Fingerprint;
use crate::transport::identity::{Identity, verify_cert_der};
use rustls::client::WebPkiServerVerifier;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::server::WebPkiClientVerifier;
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{
    CertificateError, DigitallySignedStruct, DistinguishedName, Error as RustlsError, OtherError,
    RootCertStore, SignatureScheme,
};
use std::fmt;
use std::sync::{Arc, OnceLock};

#[derive(Debug, thiserror::Error)]
pub enum TlsError {
    #[error("tls configuration error: {0}")]
    Config(#[from] rustls::Error),

    #[error("could not build the certificate verifier: {0}")]
    Verifier(#[from] rustls::server::VerifierBuilderError),
}

/// Whether peers must present a client certificate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientCertPolicy {
    /// Mandatory, like official LocalSend 1.18+. Connections without a
    /// certificate fail the handshake.
    Required,
    /// Optional: a presented certificate is still verified; without one the
    /// peer's identity falls back to the fingerprint it claims in the body.
    Optional,
}

/// Installs the ring crypto provider once. Safe to call repeatedly.
pub fn ensure_crypto_provider() {
    static INIT: OnceLock<()> = OnceLock::new();
    INIT.get_or_init(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// Server configuration presenting `identity` and verifying client
/// certificates according to `policy`.
pub fn server_config(
    identity: &Identity,
    policy: ClientCertPolicy,
) -> Result<Arc<rustls::ServerConfig>, TlsError> {
    ensure_crypto_provider();
    let verifier = AnyValidClientCert::new(identity, policy)?;
    let config = rustls::ServerConfig::builder()
        .with_client_cert_verifier(Arc::new(verifier))
        .with_single_cert(vec![identity.certificate()], identity.private_key())?;
    Ok(Arc::new(config))
}

/// Client configuration presenting `identity` as client certificate and,
/// when `pinned` is given, refusing every server whose certificate has a
/// different fingerprint. The check runs during the handshake, so nothing
/// is sent to a mismatching peer.
pub fn client_config(
    identity: &Identity,
    pinned: Option<Fingerprint>,
) -> Result<rustls::ClientConfig, TlsError> {
    ensure_crypto_provider();
    let verifier = PinnedServerCert::new(identity, pinned)?;
    let mut config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_client_auth_cert(vec![identity.certificate()], identity.private_key())?;
    // HTTP/1.1 only: HTTP/2's flow-control window caps bulk upload throughput.
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(config)
}

/// The fingerprint of the first certificate a peer presented, if any.
pub fn peer_fingerprint(certificates: Option<&[CertificateDer<'_>]>) -> Option<Fingerprint> {
    certificates
        .and_then(|certs| certs.first())
        .map(|cert| Fingerprint::from_der(cert.as_ref()))
}

/// Rejection reason carried through rustls so it appears in error messages.
#[derive(Debug)]
struct Rejected(String);

impl fmt::Display for Rejected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Rejected {}

fn rejected(reason: String) -> RustlsError {
    tracing::warn!("{reason}");
    RustlsError::InvalidCertificate(CertificateError::Other(OtherError(Arc::new(Rejected(
        reason,
    )))))
}

/// Accepts any client certificate that is self-consistent and valid.
struct AnyValidClientCert {
    /// Only used for the signature-scheme plumbing; never as an authority.
    inner: Arc<dyn ClientCertVerifier>,
    mandatory: bool,
}

impl AnyValidClientCert {
    fn new(identity: &Identity, policy: ClientCertPolicy) -> Result<Self, TlsError> {
        // The webpki verifier refuses an empty root store, so our own
        // certificate goes in. It is never consulted: `verify_client_cert`
        // below does not delegate.
        let mut roots = RootCertStore::empty();
        roots.add(identity.certificate())?;
        Ok(Self {
            inner: WebPkiClientVerifier::builder(Arc::new(roots)).build()?,
            mandatory: policy == ClientCertPolicy::Required,
        })
    }
}

impl fmt::Debug for AnyValidClientCert {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AnyValidClientCert")
            .field("mandatory", &self.mandatory)
            .finish()
    }
}

impl ClientCertVerifier for AnyValidClientCert {
    fn offer_client_auth(&self) -> bool {
        true
    }

    fn client_auth_mandatory(&self) -> bool {
        self.mandatory
    }

    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        self.inner.root_hint_subjects()
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, RustlsError> {
        verify_cert_der(end_entity.as_ref())
            .map_err(|err| rejected(format!("client certificate rejected: {err}")))?;
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

/// Verifies the server certificate by fingerprint instead of by authority.
struct PinnedServerCert {
    inner: Arc<dyn ServerCertVerifier>,
    expected: Option<Fingerprint>,
}

impl PinnedServerCert {
    fn new(identity: &Identity, expected: Option<Fingerprint>) -> Result<Self, TlsError> {
        let mut roots = RootCertStore::empty();
        roots.add(identity.certificate())?;
        Ok(Self {
            inner: WebPkiServerVerifier::builder(Arc::new(roots)).build()?,
            expected,
        })
    }
}

impl fmt::Debug for PinnedServerCert {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PinnedServerCert")
            .field("expected", &self.expected)
            .finish()
    }
}

impl ServerCertVerifier for PinnedServerCert {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        // The host name is deliberately ignored: peers are addressed by IP
        // and their certificates carry no matching SAN.
        verify_cert_der(end_entity.as_ref())
            .map_err(|err| rejected(format!("server certificate rejected: {err}")))?;
        if let Some(expected) = &self.expected {
            let actual = Fingerprint::from_der(end_entity.as_ref());
            if &actual != expected {
                return Err(rejected(format!(
                    "server certificate fingerprint mismatch: expected {expected}, got {actual}"
                )));
            }
        }
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}
