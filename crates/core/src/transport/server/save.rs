//! Streaming an upload body to disk with size and checksum enforcement.

use axum::body::Body;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tokio::io::{AsyncWriteExt, BufWriter};

const WRITE_BUFFER: usize = 512 * 1024;

/// Outcome of receiving one file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveOutcome {
    /// Received completely; the checksum matched if one was given.
    Success,
    /// Could not be received or written; the reason is for logs and the UI.
    Failed(String),
    /// Received completely but the SHA-256 did not match. The partial file
    /// is kept for a retry with the same token.
    HashMismatch,
}

impl SaveOutcome {
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Timestamps {
    pub modified: Option<SystemTime>,
    pub accessed: Option<SystemTime>,
}

/// The temporary file an upload is written to before it is renamed.
pub(super) fn part_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    path.with_file_name(format!("{name}.lan-send.part"))
}

/// Writes `body` to `path` (via its part file), enforcing that exactly
/// `expected_size` bytes arrive and, when given, that they hash to
/// `expected_sha256` (hex, any case). `progress` receives the running byte
/// count.
pub(super) async fn save_body(
    body: Body,
    path: &Path,
    expected_size: u64,
    expected_sha256: Option<&str>,
    timestamps: Timestamps,
    mut progress: impl FnMut(u64),
) -> SaveOutcome {
    let part = part_path(path);
    if let Some(parent) = path.parent()
        && let Err(err) = tokio::fs::create_dir_all(parent).await
    {
        return SaveOutcome::Failed(format!("could not create {}: {err}", parent.display()));
    }
    let file = match tokio::fs::File::create(&part).await {
        Ok(file) => file,
        Err(err) => {
            return SaveOutcome::Failed(format!("could not create {}: {err}", part.display()));
        }
    };

    let outcome = write_stream(body, file, expected_size, expected_sha256, &mut progress).await;
    match outcome {
        SaveOutcome::Success => {}
        SaveOutcome::HashMismatch => return outcome,
        SaveOutcome::Failed(_) => {
            let _ = tokio::fs::remove_file(&part).await;
            return outcome;
        }
    }

    apply_timestamps(&part, timestamps);
    match tokio::fs::rename(&part, path).await {
        Ok(()) => SaveOutcome::Success,
        Err(err) => SaveOutcome::Failed(format!("could not move into place: {err}")),
    }
}

async fn write_stream(
    body: Body,
    file: tokio::fs::File,
    expected_size: u64,
    expected_sha256: Option<&str>,
    progress: &mut impl FnMut(u64),
) -> SaveOutcome {
    let mut writer = BufWriter::with_capacity(WRITE_BUFFER, file);
    let mut hasher = expected_sha256.map(|_| Sha256::new());
    let mut written = 0u64;
    let mut stream = body.into_data_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(err) => return SaveOutcome::Failed(format!("upload aborted: {err}")),
        };
        if chunk.is_empty() {
            continue;
        }
        written += chunk.len() as u64;
        if written > expected_size {
            return SaveOutcome::Failed(format!(
                "Expected {expected_size} bytes, received at least {written}"
            ));
        }
        if let Some(hasher) = &mut hasher {
            hasher.update(&chunk);
        }
        if let Err(err) = writer.write_all(&chunk).await {
            return SaveOutcome::Failed(format!("Failed to write file: {err}"));
        }
        progress(written);
    }

    if let Err(err) = writer.flush().await {
        return SaveOutcome::Failed(format!("Failed to flush file: {err}"));
    }
    if written != expected_size {
        return SaveOutcome::Failed(format!(
            "Expected {expected_size} bytes, received {written}"
        ));
    }
    if let (Some(hasher), Some(expected)) = (hasher, expected_sha256) {
        let actual: String = hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        if !actual.eq_ignore_ascii_case(expected.trim()) {
            tracing::warn!("checksum mismatch: expected {expected}, got {actual}");
            return SaveOutcome::HashMismatch;
        }
    }
    SaveOutcome::Success
}

/// Best effort: a file whose timestamps could not be set is still received.
fn apply_timestamps(path: &Path, timestamps: Timestamps) {
    if timestamps.modified.is_none() && timestamps.accessed.is_none() {
        return;
    }
    let mut times = std::fs::FileTimes::new();
    if let Some(modified) = timestamps.modified {
        times = times.set_modified(modified);
    }
    if let Some(accessed) = timestamps.accessed {
        times = times.set_accessed(accessed);
    }
    match std::fs::File::options().write(true).open(path) {
        Ok(file) => {
            if let Err(err) = file.set_times(times) {
                tracing::debug!("could not set timestamps on {}: {err}", path.display());
            }
        }
        Err(err) => tracing::debug!("could not open {} for timestamps: {err}", path.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn enforces_size_and_checksum() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("file.bin");
        let content = b"hello world";
        let sha = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";

        let outcome = save_body(
            Body::from(&content[..]),
            &path,
            content.len() as u64,
            Some(sha),
            Timestamps::default(),
            |_| {},
        )
        .await;
        assert_eq!(outcome, SaveOutcome::Success);
        assert_eq!(std::fs::read(&path).unwrap(), content);

        let short = save_body(
            Body::from(&content[..]),
            &dir.path().join("short.bin"),
            (content.len() + 1) as u64,
            None,
            Timestamps::default(),
            |_| {},
        )
        .await;
        assert!(matches!(short, SaveOutcome::Failed(_)));
        assert!(!part_path(&dir.path().join("short.bin")).exists());

        let mismatch_path = dir.path().join("bad.bin");
        let mismatch = save_body(
            Body::from(&content[..]),
            &mismatch_path,
            content.len() as u64,
            Some("00"),
            Timestamps::default(),
            |_| {},
        )
        .await;
        assert_eq!(mismatch, SaveOutcome::HashMismatch);
        assert!(part_path(&mismatch_path).exists());
        assert!(!mismatch_path.exists());
    }
}
