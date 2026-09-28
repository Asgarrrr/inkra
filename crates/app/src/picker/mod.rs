//! A query field over a list of rows. A `PickerDelegate` owns the matches and draws each row.

use std::ops::Range;

use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, FocusHandle, Focusable, Pixels, ScrollStrategy,
    SharedString, Subscription, Task, UniformListScrollHandle, Window, actions, prelude::*,
};

use crate::field::{Cancelled, Confirmed, Edited, NameField};

#[cfg(test)]
pub(crate) mod letters;
mod render;
pub(crate) mod runs;
#[cfg(test)]
mod tests;

actions!(picker, [SelectPrev, SelectNext]);

/// The picker is done: a row was confirmed, it was cancelled, or it lost focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Dismissed;

impl<D: PickerDelegate> EventEmitter<Dismissed> for Picker<D> {}

pub(crate) trait PickerDelegate: Sized + 'static {
    /// Joins `Picker` in the key context, so a delegate's own bindings exist only in it.
    fn key_context(&self) -> &'static str;
    /// The dialog's accessible name.
    fn label(&self) -> &'static str;
    fn placeholder(&self) -> &'static str;
    /// Shown in place of the rows when there are none; `None` before the first result.
    fn empty_text(&self, cx: &App) -> Option<SharedString>;
    fn match_count(&self) -> usize;
    fn is_selectable(&self, ix: usize) -> bool;
    /// The picker holds the task, so the next query drops it.
    fn update_matches(&mut self, query: &str, cx: &mut Context<Picker<Self>>) -> Task<()>;
    fn confirm(&mut self, ix: usize, cx: &mut Context<Picker<Self>>);
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
    /// Until the delegate calls `matches_updated` or `matches_refreshed`, the rows answer an older query.
    awaiting_matches: bool,
    /// `enter` landed while `awaiting_matches`: confirm once the rows answer the current query.
    confirm_on_update: bool,
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
                picker.confirm(cx);
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
            awaiting_matches: false,
            confirm_on_update: false,
            _subscriptions: subscriptions,
        };
        picker.refresh(cx);
        picker
    }

    #[cfg(test)]
    pub(crate) fn query(&self) -> &str {
        &self.query
    }

    pub(crate) fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// Re-runs the query as it stands.
    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        self.awaiting_matches = true;
        self.refresh_in_background(cx);
    }

    /// Re-runs the query for data that changed under it: an `enter` meanwhile confirms the rows shown.
    pub(crate) fn refresh_in_background(&mut self, cx: &mut Context<Self>) {
        let query = self.query.clone();
        self.pending = Some(self.delegate.update_matches(&query, cx));
    }

    /// Selects the first selectable row from `ix`, else the nearest before it, and scrolls to it.
    pub(crate) fn matches_updated(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.select_from(ix);
        self.reveal_selected();
        self.matches_landed(cx);
    }

    /// As `matches_updated`, for a background refresh: the list keeps its scroll.
    pub(crate) fn matches_refreshed(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.select_from(ix);
        self.matches_landed(cx);
    }

    fn select_from(&mut self, ix: usize) {
        let count = self.delegate.match_count();
        let from = ix.min(count);
        self.selected = (from..count)
            .chain((0..from).rev())
            .find(|&ix| self.delegate.is_selectable(ix));
    }

    fn matches_landed(&mut self, cx: &mut Context<Self>) {
        self.awaiting_matches = false;
        cx.notify();
        if std::mem::take(&mut self.confirm_on_update) {
            self.confirm_selected(cx);
        }
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

    /// As Zed's `confirm_on_update`: `enter` typed right after a key confirms that key's result.
    fn confirm(&mut self, cx: &mut Context<Self>) {
        if self.awaiting_matches {
            self.confirm_on_update = true;
        } else {
            self.confirm_selected(cx);
        }
    }

    fn confirm_selected(&mut self, cx: &mut Context<Self>) {
        let Some(ix) = self.selected else {
            return;
        };
        self.delegate.confirm(ix, cx);
        cx.emit(Dismissed);
    }

    fn click(&mut self, ix: usize, cx: &mut Context<Self>) {
        if self.delegate.is_selectable(ix) {
            self.selected = Some(ix);
            self.confirm_selected(cx);
        }
    }
}

impl<D: PickerDelegate> Focusable for Picker<D> {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.field.read(cx).focus_handle(cx)
    }
}
