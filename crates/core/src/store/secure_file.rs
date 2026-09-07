//! Writing files that contain secrets (private keys) with owner-only
//! permissions. This module is the only place where the permission model
//! differs by platform.

use std::path::Path;

/// Writes `contents` to `path`, creating or truncating it, readable only by
/// the current user where the platform supports it.
pub(crate) fn write_private(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    platform::write_private(path, contents)
}

#[cfg(unix)]
mod platform {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    use std::path::Path;

    pub(super) fn write_private(path: &Path, contents: &[u8]) -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(contents)
    }
}

#[cfg(not(unix))]
mod platform {
    use std::path::Path;

    /// Windows: the user profile directory is already private to the user.
    pub(super) fn write_private(path: &Path, contents: &[u8]) -> std::io::Result<()> {
        std::fs::write(path, contents)
    }
}
