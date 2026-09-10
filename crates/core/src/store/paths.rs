//! Where lan-send keeps its files.

use directories::{ProjectDirs, UserDirs};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("no home directory available to place the configuration in")]
    NoHome,

    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid settings file {path}: {source}")]
    Settings {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
}

/// The directories used by this application.
///
/// Resolved with the `directories` crate: on macOS
/// `~/Library/Application Support/lan-send`, on Windows `%APPDATA%\lan-send`,
/// on Linux `~/.config/lan-send`.
#[derive(Clone, Debug)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    /// The user's Downloads directory, when the platform has one.
    pub download_dir: Option<PathBuf>,
}

impl AppPaths {
    /// The application name used for every directory.
    pub const APP_NAME: &'static str = "lan-send";

    /// The platform-standard locations.
    pub fn resolve() -> Result<Self, StoreError> {
        let dirs = ProjectDirs::from("", "", Self::APP_NAME).ok_or(StoreError::NoHome)?;
        Ok(Self {
            config_dir: dirs.config_dir().to_path_buf(),
            data_dir: dirs.data_dir().to_path_buf(),
            cache_dir: dirs.cache_dir().to_path_buf(),
            download_dir: download_dir(),
        })
    }

    /// Everything under one root, for tests and the `--config-dir` flag.
    pub fn under(root: &Path) -> Self {
        Self {
            config_dir: root.join("config"),
            data_dir: root.join("data"),
            cache_dir: root.join("cache"),
            download_dir: download_dir(),
        }
    }

    /// Creates the config, data and cache directories.
    pub fn ensure_dirs(&self) -> Result<(), StoreError> {
        for dir in [&self.config_dir, &self.data_dir, &self.cache_dir] {
            std::fs::create_dir_all(dir).map_err(|source| StoreError::Io {
                path: dir.clone(),
                source,
            })?;
        }
        Ok(())
    }

    /// The file holding this device's certificate and private key.
    pub fn identity_file(&self) -> PathBuf {
        self.config_dir.join("identity.pem")
    }

    /// The user-editable settings.
    pub fn settings_file(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }

    /// The SQLite database with history, devices and partial uploads.
    pub fn database_file(&self) -> PathBuf {
        self.data_dir.join("lan-send.sqlite3")
    }
}

/// Where received files go by default.
///
/// iOS has no Downloads directory. The `directories` crate answers with the
/// macOS layout there (`$HOME/Downloads`), which inside an app container is a
/// path the sandbox refuses to create — the container root is read-only, so
/// every transfer failed with `Operation not permitted`. The simulator does
/// not reproduce it because its container root is an ordinary directory.
/// Documents is the right answer on iOS: it exists, it is writable, and
/// `UIFileSharingEnabled` exposes it in the Files app.
fn download_dir() -> Option<PathBuf> {
    // macOS: the App Sandbox redirects `$HOME` into the container, so the
    // `directories` crate would answer with a Downloads folder buried inside
    // it. Ask the system for the real home instead; the sandboxed build
    // reaches it through `com.apple.security.files.downloads.read-write`.
    #[cfg(target_os = "macos")]
    match super::platform::macos::real_home_dir() {
        Some(home) => return Some(home.join("Downloads")),
        // Never fall back silently: inside the sandbox the generic lookup
        // yields a container path, which is exactly the situation App Review
        // flagged. Say so, then let the generic lookup answer.
        None => {
            tracing::warn!("could not resolve the account home directory; falling back to $HOME")
        }
    }
    let dirs = UserDirs::new()?;
    pick_download_dir(
        dirs.download_dir(),
        dirs.document_dir(),
        cfg!(target_os = "ios"),
    )
}

/// The choice behind [`download_dir`], separated so it can be tested on any
/// platform.
fn pick_download_dir(
    download: Option<&Path>,
    document: Option<&Path>,
    is_ios: bool,
) -> Option<PathBuf> {
    if is_ios {
        return document.map(Path::to_path_buf);
    }
    download.or(document).map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ios_receives_into_documents_not_downloads() {
        let downloads = PathBuf::from("/container/Downloads");
        let documents = PathBuf::from("/container/Documents");
        // The container root is read-only on iOS, so `Downloads` must not win
        // even though the `directories` crate offers it.
        assert_eq!(
            pick_download_dir(Some(&downloads), Some(&documents), true),
            Some(documents.clone())
        );
        assert_eq!(
            pick_download_dir(Some(&downloads), Some(&documents), false),
            Some(downloads)
        );
    }

    #[test]
    fn falls_back_to_documents_then_nothing() {
        let documents = PathBuf::from("/home/me/Documents");
        assert_eq!(
            pick_download_dir(None, Some(&documents), false),
            Some(documents)
        );
        assert_eq!(pick_download_dir(None, None, false), None);
        assert_eq!(pick_download_dir(None, None, true), None);
    }
}
