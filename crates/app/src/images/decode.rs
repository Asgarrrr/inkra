use std::io::Cursor;

use gpui::RenderImage;
use image::metadata::Orientation;
use image::{DynamicImage, Frame, ImageDecoder, ImageFormat, ImageReader, Limits};

/// The GPU texture limit: a larger side leaks the atlas.
const MAX_SIDE: u32 = 16_384;
/// Wider than any window, so a fitted image is never upscaled from it.
const DRAWN_SIDE: u32 = 4096;
const MAX_ALLOC: u64 = 256 << 20;

/// The first frame only, oriented and in gpui's BGRA. The magic bytes name the format; `named`,
/// the extension's, is the fallback for a format that has none (TGA).
pub(crate) fn decode(bytes: &[u8], named: Option<ImageFormat>) -> image::ImageResult<RenderImage> {
    let mut reader = ImageReader::new(Cursor::new(bytes));
    if let Some(format) = named {
        reader.set_format(format);
    }
    // A failed guess keeps the format set above.
    let mut reader = reader.with_guessed_format()?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    reader.limits(limits.clone());
    let mut decoder = reader.into_decoder()?;
    // `into_decoder` checks the sides only; the pixel buffer is charged here.
    limits.reserve(decoder.total_bytes())?;
    decoder.set_limits(limits)?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let mut image = DynamicImage::from_decoder(decoder)?;
    // Before the rotation: the scale is uniform, and a rotated copy of the full image doubles the peak.
    if image.width().max(image.height()) > DRAWN_SIDE {
        image = image.thumbnail(DRAWN_SIDE, DRAWN_SIDE);
    }
    image.apply_orientation(orientation);
    let mut buffer = image.into_rgba8();
    for pixel in buffer.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Ok(RenderImage::new([Frame::new(buffer)]))
}
