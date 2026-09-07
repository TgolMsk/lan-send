//! HTTPS transport: self-signed device identity (RSA-2048, fingerprint =
//! uppercase-hex SHA-256 of the DER certificate), the axum server that
//! implements the upload API, and the reqwest (rustls) client with client
//! certificate and fingerprint pinning.
//!
//! Implemented in milestone 1 (single file) and 2 (multi-file, folders,
//! checksum, resume).
