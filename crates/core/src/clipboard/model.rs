//! The clipboard item model.

use crate::protocol::Fingerprint;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    Png,
    Jpeg,
}

impl ImageFormat {
    pub const fn mime(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
        }
    }

    pub const fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PayloadKind {
    Text,
    Image,
    Files,
}

impl std::fmt::Display for PayloadKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Text => "text",
            Self::Image => "image",
            Self::Files => "files",
        })
    }
}

/// What the clipboard holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardPayload {
    Text {
        plain: String,
        html: Option<String>,
        rtf: Option<String>,
    },
    Image {
        format: ImageFormat,
        bytes: Bytes,
        width: u32,
        height: u32,
    },
    /// Local paths; across devices they become a file transfer.
    Files { paths: Vec<PathBuf> },
}

impl ClipboardPayload {
    pub fn kind(&self) -> PayloadKind {
        match self {
            Self::Text { .. } => PayloadKind::Text,
            Self::Image { .. } => PayloadKind::Image,
            Self::Files { .. } => PayloadKind::Files,
        }
    }

    /// The size that counts against the limits: the plain text, the image
    /// bytes, or the path strings.
    pub fn size(&self) -> usize {
        match self {
            Self::Text { plain, .. } => plain.len(),
            Self::Image { bytes, .. } => bytes.len(),
            Self::Files { paths } => paths.iter().map(|p| p.as_os_str().len()).sum(),
        }
    }

    /// SHA-256 over the content that matters for equality: the plain text,
    /// the image bytes, or the paths.
    pub fn content_hash(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        match self {
            Self::Text { plain, .. } => {
                hasher.update(b"text\0");
                hasher.update(plain.as_bytes());
            }
            Self::Image { bytes, .. } => {
                hasher.update(b"image\0");
                hasher.update(bytes);
            }
            Self::Files { paths } => {
                hasher.update(b"files\0");
                for path in paths {
                    hasher.update(path.to_string_lossy().as_bytes());
                    hasher.update(b"\0");
                }
            }
        }
        hasher.finalize().into()
    }

    /// A one-line description without the content (safe for logs).
    pub fn describe(&self) -> String {
        match self {
            Self::Text { plain, html, rtf } => format!(
                "text, {} chars{}{}",
                plain.chars().count(),
                if html.is_some() { " +html" } else { "" },
                if rtf.is_some() { " +rtf" } else { "" }
            ),
            Self::Image {
                format,
                bytes,
                width,
                height,
            } => format!(
                "{width}x{height} {} image, {} bytes",
                format.extension(),
                bytes.len()
            ),
            Self::Files { paths } => format!("{} file(s)", paths.len()),
        }
    }
}

/// One clipboard content, as synchronised and as kept in the history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardItem {
    pub id: String,
    /// The device the content was copied on.
    pub origin_device: Fingerprint,
    /// Unix time in milliseconds.
    pub created_at: i64,
    pub payload: ClipboardPayload,
    pub content_hash: [u8; 32],
}

impl ClipboardItem {
    /// A new item copied now on `origin_device`.
    pub fn new(origin_device: Fingerprint, payload: ClipboardPayload) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            origin_device,
            created_at: now_millis(),
            content_hash: payload.content_hash(),
            payload,
        }
    }

    pub fn hash_hex(&self) -> String {
        hex(&self.content_hash)
    }
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_depend_on_content_only() {
        let a = ClipboardPayload::Text {
            plain: "hello".into(),
            html: Some("<b>hello</b>".into()),
            rtf: None,
        };
        let b = ClipboardPayload::Text {
            plain: "hello".into(),
            html: None,
            rtf: None,
        };
        assert_eq!(a.content_hash(), b.content_hash());
        let image = ClipboardPayload::Image {
            format: ImageFormat::Png,
            bytes: Bytes::from_static(b"hello"),
            width: 1,
            height: 1,
        };
        assert_ne!(image.content_hash(), a.content_hash());
        assert_eq!(image.size(), 5);
        assert_eq!(image.kind(), PayloadKind::Image);
        assert!(!image.describe().contains("hello"));
    }
}
