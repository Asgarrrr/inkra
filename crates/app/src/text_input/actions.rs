//! The editing actions every host shares, bound once in `TextInput` (keymap.rs).

use std::time::Instant;

use gpui::{Context, Div, InteractiveElement, Window, actions};
use inkra_editor::{EditorState, LogicalMovement};

use super::InputHost;

actions!(
    text_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        Home,
        End,
        LineStart,
        LineEnd,
        SelectAll,
    ]
);

/// Every shared handler, on the host's root div. A host adds its own after.
pub(crate) fn on_actions<V: InputHost>(div: Div, cx: &mut Context<V>) -> Div {
    div.on_action(cx.listener(|view: &mut V, _: &Backspace, window, cx| {
        let now = Instant::now();
        view.edit_buffer(window, cx, |state, buffer| state.backspace(buffer, now));
    }))
    .on_action(cx.listener(|view: &mut V, _: &Delete, window, cx| {
        let now = Instant::now();
        view.edit_buffer(window, cx, |state, buffer| state.delete(buffer, now));
    }))
    .on_action(cx.listener(|view: &mut V, _: &Left, window, cx| {
        move_by(view, LogicalMovement::Left, false, window, cx);
    }))
    .on_action(cx.listener(|view: &mut V, _: &Right, window, cx| {
        move_by(view, LogicalMovement::Right, false, window, cx);
    }))
    .on_action(cx.listener(|view: &mut V, _: &SelectLeft, window, cx| {
        move_by(view, LogicalMovement::Left, true, window, cx);
    }))
    .on_action(cx.listener(|view: &mut V, _: &SelectRight, window, cx| {
        move_by(view, LogicalMovement::Right, true, window, cx);
    }))
    .on_action(cx.listener(|view: &mut V, _: &Home, window, cx| {
        to_row_edge(view, RowEdge::Start, window, cx);
    }))
    .on_action(cx.listener(|view: &mut V, _: &End, window, cx| {
        to_row_edge(view, RowEdge::End, window, cx);
    }))
    .on_action(cx.listener(|view: &mut V, _: &LineStart, window, cx| {
        move_by(view, LogicalMovement::LineStart, false, window, cx);
    }))
    .on_action(cx.listener(|view: &mut V, _: &LineEnd, window, cx| {
        move_by(view, LogicalMovement::LineEnd, false, window, cx);
    }))
    .on_action(cx.listener(|view: &mut V, _: &SelectAll, window, cx| {
        view.move_cursor(window, cx, EditorState::select_all);
    }))
}

fn move_by<V: InputHost>(
    view: &mut V,
    movement: LogicalMovement,
    extend: bool,
    window: &mut Window,
    cx: &mut Context<V>,
) {
    view.move_cursor(window, cx, |state, buffer| {
        state.move_(buffer, movement, extend);
    });
}

#[derive(Clone, Copy)]
enum RowEdge {
    Start,
    End,
}

/// The edge of the caret's visual row, as Zed's `stop_at_soft_wraps`; nothing before the first paint.
fn to_row_edge<V: InputHost>(
    view: &mut V,
    edge: RowEdge,
    window: &mut Window,
    cx: &mut Context<V>,
) {
    let input = view.input();
    let Some(layout) = input.last_layout() else {
        return;
    };
    let row = layout.row_bounds(layout.row_for_offset(input.cursor()));
    let target = match edge {
        RowEdge::Start => *row.start(),
        RowEdge::End => *row.end(),
    };
    view.move_cursor(window, cx, |state, buffer| {
        state.move_to(buffer, target, false);
    });
}
