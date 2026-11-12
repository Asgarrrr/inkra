//! `⌘⇧P`: the commands the focused view can run, found by word start (command palette spec).

mod commands;
#[cfg(test)]
mod tests;

use gpui::{
    Action, AnyElement, App, Context, EventEmitter, IntoElement, ParentElement, Pixels,
    SharedString, Styled, actions, div,
};
use inkra_index::{NoteQuery, word_starts};

use crate::open::Open;
use crate::picker::{Arrival, Picker, PickerDelegate, Update};
use crate::row_text::{LitText, styled};
use crate::theme;
use crate::ui::Color;

pub(crate) use commands::{Command, commands, keys_label};

actions!(command_palette, [Toggle]);

/// The chosen command's action; the shell dispatches it once the palette is gone (spec D4).
pub(crate) struct CommandChosen(pub(crate) Box<dyn Action>);

impl EventEmitter<CommandChosen> for Picker<CommandPaletteDelegate> {}

pub(crate) struct CommandPaletteDelegate {
    commands: Vec<Command>,
    rows: Vec<Row>,
}

/// One match: its index in `commands` and its name, lit.
pub(crate) struct Row {
    command: usize,
    name: LitText,
}

impl CommandPaletteDelegate {
    pub(crate) fn new(commands: Vec<Command>) -> Self {
        Self {
            commands,
            rows: Vec::new(),
        }
    }
}

impl PickerDelegate for CommandPaletteDelegate {
    type Matches = Vec<Row>;

    fn key_context(&self) -> &'static str {
        "CommandPalette"
    }

    fn label(&self) -> &'static str {
        "Command palette"
    }

    fn placeholder(&self) -> &'static str {
        "Run a command…"
    }

    fn empty_text(&self, _: &App) -> Option<SharedString> {
        Some("No matching command".into())
    }

    fn match_count(&self) -> usize {
        self.rows.len()
    }

    fn is_selectable(&self, _: usize) -> bool {
        true
    }

    fn update_matches(
        &mut self,
        query: &str,
        _: &mut Context<Picker<Self>>,
    ) -> Update<Self::Matches> {
        let query = NoteQuery::new(query, None, &[]);
        let mut rows: Vec<(bool, Row)> = self
            .commands
            .iter()
            .enumerate()
            .filter_map(|(ix, command)| {
                let lit = word_starts(&query, &command.name)?;
                let later =
                    !command.recent && !lit.is_empty() && !lit.iter().any(|range| range.start == 0);
                let row = Row {
                    command: ix,
                    name: LitText::line(&command.name, lit),
                };
                Some((later, row))
            })
            .collect();
        // Stable: recent keeps its order and each group stays alphabetical (palette spec, D3).
        rows.sort_by_key(|(later, _)| *later);
        Update::Ready(rows.into_iter().map(|(_, row)| row).collect())
    }

    fn apply_matches(
        &mut self,
        rows: Self::Matches,
        _: Option<usize>,
        _: &mut Context<Picker<Self>>,
    ) -> Arrival {
        self.rows = rows;
        Arrival::first()
    }

    /// `open` does not apply: `⌘↵` runs the command, as `enter`.
    fn confirm(&mut self, ix: usize, _: Open, cx: &mut Context<Picker<Self>>) {
        if let Some(Row { command, .. }) = self.rows.get(ix) {
            cx.emit(CommandChosen(self.commands[*command].action.boxed_clone()));
        }
    }

    fn render_row(&self, ix: usize, _: bool, cx: &mut Context<Picker<Self>>) -> AnyElement {
        let Some(Row { command, name }) = self.rows.get(ix) else {
            return div().into_any_element();
        };
        let key = self.commands[*command].key.clone();
        let theme = theme::current(cx);
        div()
            .h(self.row_height())
            .w_full()
            .flex()
            .justify_between()
            .gap_2()
            .whitespace_nowrap()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(styled(name, Color::Highlight.rgba(theme).into())),
            )
            .children(key.map(|key| {
                div()
                    .flex_none()
                    .text_color(Color::Placeholder.rgba(theme))
                    .child(key)
            }))
            .into_any_element()
    }

    fn row_label(&self, ix: usize) -> SharedString {
        let Some(Row { command, name }) = self.rows.get(ix) else {
            return SharedString::default();
        };
        match &self.commands[*command].key {
            Some(key) => format!("{}, {key}", name.text()).into(),
            None => name.text().clone(),
        }
    }

    fn row_height(&self) -> Pixels {
        theme::UI_LINE_HEIGHT
    }
}
