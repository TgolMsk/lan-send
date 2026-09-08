//! Operating-system image decoders. Each returns a possibly downscaled,
//! not yet oriented image, or `None` when the OS cannot read the file.

use super::decode::Decoded;
use std::path::Path;

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod apple;
#[cfg(target_os = "windows")]
mod windows;

/// Decodes `path` with the platform decoder, scaled so the long side is at
/// most `max_px`.
pub(super) fn decode_thumbnail(path: &Path, max_px: u32) -> Option<Decoded> {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        apple::decode_thumbnail(path, max_px)
    }
    #[cfg(target_os = "windows")]
    {
        windows::decode_thumbnail(path, max_px)
    }
    #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "windows")))]
    {
        let _ = (path, max_px);
        None
    }
}
