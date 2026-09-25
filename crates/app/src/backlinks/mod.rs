//! The notes linking to the one showing, beside the editor.

use std::ops::Range;
use std::time::Duration;

use gpui::{Context, Entity, EventEmitter, SharedString, Subscription, Task};
use inkra_vault::EntryId;

use crate::index::{IndexSync, NotesChanged};
use crate::one_line::one_line;
use crate::sidebar::NoteActivated;
use crate::vault::NoteStore;

mod render;
#[cfg(test)]
mod tests;

impl EventEmitter<NoteActivated> for BacklinksPanel {}

pub(crate) struct BacklinksPanel {
    links: Entity<IndexSync>,
    notes: Entity<NoteStore>,
    note: Option<EntryId>,
    /// Rebuilt when the index or the note changes, never in a draw: `backlinks` walks every
    /// link sharing the note's name.
    rows: Vec<Row>,
    building: bool,
    /// The index revision and building state the rows were computed at.
    seen: Option<(u64, bool)>,
    /// While the index builds, each loaded note notifies; rebuilding on every one would repeat
    /// a heavily linked note's `backlinks` hundreds of times.
    throttled: Option<Task<()>>,
    #[cfg(test)]
    rebuilds: usize,
    _subscriptions: Vec<Subscription>,
}

const BUILDING_REFRESH: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
    source: EntryId,
    name: SharedString,
    occurrences: Vec<Occurrence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Occurrence {
    context: SharedString,
    /// The link inside `context`.
    link: Range<usize>,
}

impl BacklinksPanel {
    pub(crate) fn new(
        links: Entity<IndexSync>,
        notes: Entity<NoteStore>,
        cx: &mut Context<Self>,
    ) -> Self {
        // On a build applying, not on the vault's notify, so names and rows come from one table.
        let subscriptions = vec![
            cx.observe(&links, |panel, _, cx| panel.refresh(cx)),
            cx.subscribe(&links, |panel, _, _: &NotesChanged, cx| panel.rebuild(cx)),
        ];
        Self {
            links,
            notes,
            note: None,
            rows: Vec::new(),
            building: false,
            seen: None,
            throttled: None,
            #[cfg(test)]
            rebuilds: 0,
            _subscriptions: subscriptions,
        }
    }

    #[cfg(test)]
    pub(crate) fn source_names(&self) -> Vec<String> {
        self.rows.iter().map(|row| row.name.to_string()).collect()
    }

    pub(crate) fn set_note(&mut self, note: Option<EntryId>, cx: &mut Context<Self>) {
        self.note = note;
        self.rebuild(cx);
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let links = self.links.read(cx);
        // `NotesChanged` already rebuilt for the notify that follows it.
        if self.seen == Some((links.revision(), links.is_building())) {
            return;
        }
        if !links.is_building() {
            self.rebuild(cx);
            return;
        }
        if self.throttled.is_some() {
            return;
        }
        self.throttled = Some(cx.spawn(async move |panel, cx| {
            cx.background_executor().timer(BUILDING_REFRESH).await;
            let _ = panel.update(cx, Self::rebuild);
        }));
    }

    fn rebuild(&mut self, cx: &mut Context<Self>) {
        self.throttled = None;
        #[cfg(test)]
        {
            self.rebuilds += 1;
        }
        let links = self.links.read(cx);
        let building = links.is_building();
        self.seen = Some((links.revision(), building));
        let vault = self.notes.read(cx).vault().map(|vault| vault.read(cx));
        let name = |id: EntryId| {
            vault
                .and_then(|vault| vault.entry(id))
                .map(|entry| one_line(&entry.path.display_name()).into_owned())
        };
        let rows: Vec<Row> = self
            .note
            .map(|note| links.index().backlinks(note))
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
                            context: link.context.clone().into(),
                            link: link.context_link.clone(),
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
