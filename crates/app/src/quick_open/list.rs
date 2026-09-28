//! The rows shown and their second lines, computed off the foreground for the rows on screen.

use std::collections::HashMap;
use std::mem;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gpui::Context;
use inkra_index::{Excerpt, IndexSnapshot, NoteMatch, NoteQuery, excerpt};
use inkra_vault::EntryId;

use super::QuickOpenDelegate;
use super::render::RowText;
use crate::picker::Picker;
use crate::picker::runs::RunId;

pub(super) struct Row {
    pub(super) found: NoteMatch,
    /// `None` until its excerpt lands, or when there is nothing to show (see `excerpt`).
    pub(super) excerpt: Option<Excerpt>,
    /// What `found` and `excerpt` draw, so a frame builds nothing.
    pub(super) text: RowText,
    /// Its excerpt was asked for: the row has been drawn.
    pub(super) asked: bool,
}

impl Row {
    pub(super) fn new(found: NoteMatch, excerpt: Option<Excerpt>) -> Self {
        Self {
            text: RowText::new(&found, excerpt.as_ref()),
            found,
            excerpt,
            asked: false,
        }
    }

    fn set_excerpt(&mut self, excerpt: Option<Excerpt>) {
        self.text.set_excerpt(&self.found.path, excerpt.as_ref());
        self.excerpt = excerpt;
    }
}

pub(super) struct List {
    /// The search that found the rows: an excerpt asked for by another list is dropped.
    id: RunId,
    pub(super) rows: Vec<Row>,
    snapshot: IndexSnapshot,
    query: Arc<NoteQuery>,
    /// Set by the next user query and on drop, so by the next list and by closing the picker.
    pub(super) cancel: Arc<AtomicBool>,
}

impl Drop for List {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl List {
    /// A new query is on its way: the excerpts in flight will not land.
    pub(super) fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub(super) fn new(snapshot: IndexSnapshot, query: Arc<NoteQuery>) -> Self {
        Self {
            id: RunId::default(),
            rows: Vec::new(),
            snapshot,
            query,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    /// On a refresh, a row keeps its previous excerpt until its new one lands, so no line blanks.
    pub(super) fn replace(
        &mut self,
        id: RunId,
        snapshot: IndexSnapshot,
        query: Arc<NoteQuery>,
        found: Vec<NoteMatch>,
        refresh: bool,
    ) {
        let mut previous: HashMap<EntryId, Excerpt> = if refresh {
            mem::take(&mut self.rows)
                .into_iter()
                .filter_map(|row| Some((row.found.id, row.excerpt?)))
                .collect()
        } else {
            HashMap::new()
        };
        let rows = found
            .into_iter()
            .map(|found| {
                let excerpt = previous.remove(&found.id);
                Row::new(found, excerpt)
            })
            .collect();
        *self = Self {
            id,
            rows,
            snapshot,
            query,
            cancel: Arc::new(AtomicBool::new(false)),
        };
    }

    fn land(&mut self, id: RunId, ix: usize, excerpt: Option<Excerpt>) -> bool {
        if self.id != id || self.cancel.load(Ordering::Relaxed) {
            return false;
        }
        self.rows
            .get_mut(ix)
            .map(|row| row.set_excerpt(excerpt))
            .is_some()
    }
}

impl QuickOpenDelegate {
    /// One background task per row of `range` not asked for yet, so the rows spread across cores.
    pub(super) fn ask_excerpts(&mut self, range: Range<usize>, cx: &mut Context<Picker<Self>>) {
        let list = &mut self.list;
        let rows = list.rows.iter_mut().enumerate();
        for (ix, row) in rows.take(range.end).skip(range.start) {
            if mem::replace(&mut row.asked, true) {
                continue;
            }
            let id = list.id;
            let (found, snapshot) = (row.found.clone(), list.snapshot.clone());
            let (query, cancel) = (Arc::clone(&list.query), Arc::clone(&list.cancel));
            let executor = cx.background_executor().clone();
            #[cfg(test)]
            let hold = self.hold_excerpts.clone();
            cx.spawn(async move |picker, cx| {
                let excerpt = executor
                    .spawn(async move { excerpt(&snapshot, &found, &query, &cancel) })
                    .await;
                #[cfg(test)]
                if let Some(hold) = hold {
                    let _ = hold.await;
                }
                let _ = picker.update(cx, |picker, cx| {
                    if picker.delegate.list.land(id, ix, excerpt) {
                        cx.notify();
                    }
                });
            })
            .detach();
        }
    }
}
