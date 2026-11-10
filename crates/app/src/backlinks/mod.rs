//! The notes linking to the one showing, beside the editor.

use std::collections::HashMap;

use gpui::{Context, Entity, EventEmitter, SharedString, Subscription};
use inkra_index::{Backlink, Slot};
use inkra_vault::{EntryId, Vault};

use crate::index::{IndexSync, IndexWatch, NotesChanged};
use crate::one_line::one_line;
use crate::open::FileActivated;
use crate::row_text::LitText;

#[cfg(test)]
mod context_tests;
mod job;
mod render;
#[cfg(test)]
mod render_tests;
#[cfg(test)]
mod stale_tests;
#[cfg(test)]
mod tests;

use job::{Job, READ_SOURCES};

impl EventEmitter<FileActivated> for BacklinksPanel {}

pub(crate) struct BacklinksPanel {
    links: Entity<IndexSync>,
    vault: Entity<Vault>,
    file: Option<EntryId>,
    /// Rebuilt when the index or the file changes, never in a draw: `backlinks` walks every
    /// link sharing the file's name.
    rows: Vec<Row>,
    /// Each source read since the file was set.
    slots: HashMap<EntryId, Slot>,
    /// Reads the sources with no slot, while the panel is drawn.
    job: Option<Job>,
    building: bool,
    /// Throttled: rebuilding on every load's notify repeats `backlinks` hundreds of times.
    watch: IndexWatch,
    #[cfg(test)]
    rebuilds: usize,
    _subscriptions: Vec<Subscription>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
    backlink: Backlink,
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
            slots: HashMap::new(),
            job: None,
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
        if file != self.file {
            self.slots.clear();
            self.job = None;
        }
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
        let mut pending = Vec::new();
        let rows: Vec<Row> = self
            .file
            .map(|file| links.index().backlinks(file))
            .unwrap_or_default()
            .into_iter()
            // A load may notify after the vault changed and before its build applies.
            .filter_map(|backlink| Some((name(backlink.source())?.into(), backlink)))
            .enumerate()
            .map(|(index, (name, backlink))| {
                let occurrences = if index < READ_SOURCES {
                    let slot = slot(&mut self.slots, links, &backlink);
                    if slot.is_none() {
                        pending.push(backlink.clone());
                    }
                    occurrences(&backlink, slot)
                } else {
                    Vec::new()
                };
                Row {
                    backlink,
                    name,
                    occurrences,
                }
            })
            .collect();
        let mut restarted = false;
        if pending.is_empty() {
            self.job = None;
        } else if !self.job.as_ref().is_some_and(|job| job.covers(&pending)) {
            self.job = Some(Job::new(pending));
            restarted = true;
        }
        // A redraw reshapes the whole editor; most index changes leave this panel as it was. A new
        // job starts only in a draw.
        let shown = |rows: &[Row], building: bool| rows.is_empty() && building;
        let changed = restarted
            || rows != self.rows
            || shown(&rows, building) != shown(&self.rows, self.building);
        self.rows = rows;
        self.building = building;
        if changed {
            cx.notify();
        }
    }
}

/// `backlink`'s valid slot, else a `Read` landed from the text cache; `None` leaves it to the job.
fn slot<'a>(
    slots: &'a mut HashMap<EntryId, Slot>,
    links: &IndexSync,
    backlink: &Backlink,
) -> Option<&'a Slot> {
    let id = backlink.source();
    if !slots.get(&id).is_some_and(|slot| slot.is_valid(backlink)) {
        let read = links.cached(backlink.candidate())?;
        let slot = Slot::land(None, backlink, &read).slot;
        if !matches!(slot, Slot::Read { .. }) {
            return None;
        }
        slots.insert(id, slot);
    }
    slots.get(&id)
}

/// One per occurrence: its context from `slot`, else an empty line until a read lands.
fn occurrences(backlink: &Backlink, slot: Option<&Slot>) -> Vec<Occurrence> {
    match slot {
        Some(Slot::Read { contexts, .. }) => contexts
            .iter()
            .map(|context| Occurrence {
                context: LitText::context(&context.text, [context.link.clone()]),
            })
            .collect(),
        Some(Slot::NoContext { .. } | Slot::Failed { .. }) | None => backlink
            .occurrences()
            .map(|_| Occurrence {
                context: LitText::default(),
            })
            .collect(),
    }
}
