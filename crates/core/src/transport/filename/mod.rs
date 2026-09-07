//! Sanitising peer-supplied file names and keeping writes inside the
//! destination directory (ADR-0004). The rules mirror the official receiver
//! so both implementations produce the same names.

mod platform;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Characters illegal on NTFS and FAT.
const ILLEGAL_WINDOWS: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
/// Characters illegal on HFS+/APFS (`:` is the classic Mac separator).
const ILLEGAL_HFS: &[char] = &['/', ':'];
/// Characters illegal on POSIX file systems.
const ILLEGAL_POSIX: &[char] = &['/'];
/// Device names Windows reserves regardless of extension.
const RESERVED_WINDOWS: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];
/// Maximum file name length in bytes (ext4, APFS, HFS+, NTFS).
const MAX_LEN: usize = 255;
/// Used when nothing legal is left of a name.
pub const PLACEHOLDER: &str = "untitled";
/// Substituted for every illegal character.
const REPLACEMENT: &str = "_";

/// Naming rules, selected by destination file system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rules {
    /// NTFS: illegal characters, reserved device names, no trailing `.`/space.
    Windows,
    /// HFS+/APFS: `/` and `:`.
    Hfs,
    /// POSIX: `/` and NUL only.
    Posix,
    /// The intersection of all of the above, for unknown destinations.
    Universal,
}

impl Rules {
    /// The rules of the platform this binary runs on.
    pub const fn current() -> Self {
        platform::current()
    }

    fn is_illegal(self, c: char) -> bool {
        if c.is_control() {
            return true;
        }
        match self {
            Self::Windows => ILLEGAL_WINDOWS.contains(&c),
            Self::Hfs => ILLEGAL_HFS.contains(&c),
            Self::Posix => ILLEGAL_POSIX.contains(&c),
            Self::Universal => ILLEGAL_WINDOWS.contains(&c) || ILLEGAL_HFS.contains(&c),
        }
    }

    fn is_windows_like(self) -> bool {
        matches!(self, Self::Windows | Self::Universal)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PathError {
    #[error("path traversal detected in file name")]
    Traversal,

    #[error("{0} escapes the destination directory")]
    Escape(PathBuf),
}

/// Rewrites a single path segment into a legal file name.
pub fn sanitize_component(name: &str, rules: Rules) -> String {
    let mut result = String::with_capacity(name.len());
    for c in name.chars() {
        if rules.is_illegal(c) {
            result.push_str(REPLACEMENT);
        } else {
            result.push(c);
        }
    }

    if rules.is_windows_like() {
        collapse_trailing_run(&mut result);
        if let Some(reserved) = reserved_prefix(&result) {
            result = format!("{REPLACEMENT}{}", &result[reserved.len()..]);
        }
    }

    truncate_bytes(&mut result, MAX_LEN);

    if rules.is_windows_like() {
        // Truncation can cut right after a `.` or ` `.
        collapse_trailing_run(&mut result);
        truncate_bytes(&mut result, MAX_LEN);
    }

    if result.is_empty() || result == "." || result == ".." {
        result = PLACEHOLDER.to_string();
    }
    result
}

/// Turns a peer-supplied file name, which may carry `/` or `\` separated
/// directory components (folder transfers), into a relative path whose every
/// component is sanitised. `..` and absolute names are refused outright.
pub fn sanitize_relative_path(file_name: &str, rules: Rules) -> Result<PathBuf, PathError> {
    let trimmed = file_name.trim();
    if trimmed.starts_with(['/', '\\']) || has_drive_prefix(trimmed) {
        return Err(PathError::Traversal);
    }

    let mut path = PathBuf::new();
    for component in trimmed.split(['/', '\\']) {
        if component == ".." {
            return Err(PathError::Traversal);
        }
        if component.is_empty() || component == "." {
            continue;
        }
        path.push(sanitize_component(component, rules));
    }

    if path.as_os_str().is_empty() {
        path.push(PLACEHOLDER);
    }
    Ok(path)
}

/// Picks a path under `dir` for `relative` that neither exists nor is in
/// `taken` (names already assigned in the same session): `name (1).ext`,
/// `name (2).ext`, …
pub fn unique_path(dir: &Path, relative: &Path, taken: &HashSet<PathBuf>) -> PathBuf {
    let candidate = dir.join(relative);
    let is_free = |path: &Path| !path.exists() && !taken.contains(path);
    if is_free(&candidate) {
        return candidate;
    }

    let parent = candidate
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| dir.to_path_buf());
    let file_name = candidate
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| PLACEHOLDER.to_string());
    let (stem, extension) = split_extension(&file_name);

    (1u32..)
        .map(|n| {
            let name = if extension.is_empty() {
                format!("{stem} ({n})")
            } else {
                format!("{stem} ({n}).{extension}")
            };
            parent.join(name)
        })
        .find(|path| is_free(path))
        .unwrap_or(candidate)
}

/// Checks that `candidate`, which need not exist yet, lies inside `base`
/// once symlinks in the existing part of the path are resolved.
pub fn ensure_within(base: &Path, candidate: &Path) -> Result<(), PathError> {
    let base = base
        .canonicalize()
        .map_err(|_| PathError::Escape(base.to_path_buf()))?;
    let resolved = resolve_existing_prefix(candidate);
    if resolved.starts_with(&base) {
        Ok(())
    } else {
        Err(PathError::Escape(candidate.to_path_buf()))
    }
}

/// Canonicalises the deepest existing ancestor of `path` and re-appends the
/// missing components.
fn resolve_existing_prefix(path: &Path) -> PathBuf {
    let mut existing = path;
    let mut missing = Vec::new();
    while !existing.exists() {
        match (existing.file_name(), existing.parent()) {
            (Some(name), Some(parent)) => {
                missing.push(name.to_os_string());
                existing = parent;
            }
            _ => break,
        }
    }
    let mut resolved = existing
        .canonicalize()
        .unwrap_or_else(|_| existing.to_path_buf());
    for component in missing.iter().rev() {
        resolved.push(component);
    }
    resolved
}

fn has_drive_prefix(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(
        (chars.next(), chars.next()),
        (Some(letter), Some(':')) if letter.is_ascii_alphabetic()
    )
}

fn collapse_trailing_run(result: &mut String) {
    let trimmed_len = result.trim_end_matches(['.', ' ']).len();
    if trimmed_len != result.len() {
        result.truncate(trimmed_len);
        result.push_str(REPLACEMENT);
    }
}

fn reserved_prefix(name: &str) -> Option<&'static str> {
    let stem = name.split('.').next().unwrap_or(name);
    RESERVED_WINDOWS
        .iter()
        .copied()
        .find(|reserved| stem.eq_ignore_ascii_case(reserved))
}

fn truncate_bytes(value: &mut String, max: usize) {
    if value.len() <= max {
        return;
    }
    let mut end = max;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
}

fn split_extension(file_name: &str) -> (String, String) {
    match file_name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() => {
            (stem.to_string(), ext.to_string())
        }
        _ => (file_name.to_string(), String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_illegal_characters() {
        assert_eq!(
            sanitize_component("a<b>c:d\"e/f\\g|h?i*j", Rules::Windows),
            "a_b_c_d_e_f_g_h_i_j"
        );
        assert_eq!(sanitize_component("a/b:c", Rules::Hfs), "a_b_c");
        assert_eq!(sanitize_component("a/b:c", Rules::Posix), "a_b:c");
        assert_eq!(sanitize_component("a\u{0}b\u{7f}c", Rules::Posix), "a_b_c");
    }

    #[test]
    fn keeps_legal_names() {
        for rules in [Rules::Windows, Rules::Hfs, Rules::Posix, Rules::Universal] {
            assert_eq!(
                sanitize_component("holiday photo (1).jpg", rules),
                "holiday photo (1).jpg"
            );
            assert_eq!(
                sanitize_component("Ünïcödé — 文件.txt", rules),
                "Ünïcödé — 文件.txt"
            );
        }
    }

    #[test]
    fn windows_trailing_and_reserved() {
        assert_eq!(sanitize_component("report.", Rules::Windows), "report_");
        assert_eq!(sanitize_component("report...  ", Rules::Windows), "report_");
        assert_eq!(sanitize_component("report.", Rules::Posix), "report.");
        assert_eq!(sanitize_component("con", Rules::Windows), "_");
        assert_eq!(sanitize_component("NUL.txt", Rules::Windows), "_.txt");
        assert_eq!(
            sanitize_component("console.txt", Rules::Windows),
            "console.txt"
        );
        assert_eq!(sanitize_component("com10.txt", Rules::Windows), "com10.txt");
    }

    #[test]
    fn placeholder_and_truncation() {
        assert_eq!(sanitize_component("", Rules::Posix), PLACEHOLDER);
        assert_eq!(sanitize_component("..", Rules::Posix), PLACEHOLDER);
        let long = "ä".repeat(200);
        let sanitized = sanitize_component(&long, Rules::Posix);
        assert_eq!(sanitized.len(), MAX_LEN - 1);
        let dot = format!("{}.{}", "a".repeat(254), "b".repeat(10));
        assert_eq!(
            sanitize_component(&dot, Rules::Windows),
            format!("{}_", "a".repeat(254))
        );
    }

    #[test]
    fn relative_paths() {
        assert_eq!(
            sanitize_relative_path("photos/2024/a.jpg", Rules::Posix).unwrap(),
            PathBuf::from("photos/2024/a.jpg")
        );
        assert_eq!(
            sanitize_relative_path("dir\\con.txt", Rules::Windows).unwrap(),
            PathBuf::from("dir/_.txt")
        );
        assert_eq!(
            sanitize_relative_path("./a//b/", Rules::Posix).unwrap(),
            PathBuf::from("a/b")
        );
        assert_eq!(
            sanitize_relative_path("", Rules::Posix).unwrap(),
            PathBuf::from(PLACEHOLDER)
        );
        assert_eq!(
            sanitize_relative_path("../../etc/passwd", Rules::Posix),
            Err(PathError::Traversal)
        );
        assert_eq!(
            sanitize_relative_path("/etc/passwd", Rules::Posix),
            Err(PathError::Traversal)
        );
        assert_eq!(
            sanitize_relative_path("C:\\Windows\\evil.exe", Rules::Windows),
            Err(PathError::Traversal)
        );
    }

    #[test]
    fn unique_names_and_containment() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        std::fs::write(base.join("a.txt"), b"x").unwrap();
        let taken = HashSet::new();
        assert_eq!(
            unique_path(base, Path::new("a.txt"), &taken),
            base.join("a (1).txt")
        );
        let mut taken = HashSet::new();
        taken.insert(base.join("b"));
        assert_eq!(
            unique_path(base, Path::new("b"), &taken),
            base.join("b (1)")
        );
        assert_eq!(
            unique_path(base, Path::new("sub/new.tar.gz"), &taken),
            base.join("sub/new.tar.gz")
        );

        ensure_within(base, &base.join("sub/new.txt")).unwrap();
        assert!(ensure_within(base, &base.join("../outside.txt")).is_err());
    }
}
