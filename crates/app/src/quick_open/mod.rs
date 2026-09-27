//! `⌘P`: the notes with a word of their name or text starting with every term, best first.

use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, IntoElement, Pixels, SharedString,
    Subscription, Task, actions, div,
};
use inkra_index::{IndexSnapshot, NoteMatch, NoteQuery};
use inkra_vault::EntryId;

use crate::count::thousands;
use crate::index::{BUILDING_REFRESH, IndexSync, IndexWatch, NotesChanged, Refresh};
use crate::picker::{Picker, PickerDelegate};
use crate::sidebar::NoteActivated;
use crate::theme;
use list::List;
use scan::{Run, Scan};

mod list;
mod render;
mod scan;
mod search;
#[cfg(test)]
mod tests;

actions!(quick_open, [QuickOpen]);

/// Waited only behind a user search, so an idle or refreshing picker answers at once.
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(100);
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
    /// Set by the next query and on drop: a scan already running on another thread ignores a dropped task.
    cancel: Arc<AtomicBool>,
    search_id: u64,
    scan: Scan,
    /// Set by a refresh for the next run only.
    next_run: Run,
    /// Loads add text, so rows; a notify refreshes at most every `BUILDING_REFRESH` while they land.
    watch: IndexWatch,
    /// A scan waits for it before it starts, so a test can hold one running.
    #[cfg(test)]
    hold: Option<futures::future::Shared<futures::channel::oneshot::Receiver<()>>>,
    /// Each excerpt, once computed, waits for it before it lands, so a test can order landings.
    #[cfg(test)]
    hold_excerpts: Option<futures::future::Shared<futures::channel::oneshot::Receiver<()>>>,
    /// Scans started, the scanning state's empty runs aside.
    #[cfg(test)]
    runs: usize,
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
    id: u64,
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
                Self::refresh_or_defer(picker, run, cx);
            }),
            cx.observe(links, |picker, links, cx| {
                match picker.delegate.watch.on_notify(links.read(cx)) {
                    Refresh::Skip => {}
                    Refresh::Now => Self::refresh_or_defer(picker, Run::Notify, cx),
                    Refresh::Later => {
                        let throttled = cx.spawn(async move |picker, cx| {
                            cx.background_executor().timer(BUILDING_REFRESH).await;
                            let _ = picker.update(cx, |picker, cx| {
                                Self::refresh_or_defer(picker, Run::Notify, cx);
                            });
                        });
                        picker.delegate.watch.throttle(throttled);
                    }
                }
            }),
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
            cancel: Arc::new(AtomicBool::new(false)),
            search_id: 0,
            scan: Scan::Idle,
            next_run: Run::Search,
            watch: IndexWatch::default(),
            #[cfg(test)]
            hold: None,
            #[cfg(test)]
            hold_excerpts: None,
            #[cfg(test)]
            runs: 0,
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
        if id != picker.delegate.search_id {
            return;
        }
        let user = run == Run::Search;
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
        let scan = std::mem::replace(&mut picker.delegate.scan, Scan::Idle);
        if matches!(scan, Scan::Stale { .. }) {
            Self::refresh_or_defer(picker, Run::Notify, cx);
        }
    }

    /// While the notes' texts load; before the first build the empty text says it is scanning.
    fn footer(&self, cx: &App) -> Option<SharedString> {
        let links = self.links.read(cx);
        (links.is_building() && !links.awaits_first_build()).then(|| {
            let (loaded, total) = links.progress();
            format!(
                "Indexing {} of {} notes",
                thousands(loaded),
                thousands(total)
            )
            .into()
        })
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
        match self.answered {
            Answered::Nothing => None,
            Answered::Scanning => Some(SCANNING),
            Answered::Recent => Some("No recent notes"),
            Answered::Query => Some("No matching notes"),
        }
    }

    fn match_count(&self) -> usize {
        self.list.rows.len()
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
        if run == Run::Search {
            self.list.cancel();
        }
        // A keystroke supersedes a background run at once; it waits only behind a user search.
        let behind_user = matches!(
            self.scan,
            Scan::Running { user: true } | Scan::Stale { user: true }
        );
        // This run reads the newest index, whatever made the scan before it stale.
        self.scan = Scan::Idle;
        self.watch.seen(self.links.read(cx));
        let snapshot = self.links.read(cx).index().snapshot();
        let query = Arc::new(NoteQuery::new(query, self.showing, &self.recent));
        if self.links.read(cx).awaits_first_build() {
            // Nothing to list yet; `NotesChanged` re-runs the query once the first build applies.
            self.answered = Answered::Scanning;
            self.list.replace(id, snapshot, query, Vec::new(), false);
            return cx.spawn(async move |picker, cx| {
                let _ = picker.update(cx, |picker, cx| {
                    if picker.delegate.search_id == id {
                        picker.matches_updated(0, cx);
                    }
                });
            });
        }
        let wait = match run {
            Run::Refresh => Some(NOTES_CHANGED_REFRESH),
            Run::Search => behind_user.then_some(SEARCH_DEBOUNCE),
            Run::Notify => None,
        };
        self.scan = Scan::Running {
            user: run == Run::Search,
        };
        #[cfg(test)]
        {
            self.runs += 1;
        }
        #[cfg(test)]
        let hold = self.hold.clone();
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
            #[cfg(test)]
            if let Some(hold) = hold {
                let _ = hold.await;
            }
            let Some(found) = search::found(executor, snapshot, query, cancel).await else {
                return;
            };
            let _ = picker.update(cx, |picker, cx| {
                Self::apply(picker, search, found, cx);
            });
        })
    }

    fn confirm(&mut self, ix: usize, cx: &mut Context<Picker<Self>>) {
        // The picker dismisses: a deferred refresh has nothing left to refresh.
        if let Scan::Stale { user } = self.scan {
            self.scan = Scan::Running { user };
        }
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
