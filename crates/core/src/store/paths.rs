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
}

fn download_dir() -> Option<PathBuf> {
    UserDirs::new().and_then(|dirs| dirs.download_dir().map(Path::to_path_buf))
}
