//! `⌘P`: the notes whose path fuzzily matches the query, best first.

use std::borrow::Cow;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use gpui::{
    AnyElement, Context, Entity, EventEmitter, IntoElement, SharedString, Subscription, Task,
    actions, div,
};
use inkra_index::{NameMatch, NameQuery, match_names, merge_names};
use inkra_vault::EntryId;

use crate::index::{IndexSync, NotesChanged};
use crate::one_line::one_line;
use crate::picker::{Picker, PickerDelegate};
use crate::sidebar::NoteActivated;

mod render;
#[cfg(test)]
mod tests;

actions!(quick_open, [QuickOpen]);

/// Zed's file finder lists 100 too.
const LIMIT: usize = 100;
/// Waited only while a search runs, so an idle picker answers the first keystroke at once.
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(100);
/// After the names change, as Zed's file finder waits after a worktree update.
const NOTES_CHANGED_REFRESH: Duration = Duration::from_millis(200);

impl EventEmitter<NoteActivated> for Picker<QuickOpenDelegate> {}

pub(crate) struct QuickOpenDelegate {
    links: Entity<IndexSync>,
    showing: Option<EntryId>,
    recent: Vec<EntryId>,
    matches: Vec<NameMatch>,
    /// False until the first result, so no empty text flashes before it.
    searched: bool,
    query_is_empty: bool,
    /// Set by the next query and on drop: a scan already running on another thread ignores a dropped task.
    cancel: Arc<AtomicBool>,
    search_id: u64,
    running: bool,
    /// Set by `NotesChanged` for the next run only.
    next_run: Run,
    _notes_changed: Subscription,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Run {
    Search,
    /// After the names changed: it waits, then keeps the selected note if still listed.
    Refresh,
}

impl QuickOpenDelegate {
    pub(crate) fn new(
        links: &Entity<IndexSync>,
        showing: Option<EntryId>,
        recent: Vec<EntryId>,
        cx: &mut Context<Picker<Self>>,
    ) -> Self {
        // Names change only when a build applies; a load changes none.
        let notes_changed = cx.subscribe(links, |picker, _, _: &NotesChanged, cx| {
            picker.delegate.next_run = Run::Refresh;
            picker.refresh(cx);
        });
        Self {
            links: links.clone(),
            showing,
            recent,
            matches: Vec::new(),
            searched: false,
            query_is_empty: true,
            cancel: Arc::new(AtomicBool::new(false)),
            search_id: 0,
            running: false,
            next_run: Run::Search,
            _notes_changed: notes_changed,
        }
    }

    fn apply(
        picker: &mut Picker<Self>,
        id: u64,
        matches: Vec<NameMatch>,
        query_is_empty: bool,
        run: Run,
        started: Instant,
        cx: &mut Context<Picker<Self>>,
    ) {
        if id != picker.delegate.search_id {
            return;
        }
        // Read now, not when the names changed: the user may have moved during the wait.
        let kept = picker
            .selected()
            .filter(|_| run == Run::Refresh)
            .and_then(|ix| picker.delegate.matches.get(ix))
            .map(|found| found.id);
        let this = &mut picker.delegate;
        this.running = false;
        this.searched = true;
        this.query_is_empty = query_is_empty;
        let select = kept
            .and_then(|kept| matches.iter().position(|found| found.id == kept))
            .unwrap_or(0);
        this.matches = matches;
        picker.matches_updated(select, cx);
        if cfg!(not(debug_assertions)) {
            eprintln!(
                "quick open: {} matches, {:?} from the edit",
                picker.delegate.matches.len(),
                started.elapsed()
            );
        }
    }
}

impl Drop for QuickOpenDelegate {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl PickerDelegate for QuickOpenDelegate {
    fn key_context(&self) -> &'static str {
        "QuickOpen"
    }

    fn label(&self) -> &'static str {
        "Quick open"
    }

    fn placeholder(&self) -> &'static str {
        "Open a note…"
    }

    fn empty_text(&self) -> Option<&'static str> {
        self.searched.then_some(if self.query_is_empty {
            "No recent notes"
        } else {
            "No matching notes"
        })
    }

    fn match_count(&self) -> usize {
        self.matches.len()
    }

    fn is_selectable(&self, _: usize) -> bool {
        true
    }

    fn update_matches(&mut self, query: &str, cx: &mut Context<Picker<Self>>) -> Task<()> {
        self.cancel.store(true, Ordering::Relaxed);
        self.cancel = Arc::new(AtomicBool::new(false));
        self.search_id += 1;
        let (id, cancel) = (self.search_id, Arc::clone(&self.cancel));
        let run = std::mem::replace(&mut self.next_run, Run::Search);
        let wait = match run {
            Run::Refresh => Some(NOTES_CHANGED_REFRESH),
            Run::Search => self.running.then_some(SEARCH_DEBOUNCE),
        };
        self.running = true;
        let names = self.links.read(cx).index().names();
        let query = Arc::new(NameQuery::new(query, self.showing, &self.recent));
        let query_is_empty = query.is_empty();
        let cpus = cx.background_executor().num_cpus();
        let started = Instant::now();
        cx.spawn(async move |picker, cx| {
            if let Some(wait) = wait {
                cx.background_executor().timer(wait).await;
            }
            let scans: Vec<Task<Vec<NameMatch>>> = names
                .chunks(cpus)
                .into_iter()
                .map(|range| {
                    let (names, query, cancel) =
                        (names.clone(), Arc::clone(&query), Arc::clone(&cancel));
                    cx.background_executor()
                        .spawn(async move { match_names(&names, range, &query, &cancel, LIMIT) })
                })
                .collect();
            let mut found = Vec::with_capacity(scans.len());
            for scan in scans {
                found.push(scan.await);
            }
            let matches = merge_names(found, LIMIT);
            let _ = picker.update(cx, |picker, cx| {
                Self::apply(picker, id, matches, query_is_empty, run, started, cx);
            });
        })
    }

    fn confirm(&mut self, ix: usize, cx: &mut Context<Picker<Self>>) {
        if let Some(found) = self.matches.get(ix) {
            cx.emit(NoteActivated(found.id));
        }
    }

    fn render_row(&self, ix: usize, _: bool, _: &mut Context<Picker<Self>>) -> AnyElement {
        self.matches
            .get(ix)
            .map_or_else(|| div().into_any_element(), render::render_row)
    }

    fn row_label(&self, ix: usize) -> SharedString {
        self.matches
            .get(ix)
            .map(|found| match one_line(&found.path) {
                Cow::Borrowed(_) => found.path.clone().into(),
                Cow::Owned(line) => line.into(),
            })
            .unwrap_or_default()
    }
}
