//! `⌘P`: the notes with a word of their name or text starting with every term, best first.

use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, IntoElement, Pixels, SharedString,
    Subscription, actions, div,
};
use inkra_index::{IndexSnapshot, NoteMatch, NoteQuery};
use inkra_vault::{EntryId, Jobs};

use crate::index::{IndexSync, NotesChanged, SCANNING, indexing_footer};
use crate::open::{NoteActivated, Open};
use crate::picker::runs::{IndexDelegate, Landed, Ran, Run, Runs};
use crate::picker::{Arrival, Picker, PickerDelegate, Update};
use crate::theme;
use list::List;

mod list;
mod render;
#[cfg(test)]
mod tests;

actions!(quick_open, [QuickOpen]);

/// Zed's file finder lists 100 too.
const LIMIT: usize = 100;

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
    /// An empty query: the recent notes.
    Recent,
    Query,
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
                let run = if picker.delegate.runs.scanning() {
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

    fn footer(&self, cx: &App) -> Option<SharedString> {
        indexing_footer(self.links.read(cx).status())
    }
}

impl IndexDelegate for QuickOpenDelegate {
    type Found = Vec<NoteMatch>;
    type Key = EntryId;

    fn runs(&mut self) -> &mut Runs {
        &mut self.runs
    }

    fn sync(&self) -> &Entity<IndexSync> {
        &self.links
    }

    fn query(&mut self, text: &str, run: Run) -> Option<NoteQuery> {
        if run.is_user() {
            self.list.cancel();
        }
        Some(NoteQuery::new(text, self.showing, &self.recent))
    }

    async fn search(
        snapshot: &IndexSnapshot,
        query: &NoteQuery,
        jobs: &dyn Jobs,
        cancel: &AtomicBool,
    ) -> Option<Vec<NoteMatch>> {
        snapshot.find_notes(query, LIMIT, jobs, cancel).await
    }

    fn show(&mut self, landed: Landed<Vec<NoteMatch>>, _: &mut Context<Picker<Self>>) {
        let Landed {
            id,
            run,
            begun,
            snapshot,
            query,
            found,
        } = landed;
        self.answered = if query.is_empty() {
            Answered::Recent
        } else {
            Answered::Query
        };
        self.list
            .replace(id, snapshot, query, found, !run.is_user());
        if run.is_user() {
            #[cfg(test)]
            {
                self.logged += 1;
            }
            if cfg!(not(debug_assertions)) {
                log::warn!(
                    "quick open: {} matches, {:?} from the edit",
                    self.list.rows.len(),
                    begun.elapsed()
                );
            }
        }
    }

    fn key(&self, ix: usize) -> Option<EntryId> {
        self.list.rows.get(ix).map(|row| row.found.id)
    }
}

impl PickerDelegate for QuickOpenDelegate {
    type Matches = Ran<<Self as IndexDelegate>::Found>;

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
        if self.runs.scanning() {
            return Some(SCANNING.into());
        }
        let text = match self.answered {
            Answered::Nothing => None,
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

    fn update_matches(
        &mut self,
        query: &str,
        cx: &mut Context<Picker<Self>>,
    ) -> Update<Self::Matches> {
        Runs::update_matches(self, query, cx)
    }

    fn apply_matches(
        &mut self,
        matches: Self::Matches,
        selected: Option<usize>,
        cx: &mut Context<Picker<Self>>,
    ) -> Arrival {
        Runs::apply_matches(self, matches, selected, cx)
    }

    fn confirm(&mut self, ix: usize, open: Open, cx: &mut Context<Picker<Self>>) {
        if let Some(row) = self.list.rows.get(ix) {
            cx.emit(NoteActivated {
                note: row.found.id,
                open,
            });
        }
    }

    fn render_row(&self, ix: usize, _: bool, cx: &mut Context<Picker<Self>>) -> AnyElement {
        let theme = theme::current(cx);
        self.list.rows.get(ix).map_or_else(
            || div().into_any_element(),
            |row| render::render_row(row, theme),
        )
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
