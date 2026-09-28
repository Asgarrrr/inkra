//! `⌘⇧F`: the word starts of every term across the vault's text, a header per note.

use std::ops::Range;
use std::sync::Arc;

use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, IntoElement, Pixels, SharedString,
    Subscription, Task, actions, div,
};
use inkra_index::{NoteQuery, Searched, search};
use inkra_text::ByteOffset;
use inkra_vault::{EntryId, Vault};

use crate::count::thousands;
use crate::index::{IndexSync, fan_out};
use crate::picker::runs::{Run, RunId, Runs, SEARCH_DEBOUNCE};
use crate::picker::{Picker, PickerDelegate};
use crate::theme;
use rows::{Results, Row};

mod render;
mod rows;
#[cfg(test)]
mod tests;

actions!(vault_search, [SearchVault]);

/// Search spec D9: past it the footer asks for a narrower query.
const LIMIT: usize = 1_000;

/// A hit was confirmed. Slice 4 selects `range`; until then the shell opens the note at its top.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HitActivated {
    pub(crate) note: EntryId,
    pub(crate) range: Range<ByteOffset>,
}

impl EventEmitter<HitActivated> for Picker<VaultSearchDelegate> {}

pub(crate) struct VaultSearchDelegate {
    links: Entity<IndexSync>,
    /// A header's path, read when the rows land.
    vault: Entity<Vault>,
    results: Results,
    answered: Answered,
    runs: Runs,
    _subscriptions: Vec<Subscription>,
}

/// What the empty text answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Answered {
    /// An empty field, or no result yet: nothing to say.
    Blank,
    /// The vault's first scan and build have not applied: there is no text to search.
    Scanning,
    /// The field holds no term, so no note can match.
    NoTerm,
    Query,
}

impl VaultSearchDelegate {
    pub(crate) fn new(
        links: &Entity<IndexSync>,
        vault: Entity<Vault>,
        cx: &mut Context<Picker<Self>>,
    ) -> Self {
        // No `NotesChanged`: every build and load notifies, and `observe` refreshes on it.
        let subscriptions = vec![Runs::observe(links, cx)];
        Self {
            links: links.clone(),
            vault,
            results: Results::default(),
            answered: Answered::Blank,
            runs: Runs::default(),
            _subscriptions: subscriptions,
        }
    }

    /// No rows and no run behind them: the scanning state, an empty field, a field with no term.
    fn answer(
        &mut self,
        id: RunId,
        answered: Answered,
        cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        self.results = Results::default();
        self.answered = answered;
        Runs::settle(id, cx)
    }

    fn apply(
        picker: &mut Picker<Self>,
        id: RunId,
        run: Run,
        searched: Searched,
        cx: &mut Context<Picker<Self>>,
    ) {
        if !picker.delegate.runs.is_current(id) {
            return;
        }
        let user = run.is_user();
        // Read now, not when the refresh started: the user may have moved meanwhile.
        let kept = picker
            .selected()
            .filter(|_| !user)
            .and_then(|ix| picker.delegate.results.key(ix));
        let this = &mut picker.delegate;
        let vault = this.vault.read(cx);
        this.results = Results::new(searched, |note| vault.entry(note).map(|entry| &entry.path));
        this.answered = Answered::Query;
        match kept.and_then(|kept| picker.delegate.results.position(kept)) {
            Some(ix) => picker.matches_refreshed(ix, cx),
            // A new query, or the kept hit is gone: the first hit, scrolled to.
            None => picker.matches_updated(0, cx),
        }
        // After the rows land: an `enter` waiting on them confirms, and that clears the mark.
        Runs::landed(picker, cx);
    }

    fn footer(&self, cx: &App) -> Option<SharedString> {
        if self.results.rows.is_empty() {
            return None;
        }
        if self.results.limit_reached {
            return Some(format!("{}+ results; refine the query", thousands(LIMIT)).into());
        }
        self.links.read(cx).indexing_footer()
    }
}

impl AsMut<Runs> for VaultSearchDelegate {
    fn as_mut(&mut self) -> &mut Runs {
        &mut self.runs
    }
}

impl PickerDelegate for VaultSearchDelegate {
    fn key_context(&self) -> &'static str {
        "VaultSearch"
    }

    fn label(&self) -> &'static str {
        "Search notes"
    }

    fn placeholder(&self) -> &'static str {
        "Search notes…"
    }

    fn empty_text(&self, cx: &App) -> Option<SharedString> {
        let links = self.links.read(cx);
        match self.answered {
            Answered::Blank => None,
            Answered::Scanning => Some("Scanning the vault…".into()),
            // While loads land, a query no note matches yet may still match.
            Answered::Query if links.is_building() => {
                let (loaded, total) = links.progress();
                Some(
                    format!(
                        "No matches in {} of {} notes indexed so far",
                        thousands(loaded),
                        thousands(total)
                    )
                    .into(),
                )
            }
            Answered::NoTerm | Answered::Query => Some("No matches".into()),
        }
    }

    fn match_count(&self) -> usize {
        self.results.rows.len()
    }

    fn is_selectable(&self, ix: usize) -> bool {
        matches!(self.results.rows.get(ix), Some(Row::Hit(_)))
    }

    fn update_matches(&mut self, query: &str, cx: &mut Context<Picker<Self>>) -> Task<()> {
        let started = self.runs.start(self.links.read(cx));
        let (id, run) = (started.id, started.run);
        if self.links.read(cx).awaits_first_build() {
            // The first build's notify refreshes the query.
            return self.answer(id, Answered::Scanning, cx);
        }
        if query.is_empty() {
            return self.answer(id, Answered::Blank, cx);
        }
        let query = Arc::new(NoteQuery::new(query, None, &[]));
        if query.is_empty() {
            // `search` cannot tell, since its caller takes the lookup (word-start spec).
            return self.answer(id, Answered::NoTerm, cx);
        }
        let wait = match run {
            Run::Search => started.behind_user.then_some(SEARCH_DEBOUNCE),
            Run::Refresh | Run::Notify => None,
        };
        let released = self.runs.scanning(&started);
        let snapshot = self.links.read(cx).index().snapshot();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |picker, cx| {
            if let Some(wait) = wait {
                cx.background_executor().timer(wait).await;
            }
            released.await;
            let searched = fan_out(
                executor,
                snapshot,
                query,
                started.cancel,
                |snapshot, lookup, range, query, cancel| {
                    search(snapshot, lookup, range, query, cancel, LIMIT)
                },
                |_, found, _| Searched::merge(found, LIMIT),
            );
            let Some(searched) = searched.await else {
                return;
            };
            let _ = picker.update(cx, |picker, cx| {
                Self::apply(picker, id, run, searched, cx);
            });
        })
    }

    fn confirm(&mut self, ix: usize, cx: &mut Context<Picker<Self>>) {
        self.runs.confirmed();
        if let Some(Row::Hit(hit)) = self.results.rows.get(ix) {
            cx.emit(HitActivated {
                note: hit.note,
                range: hit.range.clone(),
            });
        }
    }

    fn render_row(&self, ix: usize, _: bool, _: &mut Context<Picker<Self>>) -> AnyElement {
        match self.results.rows.get(ix) {
            Some(Row::Header(header)) => render::header(header, ix > 0),
            Some(Row::Hit(hit)) => render::hit(hit, self.results.digits),
            None => div().into_any_element(),
        }
    }

    fn row_label(&self, ix: usize) -> SharedString {
        match self.results.rows.get(ix) {
            Some(Row::Header(header)) => header.label.clone(),
            Some(Row::Hit(hit)) => hit.label.clone(),
            None => SharedString::default(),
        }
    }

    fn render_footer(&self, cx: &mut Context<Picker<Self>>) -> Option<AnyElement> {
        self.footer(cx).map(IntoElement::into_any_element)
    }

    fn row_height(&self) -> Pixels {
        theme::UI_LINE_HEIGHT
    }
}
