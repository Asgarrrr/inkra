//! The notes linking to the one showing, beside the editor.

use gpui::{Context, Entity, EventEmitter, SharedString, Subscription};
use inkra_vault::{EntryId, Vault};

use crate::index::{IndexSync, IndexWatch, NotesChanged};
use crate::one_line::one_line;
use crate::open::FileActivated;
use crate::row_text::LitText;

mod render;
#[cfg(test)]
mod tests;

impl EventEmitter<FileActivated> for BacklinksPanel {}

pub(crate) struct BacklinksPanel {
    links: Entity<IndexSync>,
    vault: Entity<Vault>,
    file: Option<EntryId>,
    /// Rebuilt when the index or the file changes, never in a draw: `backlinks` walks every
    /// link sharing the file's name.
    rows: Vec<Row>,
    building: bool,
    /// Throttled: rebuilding on every load's notify repeats `backlinks` hundreds of times.
    watch: IndexWatch,
    #[cfg(test)]
    rebuilds: usize,
    _subscriptions: Vec<Subscription>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
    source: EntryId,
    name: SharedString,
    occurrences: Vec<Occurrence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Occurrence {
    /// The link lit inside it.
    context: LitText,
}

impl BacklinksPanel {
    pub(crate) fn new(
        links: Entity<IndexSync>,
        vault: Entity<Vault>,
        cx: &mut Context<Self>,
    ) -> Self {
        // On a build applying, not on the vault's notify, so names and rows come from one table.
        let subscriptions = vec![
            IndexWatch::observe(&links, cx, |panel| &mut panel.watch, Self::rebuild),
            cx.subscribe(&links, |panel, _, _: &NotesChanged, cx| panel.rebuild(cx)),
        ];
        Self {
            links,
            vault,
            file: None,
            rows: Vec::new(),
            building: false,
            watch: IndexWatch::default(),
            #[cfg(test)]
            rebuilds: 0,
            _subscriptions: subscriptions,
        }
    }

    #[cfg(test)]
    pub(crate) fn source_names(&self) -> Vec<String> {
        self.rows.iter().map(|row| row.name.to_string()).collect()
    }

    /// The header's count; `None` with no rows.
    pub(crate) fn count(&self) -> Option<usize> {
        (!self.rows.is_empty()).then_some(self.rows.len())
    }

    pub(crate) fn set_file(&mut self, file: Option<EntryId>, cx: &mut Context<Self>) {
        self.file = file;
        self.rebuild(cx);
    }

    fn rebuild(&mut self, cx: &mut Context<Self>) {
        #[cfg(test)]
        {
            self.rebuilds += 1;
        }
        let links = self.links.read(cx);
        let building = !links.status().is_ready();
        self.watch.seen(links);
        let vault = self.vault.read(cx);
        let name = |id: EntryId| {
            vault
                .entry(id)
                .map(|entry| one_line(&entry.display_name()).into_owned())
        };
        let rows: Vec<Row> = self
            .file
            .map(|file| links.index().backlinks(file))
            .unwrap_or_default()
            .into_iter()
            // A load may notify after the vault changed and before its build applies.
            .filter_map(|backlink| {
                Some(Row {
                    source: backlink.source,
                    name: name(backlink.source)?.into(),
                    occurrences: backlink
                        .links
                        .into_iter()
                        .map(|link| Occurrence {
                            context: LitText::context(&link.context, [link.context_link.clone()]),
                        })
                        .collect(),
                })
            })
            .collect();
        // A redraw reshapes the whole editor; most index changes leave this panel as it was.
        let shown = |rows: &[Row], building: bool| rows.is_empty() && building;
        let changed =
            rows != self.rows || shown(&rows, building) != shown(&self.rows, self.building);
        self.rows = rows;
        self.building = building;
        if changed {
            cx.notify();
        }
    }
}
