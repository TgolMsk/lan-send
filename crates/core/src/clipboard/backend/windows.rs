//! Windows clipboard through the `clipboard-win` crate (ADR-0011): changes
//! come from an `AddClipboardFormatListener` message window on a dedicated
//! thread (no polling); images are read as the registered `PNG` format
//! first, else `CF_DIB` converted to PNG; file lists are `CF_HDROP`.

use super::{ClipboardBackend, ClipboardError};
use crate::clipboard::model::{ClipboardPayload, ImageFormat};
use bytes::Bytes;
use clipboard_win::{Clipboard, Getter, Monitor, Setter, formats, raw};
use parking_lot::{Condvar, Mutex};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Clipboard-win's `Clipboard::new_attempts` already retries the open.
const OPEN_ATTEMPTS: usize = 10;

pub struct WindowsClipboard {
    /// Number of change notifications received so far, with the condvar the
    /// monitor thread signals.
    changes: Arc<(Mutex<u64>, Condvar)>,
    /// The count `wait_for_change` last reported.
    seen: Mutex<u64>,
    png_format: Option<u32>,
    rtf_format: Option<u32>,
}

impl WindowsClipboard {
    /// Starts the listener thread. Fails when no message window can be
    /// created (e.g. a service session without a desktop).
    pub fn new() -> Result<Self, ClipboardError> {
        let changes = Arc::new((Mutex::new(0u64), Condvar::new()));
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let worker = changes.clone();
        std::thread::Builder::new()
            .name("clipboard-monitor".into())
            .spawn(move || {
                // The message window is thread-affine: create it here and
                // pump its messages here.
                let mut monitor = match Monitor::new() {
                    Ok(monitor) => monitor,
                    Err(err) => {
                        let _ = ready_tx.send(Err(err.to_string()));
                        return;
                    }
                };
                let _ = ready_tx.send(Ok(()));
                loop {
                    match monitor.recv() {
                        Ok(true) => {
                            let (count, condvar) = &*worker;
                            *count.lock() += 1;
                            condvar.notify_all();
                        }
                        Ok(false) => return,
                        Err(err) => {
                            tracing::warn!("clipboard monitor stopped: {err}");
                            return;
                        }
                    }
                }
            })
            .map_err(|err| ClipboardError::Other(err.to_string()))?;
        match ready_rx.recv() {
            Ok(Ok(())) => {}
            Ok(Err(err)) => return Err(ClipboardError::Other(err)),
            Err(_) => {
                return Err(ClipboardError::Other(
                    "clipboard monitor thread died".into(),
                ));
            }
        }
        Ok(Self {
            changes,
            seen: Mutex::new(0),
            png_format: raw::register_format("PNG").map(|format| format.get()),
            rtf_format: raw::register_format("Rich Text Format").map(|format| format.get()),
        })
    }
}

fn open() -> Result<Clipboard, ClipboardError> {
    Clipboard::new_attempts(OPEN_ATTEMPTS).map_err(|_| ClipboardError::Busy)
}

fn other(err: impl std::fmt::Display) -> ClipboardError {
    ClipboardError::Other(err.to_string())
}

impl ClipboardBackend for WindowsClipboard {
    fn wait_for_change(&self, timeout: Duration) -> Result<bool, ClipboardError> {
        let (count, condvar) = &*self.changes;
        let mut guard = count.lock();
        let seen = *self.seen.lock();
        if *guard == seen {
            condvar.wait_for(&mut guard, timeout);
        }
        let now = *guard;
        drop(guard);
        *self.seen.lock() = now;
        Ok(now != seen)
    }

    fn read(&self) -> Result<Option<ClipboardPayload>, ClipboardError> {
        let _clip = open()?;

        if raw::is_format_avail(formats::CF_HDROP) {
            let mut paths: Vec<PathBuf> = Vec::new();
            if formats::FileList.read_clipboard(&mut paths).is_ok() && !paths.is_empty() {
                return Ok(Some(ClipboardPayload::Files { paths }));
            }
        }

        if let Some(png) = self.png_format
            && raw::is_format_avail(png)
        {
            let mut bytes = Vec::new();
            if formats::RawData(png).read_clipboard(&mut bytes).is_ok() && !bytes.is_empty() {
                let bytes = Bytes::from(bytes);
                let (width, height) = png_dimensions(&bytes).unwrap_or((0, 0));
                return Ok(Some(ClipboardPayload::Image {
                    format: ImageFormat::Png,
                    bytes,
                    width,
                    height,
                }));
            }
        }
        if raw::is_format_avail(formats::CF_DIB) || raw::is_format_avail(formats::CF_DIBV5) {
            let mut bmp = Vec::new();
            if formats::Bitmap.read_clipboard(&mut bmp).is_ok()
                && let Some((bytes, width, height)) = bmp_to_png(&bmp)
            {
                return Ok(Some(ClipboardPayload::Image {
                    format: ImageFormat::Png,
                    bytes,
                    width,
                    height,
                }));
            }
        }

        if raw::is_format_avail(formats::CF_UNICODETEXT) {
            let mut plain = String::new();
            if formats::Unicode.read_clipboard(&mut plain).is_ok() {
                let html = formats::Html::new().and_then(|format| {
                    let mut html = String::new();
                    format.read_clipboard(&mut html).ok().map(|_| html)
                });
                let rtf = self.rtf_format.and_then(|format| {
                    let mut bytes = Vec::new();
                    formats::RawData(format)
                        .read_clipboard(&mut bytes)
                        .ok()
                        .and_then(|_| String::from_utf8(bytes).ok())
                });
                return Ok(Some(ClipboardPayload::Text { plain, html, rtf }));
            }
        }
        Ok(None)
    }

    fn write(&self, payload: &ClipboardPayload) -> Result<(), ClipboardError> {
        let _clip = open()?;
        raw::empty().map_err(other)?;
        match payload {
            ClipboardPayload::Text { plain, html, rtf } => {
                formats::Unicode.write_clipboard(plain).map_err(other)?;
                if let (Some(html), Some(format)) = (html, formats::Html::new()) {
                    let _ = format.write_clipboard(html);
                }
                if let (Some(rtf), Some(format)) = (rtf, self.rtf_format) {
                    let _ = formats::RawData(format).write_clipboard(&rtf.as_bytes());
                }
            }
            ClipboardPayload::Image { format, bytes, .. } => {
                let source = match format {
                    ImageFormat::Png => image::ImageFormat::Png,
                    ImageFormat::Jpeg => image::ImageFormat::Jpeg,
                };
                let png = match format {
                    ImageFormat::Png => bytes.to_vec(),
                    ImageFormat::Jpeg => transcode(bytes, source, image::ImageFormat::Png)
                        .ok_or_else(|| ClipboardError::Other("cannot decode the image".into()))?,
                };
                let mut written = false;
                if let Some(png_format) = self.png_format {
                    written |= formats::RawData(png_format).write_clipboard(&png).is_ok();
                }
                // Classic applications only take CF_DIB.
                if let Some(bmp) = transcode(bytes, source, image::ImageFormat::Bmp) {
                    written |= formats::Bitmap.write_clipboard(&bmp).is_ok();
                }
                if !written {
                    return Err(ClipboardError::Other(
                        "the clipboard refused the image".into(),
                    ));
                }
            }
            ClipboardPayload::Files { paths } => {
                let paths: Vec<String> = paths
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect();
                formats::FileList.write_clipboard(&paths).map_err(other)?;
            }
        }
        Ok(())
    }
}

/// Re-encodes an image; `None` when it cannot be decoded.
fn transcode(bytes: &[u8], from: image::ImageFormat, to: image::ImageFormat) -> Option<Vec<u8>> {
    let decoded = image::load_from_memory_with_format(bytes, from).ok()?;
    let mut out = std::io::Cursor::new(Vec::new());
    decoded.write_to(&mut out, to).ok()?;
    Some(out.into_inner())
}

fn bmp_to_png(bmp: &[u8]) -> Option<(Bytes, u32, u32)> {
    let decoded = image::load_from_memory_with_format(bmp, image::ImageFormat::Bmp).ok()?;
    let mut out = std::io::Cursor::new(Vec::new());
    decoded.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some((
        Bytes::from(out.into_inner()),
        decoded.width(),
        decoded.height(),
    ))
}

/// Width and height from a PNG's IHDR chunk.
fn png_dimensions(png: &[u8]) -> Option<(u32, u32)> {
    if png.len() < 24 || &png[..8] != b"\x89PNG\r\n\x1a\n" || &png[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
    let height = u32::from_be_bytes([png[20], png[21], png[22], png[23]]);
    Some((width, height))
}
