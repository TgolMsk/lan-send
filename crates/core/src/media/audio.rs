//! Audio metadata through `lofty`: tags, duration and the first embedded
//! picture. Read failures degrade silently to `None`.

use lofty::prelude::*;
use std::path::Path;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AudioInfo {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_ms: Option<u64>,
    /// The first embedded picture (usually the cover), encoded as stored.
    pub cover: Option<Vec<u8>>,
}

pub fn probe_audio(path: &Path) -> Option<AudioInfo> {
    let tagged = lofty::read_from_path(path).ok()?;
    let duration = tagged.properties().duration();
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag());
    let text = |value: Option<std::borrow::Cow<'_, str>>| {
        value
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    Some(AudioInfo {
        title: tag.and_then(|t| text(t.title())),
        artist: tag.and_then(|t| text(t.artist())),
        album: tag.and_then(|t| text(t.album())),
        duration_ms: Some(u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)),
        cover: tag
            .and_then(|t| t.pictures().first())
            .map(|picture| picture.data().to_vec())
            .filter(|data| !data.is_empty()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one second, 8 kHz, mono, 8-bit PCM WAV of silence.
    fn silent_wav() -> Vec<u8> {
        let samples = vec![128u8; 8000];
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + samples.len() as u32).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes()); // PCM
        out.extend_from_slice(&1u16.to_le_bytes()); // mono
        out.extend_from_slice(&8000u32.to_le_bytes());
        out.extend_from_slice(&8000u32.to_le_bytes()); // byte rate
        out.extend_from_slice(&1u16.to_le_bytes()); // block align
        out.extend_from_slice(&8u16.to_le_bytes()); // bits per sample
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(samples.len() as u32).to_le_bytes());
        out.extend_from_slice(&samples);
        out
    }

    #[test]
    fn wav_duration_without_tags() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("silence.wav");
        std::fs::write(&path, silent_wav()).unwrap();
        let info = probe_audio(&path).unwrap();
        assert_eq!(info.duration_ms, Some(1000));
        assert_eq!(info.title, None);
        assert_eq!(info.cover, None);
    }

    #[test]
    fn garbage_is_not_audio() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.mp3");
        std::fs::write(&path, b"definitely not audio").unwrap();
        assert!(probe_audio(&path).is_none());
    }
}
