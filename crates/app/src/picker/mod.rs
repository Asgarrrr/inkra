//! A query field over a list of rows. A `PickerDelegate` owns the matches and draws each row.

use std::ops::Range;

use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, FocusHandle, Focusable, Pixels, ScrollStrategy,
    SharedString, Subscription, Task, UniformListScrollHandle, Window, actions, prelude::*,
};

use crate::field::{Cancelled, Confirmed, Edited, NameField};
use crate::open::Open;

#[cfg(test)]
pub(crate) mod letters;
mod render;
pub(crate) mod runs;
#[cfg(test)]
mod tests;

actions!(picker, [SelectPrev, SelectNext, SecondaryConfirm]);

/// The picker is done: a row was confirmed, it was cancelled, or it lost focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Dismissed;

impl<D: PickerDelegate> EventEmitter<Dismissed> for Picker<D> {}

/// A query's answer: at once, or from a task the picker holds.
pub(crate) enum Update<T> {
    /// Applied and landed before `refresh` returns.
    Ready(T),
    /// Applied and landed when it finishes with `Some`. `None`, or dropping it, lands nothing.
    Pending(Task<Option<T>>),
}

/// What the picker does once the rows are set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Arrival {
    /// Selects the first selectable row from here, else the nearest before it.
    pub(crate) row: usize,
    /// The same answer refreshed: the list keeps its scroll.
    pub(crate) keep_scroll: bool,
    /// The data changed under this answer: re-run in the background, unless confirmed.
    pub(crate) rerun: bool,
}

impl Arrival {
    /// A new answer: its first selectable row, scrolled to.
    pub(crate) fn first() -> Self {
        Self {
            row: 0,
            keep_scroll: false,
            rerun: false,
        }
    }
}

/// Where the rows stand against the query; the delegate confirms at most once per picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// The rows answer the current query.
    Current,
    /// Until an answer lands, the rows answer an older query; `queued` is an `enter` or `⌘↵` meanwhile.
    Awaiting { queued: Option<Open> },
    /// A row was confirmed: the picker is dismissing, so a deferred refresh has nothing to refresh.
    Confirmed,
}

pub(crate) trait PickerDelegate: Sized + 'static {
    /// One answer, from `update_matches` to `apply_matches`.
    type Matches: 'static;

    /// Joins `Picker` in the key context, so a delegate's own bindings exist only in it.
    fn key_context(&self) -> &'static str;
    /// The dialog's accessible name.
    fn label(&self) -> &'static str;
    fn placeholder(&self) -> &'static str;
    /// Shown in place of the rows when there are none; `None` before the first result.
    fn empty_text(&self, cx: &App) -> Option<SharedString>;
    fn match_count(&self) -> usize;
    fn is_selectable(&self, ix: usize) -> bool;
    /// The picker holds a `Pending` task, so the next query drops it.
    fn update_matches(
        &mut self,
        query: &str,
        cx: &mut Context<Picker<Self>>,
    ) -> Update<Self::Matches>;
    /// Replaces the rows with `matches`; `selected` is the row selected now, before they change.
    fn apply_matches(
        &mut self,
        matches: Self::Matches,
        selected: Option<usize>,
        cx: &mut Context<Picker<Self>>,
    ) -> Arrival;
    /// `open` is where the pick shows: `⌘↵` and a `⌘`-click ask for a new tab.
    fn confirm(&mut self, ix: usize, open: Open, cx: &mut Context<Picker<Self>>);
    /// Exactly `row_height` tall: `uniform_list` measures every row from row 0.
    fn render_row(&self, ix: usize, selected: bool, cx: &mut Context<Picker<Self>>) -> AnyElement;
    /// The row's plain text, its accessible label.
    fn row_label(&self, ix: usize) -> SharedString;
    fn render_footer(&self, _cx: &mut Context<Picker<Self>>) -> Option<AnyElement> {
        None
    }
    fn row_height(&self) -> Pixels;
    /// Rows about to draw; `uniform_list` also draws row 0 alone each frame to measure it.
    fn rows_shown(&mut self, _range: Range<usize>, _cx: &mut Context<Picker<Self>>) {}
}

pub(crate) struct Picker<D: PickerDelegate> {
    pub(crate) delegate: D,
    field: Entity<NameField>,
    query: String,
    selected: Option<usize>,
    scroll: UniformListScrollHandle,
    /// Held, never detached: replacing it drops the previous query's task.
    pending: Option<Task<()>>,
    phase: Phase,
    _subscriptions: Vec<Subscription>,
}

impl<D: PickerDelegate> Picker<D> {
    pub(crate) fn new(delegate: D, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let field = cx.new(|cx| NameField::new("", cx));
        let field_focus = field.read(cx).focus_handle(cx);
        let subscriptions = vec![
            cx.subscribe(&field, |picker, field, _: &Edited, cx| {
                picker.query = field.read(cx).text(cx);
                picker.refresh(cx);
            }),
            cx.subscribe(&field, |picker, _, _: &Confirmed, cx| {
                picker.confirm(Open::Here, cx);
            }),
            cx.subscribe(&field, |_, _, _: &Cancelled, cx| cx.emit(Dismissed)),
            // Switching to another app blurs the field; that is not a dismiss.
            cx.on_blur(&field_focus, window, |_, window, cx| {
                if window.is_window_active() {
                    cx.emit(Dismissed);
                }
            }),
        ];
        let mut picker = Self {
            delegate,
            field,
            query: String::new(),
            selected: None,
            scroll: UniformListScrollHandle::new(),
            pending: None,
            phase: Phase::Current,
            _subscriptions: subscriptions,
        };
        picker.refresh(cx);
        picker
    }

    #[cfg(test)]
    pub(crate) fn query(&self) -> &str {
        &self.query
    }

    #[cfg(test)]
    pub(crate) fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// Re-runs the query as it stands.
    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.phase == Phase::Current {
            self.phase = Phase::Awaiting { queued: None };
        }
        self.refresh_in_background(cx);
    }

    /// Re-runs the query for data that changed under it: an `enter` meanwhile confirms the rows shown.
    pub(crate) fn refresh_in_background(&mut self, cx: &mut Context<Self>) {
        match self.delegate.update_matches(&self.query, cx) {
            Update::Ready(matches) => {
                self.pending = None;
                self.apply(matches, cx);
            }
            Update::Pending(task) => {
                self.pending = Some(cx.spawn(async move |picker, cx| {
                    let Some(matches) = task.await else {
                        return;
                    };
                    picker
                        .update(cx, |picker, cx| picker.apply(matches, cx))
                        .ok();
                }));
            }
        }
    }

    fn apply(&mut self, matches: D::Matches, cx: &mut Context<Self>) {
        let arrival = self.delegate.apply_matches(matches, self.selected, cx);
        self.land(arrival, cx);
    }

    /// Select, scroll, leave `Awaiting`, the queued confirm, then the re-run: this order is the Interface.
    fn land(&mut self, arrival: Arrival, cx: &mut Context<Self>) {
        self.select_from(arrival.row);
        if !arrival.keep_scroll {
            self.reveal_selected();
        }
        let queued = match self.phase {
            Phase::Awaiting { queued } => {
                self.phase = Phase::Current;
                queued
            }
            Phase::Current | Phase::Confirmed => None,
        };
        cx.notify();
        if let Some(open) = queued {
            self.confirm_selected(open, cx);
        }
        if arrival.rerun && self.phase != Phase::Confirmed {
            self.refresh_in_background(cx);
        }
    }

    fn select_from(&mut self, ix: usize) {
        let count = self.delegate.match_count();
        let from = ix.min(count);
        self.selected = (from..count)
            .chain((0..from).rev())
            .find(|&ix| self.delegate.is_selectable(ix));
    }

    fn select_next(&mut self, _: &SelectNext, _: &mut Window, cx: &mut Context<Self>) {
        let from = self.selected.map_or(0, |ix| ix + 1);
        let next = (from..self.delegate.match_count()).find(|&ix| self.delegate.is_selectable(ix));
        if let Some(ix) = next {
            self.select(ix, cx);
        }
    }

    fn select_prev(&mut self, _: &SelectPrev, _: &mut Window, cx: &mut Context<Self>) {
        let Some(to) = self.selected else {
            return;
        };
        if let Some(ix) = (0..to).rev().find(|&ix| self.delegate.is_selectable(ix)) {
            self.select(ix, cx);
        }
    }

    fn select(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.selected = Some(ix);
        self.reveal_selected();
        cx.notify();
    }

    /// The rows no one can select right before it, a group's header, show with it.
    fn reveal_selected(&self) {
        if let Some(ix) = self.selected {
            let lead = (0..ix)
                .rev()
                .take_while(|&row| !self.delegate.is_selectable(row))
                .count();
            self.scroll
                .scroll_to_item_with_offset(ix, ScrollStrategy::Nearest, lead);
        }
    }

    /// As Zed's `menu::SecondaryConfirm`: `⌘↵` confirms into a new tab.
    fn secondary_confirm(&mut self, _: &SecondaryConfirm, _: &mut Window, cx: &mut Context<Self>) {
        self.confirm(Open::NewTab, cx);
    }

    /// As Zed's `confirm_on_update`: `enter` typed right after a key confirms that key's result.
    fn confirm(&mut self, open: Open, cx: &mut Context<Self>) {
        match self.phase {
            Phase::Current => self.confirm_selected(open, cx),
            Phase::Awaiting { .. } => self.phase = Phase::Awaiting { queued: Some(open) },
            Phase::Confirmed => {}
        }
    }

    fn confirm_selected(&mut self, open: Open, cx: &mut Context<Self>) {
        let Some(ix) = self.selected.filter(|_| self.phase != Phase::Confirmed) else {
            return;
        };
        self.phase = Phase::Confirmed;
        self.delegate.confirm(ix, open, cx);
        cx.emit(Dismissed);
    }

    fn click(&mut self, ix: usize, open: Open, cx: &mut Context<Self>) {
        if self.delegate.is_selectable(ix) {
            self.selected = Some(ix);
            self.confirm_selected(open, cx);
        }
    }
}

impl<D: PickerDelegate> Focusable for Picker<D> {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.field.read(cx).focus_handle(cx)
    }
}
