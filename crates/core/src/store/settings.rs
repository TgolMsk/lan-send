//! `settings.json`: the user's preferences. Missing fields take their
//! defaults and unknown fields are ignored, so older and newer versions can
//! share the file.

use super::StoreError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// How received files are sorted into sub-directories of the receive
/// directory. Rules combine in the order device / date / type.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct OrganizeRules {
    /// `<receive dir>/<sender alias>/…`
    pub by_device: bool,
    /// `…/<YYYY-MM-DD>/…`
    pub by_date: bool,
    /// `…/<Images|Videos|Audio|Documents|Other>/…`
    pub by_type: bool,
}

impl OrganizeRules {
    pub fn any(self) -> bool {
        self.by_device || self.by_date || self.by_type
    }
}

/// What to do when a received file already exists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConflictPolicy {
    /// Save as `name (1).ext`, `name (2).ext`, …
    #[default]
    Rename,
    /// Replace the existing file.
    Overwrite,
    /// Let the user decide (the application asks; headless tools rename).
    Ask,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Display name; `None` means the host name.
    pub alias: Option<String>,
    pub port: u16,
    /// Require peers to present a client certificate (official 1.18+ does).
    pub require_client_certs: bool,
    /// Where received files go; `None` means the Downloads directory.
    pub receive_dir: Option<PathBuf>,
    pub organize: OrganizeRules,
    pub on_conflict: ConflictPolicy,
    /// Compute SHA-256 of sent files so receivers can verify them.
    pub create_checksums: bool,
    /// Verify sender-provided SHA-256 after receiving.
    pub verify_checksums: bool,
    pub parallel_uploads: usize,
    /// Skip dot-files and hidden files when sending folders.
    pub skip_hidden_files: bool,
    /// Transfer history entries kept; older ones are pruned.
    pub history_limit: usize,
    /// Announce and use the resume extension with peers that support it.
    pub resume: bool,
    /// Join the IPv6 multicast group and listen on IPv6 as well.
    pub ipv6: bool,
    /// PIN senders must know; `None` disables the PIN.
    pub pin: Option<String>,
    pub clipboard: ClipboardSettings,
}

/// Clipboard synchronisation preferences (ADR-0011).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ClipboardSettings {
    /// Largest text synchronised automatically, in bytes.
    pub text_limit: usize,
    /// Largest image synchronised automatically, in bytes.
    pub image_limit: usize,
    /// Clipboard history entries kept.
    pub history_limit: usize,
    /// How often the clipboard is checked for changes on platforms without
    /// change notifications (macOS), in milliseconds.
    pub poll_interval_ms: u64,
    /// Never keep text items in the history (in addition to the secret
    /// heuristic).
    pub never_store_text: bool,
}

impl Default for ClipboardSettings {
    fn default() -> Self {
        Self {
            text_limit: crate::clipboard::DEFAULT_TEXT_LIMIT,
            image_limit: crate::clipboard::DEFAULT_IMAGE_LIMIT,
            history_limit: 50,
            poll_interval_ms: 300,
            never_store_text: false,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            alias: None,
            port: crate::protocol::DEFAULT_PORT,
            require_client_certs: true,
            receive_dir: None,
            organize: OrganizeRules::default(),
            on_conflict: ConflictPolicy::Rename,
            create_checksums: true,
            verify_checksums: true,
            parallel_uploads: 3,
            skip_hidden_files: true,
            history_limit: 200,
            resume: true,
            ipv6: true,
            pin: None,
            clipboard: ClipboardSettings::default(),
        }
    }
}

impl Settings {
    /// Reads `path`; a missing file yields the defaults and is created so
    /// the user has something to edit.
    pub fn load_or_create(path: &Path) -> Result<Self, StoreError> {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).map_err(|source| StoreError::Settings {
                path: path.to_path_buf(),
                source,
            }),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                let settings = Self::default();
                settings.save(path)?;
                Ok(settings)
            }
            Err(source) => Err(StoreError::Io {
                path: path.to_path_buf(),
                source,
            }),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), StoreError> {
        let io = |source| StoreError::Io {
            path: path.to_path_buf(),
            source,
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|source| StoreError::Settings {
            path: path.to_path_buf(),
            source,
        })?;
        std::fs::write(path, text).map_err(io)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let created = Settings::load_or_create(&path).unwrap();
        assert_eq!(created, Settings::default());
        assert!(path.exists());

        let mut changed = created;
        changed.history_limit = 5;
        changed.on_conflict = ConflictPolicy::Overwrite;
        changed.organize.by_date = true;
        changed.save(&path).unwrap();
        assert_eq!(Settings::load_or_create(&path).unwrap(), changed);
    }

    #[test]
    fn partial_and_unknown_fields_are_tolerated() {
        let settings: Settings =
            serde_json::from_str(r#"{"port": 1234, "onConflict": "ask", "future": true}"#).unwrap();
        assert_eq!(settings.port, 1234);
        assert_eq!(settings.on_conflict, ConflictPolicy::Ask);
        assert_eq!(settings.history_limit, 200);
    }
}
