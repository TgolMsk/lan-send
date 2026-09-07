//! Certificate fingerprints: the identity of a device.

use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::str::FromStr;

/// The SHA-256 of a device's DER-encoded TLS certificate, as uppercase hex.
///
/// This is how LocalSend identifies devices in HTTPS mode. Comparisons are
/// case-insensitive because peers may send lowercase hex; the stored form is
/// always uppercase, matching the official implementation.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct Fingerprint(String);

impl Fingerprint {
    /// Computes the fingerprint of a DER-encoded certificate.
    pub fn from_der(der: &[u8]) -> Self {
        let digest = Sha256::digest(der);
        Self(digest.iter().map(|byte| format!("{byte:02X}")).collect())
    }

    /// Normalises a fingerprint received from a peer (trimmed, uppercased).
    pub fn parse(value: &str) -> Self {
        Self(value.trim().to_ascii_uppercase())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The first eight characters, for display.
    pub fn short(&self) -> &str {
        self.0.get(..8).unwrap_or(&self.0)
    }

    /// Case-insensitive equality with a fingerprint string.
    pub fn matches(&self, other: &str) -> bool {
        self.0.eq_ignore_ascii_case(other.trim())
    }

    /// Whether the fingerprint starts with `prefix` (case-insensitive).
    pub fn has_prefix(&self, prefix: &str) -> bool {
        let prefix = prefix.trim();
        !prefix.is_empty()
            && self
                .0
                .get(..prefix.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
    }
}

impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fingerprint({}…)", self.short())
    }
}

impl<'de> Deserialize<'de> for Fingerprint {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Ok(Self::parse(&value))
    }
}

impl FromStr for Fingerprint {
    type Err = std::convert::Infallible;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self::parse(value))
    }
}

impl From<Fingerprint> for String {
    fn from(fingerprint: Fingerprint) -> Self {
        fingerprint.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_der_as_uppercase_hex() {
        let fingerprint = Fingerprint::from_der(b"hello");
        assert_eq!(
            fingerprint.as_str(),
            "2CF24DBA5FB0A30E26E83B2AC5B9E29E1B161E5C1FA7425E73043362938B9824"
        );
        assert_eq!(fingerprint.short(), "2CF24DBA");
    }

    #[test]
    fn compares_case_insensitively() {
        let fingerprint = Fingerprint::parse(" abcd1234 ");
        assert_eq!(fingerprint.as_str(), "ABCD1234");
        assert!(fingerprint.matches("abcd1234"));
        assert!(fingerprint.has_prefix("abc"));
        assert!(!fingerprint.has_prefix(""));
        assert!(!fingerprint.has_prefix("abcd12345"));
        assert!(!fingerprint.has_prefix("ü"));
    }
}
