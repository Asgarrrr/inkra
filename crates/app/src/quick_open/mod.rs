//! `⌘P`: the notes with a word of their name or text starting with every term, best first.

use std::ops::Range;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, IntoElement, Pixels, SharedString,
    Subscription, Task, actions, div,
};
use inkra_index::{IndexSnapshot, NoteMatch, NoteQuery, find_notes, merge_notes};
use inkra_vault::EntryId;

use crate::index::{IndexSync, NotesChanged, fan_out};
use crate::picker::runs::{Run, RunId, Runs, SEARCH_DEBOUNCE};
use crate::picker::{Picker, PickerDelegate};
use crate::sidebar::NoteActivated;
use crate::theme;
use list::List;

mod list;
mod render;
#[cfg(test)]
mod tests;

actions!(quick_open, [QuickOpen]);

/// Zed's file finder lists 100 too.
const LIMIT: usize = 100;
/// After the names change, as Zed's file finder waits after a worktree update.
pub(crate) const NOTES_CHANGED_REFRESH: Duration = Duration::from_millis(200);

const SCANNING: &str = "Scanning the vault…";

impl EventEmitter<NoteActivated> for Picker<QuickOpenDelegate> {}

pub(crate) struct QuickOpenDelegate {
    links: Entity<IndexSync>,
    showing: Option<EntryId>,
    recent: Vec<EntryId>,
    list: List,
    answered: Answered,
    runs: Runs,
    /// Each excerpt, once computed, waits for it before it lands, so a test can order landings.
    #[cfg(test)]
    hold_excerpts: Option<futures::future::Shared<futures::channel::oneshot::Receiver<()>>>,
    /// User runs applied: the ones the release build logs.
    #[cfg(test)]
    logged: usize,
    _subscriptions: Vec<Subscription>,
}

/// What the empty text answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Answered {
    /// Before the first result, so no empty text flashes before it.
    Nothing,
    /// The vault's first scan and build have not applied: there are no names to search.
    Scanning,
    /// An empty query: the recent notes.
    Recent,
    Query,
}

/// One run of `update_matches`, applied once its matches arrive.
struct Search {
    id: RunId,
    run: Run,
    answers: Answered,
    started: Instant,
    snapshot: IndexSnapshot,
    query: Arc<NoteQuery>,
}

impl QuickOpenDelegate {
    pub(crate) fn new(
        links: &Entity<IndexSync>,
        showing: Option<EntryId>,
        recent: Vec<EntryId>,
        cx: &mut Context<Picker<Self>>,
    ) -> Self {
        let subscriptions = vec![
            cx.subscribe(links, |picker, _, _: &NotesChanged, cx| {
                // The first names after the scan list at once: the user is waiting, and nothing is selected.
                let run = if picker.delegate.answered == Answered::Scanning {
                    Run::Search
                } else {
                    Run::Refresh
                };
                Runs::refresh_or_defer(picker, run, cx);
            }),
            Runs::observe(links, cx),
        ];
        let list = List::new(
            links.read(cx).index().snapshot(),
            Arc::new(NoteQuery::new("", showing, &recent)),
        );
        Self {
            links: links.clone(),
            showing,
            recent,
            list,
            answered: Answered::Nothing,
            runs: Runs::default(),
            #[cfg(test)]
            hold_excerpts: None,
            #[cfg(test)]
            logged: 0,
            _subscriptions: subscriptions,
        }
    }

    fn apply(
        picker: &mut Picker<Self>,
        search: Search,
        found: Vec<NoteMatch>,
        cx: &mut Context<Picker<Self>>,
    ) {
        let Search {
            id,
            run,
            answers,
            started,
            snapshot,
            query,
        } = search;
        if !picker.delegate.runs.is_current(id) {
            return;
        }
        let user = run.is_user();
        // Read now, not when the names changed: the user may have moved during the wait.
        let kept = picker
            .selected()
            .filter(|_| !user)
            .and_then(|ix| picker.delegate.list.rows.get(ix))
            .map(|row| row.found.id)
            .and_then(|kept| found.iter().position(|found| found.id == kept));
        let this = &mut picker.delegate;
        this.answered = answers;
        this.list.replace(id, snapshot, query, found, !user);
        match kept {
            Some(ix) => picker.matches_refreshed(ix, cx),
            // A new query, or the kept note is gone: the first row, scrolled to.
            None => picker.matches_updated(0, cx),
        }
        if user {
            #[cfg(test)]
            {
                picker.delegate.logged += 1;
            }
            if cfg!(not(debug_assertions)) {
                eprintln!(
                    "quick open: {} matches, {:?} from the edit",
                    picker.delegate.list.rows.len(),
                    started.elapsed()
                );
            }
        }
        // Read after the rows land: an `enter` waiting on them confirms, and that clears the mark.
        Runs::landed(picker, cx);
    }

    fn footer(&self, cx: &App) -> Option<SharedString> {
        self.links.read(cx).indexing_footer()
    }
}

impl AsMut<Runs> for QuickOpenDelegate {
    fn as_mut(&mut self) -> &mut Runs {
        &mut self.runs
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

    fn empty_text(&self, _: &App) -> Option<SharedString> {
        let text = match self.answered {
            Answered::Nothing => None,
            Answered::Scanning => Some(SCANNING),
            Answered::Recent => Some("No recent notes"),
            Answered::Query => Some("No matching notes"),
        };
        text.map(SharedString::from)
    }

    fn match_count(&self) -> usize {
        self.list.rows.len()
    }

    fn is_selectable(&self, _: usize) -> bool {
        true
    }

    fn update_matches(&mut self, query: &str, cx: &mut Context<Picker<Self>>) -> Task<()> {
        let started = self.runs.start(self.links.read(cx));
        let (id, run) = (started.id, started.run);
        if run.is_user() {
            self.list.cancel();
        }
        let snapshot = self.links.read(cx).index().snapshot();
        let query = Arc::new(NoteQuery::new(query, self.showing, &self.recent));
        if self.links.read(cx).awaits_first_build() {
            // Nothing to list yet; `NotesChanged` re-runs the query once the first build applies.
            self.answered = Answered::Scanning;
            self.list.replace(id, snapshot, query, Vec::new(), false);
            return Runs::settle(id, cx);
        }
        let wait = match run {
            Run::Refresh => Some(NOTES_CHANGED_REFRESH),
            Run::Search => started.behind_user.then_some(SEARCH_DEBOUNCE),
            Run::Notify => None,
        };
        let released = self.runs.scanning(&started);
        let search = Search {
            id,
            run,
            answers: if query.is_empty() {
                Answered::Recent
            } else {
                Answered::Query
            },
            started: Instant::now(),
            snapshot: snapshot.clone(),
            query: Arc::clone(&query),
        };
        let executor = cx.background_executor().clone();
        cx.spawn(async move |picker, cx| {
            if let Some(wait) = wait {
                cx.background_executor().timer(wait).await;
            }
            released.await;
            let found = fan_out(
                executor,
                snapshot,
                query,
                started.cancel,
                |snapshot, lookup, range, query, cancel| {
                    find_notes(snapshot, lookup, range, query, cancel, LIMIT)
                },
                |snapshot, found, query| merge_notes(snapshot, found, query, LIMIT),
            );
            let Some(found) = found.await else {
                return;
            };
            let _ = picker.update(cx, |picker, cx| {
                Self::apply(picker, search, found, cx);
            });
        })
    }

    fn confirm(&mut self, ix: usize, cx: &mut Context<Picker<Self>>) {
        self.runs.confirmed();
        if let Some(row) = self.list.rows.get(ix) {
            cx.emit(NoteActivated(row.found.id));
        }
    }

    fn render_row(&self, ix: usize, _: bool, _: &mut Context<Picker<Self>>) -> AnyElement {
        self.list
            .rows
            .get(ix)
            .map_or_else(|| div().into_any_element(), render::render_row)
    }

    fn rows_shown(&mut self, range: Range<usize>, cx: &mut Context<Picker<Self>>) {
        self.ask_excerpts(range, cx);
    }

    fn row_label(&self, ix: usize) -> SharedString {
        self.list
            .rows
            .get(ix)
            .map(|row| row.text.label.clone())
            .unwrap_or_default()
    }

    fn render_footer(&self, cx: &mut Context<Picker<Self>>) -> Option<AnyElement> {
        self.footer(cx).map(IntoElement::into_any_element)
    }

    fn row_height(&self) -> Pixels {
        theme::UI_LINE_HEIGHT * 2.
    }
}
