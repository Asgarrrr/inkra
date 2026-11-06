//! One parse per buffer version, shared by every view of that buffer (splits spec, S2).

use std::collections::HashMap;
use std::rc::Rc;

use gpui::{App, Entity, EntityId, Global, WeakEntity};
use inkra_markdown::Markdown;
use inkra_text::{Buffer, Version};

#[derive(Default)]
pub(crate) struct Parses(HashMap<EntityId, Parse>);

impl Global for Parses {}

struct Parse {
    version: Version,
    markdown: Rc<Markdown>,
    buffer: WeakEntity<Buffer>,
}

/// A miss parses once and drops the parses of released buffers.
pub(crate) fn parsed(buffer: &Entity<Buffer>, cx: &mut App) -> Rc<Markdown> {
    let (id, version) = (buffer.entity_id(), buffer.read(cx).version());
    if let Some(parse) = cx.default_global::<Parses>().0.get(&id)
        && parse.version == version
    {
        return Rc::clone(&parse.markdown);
    }
    let markdown = Rc::new(inkra_markdown::parse(buffer.read(cx).text()));
    let parses = &mut cx.default_global::<Parses>().0;
    parses.retain(|_, parse| parse.buffer.is_upgradable());
    parses.insert(
        id,
        Parse {
            version,
            markdown: Rc::clone(&markdown),
            buffer: buffer.downgrade(),
        },
    );
    markdown
}
