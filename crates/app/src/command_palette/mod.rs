//! `⌘⇧P`: the commands the focused view can run, found by word start (command palette spec).

mod commands;
#[cfg(test)]
mod tests;

use gpui::{
    Action, AnyElement, App, Context, EventEmitter, IntoElement, ParentElement, Pixels,
    SharedString, Styled, Task, actions, div,
};
use inkra_index::{NoteQuery, word_starts};

use crate::picker::{Picker, PickerDelegate};
use crate::row_text::{LitText, styled};
use crate::theme;

pub(crate) use commands::{Command, commands};

actions!(command_palette, [Toggle]);

/// The chosen command's action; the shell dispatches it once the palette is gone (spec D4).
pub(crate) struct CommandChosen(pub(crate) Box<dyn Action>);

impl EventEmitter<CommandChosen> for Picker<CommandPaletteDelegate> {}

pub(crate) struct CommandPaletteDelegate {
    commands: Vec<Command>,
    rows: Vec<Row>,
}

/// One match: its index in `commands` and its name, lit.
struct Row {
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

    fn update_matches(&mut self, query: &str, cx: &mut Context<Picker<Self>>) -> Task<()> {
        let query = NoteQuery::new(query, None, &[]);
        self.rows = self
            .commands
            .iter()
            .enumerate()
            .filter_map(|(ix, command)| {
                let lit = word_starts(&query, &command.name)?;
                Some(Row {
                    command: ix,
                    name: LitText::line(&command.name, lit),
                })
            })
            .collect();
        // The picker is borrowed here; `App::defer` lands in this flush, before any draw or key.
        let picker = cx.weak_entity();
        cx.defer(move |cx| {
            picker
                .update(cx, |picker, cx| picker.matches_updated(0, cx))
                .ok();
        });
        Task::ready(())
    }

    fn confirm(&mut self, ix: usize, cx: &mut Context<Picker<Self>>) {
        if let Some(Row { command, .. }) = self.rows.get(ix) {
            cx.emit(CommandChosen(self.commands[*command].action.boxed_clone()));
        }
    }

    fn render_row(&self, ix: usize, _: bool, _: &mut Context<Picker<Self>>) -> AnyElement {
        let Some(Row { command, name }) = self.rows.get(ix) else {
            return div().into_any_element();
        };
        let key = self.commands[*command].key.clone();
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
                    .child(styled(name, theme::match_text().into())),
            )
            .children(key.map(|key| {
                div()
                    .flex_none()
                    .text_color(theme::placeholder_text())
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
