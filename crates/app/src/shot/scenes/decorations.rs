use gpui::WindowHandle;
use inkra_text::ByteOffset;

use crate::decorations::{self, LayerDecl, LayerId, MarkStyle};
use crate::feature::FeatureId;
use crate::shell::AppShell;
use crate::shot::stage::Stage;
use crate::theme::ColorToken;

const TODOS: &str = "TODO call the printer back.\n\
                     \n\
                     The draft is **done but TODO the summary** for Friday.\n\
                     \n\
                     TODO water the plants.";

const FEATURE: FeatureId = FeatureId::new("todo_marks");

static TODO_MARKS: LayerDecl = LayerDecl {
    id: LayerId::new(FEATURE, "todo"),
    open: None,
};

/// Published by hand, with no feature: the scene shows the drawing, not a listener.
pub(super) fn todo_marks(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_note(TODOS);
    stage.focus(window);
    let keys = format!("cmd-up down down {}", "right ".repeat(20));
    stage.keys(window, keys.trim_end());
    stage.probe(window, "the note's buffer", |shell, cx| {
        let buffer = shell.note_buffer(cx)?;
        decorations::declare([&TODO_MARKS], cx);
        decorations::enable([FEATURE], cx);
        let warning = MarkStyle {
            color: Some(ColorToken::Warning),
        };
        let marks = TODOS
            .match_indices("TODO")
            .map(|(start, word)| {
                let end = start + word.len();
                (ByteOffset::new(start)..ByteOffset::new(end), warning)
            })
            .collect();
        let (len, at) = (buffer.read(cx).len(), buffer.read(cx).edit_mark());
        let chunk = ByteOffset::new(0)..len;
        decorations::publish(TODO_MARKS.id, &buffer.downgrade(), chunk, 0, marks, at, cx);
        Some(())
    });
    window
}
