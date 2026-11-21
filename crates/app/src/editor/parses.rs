//! One parse per buffer version, shared by every view of that buffer (splits spec, S2).

use std::collections::HashMap;
use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::rc::Rc;

use gpui::{App, BorrowAppContext, Entity, EntityId, Global, Subscription};
use inkra_markdown::Markdown;
use inkra_text::{Buffer, Version};

#[derive(Default)]
pub(crate) struct Parses(HashMap<EntityId, Parse>);

impl Global for Parses {}

struct Parse {
    version: Version,
    markdown: Rc<Markdown>,
    _released: Subscription,
}

/// A miss parses once; a parse drops at its buffer's release.
pub(crate) fn parsed(buffer: &Entity<Buffer>, cx: &mut App) -> Rc<Markdown> {
    let (id, version) = (buffer.entity_id(), buffer.read(cx).version());
    if let Some(parse) = cx.default_global::<Parses>().0.get(&id)
        && parse.version == version
    {
        return Rc::clone(&parse.markdown);
    }
    let markdown = Rc::new(inkra_markdown::parse(buffer.read(cx).text()));
    cx.update_global(|parses: &mut Parses, cx| match parses.0.entry(id) {
        Occupied(mut known) => {
            let parse = known.get_mut();
            parse.version = version;
            parse.markdown = Rc::clone(&markdown);
        }
        Vacant(new) => {
            new.insert(Parse {
                version,
                markdown: Rc::clone(&markdown),
                _released: cx.observe_release(buffer, move |_, cx| {
                    cx.global_mut::<Parses>().0.remove(&id);
                }),
            });
        }
    });
    markdown
}

#[cfg(test)]
pub(crate) fn cached(cx: &App) -> usize {
    cx.try_global::<Parses>().map_or(0, |parses| parses.0.len())
}
