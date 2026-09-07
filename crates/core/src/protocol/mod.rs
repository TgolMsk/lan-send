//! LocalSend protocol v2.2 data structures and constants, plus the private
//! extension fields that official clients ignore.
//!
//! Everything on the wire is JSON with camelCase keys. Unknown fields are
//! ignored on both sides (official behaviour), which is what makes the
//! `x-lanext` extension safe. See `docs/localsend-v2-interface-checklist.md`
//! for the observed behaviour of the official implementation and
//! `docs/protocol-extensions.md` for our extensions.

mod fingerprint;
mod timestamp;

pub use fingerprint::Fingerprint;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::fmt;
use std::net::Ipv4Addr;
use std::path::Path;
use std::time::SystemTime;

/// The protocol version (major.minor) this crate implements and advertises.
pub const PROTOCOL_VERSION: &str = "2.2";

/// Default HTTP(S) port, identical to the multicast port.
pub const DEFAULT_PORT: u16 = 53317;

/// Multicast group used by LocalSend. It is inside `224.0.0.0/24` because
/// some Android devices reject any other group.
pub const MULTICAST_GROUP_V4: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 167);

/// Multicast port. Always this value, independent of the HTTP port.
pub const MULTICAST_PORT: u16 = 53317;

/// IPv6 multicast group, a LocalSend extension on top of v2.2: transient,
/// link-local scope. IPv4 stays the baseline; IPv6 is announced in parallel.
pub const MULTICAST_GROUP_V6: std::net::Ipv6Addr =
    std::net::Ipv6Addr::new(0xff12, 0, 0, 0, 0, 0, 0xfd3a, 0xe420);

/// Base path of the v2 HTTP API.
pub const API_PREFIX_V2: &str = "/api/localsend/v2";

/// Version of the `x-lanext` extension block this crate speaks.
pub const EXTENSION_VERSION: u32 = 1;

/// Extension feature: resumable uploads (milestone 2).
pub const FEATURE_RESUME: &str = "resume";

/// Extension feature: clipboard sync endpoint (milestone 3).
pub const FEATURE_CLIPBOARD: &str = "clipboard";

/// Device category, only used for icons in user interfaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DeviceType {
    Mobile,
    Desktop,
    Web,
    Headless,
    Server,
}

impl DeviceType {
    /// The wire value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mobile => "mobile",
            Self::Desktop => "desktop",
            Self::Web => "web",
            Self::Headless => "headless",
            Self::Server => "server",
        }
    }

    /// Parses a wire value. Unknown values fall back to `Desktop`, as the
    /// protocol requires (section 7.1).
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "mobile" => Self::Mobile,
            "web" => Self::Web,
            "headless" => Self::Headless,
            "server" => Self::Server,
            _ => Self::Desktop,
        }
    }
}

impl fmt::Display for DeviceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for DeviceType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for DeviceType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Ok(Self::parse(&value))
    }
}

/// Whether a device's HTTP server uses TLS.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProtocolType {
    Http,
    Https,
}

impl ProtocolType {
    /// The URL scheme.
    pub const fn scheme(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
        }
    }
}

impl fmt::Display for ProtocolType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.scheme())
    }
}

/// The private extension block, sent as the `x-lanext` field. Official
/// clients ignore it; a feature is only used when both sides announce it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Extensions {
    /// Version of this block.
    pub v: u32,

    /// Feature names, see the `FEATURE_*` constants.
    #[serde(default)]
    pub features: Vec<String>,
}

impl Extensions {
    /// The block this crate announces for the given features.
    pub fn current(features: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            v: EXTENSION_VERSION,
            features: features.into_iter().map(Into::into).collect(),
        }
    }

    /// Whether the peer announced `feature`.
    pub fn supports(&self, feature: &str) -> bool {
        self.features.iter().any(|f| f == feature)
    }
}

/// The device information sent in the multicast announcement, in
/// `POST /register` and as `info` of `POST /prepare-upload`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    /// Display name.
    pub alias: String,

    /// Protocol version (major.minor), e.g. `"2.2"`.
    pub version: String,

    /// Device model, e.g. `"macOS"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_model: Option<String>,

    /// Device category.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_type: Option<DeviceType>,

    /// In HTTPS mode the certificate fingerprint; in HTTP mode a random string.
    pub fingerprint: String,

    /// Port of the device's HTTP server.
    pub port: u16,

    /// Whether the device's HTTP server uses TLS.
    pub protocol: ProtocolType,

    /// Whether the download API (browser sharing) is active.
    #[serde(default)]
    pub download: bool,

    /// Our private extension block. Absent for official clients.
    #[serde(default, rename = "x-lanext", skip_serializing_if = "Option::is_none")]
    pub ext: Option<Extensions>,
}

/// Response body of `POST /register` and `GET /info`. Unlike
/// [`DeviceInfo`] it carries no `port`/`protocol`: the caller already knows
/// where it connected to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerInfo {
    pub alias: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_type: Option<DeviceType>,
    /// Empty when the peer omitted it; in HTTPS mode the TLS certificate is
    /// the identity anyway.
    #[serde(default)]
    pub fingerprint: String,
    #[serde(default)]
    pub download: bool,
    #[serde(default, rename = "x-lanext", skip_serializing_if = "Option::is_none")]
    pub ext: Option<Extensions>,
}

impl From<&DeviceInfo> for PeerInfo {
    fn from(info: &DeviceInfo) -> Self {
        Self {
            alias: info.alias.clone(),
            version: info.version.clone(),
            device_model: info.device_model.clone(),
            device_type: info.device_type,
            fingerprint: info.fingerprint.clone(),
            download: info.download,
            ext: info.ext.clone(),
        }
    }
}

/// The UDP multicast datagram: a [`DeviceInfo`] plus the legacy `announce`
/// flag. Official 1.18 ignores the flag; older clients only answer when it
/// is `true`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MulticastAnnouncement {
    #[serde(flatten)]
    pub info: DeviceInfo,

    #[serde(default = "default_true")]
    pub announce: bool,
}

const fn default_true() -> bool {
    true
}

/// Metadata of one offered file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDto {
    /// Sender-chosen id, unique within the request.
    pub id: String,

    /// File name; may contain `/`-separated directory components for folder
    /// transfers. Untrusted: sanitize before use.
    pub file_name: String,

    /// Size in bytes. The receiver enforces it exactly.
    pub size: u64,

    /// MIME type, e.g. `image/png`; `application/octet-stream` when unknown.
    pub file_type: String,

    /// Lowercase hex SHA-256 of the content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,

    /// For a single `text/*` file: the message text itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,

    /// Timestamps, applied to the received file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<FileMetadata>,
}

/// File timestamps as RFC 3339 strings.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accessed: Option<String>,
}

impl FileMetadata {
    /// The timestamps of the file at `path`, or `None` when it has none.
    pub fn from_path(path: &Path) -> Option<Self> {
        let metadata = std::fs::metadata(path).ok()?;
        let modified = metadata.modified().ok().and_then(timestamp::format);
        let accessed = metadata.accessed().ok().and_then(timestamp::format);
        if modified.is_none() && accessed.is_none() {
            return None;
        }
        Some(Self { modified, accessed })
    }

    /// `modified` parsed, or `None` when absent or unparsable.
    pub fn modified_time(&self) -> Option<SystemTime> {
        timestamp::parse(self.modified.as_deref()?)
    }

    /// `accessed` parsed, or `None` when absent or unparsable.
    pub fn accessed_time(&self) -> Option<SystemTime> {
        timestamp::parse(self.accessed.as_deref()?)
    }
}

/// Body of `POST /prepare-upload`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrepareUploadRequest {
    pub info: DeviceInfo,
    /// File id → file.
    pub files: HashMap<String, FileDto>,
}

/// Response of `POST /prepare-upload` (status 200).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareUploadResponse {
    pub session_id: String,
    /// File id → upload token, only for the accepted files.
    pub files: HashMap<String, String>,

    /// Resume extension: the token to send with `Range` uploads and resume
    /// queries. Only present for senders that announced `resume`.
    #[serde(
        default,
        rename = "x-resume-token",
        skip_serializing_if = "Option::is_none"
    )]
    pub resume_token: Option<String>,

    /// Resume extension: file id → bytes the receiver already holds.
    #[serde(
        default,
        rename = "x-resume-offsets",
        skip_serializing_if = "Option::is_none"
    )]
    pub resume_offsets: Option<HashMap<String, u64>>,
}

impl PrepareUploadResponse {
    /// Bytes the receiver already has for `file_id` (0 when unknown).
    pub fn resume_offset(&self, file_id: &str) -> u64 {
        self.resume_offsets
            .as_ref()
            .and_then(|offsets| offsets.get(file_id).copied())
            .unwrap_or(0)
    }
}

/// Response of `GET /api/ext/v1/resume` (resume extension).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumeOffsetResponse {
    /// Bytes the receiver holds for the file.
    pub offset: u64,
}

/// Header carrying the resume token on `Range` uploads and resume queries.
pub const RESUME_TOKEN_HEADER: &str = "x-resume-token";

/// Header on a 416 response: the offset the receiver expects.
pub const RESUME_OFFSET_HEADER: &str = "x-resume-offset";

/// Path of the resume query endpoint.
pub const RESUME_PATH: &str = "/api/ext/v1/resume";

/// Error body used by every route.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> DeviceInfo {
        DeviceInfo {
            alias: "Nice Orange".into(),
            version: PROTOCOL_VERSION.into(),
            device_model: Some("macOS".into()),
            device_type: Some(DeviceType::Desktop),
            fingerprint: "ABCD".into(),
            port: 53317,
            protocol: ProtocolType::Https,
            download: false,
            ext: Some(Extensions::current([FEATURE_RESUME])),
        }
    }

    #[test]
    fn device_info_round_trips_with_extension() {
        let json = serde_json::to_value(info()).unwrap();
        assert_eq!(json["deviceType"], "desktop");
        assert_eq!(json["protocol"], "https");
        assert_eq!(json["x-lanext"]["v"], 1);
        assert_eq!(json["x-lanext"]["features"][0], "resume");
        let back: DeviceInfo = serde_json::from_value(json).unwrap();
        assert_eq!(back, info());
    }

    #[test]
    fn official_payload_without_extension_parses() {
        let json = r#"{"alias":"Secret Banana","version":"2.0","deviceModel":"Windows",
            "deviceType":"desktop","fingerprint":"random string","port":53317,
            "protocol":"https","download":true,"someFutureField":1}"#;
        let info: DeviceInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.alias, "Secret Banana");
        assert!(info.download);
        assert!(info.ext.is_none());
        let out = serde_json::to_string(&info).unwrap();
        assert!(!out.contains("x-lanext"));
    }

    #[test]
    fn unknown_device_type_falls_back_to_desktop() {
        let json = r#"{"alias":"a","version":"2.2","deviceType":"fridge",
            "fingerprint":"x","port":1,"protocol":"http"}"#;
        let info: DeviceInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.device_type, Some(DeviceType::Desktop));
        let json = r#"{"alias":"a","version":"2.2","deviceType":null,
            "fingerprint":"x","port":1,"protocol":"http"}"#;
        let info: DeviceInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.device_type, None);
    }

    #[test]
    fn multicast_announcement_flattens_info() {
        let announcement = MulticastAnnouncement {
            info: info(),
            announce: true,
        };
        let json = serde_json::to_value(&announcement).unwrap();
        assert_eq!(json["announce"], true);
        assert_eq!(json["alias"], "Nice Orange");
        let back: MulticastAnnouncement = serde_json::from_value(json).unwrap();
        assert_eq!(back, announcement);
        let without_flag =
            r#"{"alias":"a","version":"2.2","fingerprint":"x","port":1,"protocol":"https"}"#;
        let back: MulticastAnnouncement = serde_json::from_str(without_flag).unwrap();
        assert!(back.announce);
    }

    #[test]
    fn file_dto_skips_absent_optionals() {
        let file = FileDto {
            id: "1".into(),
            file_name: "a.txt".into(),
            size: 3,
            file_type: "text/plain".into(),
            sha256: None,
            preview: None,
            metadata: None,
        };
        let json = serde_json::to_string(&file).unwrap();
        assert_eq!(
            json,
            r#"{"id":"1","fileName":"a.txt","size":3,"fileType":"text/plain"}"#
        );
    }

    #[test]
    fn metadata_timestamps_parse() {
        let metadata = FileMetadata {
            modified: Some("2000-01-01T00:00:00Z".into()),
            accessed: Some("garbage".into()),
        };
        assert_eq!(
            metadata.modified_time(),
            Some(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(946_684_800))
        );
        assert_eq!(metadata.accessed_time(), None);
    }

    #[test]
    fn prepare_upload_response_uses_camel_case() {
        let response = PrepareUploadResponse {
            session_id: "s".into(),
            files: HashMap::from([("f".to_string(), "t".to_string())]),
            resume_token: None,
            resume_offsets: None,
        };
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(json, r#"{"sessionId":"s","files":{"f":"t"}}"#);

        let official = r#"{"sessionId":"s","files":{"f":"t"}}"#;
        let parsed: PrepareUploadResponse = serde_json::from_str(official).unwrap();
        assert_eq!(parsed.resume_offset("f"), 0);

        let resumable = PrepareUploadResponse {
            resume_token: Some("tok".into()),
            resume_offsets: Some(HashMap::from([("f".to_string(), 7u64)])),
            ..response
        };
        let json = serde_json::to_value(&resumable).unwrap();
        assert_eq!(json["x-resume-token"], "tok");
        assert_eq!(json["x-resume-offsets"]["f"], 7);
        let back: PrepareUploadResponse = serde_json::from_value(json).unwrap();
        assert_eq!(back.resume_offset("f"), 7);
    }
}
