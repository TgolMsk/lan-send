//! The thumbnail cache: `<cache dir>/thumbs/<key>.jpg` plus a `.meta`
//! sidecar with the source dimensions. Bounded by total size, evicting the
//! least recently used files (a hit touches the modification time).

use super::decode::Pixels;
use image::codecs::jpeg::JpegEncoder;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Long side of a thumbnail (brief §6).
pub const THUMBNAIL_MAX_PX: u32 = 256;
/// Total bytes kept before the least recently used thumbnails are evicted.
pub const THUMBNAIL_CACHE_LIMIT: u64 = 200 * 1024 * 1024;
const JPEG_QUALITY: u8 = 85;
/// After eviction the cache is trimmed to this share of the limit so every
/// store does not trigger another walk.
const TRIM_TO_PERCENT: u64 = 90;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CachedThumbnail {
    pub path: PathBuf,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct ThumbnailCache {
    dir: PathBuf,
    limit: u64,
}

impl ThumbnailCache {
    /// `cache_dir` is the application cache directory; thumbnails go to its
    /// `thumbs` subdirectory.
    pub fn new(cache_dir: &Path) -> Self {
        Self::with_limit(cache_dir, THUMBNAIL_CACHE_LIMIT)
    }

    pub fn with_limit(cache_dir: &Path, limit: u64) -> Self {
        Self {
            dir: cache_dir.join("thumbs"),
            limit,
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The cached thumbnail of `source`, if its size and modification time
    /// still match. Touches the file so eviction sees it as fresh.
    pub fn lookup(&self, source: &Path) -> Option<CachedThumbnail> {
        let key = self.key(source)?;
        let path = self.dir.join(format!("{key}.jpg"));
        if !path.is_file() {
            return None;
        }
        touch(&path);
        let (width, height) = read_meta(&self.dir.join(format!("{key}.meta"))).unzip();
        Some(CachedThumbnail {
            path,
            width,
            height,
        })
    }

    /// Writes `pixels` as the thumbnail of `source` and returns its path.
    pub fn store(&self, source: &Path, pixels: &Pixels) -> io::Result<PathBuf> {
        let key = self
            .key(source)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "source not readable"))?;
        fs::create_dir_all(&self.dir)?;
        let path = self.dir.join(format!("{key}.jpg"));
        let tmp = self.dir.join(format!("{key}.jpg.part"));
        {
            let mut writer = BufWriter::new(File::create(&tmp)?);
            let encoder = JpegEncoder::new_with_quality(&mut writer, JPEG_QUALITY);
            pixels
                .image
                .write_with_encoder(encoder)
                .map_err(|err| io::Error::other(err.to_string()))?;
            writer.flush()?;
        }
        fs::rename(&tmp, &path)?;
        fs::write(
            self.dir.join(format!("{key}.meta")),
            format!("{} {}\n", pixels.source_width, pixels.source_height),
        )?;
        self.evict_if_needed();
        Ok(path)
    }

    /// Bytes currently used.
    pub fn size(&self) -> u64 {
        self.entries().into_iter().map(|entry| entry.size).sum()
    }

    /// Removes every thumbnail; returns the bytes freed.
    pub fn clear(&self) -> io::Result<u64> {
        let mut freed = 0;
        for entry in self.entries() {
            if fs::remove_file(&entry.path).is_ok() {
                freed += entry.size;
            }
        }
        Ok(freed)
    }

    /// `sha256(path, size, mtime)` shortened to 32 hex characters: the same
    /// file gets the same thumbnail, a changed file gets a new one.
    fn key(&self, source: &Path) -> Option<String> {
        let meta = fs::metadata(source).ok()?;
        let mtime = meta
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mut hasher = Sha256::new();
        hasher.update(source.to_string_lossy().as_bytes());
        hasher.update(meta.len().to_le_bytes());
        hasher.update(mtime.to_le_bytes());
        let digest = hasher.finalize();
        Some(digest.iter().take(16).map(|b| format!("{b:02x}")).collect())
    }

    fn entries(&self) -> Vec<Entry> {
        let Ok(read) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        read.flatten()
            .filter_map(|entry| {
                let meta = entry.metadata().ok()?;
                if !meta.is_file() {
                    return None;
                }
                Some(Entry {
                    path: entry.path(),
                    size: meta.len(),
                    modified: meta.modified().unwrap_or(UNIX_EPOCH),
                })
            })
            .collect()
    }

    fn evict_if_needed(&self) {
        let mut entries = self.entries();
        let mut total: u64 = entries.iter().map(|entry| entry.size).sum();
        if total <= self.limit {
            return;
        }
        let target = self.limit * TRIM_TO_PERCENT / 100;
        entries.sort_by_key(|entry| entry.modified);
        // The newest image (the one just stored) always survives.
        let newest = entries
            .iter()
            .rev()
            .find(|entry| entry.path.extension().is_some_and(|ext| ext == "jpg"))
            .map(|entry| entry.path.clone());
        for entry in entries {
            if total <= target {
                break;
            }
            if newest.as_deref() == Some(entry.path.as_path())
                || newest.as_deref() == Some(entry.path.with_extension("jpg").as_path())
            {
                continue;
            }
            if fs::remove_file(&entry.path).is_ok() {
                total = total.saturating_sub(entry.size);
                // The sidecar goes with its image.
                if entry.path.extension().is_some_and(|ext| ext == "jpg") {
                    let _ = fs::remove_file(entry.path.with_extension("meta"));
                }
            }
        }
    }
}

struct Entry {
    path: PathBuf,
    size: u64,
    modified: SystemTime,
}

fn touch(path: &Path) {
    if let Ok(file) = File::options().write(true).open(path) {
        let _ = file.set_modified(SystemTime::now());
    }
}

fn read_meta(path: &Path) -> Option<(u32, u32)> {
    let text = fs::read_to_string(path).ok()?;
    let mut parts = text.split_whitespace();
    let width = parts.next()?.parse().ok()?;
    let height = parts.next()?.parse().ok()?;
    Some((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbImage;

    fn pixels(w: u32, h: u32) -> Pixels {
        Pixels {
            image: RgbImage::from_fn(w, h, |x, y| image::Rgb([(x * 7) as u8, (y * 3) as u8, 90])),
            source_width: w * 10,
            source_height: h * 10,
        }
    }

    #[test]
    fn store_then_lookup() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("photo.png");
        std::fs::write(&source, b"pretend").unwrap();
        let cache = ThumbnailCache::new(dir.path());
        assert!(cache.lookup(&source).is_none());
        let stored = cache.store(&source, &pixels(64, 48)).unwrap();
        assert!(stored.is_file());
        let hit = cache.lookup(&source).unwrap();
        assert_eq!(hit.path, stored);
        assert_eq!((hit.width, hit.height), (Some(640), Some(480)));
        assert!(cache.size() > 0);
        // Changing the file invalidates the key.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&source, b"pretend, but longer").unwrap();
        assert!(cache.lookup(&source).is_none());
        assert!(cache.clear().unwrap() > 0);
        assert_eq!(cache.size(), 0);
    }

    #[test]
    fn evicts_least_recently_used() {
        let dir = tempfile::tempdir().unwrap();
        // Room for about three thumbnails.
        let probe = ThumbnailCache::new(dir.path());
        let sample = dir.path().join("sample.png");
        std::fs::write(&sample, b"s").unwrap();
        probe.store(&sample, &pixels(200, 200)).unwrap();
        let limit = probe.size() * 7 / 2;
        probe.clear().unwrap();
        let cache = ThumbnailCache::with_limit(dir.path(), limit);
        let mut sources = Vec::new();
        for i in 0..6 {
            let source = dir.path().join(format!("{i}.png"));
            std::fs::write(&source, vec![i as u8; 10 + i]).unwrap();
            cache.store(&source, &pixels(200, 200)).unwrap();
            sources.push(source);
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        assert!(cache.size() <= limit);
        // The oldest ones went first.
        assert!(cache.lookup(&sources[0]).is_none());
        assert!(cache.lookup(&sources[5]).is_some());
    }

    #[test]
    fn missing_source_cannot_be_cached() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ThumbnailCache::new(dir.path());
        assert!(
            cache
                .store(&dir.path().join("nope.png"), &pixels(8, 8))
                .is_err()
        );
        assert_eq!(cache.size(), 0);
        assert_eq!(cache.clear().unwrap(), 0);
    }
}
