//! Streaming an upload body to disk with size and checksum enforcement,
//! optionally appending to a partial file (resume extension).

use axum::body::Body;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter};

const WRITE_BUFFER: usize = 512 * 1024;

/// Outcome of receiving one file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveOutcome {
    /// Received completely; the checksum matched if one was given.
    Success,
    /// The connection dropped or the sender sent too few bytes. `received`
    /// is what the part file holds; it is kept when the session can be
    /// resumed.
    Interrupted { received: u64, reason: String },
    /// Could not be written, or too many bytes arrived; the part file is
    /// removed.
    Failed(String),
    /// Received completely but the SHA-256 did not match. The partial file
    /// is kept for a retry with the same token.
    HashMismatch,
    /// A `Range` upload started at an offset other than the part file's
    /// length; nothing was written.
    OffsetMismatch { expected: u64 },
}

impl SaveOutcome {
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success)
    }

    /// Bytes of the file that are on disk after this attempt.
    pub fn received(&self) -> Option<u64> {
        match self {
            Self::Interrupted { received, .. } => Some(*received),
            Self::OffsetMismatch { expected } => Some(*expected),
            _ => None,
        }
    }
}

impl std::fmt::Display for SaveOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Success => f.write_str("received"),
            Self::Interrupted { received, reason } => {
                write!(f, "interrupted after {received} bytes: {reason}")
            }
            Self::Failed(reason) => f.write_str(reason),
            Self::HashMismatch => f.write_str("checksum mismatch"),
            Self::OffsetMismatch { expected } => write!(f, "expected upload offset {expected}"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Timestamps {
    pub modified: Option<SystemTime>,
    pub accessed: Option<SystemTime>,
}

/// What to do with the part file.
#[derive(Clone, Copy, Debug)]
pub(super) struct SaveOptions<'a> {
    pub expected_size: u64,
    pub expected_sha256: Option<&'a str>,
    pub timestamps: Timestamps,
    /// `Some(offset)`: append after `offset` bytes, which must be exactly
    /// what the part file holds. `None`: start from scratch.
    pub resume_from: Option<u64>,
    /// Keep the part file when the upload is interrupted (resumable session).
    pub keep_partial: bool,
    /// No data for this long counts as an interruption: a sender that
    /// vanished without closing the connection would otherwise hold the file
    /// until the TCP stack gives up.
    pub idle_timeout: std::time::Duration,
}

/// The temporary file an upload is written to before it is renamed.
pub fn part_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    path.with_file_name(format!("{name}.lan-send.part"))
}

/// Bytes the part file of `path` holds, 0 when there is none.
pub async fn partial_length(path: &Path) -> u64 {
    tokio::fs::metadata(part_path(path))
        .await
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}

/// Writes `body` to `path` (via its part file) according to `options`;
/// `progress` receives the running byte count including any resumed prefix.
pub(super) async fn save_body(
    body: Body,
    path: &Path,
    options: SaveOptions<'_>,
    mut progress: impl FnMut(u64),
) -> SaveOutcome {
    let part = part_path(path);
    if let Some(parent) = path.parent()
        && let Err(err) = tokio::fs::create_dir_all(parent).await
    {
        return SaveOutcome::Failed(format!("could not create {}: {err}", parent.display()));
    }

    let existing = tokio::fs::metadata(&part)
        .await
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let offset = options.resume_from.unwrap_or(0);
    if options.resume_from.is_some() && offset != existing {
        return SaveOutcome::OffsetMismatch { expected: existing };
    }

    let mut hasher = options.expected_sha256.map(|_| Sha256::new());
    let file = if offset > 0 {
        // Hash the prefix that is already on disk, then append.
        let open = tokio::fs::OpenOptions::new()
            .read(true)
            .append(true)
            .open(&part)
            .await;
        let mut file = match open {
            Ok(file) => file,
            Err(err) => {
                return SaveOutcome::Failed(format!("could not open {}: {err}", part.display()));
            }
        };
        if let Some(hasher) = &mut hasher
            && let Err(err) = hash_prefix(&mut file, offset, hasher).await
        {
            return SaveOutcome::Failed(format!("could not read {}: {err}", part.display()));
        }
        file
    } else {
        match tokio::fs::File::create(&part).await {
            Ok(file) => file,
            Err(err) => {
                return SaveOutcome::Failed(format!("could not create {}: {err}", part.display()));
            }
        }
    };

    let outcome = write_stream(body, file, offset, &options, hasher, &mut progress).await;
    match &outcome {
        SaveOutcome::Success => {}
        SaveOutcome::HashMismatch | SaveOutcome::OffsetMismatch { .. } => return outcome,
        SaveOutcome::Interrupted { .. } if options.keep_partial => return outcome,
        SaveOutcome::Interrupted { .. } | SaveOutcome::Failed(_) => {
            let _ = tokio::fs::remove_file(&part).await;
            return outcome;
        }
    }

    apply_timestamps(&part, options.timestamps);
    match tokio::fs::rename(&part, path).await {
        Ok(()) => SaveOutcome::Success,
        Err(err) => SaveOutcome::Failed(format!("could not move into place: {err}")),
    }
}

async fn hash_prefix(
    file: &mut tokio::fs::File,
    length: u64,
    hasher: &mut Sha256,
) -> std::io::Result<()> {
    let mut remaining = length;
    let mut buffer = vec![0u8; WRITE_BUFFER];
    while remaining > 0 {
        let want = usize::try_from(remaining.min(buffer.len() as u64)).unwrap_or(buffer.len());
        let read = file.read(&mut buffer[..want]).await?;
        if read == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "part file shorter than expected",
            ));
        }
        hasher.update(&buffer[..read]);
        remaining -= read as u64;
    }
    Ok(())
}

async fn write_stream(
    body: Body,
    file: tokio::fs::File,
    offset: u64,
    options: &SaveOptions<'_>,
    mut hasher: Option<Sha256>,
    progress: &mut impl FnMut(u64),
) -> SaveOutcome {
    let expected_size = options.expected_size;
    let mut writer = BufWriter::with_capacity(WRITE_BUFFER, file);
    let mut written = offset;
    let mut stream = body.into_data_stream();

    loop {
        let next = match tokio::time::timeout(options.idle_timeout, stream.next()).await {
            Ok(next) => next,
            Err(_) => {
                let _ = writer.flush().await;
                return SaveOutcome::Interrupted {
                    received: written,
                    reason: format!("no data for {} s", options.idle_timeout.as_secs()),
                };
            }
        };
        let Some(chunk) = next else {
            break;
        };
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(err) => {
                let _ = writer.flush().await;
                return SaveOutcome::Interrupted {
                    received: written,
                    reason: format!("upload aborted: {err}"),
                };
            }
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
        return SaveOutcome::Interrupted {
            received: written,
            reason: format!("Expected {expected_size} bytes, received {written}"),
        };
    }
    if let (Some(hasher), Some(expected)) = (hasher, options.expected_sha256) {
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

    const CONTENT: &[u8] = b"hello world";
    const SHA: &str = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";

    fn options(expected_size: u64, sha: Option<&str>) -> SaveOptions<'_> {
        SaveOptions {
            expected_size,
            expected_sha256: sha,
            timestamps: Timestamps::default(),
            resume_from: None,
            keep_partial: false,
            idle_timeout: std::time::Duration::from_secs(30),
        }
    }

    #[tokio::test]
    async fn enforces_size_and_checksum() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("file.bin");
        let outcome = save_body(
            Body::from(CONTENT),
            &path,
            options(CONTENT.len() as u64, Some(SHA)),
            |_| {},
        )
        .await;
        assert_eq!(outcome, SaveOutcome::Success);
        assert_eq!(std::fs::read(&path).unwrap(), CONTENT);

        let short_path = dir.path().join("short.bin");
        let short = save_body(
            Body::from(CONTENT),
            &short_path,
            options(CONTENT.len() as u64 + 1, None),
            |_| {},
        )
        .await;
        assert!(matches!(
            short,
            SaveOutcome::Interrupted { received: 11, .. }
        ));
        assert!(!part_path(&short_path).exists());

        let mismatch_path = dir.path().join("bad.bin");
        let mismatch = save_body(
            Body::from(CONTENT),
            &mismatch_path,
            options(CONTENT.len() as u64, Some("00")),
            |_| {},
        )
        .await;
        assert_eq!(mismatch, SaveOutcome::HashMismatch);
        assert!(part_path(&mismatch_path).exists());
        assert!(!mismatch_path.exists());
    }

    #[tokio::test]
    async fn resumes_from_partial_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file.bin");
        let size = CONTENT.len() as u64;

        let interrupted = save_body(
            Body::from(&CONTENT[..5]),
            &path,
            SaveOptions {
                keep_partial: true,
                ..options(size, Some(SHA))
            },
            |_| {},
        )
        .await;
        assert!(matches!(
            interrupted,
            SaveOutcome::Interrupted { received: 5, .. }
        ));
        assert_eq!(partial_length(&path).await, 5);

        let wrong_offset = save_body(
            Body::from(&CONTENT[3..]),
            &path,
            SaveOptions {
                resume_from: Some(3),
                keep_partial: true,
                ..options(size, Some(SHA))
            },
            |_| {},
        )
        .await;
        assert_eq!(wrong_offset, SaveOutcome::OffsetMismatch { expected: 5 });
        assert_eq!(partial_length(&path).await, 5);

        let mut seen = Vec::new();
        let resumed = save_body(
            Body::from(&CONTENT[5..]),
            &path,
            SaveOptions {
                resume_from: Some(5),
                keep_partial: true,
                ..options(size, Some(SHA))
            },
            |received| seen.push(received),
        )
        .await;
        assert_eq!(resumed, SaveOutcome::Success);
        assert_eq!(std::fs::read(&path).unwrap(), CONTENT);
        assert_eq!(seen.last(), Some(&size));
    }
}
