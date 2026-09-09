//! Windows Imaging Component: decodes whatever codecs are installed (HEIC
//! and AVIF need the HEIF / AV1 extensions from the Store), scales with the
//! Fant filter and converts to straight RGBA. Orientation is left to the
//! caller.

#![allow(unsafe_code)]

use crate::media::decode::Decoded;
use std::path::Path;
use windows::Win32::Foundation::{GENERIC_READ, RPC_E_CHANGED_MODE};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppRGBA, IWICBitmapSource, IWICImagingFactory,
    WICBitmapDitherTypeNone, WICBitmapInterpolationModeFant, WICBitmapPaletteTypeCustom,
    WICDecodeMetadataCacheOnDemand,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::core::{HSTRING, PCWSTR};

pub(super) fn decode_thumbnail(path: &Path, max_px: u32) -> Option<Decoded> {
    // SAFETY: COM calls on this thread only; every interface is released by
    // its wrapper's Drop before `CoUninitialize`.
    unsafe {
        let init = CoInitializeEx(None, COINIT_MULTITHREADED);
        let initialised = init.is_ok();
        if init.is_err() && init != RPC_E_CHANGED_MODE {
            return None;
        }
        let result = decode_inner(path, max_px);
        if initialised {
            CoUninitialize();
        }
        result
    }
}

unsafe fn decode_inner(path: &Path, max_px: u32) -> Option<Decoded> {
    // SAFETY: every call below is a COM call on interfaces this function
    // created itself; `buffer` is exactly `stride * height` bytes.
    unsafe {
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()?;
        let file = HSTRING::from(path.as_os_str());
        let decoder = factory
            .CreateDecoderFromFilename(
                PCWSTR(file.as_ptr()),
                None,
                GENERIC_READ,
                WICDecodeMetadataCacheOnDemand,
            )
            .ok()?;
        let frame = decoder.GetFrame(0).ok()?;
        let (mut source_width, mut source_height) = (0u32, 0u32);
        frame.GetSize(&mut source_width, &mut source_height).ok()?;
        if source_width == 0 || source_height == 0 {
            return None;
        }
        let (width, height) = fit(source_width, source_height, max_px);

        let source: IWICBitmapSource = if (width, height) != (source_width, source_height) {
            let scaler = factory.CreateBitmapScaler().ok()?;
            scaler
                .Initialize(&frame, width, height, WICBitmapInterpolationModeFant)
                .ok()?;
            scaler.into()
        } else {
            frame.into()
        };
        let converter = factory.CreateFormatConverter().ok()?;
        converter
            .Initialize(
                &source,
                &GUID_WICPixelFormat32bppRGBA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeCustom,
            )
            .ok()?;
        let stride = width.checked_mul(4)?;
        let mut buffer = vec![
            0u8;
            usize::try_from(stride)
                .ok()?
                .checked_mul(usize::try_from(height).ok()?)?
        ];
        converter
            .CopyPixels(std::ptr::null(), stride, &mut buffer)
            .ok()?;
        Decoded::from_rgba(width, height, buffer, source_width, source_height)
    }
}

/// Scales `(w, h)` so the long side is at most `max_px`, never upscaling.
fn fit(width: u32, height: u32, max_px: u32) -> (u32, u32) {
    let long = width.max(height);
    if long <= max_px {
        return (width, height);
    }
    let scale = f64::from(max_px) / f64::from(long);
    let w = ((f64::from(width) * scale).round() as u32).max(1);
    let h = ((f64::from(height) * scale).round() as u32).max(1);
    (w, h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageBuffer, Rgba};

    #[test]
    fn fit_keeps_aspect() {
        assert_eq!(fit(800, 200, 256), (256, 64));
        assert_eq!(fit(200, 800, 256), (64, 256));
        assert_eq!(fit(100, 50, 256), (100, 50));
    }

    #[test]
    fn wic_decodes_png() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.png");
        DynamicImage::ImageRgba8(ImageBuffer::from_fn(800, 200, |_, _| {
            Rgba([200, 30, 30, 255])
        }))
        .save(&path)
        .unwrap();
        let decoded = decode_thumbnail(&path, 256).unwrap();
        assert_eq!((decoded.source_width, decoded.source_height), (800, 200));
        assert_eq!((decoded.image.width(), decoded.image.height()), (256, 64));
        let px = decoded.image.to_rgba8().get_pixel(10, 10).0;
        assert!(px[0] > 150 && px[1] < 80, "{px:?}");
    }
}
