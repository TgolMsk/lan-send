//! Turning local paths into the files of a transfer.

use crate::protocol::{FileDto, FileMetadata};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollectOptions {
    /// Skip dot-files and hidden files inside folders.
    pub skip_hidden: bool,
}

impl Default for CollectOptions {
    fn default() -> Self {
        Self { skip_hidden: true }
    }
}

/// One file to send.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutgoingFile {
    /// Protocol file id, unique within the transfer.
    pub id: String,
    /// Where to read it from.
    pub path: PathBuf,
    /// The name sent to the peer: the file name, or `<folder>/<relative>`
    /// with `/` separators for files inside a folder.
    pub name: String,
    pub size: u64,
    pub mime: String,
    pub metadata: Option<FileMetadata>,
}

impl OutgoingFile {
    pub fn to_dto(&self, sha256: Option<String>) -> FileDto {
        FileDto {
            id: self.id.clone(),
            file_name: self.name.clone(),
            size: self.size,
            file_type: self.mime.clone(),
            sha256,
            preview: None,
            metadata: self.metadata.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkipReason {
    /// Symbolic links inside folders are never followed.
    Symlink,
    Hidden,
    /// Sockets, pipes, devices.
    Special,
    Unreadable(String),
}

impl std::fmt::Display for SkipReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Symlink => f.write_str("symbolic link"),
            Self::Hidden => f.write_str("hidden"),
            Self::Special => f.write_str("not a regular file"),
            Self::Unreadable(reason) => write!(f, "unreadable: {reason}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skipped {
    pub path: PathBuf,
    pub reason: SkipReason,
}

/// The result of collecting: what will be sent and what was left out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outgoing {
    pub files: Vec<OutgoingFile>,
    pub skipped: Vec<Skipped>,
    pub total_size: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum CollectError {
    #[error("{0} does not exist")]
    Missing(PathBuf),

    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("nothing to send")]
    Empty,
}

/// Collects the files under `paths`. Paths given here are followed even
/// when they are symbolic links (the user pointed at them); links found
/// inside folders are skipped.
pub fn collect(paths: &[PathBuf], options: &CollectOptions) -> Result<Outgoing, CollectError> {
    let mut outgoing = Outgoing::default();
    for path in paths {
        let metadata = std::fs::metadata(path).map_err(|source| match source.kind() {
            std::io::ErrorKind::NotFound => CollectError::Missing(path.clone()),
            _ => CollectError::Io {
                path: path.clone(),
                source,
            },
        })?;
        if metadata.is_dir() {
            collect_folder(path, options, &mut outgoing);
        } else if metadata.is_file() {
            let name = file_name_of(path);
            push_file(path, name, metadata.len(), &mut outgoing);
        } else {
            outgoing.skipped.push(Skipped {
                path: path.clone(),
                reason: SkipReason::Special,
            });
        }
    }
    if outgoing.files.is_empty() {
        return Err(CollectError::Empty);
    }
    outgoing.files.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(outgoing)
}

fn collect_folder(root: &Path, options: &CollectOptions, outgoing: &mut Outgoing) {
    let root_name = file_name_of(root);
    let mut walker = walkdir::WalkDir::new(root)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter();
    while let Some(entry) = walker.next() {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                let path = err
                    .path()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| root.to_path_buf());
                outgoing.skipped.push(Skipped {
                    path,
                    reason: SkipReason::Unreadable(err.to_string()),
                });
                continue;
            }
        };
        if entry.depth() == 0 {
            continue;
        }
        let path = entry.path();
        if entry.path_is_symlink() {
            outgoing.skipped.push(Skipped {
                path: path.to_path_buf(),
                reason: SkipReason::Symlink,
            });
            if entry.file_type().is_dir() {
                walker.skip_current_dir();
            }
            continue;
        }
        if options.skip_hidden && platform::is_hidden(&entry) {
            outgoing.skipped.push(Skipped {
                path: path.to_path_buf(),
                reason: SkipReason::Hidden,
            });
            if entry.file_type().is_dir() {
                walker.skip_current_dir();
            }
            continue;
        }
        if !entry.file_type().is_file() {
            if !entry.file_type().is_dir() {
                outgoing.skipped.push(Skipped {
                    path: path.to_path_buf(),
                    reason: SkipReason::Special,
                });
            }
            continue;
        }
        let size = match entry.metadata() {
            Ok(metadata) => metadata.len(),
            Err(err) => {
                outgoing.skipped.push(Skipped {
                    path: path.to_path_buf(),
                    reason: SkipReason::Unreadable(err.to_string()),
                });
                continue;
            }
        };
        let relative = path
            .strip_prefix(root)
            .map(|rel| {
                rel.components()
                    .map(|component| component.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/")
            })
            .unwrap_or_else(|_| file_name_of(path));
        push_file(path, format!("{root_name}/{relative}"), size, outgoing);
    }
}

fn push_file(path: &Path, name: String, size: u64, outgoing: &mut Outgoing) {
    outgoing.total_size += size;
    outgoing.files.push(OutgoingFile {
        id: uuid::Uuid::new_v4().to_string(),
        path: path.to_path_buf(),
        name,
        size,
        mime: crate::media::sniff_mime(path),
        metadata: FileMetadata::from_path(path),
    });
}

fn file_name_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// Hidden-file detection: a leading dot everywhere, plus the hidden
/// attribute on Windows. The only platform-specific code in this module.
mod platform {
    pub(super) fn is_hidden(entry: &walkdir::DirEntry) -> bool {
        entry.file_name().to_string_lossy().starts_with('.') || has_hidden_attribute(entry)
    }

    #[cfg(windows)]
    fn has_hidden_attribute(entry: &walkdir::DirEntry) -> bool {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        entry
            .metadata()
            .map(|metadata| metadata.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0)
            .unwrap_or(false)
    }

    #[cfg(not(windows))]
    fn has_hidden_attribute(_entry: &walkdir::DirEntry) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_folders_with_relative_names() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("photos");
        std::fs::create_dir_all(root.join("2024/.cache")).unwrap();
        std::fs::write(root.join("a.jpg"), b"aaa").unwrap();
        std::fs::write(root.join("2024/b.png"), b"bb").unwrap();
        std::fs::write(root.join("2024/.hidden"), b"h").unwrap();
        std::fs::write(root.join("2024/.cache/c"), b"c").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(root.join("a.jpg"), root.join("link.jpg")).unwrap();
        let single = dir.path().join("note.txt");
        std::fs::write(&single, b"hello").unwrap();

        let outgoing = collect(&[root.clone(), single], &CollectOptions::default()).unwrap();
        let names: Vec<&str> = outgoing.files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["note.txt", "photos/2024/b.png", "photos/a.jpg"]);
        assert_eq!(outgoing.total_size, 3 + 2 + 5);
        assert_eq!(outgoing.files[0].mime, "text/plain");
        assert_eq!(outgoing.files[1].mime, "image/png");
        let reasons: Vec<&SkipReason> = outgoing.skipped.iter().map(|s| &s.reason).collect();
        assert!(reasons.contains(&&SkipReason::Hidden));
        #[cfg(unix)]
        assert!(reasons.contains(&&SkipReason::Symlink));

        let with_hidden = collect(&[root], &CollectOptions { skip_hidden: false }).unwrap();
        assert_eq!(with_hidden.files.len(), 4);
    }

    #[test]
    fn errors() {
        assert!(matches!(
            collect(
                &[PathBuf::from("/definitely/missing")],
                &CollectOptions::default()
            ),
            Err(CollectError::Missing(_))
        ));
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            collect(&[dir.path().to_path_buf()], &CollectOptions::default()),
            Err(CollectError::Empty)
        ));
    }
}
