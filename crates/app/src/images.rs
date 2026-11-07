use std::collections::{HashMap, HashSet};
use std::error::Error;
#[cfg(any(test, feature = "shot"))]
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{Context, RenderImage, Task, prelude::*};
use image::ImageFormat;
use inkra_vault::{EntryId, Fs};

use decode::decode;

mod decode;
#[cfg(test)]
mod tests;

const MAX_IMAGE_BYTES: u64 = 64 << 20;

/// The images tabs show, by file: the image twin of `NoteStore`'s buffers.
pub(crate) struct Images {
    fs: Arc<dyn Fs>,
    slots: HashMap<EntryId, Slot>,
}

pub(crate) enum Slot {
    /// Dropping the load cancels it.
    Loading {
        _load: Task<()>,
    },
    Ready(Arc<RenderImage>),
    Failed,
}

impl Images {
    pub(crate) fn new(fs: Arc<dyn Fs>) -> Self {
        Self {
            fs,
            slots: HashMap::new(),
        }
    }

    /// Starts a load unless `id` has a slot; the read and the decode run off the foreground.
    pub(crate) fn want(&mut self, id: EntryId, path: Option<PathBuf>, cx: &mut Context<Self>) {
        if self.slots.contains_key(&id) {
            return;
        }
        let Some(path) = path else {
            self.slots.insert(id, Slot::Failed);
            return;
        };
        let fs = Arc::clone(&self.fs);
        let task = cx.spawn(async move |this, cx| {
            let (path, decoded) = cx
                .background_spawn(async move {
                    let decoded = load(fs.as_ref(), &path).await;
                    (path, decoded)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let Some(slot @ Slot::Loading { .. }) = this.slots.get_mut(&id) else {
                    return;
                };
                *slot = match decoded {
                    Ok(image) => Slot::Ready(Arc::new(image)),
                    Err(error) => {
                        log::warn!("image {}: {error}", path.display());
                        Slot::Failed
                    }
                };
                cx.notify();
            });
        });
        self.slots.insert(id, Slot::Loading { _load: task });
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

async fn load(fs: &dyn Fs, path: &Path) -> Result<RenderImage, Box<dyn Error + Send + Sync>> {
    let bytes = fs.read_bytes(path, MAX_IMAGE_BYTES).await?;
    Ok(decode(&bytes, ImageFormat::from_path(path).ok())?)
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
