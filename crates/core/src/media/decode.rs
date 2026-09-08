//! Decoding to a small RGB thumbnail: the platform decoder first, then the
//! pure-Rust `image` crate. EXIF orientation is read once here and applied
//! to the pixels, so every decoder behaves the same.

use super::platform;
use image::imageops::FilterType;
use image::metadata::Orientation;
use image::{DynamicImage, ImageReader, RgbImage, RgbaImage};
use std::fs::File;
use std::io::{BufReader, Cursor, Seek, SeekFrom};
use std::path::Path;

/// Neutral grey behind transparent pixels (thumbnails are JPEG).
pub const MATTE: [u8; 3] = [0x2a, 0x2d, 0x3a];

/// A decoded thumbnail plus the size of the image it came from (after
/// orientation, i.e. as displayed).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pixels {
    pub image: RgbImage,
    pub source_width: u32,
    pub source_height: u32,
}

/// Decodes `path` into an RGB image no larger than `max_px` on its long
/// side, oriented per EXIF. `None` when no decoder could read it.
pub fn thumbnail_pixels(path: &Path, max_px: u32) -> Option<Pixels> {
    let orientation = exif_orientation(path).unwrap_or(Orientation::NoTransforms);
    let decoded = platform::decode_thumbnail(path, max_px)
        .or_else(|| decode_with_image_crate(path, max_px))?;
    Some(finish(decoded, orientation, max_px))
}

/// Same as [`thumbnail_pixels`] for in-memory bytes (embedded cover art).
pub fn thumbnail_from_bytes(bytes: &[u8], max_px: u32) -> Option<Pixels> {
    let orientation = exif::Reader::new()
        .read_from_container(&mut Cursor::new(bytes))
        .ok()
        .and_then(|exif| orientation_from_exif(&exif))
        .unwrap_or(Orientation::NoTransforms);
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let (source_width, source_height) = reader.into_dimensions().ok()?;
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let image = reader.decode().ok()?;
    let decoded = Decoded {
        image,
        source_width,
        source_height,
    };
    Some(finish(decoded, orientation, max_px))
}

/// What a decoder hands back: possibly already downscaled, not yet oriented.
pub(super) struct Decoded {
    pub image: DynamicImage,
    pub source_width: u32,
    pub source_height: u32,
}

impl Decoded {
    /// Builds from raw RGBA8 rows (platform decoders).
    pub(super) fn from_rgba(
        width: u32,
        height: u32,
        rgba: Vec<u8>,
        source_width: u32,
        source_height: u32,
    ) -> Option<Self> {
        let image = RgbaImage::from_raw(width, height, rgba)?;
        Some(Self {
            image: DynamicImage::ImageRgba8(image),
            source_width,
            source_height,
        })
    }
}

fn decode_with_image_crate(path: &Path, max_px: u32) -> Option<Decoded> {
    let reader = ImageReader::open(path).ok()?.with_guessed_format().ok()?;
    let (source_width, source_height) = reader.into_dimensions().ok()?;
    let reader = ImageReader::open(path).ok()?.with_guessed_format().ok()?;
    let image = reader.decode().ok()?;
    // Downscale early: orientation on the small image is much cheaper.
    let image = if image.width() > max_px || image.height() > max_px {
        image.resize(max_px, max_px, FilterType::Triangle)
    } else {
        image
    };
    Some(Decoded {
        image,
        source_width,
        source_height,
    })
}

fn finish(decoded: Decoded, orientation: Orientation, max_px: u32) -> Pixels {
    let mut image = decoded.image;
    if image.width() > max_px || image.height() > max_px {
        image = image.resize(max_px, max_px, FilterType::Lanczos3);
    }
    image.apply_orientation(orientation);
    let (source_width, source_height) = match orientation {
        Orientation::Rotate90
        | Orientation::Rotate270
        | Orientation::Rotate90FlipH
        | Orientation::Rotate270FlipH => (decoded.source_height, decoded.source_width),
        _ => (decoded.source_width, decoded.source_height),
    };
    Pixels {
        image: flatten(image),
        source_width,
        source_height,
    }
}

/// Composites transparency onto [`MATTE`] and drops the alpha channel.
fn flatten(image: DynamicImage) -> RgbImage {
    let rgba = image.into_rgba8();
    let (width, height) = rgba.dimensions();
    let mut rgb = RgbImage::new(width, height);
    for (out, px) in rgb.pixels_mut().zip(rgba.pixels()) {
        let alpha = u32::from(px[3]);
        for channel in 0..3 {
            let value = u32::from(px[channel]) * alpha + u32::from(MATTE[channel]) * (255 - alpha);
            out[channel] = (value / 255) as u8;
        }
    }
    rgb
}

fn exif_orientation(path: &Path) -> Option<Orientation> {
    let file = File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let exif = exif::Reader::new().read_from_container(&mut reader).ok()?;
    let _ = reader.seek(SeekFrom::Start(0));
    orientation_from_exif(&exif)
}

fn orientation_from_exif(exif: &exif::Exif) -> Option<Orientation> {
    let field = exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)?;
    let value = field.value.get_uint(0)?;
    Orientation::from_exif(u8::try_from(value).ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};

    fn gradient(width: u32, height: u32) -> DynamicImage {
        DynamicImage::ImageRgba8(ImageBuffer::from_fn(width, height, |x, y| {
            let alpha = if x < width / 2 { 255 } else { 0 };
            Rgba([(x % 256) as u8, (y % 256) as u8, 128, alpha])
        }))
    }

    #[test]
    fn png_is_downscaled_and_flattened() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.png");
        gradient(1200, 400).save(&path).unwrap();
        let pixels = thumbnail_pixels(&path, 256).unwrap();
        assert_eq!((pixels.source_width, pixels.source_height), (1200, 400));
        assert_eq!(pixels.image.width(), 256);
        assert!(pixels.image.height() <= 86);
        // The right half was transparent: it is the matte now.
        let px = pixels.image.get_pixel(250, 40);
        assert_eq!(px.0, MATTE);
    }

    #[test]
    fn small_images_are_not_upscaled() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("small.png");
        gradient(40, 30).save(&path).unwrap();
        let pixels = thumbnail_pixels(&path, 256).unwrap();
        assert_eq!((pixels.image.width(), pixels.image.height()), (40, 30));
    }

    #[test]
    fn orientation_swaps_dimensions() {
        let decoded = Decoded {
            image: gradient(100, 50),
            source_width: 100,
            source_height: 50,
        };
        let pixels = finish(decoded, Orientation::Rotate90, 256);
        assert_eq!((pixels.image.width(), pixels.image.height()), (50, 100));
        assert_eq!((pixels.source_width, pixels.source_height), (50, 100));
    }

    #[test]
    fn unreadable_gives_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("not-an-image.png");
        std::fs::write(&path, b"nope").unwrap();
        assert!(thumbnail_pixels(&path, 256).is_none());
        assert!(thumbnail_pixels(&dir.path().join("missing.png"), 256).is_none());
    }

    #[test]
    fn cover_bytes_decode() {
        let mut bytes = Vec::new();
        gradient(300, 300)
            .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();
        let pixels = thumbnail_from_bytes(&bytes, 64).unwrap();
        assert_eq!((pixels.image.width(), pixels.image.height()), (64, 64));
    }
}
