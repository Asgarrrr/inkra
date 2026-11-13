//! The notes linking to the one showing, beside the editor.

use std::collections::HashMap;

use gpui::{App, Context, Entity, EventEmitter, SharedString, Subscription, actions};
use inkra_index::{Backlink, Failures, Slot};
use inkra_layout::{Side, ViewId};
use inkra_vault::{EntryId, Vault};

use crate::feature::{FeatureId, Manifest};
use crate::index::{IndexSync, IndexWatch, NotesChanged};
use crate::one_line::one_line;
use crate::open::FileActivated;
use crate::pane::{Pane, PaneDecl, PaneHeader, boxed};
use crate::row_text::LitText;
use crate::shell::FeatureCx;
use crate::ui::IconName;

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

use job::{Job, READ_SOURCES, Wanted};

actions!(docks, [MoveBacklinksToOtherDock]);

pub(crate) const BACKLINKS: ViewId = ViewId::new("backlinks");

pub(crate) static BACKLINKS_PANE: PaneDecl = PaneDecl {
    id: BACKLINKS,
    title: "Backlinks",
    icon: IconName::Link,
    icon_id: "dock-icon-backlinks",
    header_id: "panel-header-backlinks",
    side: Side::Right,
    move_action: boxed::<MoveBacklinksToOtherDock>,
};

pub(crate) static MANIFEST: Manifest = Manifest {
    id: FeatureId::new("backlinks"),
    default_on: true,
    after_first_frame: false,
    commands: &[],
    panes: &[&BACKLINKS_PANE],
    buttons: &[],
    activate,
};

/// Activated once its pane shows, so `shown()` is read late and `NoteShown` keeps it current.
fn activate(fcx: &mut FeatureCx) {
    let (Some(index), Some(vault)) = (fcx.index(), fcx.vault()) else {
        return;
    };
    let panel = fcx.new(|cx| BacklinksPanel::new(index, vault, cx));
    // A note linking to itself lists itself.
    fcx.subscribe(&panel, |event: &FileActivated, fcx| {
        fcx.open(event.file, None, event.open);
    });
    let shown = fcx.shown();
    fcx.update(&panel, |panel, cx| panel.set_file(shown, cx));
    fcx.on_note_shown({
        let panel = panel.clone();
        move |file, fcx| fcx.update(&panel, |panel, cx| panel.set_file(file, cx))
    });
    fcx.register_pane(BACKLINKS, panel);
}

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
    /// Since the file was set: one stop per note, not per job.
    failures: Failures,
    building: bool,
    /// Throttled: rebuilding on every load's notify repeats `backlinks` hundreds of times.
    watch: IndexWatch,
    hidden: bool,
    /// A rebuild was skipped while hidden: showing rebuilds once.
    stale: bool,
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

/// One context line, split at its link so a narrow panel cuts the text before it, not the link.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Occurrence {
    before: SharedString,
    /// The link, lit, and the text after it.
    link: LitText,
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
            failures: Failures::default(),
            building: false,
            watch: IndexWatch::default(),
            hidden: false,
            stale: false,
            #[cfg(test)]
            rebuilds: 0,
            _subscriptions: subscriptions,
        }
    }

    #[cfg(test)]
    pub(crate) fn source_names(&self) -> Vec<String> {
        self.rows.iter().map(|row| row.name.to_string()).collect()
    }

    #[cfg(test)]
    pub(crate) fn rebuilds(&self) -> usize {
        self.rebuilds
    }

    /// The header's count; `None` with no rows.
    pub(crate) fn count(&self) -> Option<usize> {
        (!self.rows.is_empty()).then_some(self.rows.len())
    }

    pub(crate) fn set_file(&mut self, file: Option<EntryId>, cx: &mut Context<Self>) {
        if file != self.file {
            self.slots.clear();
            self.job = None;
            self.failures = Failures::default();
        }
        self.file = file;
        self.rebuild(cx);
    }

    fn rebuild(&mut self, cx: &mut Context<Self>) {
        if self.hidden {
            self.stale = true;
            return;
        }
        #[cfg(test)]
        {
            self.rebuilds += 1;
        }
        let links = self.links.read(cx);
        let building = self.file.is_some() && !links.status().is_ready();
        self.watch.seen(links);
        let vault = self.vault.read(cx);
        let name = |id: EntryId| {
            vault
                .entry(id)
                .map(|entry| one_line(&entry.display_name()).into_owned())
        };
        let mut pending = HashMap::new();
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
                        let backlink = backlink.clone();
                        pending.insert(
                            backlink.source(),
                            Wanted {
                                row: index,
                                backlink,
                            },
                        );
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
        match &mut self.job {
            _ if pending.is_empty() => self.job = None,
            Some(job) if job.covers(&pending) => job.narrow(pending),
            _ => {
                self.job = Some(Job::new(pending));
                // Only a draw starts a job, and these rows may draw as they did.
                cx.notify();
            }
        }
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
            .map(|context| {
                let (before, link) =
                    LitText::context(&context.text, [context.link.clone()]).split_at_first();
                Occurrence { before, link }
            })
            .collect(),
        Some(Slot::NoContext { .. } | Slot::Failed { .. }) | None => backlink
            .occurrences()
            .map(|_| Occurrence {
                before: SharedString::default(),
                link: LitText::default(),
            })
            .collect(),
    }
}

impl Pane for BacklinksPanel {
    fn header(&self, _: &App) -> PaneHeader {
        PaneHeader::Title {
            count: self.count(),
        }
    }

    fn hidden(&mut self, _: &mut Context<Self>) {
        self.hidden = true;
    }

    fn shown(&mut self, cx: &mut Context<Self>) {
        self.hidden = false;
        if std::mem::take(&mut self.stale) {
            self.rebuild(cx);
        }
    }
}
