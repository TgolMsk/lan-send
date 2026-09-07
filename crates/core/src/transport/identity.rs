//! Device identity: an RSA-2048 self-signed certificate whose SHA-256
//! fingerprint identifies the device (ADR-0002).

use crate::protocol::Fingerprint;
use crate::store::write_private;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::path::{Path, PathBuf};

/// Common name written into generated certificates. Peers ignore it.
const COMMON_NAME: &str = "lan-send";

#[derive(Debug, thiserror::Error)]
pub enum IdentityError {
    #[error("could not generate the RSA key: {0}")]
    KeyGeneration(#[from] rsa::Error),

    #[error("could not encode the key: {0}")]
    KeyEncoding(#[from] rsa::pkcs8::Error),

    #[error("could not build the certificate: {0}")]
    Certificate(#[from] rcgen::Error),

    #[error("invalid identity file: {0}")]
    Pem(String),

    #[error("stored certificate is not usable: {0}")]
    InvalidCertificate(#[from] CertError),

    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Why a peer certificate was rejected.
#[derive(Debug, thiserror::Error)]
pub enum CertError {
    #[error("cannot parse certificate: {0}")]
    Parse(String),

    #[error("certificate is outside its validity period")]
    Expired,

    #[error("certificate signature is invalid: {0}")]
    Signature(String),
}

/// This device's certificate and private key.
pub struct Identity {
    cert_der: Vec<u8>,
    key: PrivateKeyDer<'static>,
    pem_bundle: String,
    fingerprint: Fingerprint,
}

impl Clone for Identity {
    fn clone(&self) -> Self {
        Self {
            cert_der: self.cert_der.clone(),
            key: self.key.clone_key(),
            pem_bundle: self.pem_bundle.clone(),
            fingerprint: self.fingerprint.clone(),
        }
    }
}

impl std::fmt::Debug for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Identity")
            .field("fingerprint", &self.fingerprint)
            .finish_non_exhaustive()
    }
}

impl Identity {
    /// Generates a fresh identity: RSA-2048, self-signed, `CN=lan-send`, no
    /// SANs, validity is rcgen's default (1975–4096) so it never needs to be
    /// rotated for time reasons. Takes a moment in unoptimised builds.
    pub fn generate() -> Result<Self, IdentityError> {
        use rsa::pkcs8::{EncodePrivateKey, LineEnding};

        let mut rng = rsa::rand_core::OsRng;
        let private_key = rsa::RsaPrivateKey::new(&mut rng, 2048)?;
        let key_pem = private_key.to_pkcs8_pem(LineEnding::LF)?.to_string();
        let key_der = private_key.to_pkcs8_der()?;

        let key_pair = rcgen::KeyPair::try_from(key_der.as_bytes())?;
        let mut params = rcgen::CertificateParams::default();
        params.distinguished_name = rcgen::DistinguishedName::new();
        params
            .distinguished_name
            .push(rcgen::DnType::CommonName, COMMON_NAME);
        let certificate = params.self_signed(&key_pair)?;

        let pem_bundle = format!("{}{}", certificate.pem(), key_pem);
        Self::from_pem_bundle(&pem_bundle)
    }

    /// Parses a PEM text containing one CERTIFICATE and one private key
    /// block (in any order).
    pub fn from_pem_bundle(pem: &str) -> Result<Self, IdentityError> {
        let cert = CertificateDer::from_pem_slice(pem.as_bytes())
            .map_err(|err| IdentityError::Pem(format!("certificate: {err}")))?;
        let key = PrivateKeyDer::from_pem_slice(pem.as_bytes())
            .map_err(|err| IdentityError::Pem(format!("private key: {err}")))?;
        verify_cert_der(&cert)?;
        Ok(Self {
            fingerprint: Fingerprint::from_der(&cert),
            cert_der: cert.to_vec(),
            key,
            pem_bundle: pem.to_string(),
        })
    }

    /// Loads the identity from `path`, generating and saving a new one when
    /// the file does not exist yet.
    pub fn load_or_generate(path: &Path) -> Result<Self, IdentityError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::from_pem_bundle(&text),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                let identity = Self::generate()?;
                identity.save(path)?;
                Ok(identity)
            }
            Err(source) => Err(IdentityError::Io {
                path: path.to_path_buf(),
                source,
            }),
        }
    }

    /// Writes the PEM bundle to `path` with owner-only permissions.
    pub fn save(&self, path: &Path) -> Result<(), IdentityError> {
        let io = |source| IdentityError::Io {
            path: path.to_path_buf(),
            source,
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        write_private(path, self.pem_bundle.as_bytes()).map_err(io)
    }

    pub fn fingerprint(&self) -> &Fingerprint {
        &self.fingerprint
    }

    /// The certificate for rustls.
    pub fn certificate(&self) -> CertificateDer<'static> {
        CertificateDer::from(self.cert_der.clone())
    }

    /// The private key for rustls.
    pub fn private_key(&self) -> PrivateKeyDer<'static> {
        self.key.clone_key()
    }

    /// Certificate and key as PEM text, the on-disk format.
    pub fn pem_bundle(&self) -> &str {
        &self.pem_bundle
    }
}

/// Checks that a DER certificate is self-consistent (its signature verifies
/// against its own public key) and currently valid. No issuer is checked:
/// LocalSend peers are identified by fingerprint, not by a CA.
pub fn verify_cert_der(der: &[u8]) -> Result<(), CertError> {
    use x509_parser::prelude::*;

    let (_, cert) =
        X509Certificate::from_der(der).map_err(|err| CertError::Parse(err.to_string()))?;
    if !cert.validity().is_valid() {
        return Err(CertError::Expired);
    }
    cert.verify_signature(None)
        .map_err(|err| CertError::Signature(err.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_identity_round_trips_through_pem() {
        let identity = Identity::generate().unwrap();
        assert_eq!(identity.fingerprint().as_str().len(), 64);
        let reloaded = Identity::from_pem_bundle(identity.pem_bundle()).unwrap();
        assert_eq!(reloaded.fingerprint(), identity.fingerprint());
        verify_cert_der(&reloaded.certificate()).unwrap();
    }

    #[test]
    fn load_or_generate_persists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("identity.pem");
        let first = Identity::load_or_generate(&path).unwrap();
        let second = Identity::load_or_generate(&path).unwrap();
        assert_eq!(first.fingerprint(), second.fingerprint());
    }

    #[test]
    fn tampered_certificate_is_rejected() {
        let identity = Identity::generate().unwrap();
        let mut der = identity.certificate().to_vec();
        let last = der.len() - 1;
        der[last] ^= 0xFF;
        assert!(matches!(
            verify_cert_der(&der),
            Err(CertError::Signature(_))
        ));
    }
}
