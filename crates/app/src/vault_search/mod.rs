//! `⌘⇧F`: the word starts of every term across the vault's text, a header per note.

use std::ops::Range;
use std::sync::atomic::AtomicBool;

use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, IntoElement, Pixels, SharedString,
    Subscription, Task, actions, div,
};
use inkra_index::{IndexSnapshot, NoteQuery, Searched};
use inkra_text::ByteOffset;
use inkra_vault::{EntryId, Jobs, Vault};

use crate::count::thousands;
use crate::index::{IndexSync, SCANNING};
use crate::open::Open;
use crate::picker::runs::{IndexDelegate, Landed, Run, Runs};
use crate::picker::{Picker, PickerDelegate};
use crate::theme;
use rows::{HitKey, Results, Row};

mod render;
mod rows;
#[cfg(test)]
mod tests;

actions!(vault_search, [SearchVault]);

/// Search spec D9: past it the footer asks for a narrower query.
const LIMIT: usize = 1_000;

/// A hit was confirmed; the shell opens its note with `range` selected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HitActivated {
    pub(crate) note: EntryId,
    pub(crate) range: Range<ByteOffset>,
    pub(crate) open: Open,
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
    fn answer(&mut self, answered: Answered) {
        self.results = Results::default();
        self.answered = answered;
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

impl IndexDelegate for VaultSearchDelegate {
    type Found = Searched;
    type Key = HitKey;

    fn runs(&mut self) -> &mut Runs {
        &mut self.runs
    }

    fn sync(&self) -> &Entity<IndexSync> {
        &self.links
    }

    fn scanning(&mut self) {
        self.answer(Answered::Scanning);
    }

    fn query(&mut self, text: &str, _: Run) -> Option<NoteQuery> {
        if text.is_empty() {
            self.answer(Answered::Blank);
            return None;
        }
        let query = NoteQuery::new(text, None, &[]);
        if query.is_empty() {
            self.answer(Answered::NoTerm);
            return None;
        }
        Some(query)
    }

    async fn search(
        snapshot: &IndexSnapshot,
        query: &NoteQuery,
        jobs: &dyn Jobs,
        cancel: &AtomicBool,
    ) -> Option<Searched> {
        snapshot.search(query, LIMIT, jobs, cancel).await
    }

    fn show(&mut self, landed: Landed<Searched>, cx: &mut Context<Picker<Self>>) {
        let vault = self.vault.read(cx);
        self.results = Results::new(landed.found, |note| {
            vault.entry(note).map(|entry| &entry.path)
        });
        self.answered = Answered::Query;
    }

    fn key(&self, ix: usize) -> Option<HitKey> {
        self.results.key(ix)
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
            Answered::Scanning => Some(SCANNING.into()),
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
        Runs::update_matches(self, query, cx)
    }

    fn confirm(&mut self, ix: usize, open: Open, cx: &mut Context<Picker<Self>>) {
        if let Some(Row::Hit(hit)) = self.results.rows.get(ix) {
            cx.emit(HitActivated {
                note: hit.note,
                range: hit.range.clone(),
                open,
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
