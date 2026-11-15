use std::collections::{HashMap, HashSet};
use std::error::Error;
#[cfg(any(test, feature = "shot"))]
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;

use gpui::{App, Context, RenderImage, Task, prelude::*};
use image::ImageFormat;
use inkra_vault::EntryId;

use crate::vault::BytesRead;
use decode::decode;

mod decode;
#[cfg(test)]
mod tests;

const MAX_IMAGE_BYTES: u64 = 64 << 20;

/// The images tabs show, by file: the image twin of `NoteStore`'s buffers.
#[derive(Default)]
pub(crate) struct Images {
    slots: HashMap<EntryId, Slot>,
}

pub(crate) enum Slot {
    /// Dropping it cancels the decode; a read already on the chain still finishes.
    Loading {
        _load: Task<()>,
    },
    Ready(Arc<RenderImage>),
    /// The path the read failed at; `None` when no path was safe.
    Failed(Option<PathBuf>),
}

impl Images {
    /// Starts a load unless `id` has a slot: `read`, given the byte cap, reads the file; the
    /// decode runs off the foreground.
    pub(crate) fn want(
        &mut self,
        id: EntryId,
        read: impl FnOnce(u64, &mut App) -> Task<Option<BytesRead>>,
        cx: &mut Context<Self>,
    ) {
        if self.slots.contains_key(&id) {
            return;
        }
        let read = read(MAX_IMAGE_BYTES, cx);
        let task = cx.spawn(async move |this, cx| {
            let landed = match read.await {
                Some(read) => Some(cx.background_spawn(async move { decode_read(read) }).await),
                None => None,
            };
            let _ = this.update(cx, |this, cx| {
                let Some(slot @ Slot::Loading { .. }) = this.slots.get_mut(&id) else {
                    return;
                };
                *slot = match landed {
                    Some((_, Ok(image))) => Slot::Ready(Arc::new(image)),
                    Some((path, Err(error))) => {
                        log::warn!("image {}: {error}", path.display());
                        Slot::Failed(Some(path))
                    }
                    None => {
                        log::warn!("image {id:?}: no path to read");
                        Slot::Failed(None)
                    }
                };
                cx.notify();
            });
        });
        self.slots.insert(id, Slot::Loading { _load: task });
    }

    /// Drops each failed slot whose path is no longer `path_of` its id, so the next `want` reads
    /// the new path.
    pub(crate) fn forget_moved(&mut self, path_of: impl Fn(EntryId) -> Option<PathBuf>) {
        self.slots.retain(|&id, slot| match slot {
            Slot::Failed(failed) => *failed == path_of(id),
            Slot::Loading { .. } | Slot::Ready(_) => true,
        });
    }

    pub(crate) fn slot(&self, id: EntryId) -> Option<&Slot> {
        self.slots.get(&id)
    }

    /// Drops every slot not in `keep`, cancelling its load and freeing its atlas entry.
    pub(crate) fn retain(&mut self, keep: &HashSet<EntryId>, cx: &mut Context<Self>) {
        self.slots.retain(|id, slot| {
            if keep.contains(id) {
                return true;
            }
            if let Slot::Ready(image) = slot {
                let image = Arc::clone(image);
                // `drop_image` skips the window being updated.
                cx.defer(move |cx| cx.drop_image(image, None));
            }
            false
        });
    }

    #[cfg(test)]
    pub(crate) fn ids(&self) -> HashSet<EntryId> {
        self.slots.keys().copied().collect()
    }
}

/// The path names the format, and the file in a failure's log.
fn decode_read(
    (path, read): BytesRead,
) -> (PathBuf, Result<RenderImage, Box<dyn Error + Send + Sync>>) {
    let decoded = match read {
        Ok(bytes) => decode(&bytes, ImageFormat::from_path(&path).ok()).map_err(Into::into),
        Err(error) => Err(error.into()),
    };
    (path, decoded)
}

/// A gradient, so a drawn image shows its orientation and its fit.
#[cfg(any(test, feature = "shot"))]
pub(crate) fn png(width: u32, height: u32) -> Vec<u8> {
    let shade = |at: u32, side: u32| u8::try_from(at * 255 / side).unwrap_or(u8::MAX);
    let gradient = image::RgbaImage::from_fn(width, height, |x, y| {
        image::Rgba([shade(x, width), shade(y, height), u8::MAX / 2, u8::MAX])
    });
    let mut bytes = Vec::new();
    gradient
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
        .unwrap_or_else(|error| panic!("encode a PNG: {error}"));
    bytes
}
