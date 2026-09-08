//! Media support (ADR-0015): MIME sniffing by magic bytes, image thumbnails
//! with EXIF orientation, audio metadata, and a bounded thumbnail cache.
//! Never modifies transferred bytes; every failure degrades to "no preview".
//!
//! Decoding tries the operating system first (`platform::decode_thumbnail`:
//! ImageIO on macOS / iOS, WIC on Windows — the only way to get HEIC / AVIF
//! without GPL/LGPL code), then the pure-Rust `image` crate.

mod audio;
mod cache;
mod decode;
mod platform;
mod sniff;

pub use audio::{AudioInfo, probe_audio};
pub use cache::{THUMBNAIL_CACHE_LIMIT, THUMBNAIL_MAX_PX, ThumbnailCache};
pub use decode::{Pixels, thumbnail_pixels};
pub use sniff::{is_generic_mime, sniff_mime, sniff_mime_bytes};

use serde::Serialize;
use std::path::{Path, PathBuf};

/// What a file is, as far as previews are concerned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    Image,
    Audio,
    Video,
    Other,
}

impl MediaKind {
    pub fn from_mime(mime: &str) -> Self {
        let mime = mime.to_ascii_lowercase();
        if mime.starts_with("image/") {
            Self::Image
        } else if mime.starts_with("audio/") {
            Self::Audio
        } else if mime.starts_with("video/") {
            Self::Video
        } else {
            Self::Other
        }
    }
}

/// Everything a user interface shows next to a file: a cached thumbnail
/// (JPEG on disk), image dimensions, audio tags. Every field is optional
/// because every decoder may fail; the file itself is untouched either way.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    pub kind: Option<MediaKind>,
    pub mime: String,
    pub thumbnail: Option<PathBuf>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_ms: Option<u64>,
}

/// Probes `path` and makes sure a thumbnail is cached when one can be made.
/// Blocking: call from a blocking thread. Never returns an error — a file
/// that cannot be decoded simply has no preview.
pub fn probe(path: &Path, cache: &ThumbnailCache) -> MediaInfo {
    let mime = sniff_mime(path);
    let kind = MediaKind::from_mime(&mime);
    let mut info = MediaInfo {
        kind: Some(kind),
        mime,
        ..MediaInfo::default()
    };
    match kind {
        MediaKind::Image => {
            if let Some(existing) = cache.lookup(path) {
                info.thumbnail = Some(existing.path);
                info.width = existing.width;
                info.height = existing.height;
                return info;
            }
            match thumbnail_pixels(path, THUMBNAIL_MAX_PX) {
                Some(pixels) => {
                    info.width = Some(pixels.source_width);
                    info.height = Some(pixels.source_height);
                    match cache.store(path, &pixels) {
                        Ok(stored) => info.thumbnail = Some(stored),
                        Err(err) => {
                            tracing::debug!("thumbnail not cached for {}: {err}", path.display())
                        }
                    }
                }
                None => tracing::debug!("no decoder produced a preview for {}", path.display()),
            }
        }
        MediaKind::Audio => {
            if let Some(audio) = probe_audio(path) {
                info.title = audio.title;
                info.artist = audio.artist;
                info.album = audio.album;
                info.duration_ms = audio.duration_ms;
                if let Some(existing) = cache.lookup(path) {
                    info.thumbnail = Some(existing.path);
                } else if let Some(cover) = audio.cover {
                    if let Some(pixels) = decode::thumbnail_from_bytes(&cover, THUMBNAIL_MAX_PX) {
                        if let Ok(stored) = cache.store(path, &pixels) {
                            info.thumbnail = Some(stored);
                        }
                    }
                }
            }
        }
        MediaKind::Video | MediaKind::Other => {}
    }
    info
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageBuffer, Rgba};

    #[test]
    fn probe_image_caches_thumbnail_and_reports_size() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ThumbnailCache::new(dir.path());
        let photo = dir.path().join("photo.png");
        DynamicImage::ImageRgba8(ImageBuffer::from_fn(900, 600, |x, y| {
            Rgba([(x % 256) as u8, (y % 256) as u8, 40, 255])
        }))
        .save(&photo)
        .unwrap();

        let info = probe(&photo, &cache);
        assert_eq!(info.kind, Some(MediaKind::Image));
        assert_eq!(info.mime, "image/png");
        assert_eq!((info.width, info.height), (Some(900), Some(600)));
        let thumb = info.thumbnail.clone().expect("thumbnail cached");
        assert!(thumb.starts_with(cache.dir()));
        let (w, h) = image::image_dimensions(&thumb).unwrap();
        assert_eq!((w, h), (256, 171));

        // Second probe is a cache hit with the same answer.
        let again = probe(&photo, &cache);
        assert_eq!(again, info);
    }

    #[test]
    fn probe_other_files_has_no_preview() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ThumbnailCache::new(dir.path());
        let doc = dir.path().join("notes.txt");
        std::fs::write(&doc, "just text").unwrap();
        let info = probe(&doc, &cache);
        assert_eq!(info.kind, Some(MediaKind::Other));
        assert_eq!(info.mime, "text/plain");
        assert!(info.thumbnail.is_none());
        assert_eq!(cache.size(), 0);
    }
}
