//! Where a received file goes: sub-directory rules and conflict handling.

use crate::protocol::FileDto;
use crate::store::{ConflictPolicy, OrganizeRules};
use crate::transport::filename::{
    PathError, Rules, ensure_within, sanitize_component, sanitize_relative_path, unique_path,
};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// The type folders used by [`OrganizeRules::by_type`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeBucket {
    Images,
    Videos,
    Audio,
    Documents,
    Other,
}

impl TypeBucket {
    /// Classifies by MIME type, falling back to the file extension.
    pub fn of(mime: &str, name: &str) -> Self {
        let mime = mime.to_ascii_lowercase();
        if mime.starts_with("image/") {
            return Self::Images;
        }
        if mime.starts_with("video/") {
            return Self::Videos;
        }
        if mime.starts_with("audio/") {
            return Self::Audio;
        }
        if mime.starts_with("text/") || DOCUMENT_MIMES.iter().any(|doc| mime == *doc) {
            return Self::Documents;
        }
        let extension = name
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase())
            .unwrap_or_default();
        if DOCUMENT_EXTENSIONS.contains(&extension.as_str()) {
            Self::Documents
        } else {
            Self::Other
        }
    }

    /// The folder name.
    pub const fn folder(self) -> &'static str {
        match self {
            Self::Images => "Images",
            Self::Videos => "Videos",
            Self::Audio => "Audio",
            Self::Documents => "Documents",
            Self::Other => "Other",
        }
    }
}

const DOCUMENT_MIMES: &[&str] = &[
    "application/pdf",
    "application/msword",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    "application/vnd.ms-excel",
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    "application/vnd.ms-powerpoint",
    "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    "application/rtf",
    "application/epub+zip",
    "application/json",
];

const DOCUMENT_EXTENSIONS: &[&str] = &[
    "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "md", "rtf", "epub", "pages",
    "numbers", "key", "csv", "json",
];

/// Where a file will be written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Placement {
    /// A path that does not exist yet.
    New(PathBuf),
    /// The existing file at this path will be replaced.
    Replace(PathBuf),
    /// The file exists and the policy is `Ask`: the application decides
    /// between replacing `existing` and writing to `renamed`.
    NeedsDecision { existing: PathBuf, renamed: PathBuf },
}

impl Placement {
    /// The path, taking the safe choice (rename) when a decision is pending.
    pub fn path_or_renamed(&self) -> &Path {
        match self {
            Self::New(path) | Self::Replace(path) => path,
            Self::NeedsDecision { renamed, .. } => renamed,
        }
    }
}

/// The receive directory and its rules.
#[derive(Clone, Debug)]
pub struct Destination {
    root: PathBuf,
    organize: OrganizeRules,
    conflict: ConflictPolicy,
    rules: Rules,
}

impl Destination {
    /// Creates `root` if needed and resolves it.
    pub fn new(
        root: &Path,
        organize: OrganizeRules,
        conflict: ConflictPolicy,
    ) -> std::io::Result<Self> {
        std::fs::create_dir_all(root)?;
        Ok(Self {
            root: root.canonicalize()?,
            organize,
            conflict,
            rules: Rules::current(),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Decides where `file` from `sender_alias` goes. `taken` holds the
    /// paths already assigned in the same session.
    pub fn place(
        &self,
        file: &FileDto,
        sender_alias: &str,
        taken: &HashSet<PathBuf>,
    ) -> Result<Placement, PathError> {
        let relative = sanitize_relative_path(&file.file_name, self.rules)?;
        let mut dir = self.root.clone();
        if self.organize.by_device {
            dir.push(sanitize_component(sender_alias, self.rules));
        }
        if self.organize.by_date {
            dir.push(today());
        }
        if self.organize.by_type {
            dir.push(TypeBucket::of(&file.file_type, &file.file_name).folder());
        }
        let candidate = dir.join(&relative);
        ensure_within(&self.root, &candidate)?;

        let exists = candidate.exists() || taken.contains(&candidate);
        if !exists {
            return Ok(Placement::New(candidate));
        }
        match self.conflict {
            ConflictPolicy::Rename => Ok(Placement::New(unique_path(&dir, &relative, taken))),
            ConflictPolicy::Overwrite => Ok(Placement::Replace(candidate)),
            ConflictPolicy::Ask => Ok(Placement::NeedsDecision {
                renamed: unique_path(&dir, &relative, taken),
                existing: candidate,
            }),
        }
    }
}

/// Today's date as `YYYY-MM-DD` in local time (UTC when the local offset
/// cannot be determined).
fn today() -> String {
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    format!(
        "{:04}-{:02}-{:02}",
        now.year(),
        u8::from(now.month()),
        now.day()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dto(name: &str, mime: &str) -> FileDto {
        FileDto {
            id: "1".into(),
            file_name: name.into(),
            size: 1,
            file_type: mime.into(),
            sha256: None,
            preview: None,
            metadata: None,
        }
    }

    #[test]
    fn type_buckets() {
        assert_eq!(TypeBucket::of("image/png", "a.png"), TypeBucket::Images);
        assert_eq!(
            TypeBucket::of("application/pdf", "a.pdf"),
            TypeBucket::Documents
        );
        assert_eq!(
            TypeBucket::of("application/octet-stream", "a.DOCX"),
            TypeBucket::Documents
        );
        assert_eq!(
            TypeBucket::of("application/zip", "a.zip"),
            TypeBucket::Other
        );
    }

    #[test]
    fn places_with_rules_and_conflicts() {
        let dir = tempfile::tempdir().unwrap();
        let taken = HashSet::new();

        let plain =
            Destination::new(dir.path(), OrganizeRules::default(), ConflictPolicy::Rename).unwrap();
        let placed = plain
            .place(&dto("docs/a.txt", "text/plain"), "Peer", &taken)
            .unwrap();
        assert_eq!(placed, Placement::New(plain.root().join("docs/a.txt")));
        assert!(matches!(
            plain.place(&dto("../x", "text/plain"), "Peer", &taken),
            Err(PathError::Traversal)
        ));

        let organized = Destination::new(
            dir.path(),
            OrganizeRules {
                by_device: true,
                by_date: false,
                by_type: true,
            },
            ConflictPolicy::Overwrite,
        )
        .unwrap();
        // The device folder follows the platform's file name rules (`:` is
        // legal on Linux, replaced on macOS and Windows).
        let placed = organized
            .place(&dto("a.png", "image/png"), "Nice: Orange", &taken)
            .unwrap();
        let device_folder = sanitize_component("Nice: Orange", Rules::current());
        assert_eq!(
            placed,
            Placement::New(organized.root().join(device_folder).join("Images/a.png"))
        );

        std::fs::write(plain.root().join("dup.txt"), b"x").unwrap();
        assert_eq!(
            plain
                .place(&dto("dup.txt", "text/plain"), "Peer", &taken)
                .unwrap(),
            Placement::New(plain.root().join("dup (1).txt"))
        );
        assert_eq!(
            organized
                .place(&dto("dup.txt", "text/plain"), "Peer", &taken)
                .unwrap(),
            Placement::New(organized.root().join("Peer/Documents/dup.txt"))
        );
        let asking =
            Destination::new(dir.path(), OrganizeRules::default(), ConflictPolicy::Ask).unwrap();
        assert_eq!(
            asking
                .place(&dto("dup.txt", "text/plain"), "Peer", &taken)
                .unwrap(),
            Placement::NeedsDecision {
                existing: asking.root().join("dup.txt"),
                renamed: asking.root().join("dup (1).txt"),
            }
        );
        let replacing = Destination::new(
            dir.path(),
            OrganizeRules::default(),
            ConflictPolicy::Overwrite,
        )
        .unwrap();
        assert_eq!(
            replacing
                .place(&dto("dup.txt", "text/plain"), "Peer", &taken)
                .unwrap(),
            Placement::Replace(replacing.root().join("dup.txt"))
        );
    }
}
