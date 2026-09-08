//! MIME detection: magic bytes first (`infer`), then the extension
//! (`mime_guess`), then `application/octet-stream`.

use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Bytes read from the head of a file for sniffing. `infer` needs at most a
/// few hundred; 8 KB also covers ISO-BMFF brands placed after a large `ftyp`.
const SNIFF_LEN: usize = 8192;

const GENERIC: &str = "application/octet-stream";

/// The MIME type of the file at `path`. Unreadable files fall back to the
/// extension so a name like `photo.jpg` still gets `image/jpeg`.
pub fn sniff_mime(path: &Path) -> String {
    let mut head = [0u8; SNIFF_LEN];
    let read = File::open(path)
        .and_then(|mut file| file.read(&mut head))
        .unwrap_or(0);
    sniff_mime_bytes(&head[..read], Some(path))
}

/// The MIME type of content whose first bytes are `head`; `path` supplies
/// the extension fallback.
pub fn sniff_mime_bytes(head: &[u8], path: Option<&Path>) -> String {
    if let Some(kind) = infer::get(head) {
        // `infer` reports SVG as XML; the extension knows better.
        if kind.mime_type() != "text/xml" && kind.mime_type() != "application/xml" {
            return kind.mime_type().to_string();
        }
    }
    if let Some(path) = path {
        if let Some(guess) = mime_guess::from_path(path).first() {
            return guess.essence_str().to_string();
        }
    }
    GENERIC.to_string()
}

/// `application/octet-stream` and friends: worth re-sniffing after receipt.
pub fn is_generic_mime(mime: &str) -> bool {
    let mime = mime.trim().to_ascii_lowercase();
    mime.is_empty() || mime == GENERIC || mime == "binary/octet-stream" || mime == "*/*"
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn png_by_magic_bytes_even_with_wrong_extension() {
        let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
        assert_eq!(
            sniff_mime_bytes(png, Some(&PathBuf::from("x.txt"))),
            "image/png"
        );
    }

    #[test]
    fn extension_when_magic_unknown() {
        assert_eq!(
            sniff_mime_bytes(b"hello", Some(&PathBuf::from("notes.md"))),
            "text/markdown"
        );
        assert_eq!(
            sniff_mime_bytes(
                b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>",
                Some(&PathBuf::from("a.svg"))
            ),
            "image/svg+xml"
        );
    }

    #[test]
    fn generic_when_nothing_known() {
        assert_eq!(sniff_mime_bytes(b"", Some(&PathBuf::from("blob"))), GENERIC);
        assert_eq!(sniff_mime_bytes(b"", None), GENERIC);
        assert!(is_generic_mime(GENERIC));
        assert!(is_generic_mime(""));
        assert!(!is_generic_mime("image/png"));
    }

    #[test]
    fn reads_file_head() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.bin");
        std::fs::write(&path, b"GIF89a\x01\x00\x01\x00").unwrap();
        assert_eq!(sniff_mime(&path), "image/gif");
        assert_eq!(sniff_mime(&dir.path().join("missing.jpg")), "image/jpeg");
    }
}
