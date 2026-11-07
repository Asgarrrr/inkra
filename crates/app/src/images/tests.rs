use std::collections::HashSet;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{Entity, TestAppContext, prelude::*};
use image::codecs::gif::GifEncoder;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{Frame, ImageBuffer, ImageEncoder, ImageFormat, Rgba, RgbaImage};
use inkra_vault::EntryId;
use inkra_vault::mem_fs::MemFs;

use super::decode::decode;
use super::{Images, MAX_IMAGE_BYTES, Slot, png};
use crate::test_support::{entry_id, scan_notes};

#[test]
fn an_image_wider_than_the_texture_limit_is_refused() {
    assert!(decode(&png(16_385, 1), None).is_err());
}

/// Small on disk and inside both side limits, but 288 MB decoded.
#[test]
fn an_image_whose_pixels_exceed_the_allocation_limit_is_refused() {
    let mut bytes = Vec::new();
    let encoder =
        PngEncoder::new_with_quality(&mut bytes, CompressionType::Fast, FilterType::NoFilter);
    ImageBuffer::<Rgba<u16>, _>::from_pixel(6000, 6000, Rgba([0, 0, 0, 0]))
        .write_with_encoder(encoder)
        .expect("encode");

    assert!(decode(&bytes, None).is_err());
}

#[test]
fn a_wide_image_is_scaled_down_to_the_drawn_side() {
    let image = decode(&png(9_000, 10), None).expect("decode");

    assert_eq!(image.size(0).width.0, 4096);
}

#[test]
fn an_animated_gif_keeps_its_first_frame() {
    let mut bytes = Vec::new();
    let mut encoder = GifEncoder::new(&mut bytes);
    let frames = [Rgba([0, 0, 0, 255]), Rgba([255, 255, 255, 255])]
        .map(|pixel| Frame::new(RgbaImage::from_pixel(2, 2, pixel)));
    encoder.encode_frames(frames).expect("encode");
    drop(encoder);

    let image = decode(&bytes, None).expect("decode");

    assert_eq!(image.frame_count(), 1);
}

/// A little-endian TIFF header and one IFD entry: orientation (0x0112) is 6, a quarter turn clockwise.
const QUARTER_TURN_EXIF: [u8; 26] = [
    0x49, 0x49, 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00, 0x01, 0x00, 0x12, 0x01, 0x03, 0x00, 0x01, 0x00,
    0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

#[test]
fn an_exif_orientation_turns_the_image() {
    let mut bytes = Vec::new();
    let mut encoder = PngEncoder::new(&mut bytes);
    encoder
        .set_exif_metadata(QUARTER_TURN_EXIF.to_vec())
        .expect("exif");
    RgbaImage::new(2, 1)
        .write_with_encoder(encoder)
        .expect("encode");

    let size = decode(&bytes, None).expect("decode").size(0);

    assert_eq!((size.width.0, size.height.0), (1, 2));
}

#[test]
fn garbage_bytes_are_refused() {
    assert!(decode(b"not an image", None).is_err());
}

#[test]
fn the_magic_bytes_win_over_the_extension() {
    assert!(decode(&png(4, 3), Some(ImageFormat::Jpeg)).is_ok());
}

#[test]
fn pixels_are_swapped_to_bgra() {
    let mut bytes = Vec::new();
    RgbaImage::from_pixel(1, 1, Rgba([255, 0, 0, 255]))
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .expect("encode");

    let image = decode(&bytes, None).expect("decode");

    assert_eq!(image.as_bytes(0), Some(&[0, 0, 255, 255][..]));
}

/// `files` are `(name, bytes)` in `/vault`; the ids come back in the same order.
fn images(
    cx: &mut TestAppContext,
    files: &[(&str, Vec<u8>)],
) -> (Arc<MemFs>, Entity<Images>, Vec<EntryId>) {
    let fs = Arc::new(MemFs::new());
    for (name, bytes) in files {
        fs.insert_bytes(format!("/vault/{name}"), bytes.clone());
    }
    let notes = scan_notes(cx, fs.clone());
    let ids = files
        .iter()
        .map(|(name, _)| entry_id(&notes, cx, name))
        .collect();
    let images = cx.update(|cx| cx.new(|_| Images::new(fs.clone())));
    (fs, images, ids)
}

fn want(images: &Entity<Images>, id: EntryId, path: Option<&str>, cx: &mut TestAppContext) {
    images.update(cx, |images, cx| {
        images.want(id, path.map(PathBuf::from), cx);
    });
}

fn slot(images: &Entity<Images>, id: EntryId, cx: &mut TestAppContext) -> Option<&'static str> {
    images.read_with(cx, |images, _| {
        images.slot(id).map(|slot| match slot {
            Slot::Loading { .. } => "loading",
            Slot::Ready(_) => "ready",
            Slot::Failed => "failed",
        })
    })
}

#[gpui::test]
fn an_image_is_loading_until_its_decode_lands(cx: &mut TestAppContext) {
    let (_fs, images, ids) = images(cx, &[("photo.png", png(4, 3))]);

    want(&images, ids[0], Some("/vault/photo.png"), cx);
    let before = slot(&images, ids[0], cx);
    cx.run_until_parked();

    assert_eq!(before, Some("loading"));
    assert_eq!(slot(&images, ids[0], cx), Some("ready"));
}

#[gpui::test]
fn an_image_wanted_twice_is_read_once(cx: &mut TestAppContext) {
    let (fs, images, ids) = images(cx, &[("photo.png", png(4, 3))]);

    want(&images, ids[0], Some("/vault/photo.png"), cx);
    cx.run_until_parked();
    want(&images, ids[0], Some("/vault/photo.png"), cx);
    cx.run_until_parked();

    assert_eq!(fs.reads(Path::new("/vault/photo.png")), 1);
}

/// TGA has no magic bytes, so only its extension names its format.
#[gpui::test]
fn a_tga_image_decodes_by_its_extension(cx: &mut TestAppContext) {
    let mut tga = Vec::new();
    RgbaImage::new(2, 2)
        .write_to(&mut Cursor::new(&mut tga), ImageFormat::Tga)
        .expect("encode");
    let (_fs, images, ids) = images(cx, &[("photo.tga", tga)]);

    want(&images, ids[0], Some("/vault/photo.tga"), cx);
    cx.run_until_parked();

    assert_eq!(slot(&images, ids[0], cx), Some("ready"));
}

#[gpui::test]
fn an_image_with_no_path_fails(cx: &mut TestAppContext) {
    let (_fs, images, ids) = images(cx, &[("photo.png", png(4, 3))]);

    want(&images, ids[0], None, cx);

    assert_eq!(slot(&images, ids[0], cx), Some("failed"));
}

#[gpui::test]
fn an_image_that_is_no_image_fails(cx: &mut TestAppContext) {
    let (_fs, images, ids) = images(cx, &[("photo.png", b"not an image".to_vec())]);

    want(&images, ids[0], Some("/vault/photo.png"), cx);
    cx.run_until_parked();

    assert_eq!(slot(&images, ids[0], cx), Some("failed"));
}

#[gpui::test]
fn an_image_one_byte_over_the_read_cap_fails(cx: &mut TestAppContext) {
    let cap = usize::try_from(MAX_IMAGE_BYTES).expect("the cap fits in memory");
    let mut at_cap = png(4, 3);
    at_cap.resize(cap, 0);
    let mut over = at_cap.clone();
    over.push(0);
    let (_fs, images, ids) = images(cx, &[("at.png", at_cap), ("over.png", over)]);

    want(&images, ids[0], Some("/vault/at.png"), cx);
    want(&images, ids[1], Some("/vault/over.png"), cx);
    cx.run_until_parked();

    assert_eq!(slot(&images, ids[0], cx), Some("ready"));
    assert_eq!(slot(&images, ids[1], cx), Some("failed"));
}

#[gpui::test]
fn retain_drops_every_image_not_kept(cx: &mut TestAppContext) {
    let (_fs, images, ids) = images(cx, &[("a.png", png(4, 3)), ("b.png", png(3, 4))]);
    want(&images, ids[0], Some("/vault/a.png"), cx);
    want(&images, ids[1], Some("/vault/b.png"), cx);
    cx.run_until_parked();

    images.update(cx, |images, cx| {
        images.retain(&HashSet::from([ids[1]]), cx);
    });

    assert_eq!(
        images.read_with(cx, |images, _| images.ids()),
        HashSet::from([ids[1]])
    );
    assert_eq!(slot(&images, ids[1], cx), Some("ready"));
}

#[gpui::test]
fn a_load_dropped_before_it_lands_never_lands(cx: &mut TestAppContext) {
    let (fs, images, ids) = images(cx, &[("photo.png", png(4, 3))]);
    let gate = fs.block_read_bytes("/vault/photo.png");
    want(&images, ids[0], Some("/vault/photo.png"), cx);
    cx.run_until_parked();
    let before = slot(&images, ids[0], cx);

    images.update(cx, |images, cx| images.retain(&HashSet::new(), cx));
    gate.release();
    cx.run_until_parked();

    assert_eq!(before, Some("loading"));
    assert_eq!(slot(&images, ids[0], cx), None);
}
