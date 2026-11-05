//! `Select theme`: the selected row previews its theme; a dismiss without confirming restores the prior one (D3).

use std::sync::Arc;

use gpui::{AnyElement, App, Context, EventEmitter, IntoElement, Pixels, SharedString, actions};
use inkra_index::{NoteQuery, word_starts};

use super::{Theme, store::Choice};
use crate::open::Open;
use crate::picker::{Arrival, Picker, PickerDelegate, Update};
use crate::theme;
use crate::ui::Label;

actions!(theme, [SelectTheme]);

#[derive(Clone)]
pub(crate) struct ThemeRow {
    pub(crate) label: SharedString,
    pub(crate) choice: Choice,
    /// `None` follows the system.
    pub(crate) theme: Option<Arc<Theme>>,
}

/// The confirmed row's choice, for the shell to adopt.
pub(crate) struct ThemeChosen(pub(crate) Choice);

impl EventEmitter<ThemeChosen> for Picker<ThemeDelegate> {}

pub(crate) struct ThemeDelegate {
    rows: Vec<ThemeRow>,
    /// Indices into `rows`, in the order shown.
    shown: Vec<usize>,
    current: Choice,
    /// Installed again unless a row is confirmed; `None` follows the system.
    before: Option<Arc<Theme>>,
    confirmed: bool,
}

impl ThemeDelegate {
    pub(crate) fn new(
        rows: Vec<ThemeRow>,
        current: Choice,
        before: Option<Arc<Theme>>,
        cx: &mut Context<Picker<Self>>,
    ) -> Self {
        cx.on_release(|picker, cx| {
            if !picker.delegate.confirmed {
                theme::install(picker.delegate.before.take(), cx);
            }
        })
        .detach();
        Self {
            rows,
            shown: Vec::new(),
            current,
            before,
            confirmed: false,
        }
    }

    pub(crate) fn set_before(&mut self, before: Option<Arc<Theme>>) {
        self.before = before;
    }

    fn row(&self, ix: usize) -> Option<&ThemeRow> {
        self.rows.get(*self.shown.get(ix)?)
    }
}

impl PickerDelegate for ThemeDelegate {
    /// The rows shown, and the one to arrive on.
    type Matches = (Vec<usize>, usize);

    fn key_context(&self) -> &'static str {
        "ThemePicker"
    }

    fn label(&self) -> &'static str {
        "Select theme"
    }

    fn placeholder(&self) -> &'static str {
        "Select a theme…"
    }

    fn empty_text(&self, _: &App) -> Option<SharedString> {
        Some("No matching theme".into())
    }

    fn match_count(&self) -> usize {
        self.shown.len()
    }

    fn is_selectable(&self, _: usize) -> bool {
        true
    }

    fn update_matches(
        &mut self,
        query: &str,
        _: &mut Context<Picker<Self>>,
    ) -> Update<Self::Matches> {
        let parsed = NoteQuery::new(query, None, &[]);
        let shown: Vec<usize> = (0..self.rows.len())
            .filter(|&ix| word_starts(&parsed, &self.rows[ix].label).is_some())
            .collect();
        let arrival = if query.trim().is_empty() {
            shown
                .iter()
                .position(|&ix| self.rows[ix].choice == self.current)
                .unwrap_or(0)
        } else {
            0
        };
        Update::Ready((shown, arrival))
    }

    fn apply_matches(
        &mut self,
        (shown, row): Self::Matches,
        _: Option<usize>,
        _: &mut Context<Picker<Self>>,
    ) -> Arrival {
        self.shown = shown;
        Arrival {
            row,
            ..Arrival::first()
        }
    }

    fn selected_changed(&mut self, ix: usize, cx: &mut Context<Picker<Self>>) {
        if let Some(row) = self.row(ix) {
            theme::install(row.theme.clone(), cx);
        }
    }

    /// `open` does not apply: a theme has no tab.
    fn confirm(&mut self, ix: usize, _: Open, cx: &mut Context<Picker<Self>>) {
        let Some(row) = self.row(ix).cloned() else {
            return;
        };
        self.confirmed = true;
        theme::install(row.theme, cx);
        cx.emit(ThemeChosen(row.choice));
    }

    fn render_row(&self, ix: usize, _: bool, _: &mut Context<Picker<Self>>) -> AnyElement {
        Label::new(self.row_label(ix)).into_any_element()
    }

    fn row_label(&self, ix: usize) -> SharedString {
        self.row(ix)
            .map(|row| row.label.clone())
            .unwrap_or_default()
    }

    fn row_height(&self) -> Pixels {
        theme::UI_LINE_HEIGHT
    }
}
