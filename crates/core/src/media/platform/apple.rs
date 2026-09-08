//! ImageIO on macOS and iOS: decodes everything the OS knows (HEIC, AVIF,
//! WebP, JPEG XL, …) straight into a thumbnail-sized `CGImage`, which is then
//! drawn into an RGBX buffer pre-filled with the matte (so transparency is
//! composited by CoreGraphics). Orientation is left to the caller.

#![allow(unsafe_code)]

use crate::media::decode::{Decoded, MATTE};
use objc2_core_foundation::{
    CFBoolean, CFDictionary, CFNumber, CFRetained, CFString, CFType, CFURL, CFURLPathStyle,
    CGPoint, CGRect, CGSize,
};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGBitmapContextGetBytesPerRow, CGBitmapContextGetData, CGColorSpace,
    CGContext, CGImage, CGInterpolationQuality,
};
use objc2_image_io::{
    CGImageSource, kCGImagePropertyPixelHeight, kCGImagePropertyPixelWidth,
    kCGImageSourceCreateThumbnailFromImageAlways, kCGImageSourceCreateThumbnailWithTransform,
    kCGImageSourceShouldCache, kCGImageSourceThumbnailMaxPixelSize,
};
use std::ffi::c_void;
use std::path::Path;

/// `kCGImageAlphaNoneSkipLast | kCGBitmapByteOrder32Big`: RGBX, 8 bits each.
const BITMAP_INFO: u32 = 5 | (4 << 12);

pub(super) fn decode_thumbnail(path: &Path, max_px: u32) -> Option<Decoded> {
    let url = CFURL::with_file_system_path(
        None,
        Some(&CFString::from_str(&path.to_string_lossy())),
        CFURLPathStyle::CFURLPOSIXPathStyle,
        false,
    )?;
    // SAFETY: plain ImageIO calls with valid, retained CF objects.
    let source = unsafe { CGImageSource::with_url(&url, None)? };

    let (source_width, source_height) = unsafe { source_size(&source) }?;

    let max = CFNumber::new_i64(i64::from(max_px));
    // SAFETY: reading the framework's constant keys.
    let keys: [&CFType; 4] = unsafe {
        [
            kCGImageSourceCreateThumbnailFromImageAlways,
            kCGImageSourceThumbnailMaxPixelSize,
            kCGImageSourceCreateThumbnailWithTransform,
            kCGImageSourceShouldCache,
        ]
    };
    let values: [&CFType; 4] = [
        CFBoolean::new(true),
        &max,
        CFBoolean::new(false),
        CFBoolean::new(false),
    ];
    let options = CFDictionary::from_slices(&keys, &values);
    // SAFETY: the option keys and values are what ImageIO documents; the
    // generic parameters are phantom, so the untyped view has the same layout.
    let image = unsafe {
        let untyped: &CFDictionary = &*(&*options as *const CFDictionary<CFType, CFType>).cast();
        source.thumbnail_at_index(0, Some(untyped))?
    };
    let width = CGImage::width(Some(&image));
    let height = CGImage::height(Some(&image));
    if width == 0 || height == 0 || width > 1 << 14 || height > 1 << 14 {
        return None;
    }
    let rgba = unsafe { draw_rgbx(&image, width, height) }?;
    Decoded::from_rgba(
        u32::try_from(width).ok()?,
        u32::try_from(height).ok()?,
        rgba,
        source_width,
        source_height,
    )
}

/// Pixel size from the container without decoding.
unsafe fn source_size(source: &CGImageSource) -> Option<(u32, u32)> {
    // SAFETY: `source` is a live CGImageSource; the property keys are the
    // framework's own constants.
    unsafe {
        let properties = source.properties_at_index(0, None)?;
        let width = number(&properties, kCGImagePropertyPixelWidth)?;
        let height = number(&properties, kCGImagePropertyPixelHeight)?;
        Some((u32::try_from(width).ok()?, u32::try_from(height).ok()?))
    }
}

/// A CFNumber value of an untyped property dictionary.
unsafe fn number(dict: &CFDictionary, key: &CFString) -> Option<i64> {
    // SAFETY: CFDictionaryGetValue returns a borrowed pointer that stays
    // valid while `dict` lives; the type is checked before use.
    unsafe {
        let ptr = dict.value((key as *const CFString).cast::<c_void>());
        if ptr.is_null() {
            return None;
        }
        let value: &CFType = &*ptr.cast::<CFType>();
        value.downcast_ref::<CFNumber>()?.as_i64()
    }
}

/// Draws `image` into an RGBX8 buffer pre-filled with the matte and returns
/// it as straight RGBA (alpha 255).
unsafe fn draw_rgbx(image: &CGImage, width: usize, height: usize) -> Option<Vec<u8>> {
    let bytes_per_row = width.checked_mul(4)?;
    let mut buffer = vec![0u8; bytes_per_row.checked_mul(height)?];
    for px in buffer.chunks_exact_mut(4) {
        px[..3].copy_from_slice(&MATTE);
        px[3] = 255;
    }
    let space = CGColorSpace::new_device_rgb()?;
    // SAFETY: `buffer` is exactly `bytes_per_row * height` bytes and outlives
    // the context, which is dropped before the buffer is returned.
    let context: CFRetained<CGContext> = unsafe {
        CGBitmapContextCreate(
            buffer.as_mut_ptr().cast::<c_void>(),
            width,
            height,
            8,
            bytes_per_row,
            Some(&space),
            BITMAP_INFO,
        )
    }?;
    if CGBitmapContextGetData(Some(&context)).is_null()
        || CGBitmapContextGetBytesPerRow(Some(&context)) != bytes_per_row
    {
        return None;
    }
    CGContext::set_interpolation_quality(Some(&context), CGInterpolationQuality::High);
    let rect = CGRect::new(
        CGPoint::new(0.0, 0.0),
        CGSize::new(width as f64, height as f64),
    );
    CGContext::draw_image(Some(&context), rect, Some(image));
    // Everything CoreGraphics wrote went into `buffer`; the context is
    // released here, before the buffer is returned.
    drop(context);
    for px in buffer.chunks_exact_mut(4) {
        px[3] = 255;
    }
    Some(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageBuffer, Rgba};

    fn write_png(dir: &Path, name: &str, w: u32, h: u32) -> std::path::PathBuf {
        let path = dir.join(name);
        DynamicImage::ImageRgba8(ImageBuffer::from_fn(w, h, |x, _| {
            Rgba([200, 30, 30, if x < w / 2 { 255 } else { 0 }])
        }))
        .save(&path)
        .unwrap();
        path
    }

    #[test]
    fn imageio_decodes_and_composites() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_png(dir.path(), "a.png", 800, 200);
        let decoded = decode_thumbnail(&path, 256).unwrap();
        assert_eq!((decoded.source_width, decoded.source_height), (800, 200));
        assert!(decoded.image.width() <= 256 && decoded.image.height() <= 64);
        let rgba = decoded.image.to_rgba8();
        let left = rgba.get_pixel(10, 10).0;
        let right = rgba.get_pixel(rgba.width() - 10, 10).0;
        assert!(
            left[0] > 150 && left[1] < 80,
            "opaque red survives: {left:?}"
        );
        assert_eq!(&right[..3], &MATTE, "transparent becomes the matte");
    }

    /// HEIC comes only from the OS; `sips` makes one when available.
    #[cfg(target_os = "macos")]
    #[test]
    fn imageio_decodes_heic_when_sips_exists() {
        let dir = tempfile::tempdir().unwrap();
        let png = write_png(dir.path(), "src.png", 640, 480);
        let heic = dir.path().join("photo.heic");
        let ok = std::process::Command::new("/usr/bin/sips")
            .args(["-s", "format", "heic", png.to_str().unwrap(), "--out"])
            .arg(&heic)
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false);
        if !ok || !heic.is_file() {
            eprintln!("sips could not write HEIC here; skipping");
            return;
        }
        let mime = crate::media::sniff_mime(&heic);
        assert!(mime == "image/heic" || mime == "image/heif", "{mime}");
        let decoded = decode_thumbnail(&heic, 128).unwrap();
        assert_eq!((decoded.source_width, decoded.source_height), (640, 480));
        assert_eq!(decoded.image.width(), 128);
    }

    #[test]
    fn unreadable_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.heic");
        std::fs::write(&path, b"nope").unwrap();
        assert!(decode_thumbnail(&path, 256).is_none());
    }
}
