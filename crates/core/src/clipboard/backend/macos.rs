//! macOS clipboard through `NSPasteboard` (ADR-0011): changes are detected
//! by polling `changeCount`; images are read as PNG first, else TIFF
//! converted through `NSBitmapImageRep`; file lists are file URLs.
//!
//! AppKit calls need `unsafe` for the extern statics and one conversion
//! call; every use is confined to this file.
#![allow(unsafe_code)]

use super::{ClipboardBackend, ClipboardError};
use crate::clipboard::model::{ClipboardPayload, ImageFormat};
use bytes::Bytes;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{
    NSBitmapImageFileType, NSBitmapImageRep, NSPasteboard, NSPasteboardType,
    NSPasteboardTypeFileURL, NSPasteboardTypeHTML, NSPasteboardTypePNG, NSPasteboardTypeRTF,
    NSPasteboardTypeString, NSPasteboardTypeTIFF, NSPasteboardWriting,
};
use objc2_foundation::{NSArray, NSData, NSDictionary, NSString, NSURL};
use parking_lot::Mutex;
use std::path::PathBuf;
use std::time::Duration;

pub struct MacClipboard {
    last_count: Mutex<isize>,
}

impl MacClipboard {
    pub fn new() -> Self {
        let count = NSPasteboard::generalPasteboard().changeCount();
        Self {
            last_count: Mutex::new(count),
        }
    }
}

impl Default for MacClipboard {
    fn default() -> Self {
        Self::new()
    }
}

/// The extern pasteboard type constants are plain statics; reading them is
/// sound, they are initialised by AppKit.
fn type_png() -> &'static NSPasteboardType {
    unsafe { NSPasteboardTypePNG }
}
fn type_tiff() -> &'static NSPasteboardType {
    unsafe { NSPasteboardTypeTIFF }
}
fn type_string() -> &'static NSPasteboardType {
    unsafe { NSPasteboardTypeString }
}
fn type_html() -> &'static NSPasteboardType {
    unsafe { NSPasteboardTypeHTML }
}
fn type_rtf() -> &'static NSPasteboardType {
    unsafe { NSPasteboardTypeRTF }
}
fn type_file_url() -> &'static NSPasteboardType {
    unsafe { NSPasteboardTypeFileURL }
}

impl ClipboardBackend for MacClipboard {
    fn wait_for_change(&self, timeout: Duration) -> Result<bool, ClipboardError> {
        std::thread::sleep(timeout);
        let count = NSPasteboard::generalPasteboard().changeCount();
        let mut last = self.last_count.lock();
        let changed = count != *last;
        *last = count;
        Ok(changed)
    }

    fn read(&self) -> Result<Option<ClipboardPayload>, ClipboardError> {
        let pasteboard = NSPasteboard::generalPasteboard();

        // File URLs first: a copied file also carries its name as a string.
        if let Some(items) = pasteboard.pasteboardItems() {
            let mut paths = Vec::new();
            for item in items.iter() {
                if let Some(url) = item.stringForType(type_file_url())
                    && let Some(url) = NSURL::URLWithString(&url)
                    && let Some(path) = url.path()
                {
                    paths.push(PathBuf::from(path.to_string()));
                }
            }
            if !paths.is_empty() {
                return Ok(Some(ClipboardPayload::Files { paths }));
            }
        }

        if let Some(png) = pasteboard.dataForType(type_png()) {
            let bytes = Bytes::copy_from_slice(&png.to_vec());
            let (width, height) = png_dimensions(&bytes).unwrap_or((0, 0));
            return Ok(Some(ClipboardPayload::Image {
                format: ImageFormat::Png,
                bytes,
                width,
                height,
            }));
        }
        if let Some(tiff) = pasteboard.dataForType(type_tiff()) {
            return Ok(
                tiff_to_png(&tiff).map(|(bytes, width, height)| ClipboardPayload::Image {
                    format: ImageFormat::Png,
                    bytes,
                    width,
                    height,
                }),
            );
        }

        if let Some(plain) = pasteboard.stringForType(type_string()) {
            let html = pasteboard
                .stringForType(type_html())
                .map(|value| value.to_string());
            let rtf = pasteboard
                .dataForType(type_rtf())
                .and_then(|data| String::from_utf8(data.to_vec()).ok());
            return Ok(Some(ClipboardPayload::Text {
                plain: plain.to_string(),
                html,
                rtf,
            }));
        }
        Ok(None)
    }

    fn write(&self, payload: &ClipboardPayload) -> Result<(), ClipboardError> {
        let pasteboard = NSPasteboard::generalPasteboard();
        let count = pasteboard.clearContents();
        let ok = match payload {
            ClipboardPayload::Text { plain, html, rtf } => {
                let mut ok =
                    pasteboard.setString_forType(&NSString::from_str(plain), type_string());
                if let Some(html) = html {
                    ok &= pasteboard.setString_forType(&NSString::from_str(html), type_html());
                }
                if let Some(rtf) = rtf {
                    let data = NSData::with_bytes(rtf.as_bytes());
                    ok &= pasteboard.setData_forType(Some(&data), type_rtf());
                }
                ok
            }
            ClipboardPayload::Image { format, bytes, .. } => {
                let data = NSData::with_bytes(bytes);
                match format {
                    ImageFormat::Png => {
                        let mut ok = pasteboard.setData_forType(Some(&data), type_png());
                        // Older apps only take TIFF.
                        if let Some(rep) = NSBitmapImageRep::imageRepWithData(&data)
                            && let Some(tiff) = rep.TIFFRepresentation()
                        {
                            ok &= pasteboard.setData_forType(Some(&tiff), type_tiff());
                        }
                        ok
                    }
                    ImageFormat::Jpeg => match NSBitmapImageRep::imageRepWithData(&data) {
                        Some(rep) => {
                            let mut ok = false;
                            if let Some(tiff) = rep.TIFFRepresentation() {
                                ok = pasteboard.setData_forType(Some(&tiff), type_tiff());
                            }
                            if let Some(png) = png_representation(&rep) {
                                ok |= pasteboard.setData_forType(Some(&png), type_png());
                            }
                            ok
                        }
                        None => false,
                    },
                }
            }
            ClipboardPayload::Files { paths } => {
                let urls: Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>> = paths
                    .iter()
                    .map(|path| {
                        let url =
                            NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
                        ProtocolObject::from_retained(url)
                    })
                    .collect();
                let array = NSArray::from_retained_slice(&urls);
                pasteboard.writeObjects(&array)
            }
        };
        // The write moved the change count past what the watcher saw; make
        // sure our own write is not reported as a change.
        *self.last_count.lock() = pasteboard.changeCount().max(count);
        if ok {
            Ok(())
        } else {
            Err(ClipboardError::Other(
                "NSPasteboard refused the content".into(),
            ))
        }
    }
}

/// Converts TIFF data to PNG through AppKit; `None` when it is not an image.
fn tiff_to_png(tiff: &NSData) -> Option<(Bytes, u32, u32)> {
    let rep = NSBitmapImageRep::imageRepWithData(tiff)?;
    let png = png_representation(&rep)?;
    let width = u32::try_from(rep.pixelsWide()).unwrap_or(0);
    let height = u32::try_from(rep.pixelsHigh()).unwrap_or(0);
    Some((Bytes::copy_from_slice(&png.to_vec()), width, height))
}

fn png_representation(rep: &NSBitmapImageRep) -> Option<Retained<NSData>> {
    let properties = NSDictionary::new();
    // SAFETY: the file type is a valid enum value and the properties
    // dictionary is empty; AppKit only reads both.
    unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &properties) }
}

/// Width and height from a PNG's IHDR chunk.
pub(crate) fn png_dimensions(png: &[u8]) -> Option<(u32, u32)> {
    if png.len() < 24 || &png[..8] != b"\x89PNG\r\n\x1a\n" || &png[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
    let height = u32::from_be_bytes([png[20], png[21], png[22], png[23]]);
    Some((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_png_dimensions() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend_from_slice(&640u32.to_be_bytes());
        png.extend_from_slice(&480u32.to_be_bytes());
        assert_eq!(png_dimensions(&png), Some((640, 480)));
        assert_eq!(png_dimensions(b"nope"), None);
    }

    /// Touches the real pasteboard; run manually with `--ignored`.
    #[test]
    #[ignore]
    fn round_trips_text_on_the_real_pasteboard() {
        let clipboard = MacClipboard::new();
        let payload = ClipboardPayload::Text {
            plain: "lan-send test".into(),
            html: None,
            rtf: None,
        };
        clipboard.write(&payload).unwrap();
        assert_eq!(clipboard.read().unwrap(), Some(payload));
    }
}
