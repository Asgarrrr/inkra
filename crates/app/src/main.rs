//! Minimal GPUI text editor: `EntityInputHandler` over `core::Buffer`.
//!
//! Single-line only — multi-line layout is still an open design question.

use std::ops::Range;
use std::time::Instant;

use gpui::{
    App, Bounds, Context, CursorStyle, ElementId, ElementInputHandler, Entity, EntityInputHandler,
    FocusHandle, Focusable, GlobalElementId, KeyBinding, LayoutId, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, ShapedLine, SharedString, Style, TextAlign,
    TextRun, UTF16Selection, UnderlineStyle, Window, WindowBounds, WindowOptions, actions, div,
    fill, point, prelude::*, px, relative, rgb, rgba, size,
};
use gpui_test_core::{Buffer, ByteOffset, Utf16Offset};

actions!(
    editor,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        Undo,
        Redo,
        Quit,
    ]
);

struct EditorView {
    buffer: Buffer,
    focus_handle: FocusHandle,
    /// Normalized: `start <= end`. `selection_reversed` says which end the
    /// cursor sits on.
    selection: Range<ByteOffset>,
    selection_reversed: bool,
    marked_range: Option<Range<ByteOffset>>,
    is_selecting: bool,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
}

impl EditorView {
    fn new(text: &str, cx: &mut Context<Self>) -> Self {
        let buffer = Buffer::new(text);
        let end = buffer.len();
        Self {
            buffer,
            focus_handle: cx.focus_handle(),
            selection: end..end,
            selection_reversed: false,
            marked_range: None,
            is_selecting: false,
            last_layout: None,
            last_bounds: None,
        }
    }

    fn cursor(&self) -> ByteOffset {
        if self.selection_reversed {
            self.selection.start
        } else {
            self.selection.end
        }
    }

    fn move_to(&mut self, offset: ByteOffset, cx: &mut Context<Self>) {
        self.selection = offset..offset;
        self.selection_reversed = false;
        cx.notify();
    }

    fn select_to(&mut self, offset: ByteOffset, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selection.start = offset;
        } else {
            self.selection.end = offset;
        }
        if self.selection.end < self.selection.start {
            self.selection_reversed = !self.selection_reversed;
            self.selection = self.selection.end..self.selection.start;
        }
        cx.notify();
    }

    fn range_to_utf16(&self, range: &Range<ByteOffset>) -> Range<usize> {
        self.buffer.to_utf16(range.start).0..self.buffer.to_utf16(range.end).0
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<ByteOffset> {
        let start = self.buffer.from_utf16(Utf16Offset(range.start));
        let end = self.buffer.from_utf16(Utf16Offset(range.end));
        start.min(end)..start.max(end)
    }

    fn slice(&self, range: &Range<ByteOffset>) -> &str {
        &self.buffer.text()[range.start.0..range.end.0]
    }

    /// The range an input-method edit applies to, following GPUI's precedence:
    /// explicit range, else the composing range, else the selection.
    fn target_range(&self, range_utf16: Option<&Range<usize>>) -> Range<ByteOffset> {
        range_utf16
            .map(|range| self.range_from_utf16(range))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selection.clone())
    }

    /// Every buffer mutation goes through here, so one user gesture is one undo
    /// step even when it deletes and inserts, and a continuous typing run
    /// collapses into a single step.
    fn edit(&mut self, range: Range<ByteOffset>, text: &str) {
        let start = range.start;
        self.buffer
            .transact_at(Instant::now(), |buffer| buffer.replace(range, text));
        let end = ByteOffset(start.0 + text.len());
        self.selection = end..end;
        self.selection_reversed = false;
    }

    fn position_for_x(&self, x: Pixels) -> Option<ByteOffset> {
        let (bounds, line) = (self.last_bounds?, self.last_layout.as_ref()?);
        let index = line.closest_index_for_x(x - bounds.left());
        Some(self.buffer.clip(ByteOffset(index)))
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        let target = if self.selection.is_empty() {
            self.buffer.previous_grapheme(self.cursor())
        } else {
            self.selection.start
        };
        self.move_to(target, cx);
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        let target = if self.selection.is_empty() {
            self.buffer.next_grapheme(self.cursor())
        } else {
            self.selection.end
        };
        self.move_to(target, cx);
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        let target = self.buffer.previous_grapheme(self.cursor());
        self.select_to(target, cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        let target = self.buffer.next_grapheme(self.cursor());
        self.select_to(target, cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.selection = ByteOffset(0)..self.buffer.len();
        self.selection_reversed = false;
        cx.notify();
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(ByteOffset(0), cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        let end = self.buffer.len();
        self.move_to(end, cx);
    }

    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        let range = if self.selection.is_empty() {
            self.buffer.previous_grapheme(self.cursor())..self.cursor()
        } else {
            self.selection.clone()
        };
        if !range.is_empty() {
            self.edit(range, "");
            self.marked_range = None;
            cx.notify();
        }
    }

    fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        let range = if self.selection.is_empty() {
            self.cursor()..self.buffer.next_grapheme(self.cursor())
        } else {
            self.selection.clone()
        };
        if !range.is_empty() {
            self.edit(range, "");
            self.marked_range = None;
            cx.notify();
        }
    }

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(range) = self.buffer.undo() {
            self.collapse_to(range.end);
            cx.notify();
        }
    }

    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(range) = self.buffer.redo() {
            self.collapse_to(range.end);
            cx.notify();
        }
    }

    fn collapse_to(&mut self, offset: ByteOffset) {
        let offset = self.buffer.clip(offset);
        self.selection = offset..offset;
        self.selection_reversed = false;
        self.marked_range = None;
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle, cx);
        self.is_selecting = true;
        let Some(offset) = self.position_for_x(event.position.x) else {
            return;
        };
        if event.modifiers.shift {
            self.select_to(offset, cx);
        } else {
            self.move_to(offset, cx);
        }
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if !self.is_selecting {
            return;
        }
        if let Some(offset) = self.position_for_x(event.position.x) {
            self.select_to(offset, cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }
}

impl EntityInputHandler for EditorView {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.slice(&range).to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selection),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.target_range(range_utf16.as_ref());
        self.edit(range, new_text);
        self.marked_range = None;
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.target_range(range_utf16.as_ref());
        let start = range.start;
        self.edit(range, new_text);

        self.marked_range =
            (!new_text.is_empty()).then(|| start..ByteOffset(start.0 + new_text.len()));

        // The input method reports the caret in UTF-16 units relative to the
        // text it just inserted. Rebase it while still in UTF-16 and convert
        // once: converting first would resolve it against the whole buffer,
        // and adding `start` on top would then count the offset twice.
        if let Some(selected) = new_selected_range_utf16 {
            let inserted_at = self.buffer.to_utf16(start).0;
            let cursor = self
                .buffer
                .from_utf16(Utf16Offset(inserted_at + selected.end));
            self.selection = cursor..cursor;
            self.selection_reversed = false;
        }
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let line = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        Some(Bounds::from_corners(
            point(
                element_bounds.left() + line.x_for_index(range.start.0),
                element_bounds.top(),
            ),
            point(
                element_bounds.left() + line.x_for_index(range.end.0),
                element_bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        position: gpui::Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let bounds = self.last_bounds?;
        let line = self.last_layout.as_ref()?;
        let index = line.index_for_x(position.x - bounds.left())?;
        Some(self.buffer.to_utf16(self.buffer.clip(ByteOffset(index))).0)
    }
}

struct TextElement {
    editor: Entity<EditorView>,
}

struct PrepaintState {
    line: Option<ShapedLine>,
    selection: Option<PaintQuad>,
    cursor: Option<PaintQuad>,
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    #[allow(clippy::unreadable_literal)]
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        (): &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let editor = self.editor.read(cx);
        let text = SharedString::from(editor.buffer.text().to_string());
        let selection = editor.selection.clone();
        let cursor = editor.cursor();
        let marked = editor.marked_range.clone();

        let style = window.text_style();
        let run = TextRun {
            len: text.len(),
            font: style.font(),
            color: style.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = match marked {
            Some(marked) => vec![
                TextRun {
                    len: marked.start.0,
                    ..run.clone()
                },
                TextRun {
                    len: marked.end.0 - marked.start.0,
                    underline: Some(UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: text.len() - marked.end.0,
                    ..run
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect(),
            None => vec![run],
        };

        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(text, font_size, &runs, None);

        let (selection, cursor) = if selection.is_empty() {
            let quad = fill(
                Bounds::new(
                    point(bounds.left() + line.x_for_index(cursor.0), bounds.top()),
                    size(px(2.), bounds.bottom() - bounds.top()),
                ),
                rgba(0x4ea1ffff),
            );
            (None, Some(quad))
        } else {
            let quad = fill(
                Bounds::from_corners(
                    point(
                        bounds.left() + line.x_for_index(selection.start.0),
                        bounds.top(),
                    ),
                    point(
                        bounds.left() + line.x_for_index(selection.end.0),
                        bounds.bottom(),
                    ),
                ),
                rgba(0x3311ff55),
            );
            (Some(quad), None)
        };

        PrepaintState {
            line: Some(line),
            selection,
            cursor,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        (): &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.editor.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );

        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }

        let Some(line) = prepaint.line.take() else {
            return;
        };
        if let Err(err) = line.paint(
            bounds.origin,
            window.line_height(),
            TextAlign::Left,
            None,
            window,
            cx,
        ) {
            eprintln!("failed to paint line: {err}");
        }

        if focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }

        self.editor.update(cx, |editor, _| {
            editor.last_layout = Some(line);
            editor.last_bounds = Some(bounds);
        });
    }
}

impl Focusable for EditorView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for EditorView {
    // RRGGBB hex literals are the idiomatic gpui color form; clippy's suggested
    // underscore grouping (e.g. 0x001e_1e1e) obscures the RGB byte boundaries.
    #[allow(clippy::unreadable_literal)]
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context("EditorView")
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x1e1e1e))
            .text_color(rgb(0xffffff))
            .text_size(px(18.))
            .line_height(px(28.))
            .p_4()
            .child(TextElement {
                editor: cx.entity(),
            })
    }
}

fn key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("backspace", Backspace, None),
        KeyBinding::new("delete", Delete, None),
        KeyBinding::new("left", Left, None),
        KeyBinding::new("right", Right, None),
        KeyBinding::new("shift-left", SelectLeft, None),
        KeyBinding::new("shift-right", SelectRight, None),
        KeyBinding::new("cmd-a", SelectAll, None),
        KeyBinding::new("home", Home, None),
        KeyBinding::new("end", End, None),
        KeyBinding::new("cmd-z", Undo, None),
        KeyBinding::new("cmd-shift-z", Redo, None),
        KeyBinding::new("cmd-q", Quit, None),
    ]
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        cx.bind_keys(key_bindings());
        cx.on_action(|_: &Quit, cx| cx.quit());

        let bounds = Bounds::centered(None, size(px(600.0), px(200.0)), cx);
        let window = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|cx| EditorView::new("héllo 😀 — type here", cx)),
        );

        match window {
            Ok(window) => {
                let focused = window.update(cx, |view, window, cx| {
                    window.focus(&view.focus_handle(cx), cx);
                    cx.activate(true);
                });
                if let Err(err) = focused {
                    eprintln!("failed to focus window: {err}");
                }
            }
            Err(err) => {
                eprintln!("failed to open window: {err}");
                std::process::exit(1);
            }
        }
    });
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use gpui::{TestAppContext, VisualTestContext};

    /// Drives the entity through a headless window, the only way to reach
    /// `EntityInputHandler`, whose methods all require a `Window`.
    fn with_editor<R>(
        cx: &mut TestAppContext,
        text: &str,
        f: impl FnOnce(&mut EditorView, &mut Window, &mut Context<EditorView>) -> R,
    ) -> R {
        let text = text.to_string();
        let window = cx.add_window(move |_, cx| EditorView::new(&text, cx));
        window.update(cx, f).unwrap()
    }

    fn at(offset: usize) -> ByteOffset {
        ByteOffset(offset)
    }

    #[gpui::test]
    fn the_selection_is_reported_in_utf16_units_not_bytes(cx: &mut TestAppContext) {
        with_editor(cx, "a😀b", |view, window, cx| {
            // 'a'@0, '😀'@1..5 (2 UTF-16 units), 'b'@5.
            view.selection = at(5)..at(5);
            let selection = view.selected_text_range(true, window, cx).unwrap();
            assert_eq!(selection.range, 3..3);
        });
    }

    #[gpui::test]
    fn a_utf16_range_spanning_a_surrogate_pair_deletes_the_whole_char(cx: &mut TestAppContext) {
        with_editor(cx, "a😀b", |view, window, cx| {
            view.replace_text_in_range(Some(1..3), "", window, cx);
            assert_eq!(view.buffer.text(), "ab");
            assert_eq!(view.cursor(), at(1));
        });
    }

    #[gpui::test]
    fn a_utf16_offset_inside_a_surrogate_pair_snaps_to_a_char_boundary(cx: &mut TestAppContext) {
        with_editor(cx, "a😀b", |view, window, cx| {
            view.replace_text_in_range(Some(2..2), "X", window, cx);
            assert_eq!(view.buffer.text(), "a😀Xb");
        });
    }

    #[gpui::test]
    fn text_for_range_round_trips_through_utf16(cx: &mut TestAppContext) {
        with_editor(cx, "héllo", |view, window, cx| {
            let mut actual = None;
            let text = view.text_for_range(1..3, &mut actual, window, cx).unwrap();
            assert_eq!(text, "él");
            assert_eq!(actual, Some(1..3));
        });
    }

    #[gpui::test]
    fn input_method_composition_marks_then_commits(cx: &mut TestAppContext) {
        with_editor(cx, "", |view, window, cx| {
            view.replace_and_mark_text_in_range(None, "か", Some(1..1), window, cx);
            assert_eq!(view.buffer.text(), "か");
            assert_eq!(view.marked_text_range(window, cx), Some(0..1));

            // The next composition keystroke replaces the marked region.
            view.replace_and_mark_text_in_range(None, "かん", Some(2..2), window, cx);
            assert_eq!(view.buffer.text(), "かん");
            assert_eq!(view.marked_text_range(window, cx), Some(0..2));

            view.replace_text_in_range(None, "簡", window, cx);
            assert_eq!(view.buffer.text(), "簡");
            assert_eq!(view.marked_text_range(window, cx), None);
        });
    }

    #[gpui::test]
    fn a_whole_composition_undoes_in_one_step(cx: &mut TestAppContext) {
        with_editor(cx, "", |view, window, cx| {
            view.replace_and_mark_text_in_range(None, "か", Some(1..1), window, cx);
            view.replace_and_mark_text_in_range(None, "かん", Some(2..2), window, cx);
            view.replace_text_in_range(None, "簡", window, cx);

            // The three composition steps land well inside GROUP_INTERVAL, so
            // they coalesce: one undo discards the whole composition rather
            // than walking back through its intermediate states.
            view.undo(&Undo, window, cx);
            assert_eq!(view.buffer.text(), "");
        });
    }

    #[gpui::test]
    fn a_typing_run_undoes_in_one_step(cx: &mut TestAppContext) {
        cx.update(|cx| cx.bind_keys(key_bindings()));
        let window = cx.add_window(|_, cx| EditorView::new("", cx));
        window
            .update(cx, |view, window, cx| window.focus(&view.focus_handle, cx))
            .unwrap();

        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.simulate_input("hello");
        cx.simulate_keystrokes("cmd-z");
        window
            .update(&mut cx, |view, _, _| {
                assert_eq!(view.buffer.text(), "");
            })
            .unwrap();
    }

    #[gpui::test]
    fn composition_away_from_the_start_leaves_the_caret_after_the_text(cx: &mut TestAppContext) {
        with_editor(cx, "xy", |view, window, cx| {
            view.end(&End, window, cx);
            view.replace_and_mark_text_in_range(None, "か", Some(1..1), window, cx);
            assert_eq!(view.buffer.text(), "xyか");
            assert_eq!(view.cursor(), view.buffer.len());
        });
    }

    #[gpui::test]
    fn backspace_deletes_a_grapheme_not_a_char(cx: &mut TestAppContext) {
        // "e" + combining acute is two chars but one grapheme.
        with_editor(cx, "xe\u{301}", |view, window, cx| {
            view.backspace(&Backspace, window, cx);
            assert_eq!(view.buffer.text(), "x");
        });
    }

    #[gpui::test]
    fn arrows_step_over_a_zwj_emoji_as_one_unit(cx: &mut TestAppContext) {
        with_editor(cx, "👨\u{200d}👩\u{200d}👧", |view, window, cx| {
            view.home(&Home, window, cx);
            view.right(&Right, window, cx);
            assert_eq!(view.cursor(), view.buffer.len());
            view.left(&Left, window, cx);
            assert_eq!(view.cursor(), at(0));
        });
    }

    #[gpui::test]
    fn shift_arrows_extend_and_typing_replaces_the_selection(cx: &mut TestAppContext) {
        with_editor(cx, "abc", |view, window, cx| {
            view.select_left(&SelectLeft, window, cx);
            view.select_left(&SelectLeft, window, cx);
            assert_eq!(view.selection, at(1)..at(3));
            assert!(view.selection_reversed);

            view.replace_text_in_range(None, "X", window, cx);
            assert_eq!(view.buffer.text(), "aX");
        });
    }

    #[gpui::test]
    fn a_reversed_selection_collapses_to_its_near_end(cx: &mut TestAppContext) {
        with_editor(cx, "abc", |view, window, cx| {
            view.select_left(&SelectLeft, window, cx);
            view.left(&Left, window, cx);
            assert_eq!(view.cursor(), at(2));
        });
    }

    #[gpui::test]
    fn replacing_a_selection_is_a_single_undo_step(cx: &mut TestAppContext) {
        with_editor(cx, "abc", |view, window, cx| {
            view.select_all(&SelectAll, window, cx);
            view.replace_text_in_range(None, "Z", window, cx);
            assert_eq!(view.buffer.text(), "Z");

            view.undo(&Undo, window, cx);
            assert_eq!(view.buffer.text(), "abc");
            assert_eq!(view.cursor(), at(3));
        });
    }

    #[gpui::test]
    fn undo_places_the_cursor_on_the_restored_text(cx: &mut TestAppContext) {
        with_editor(cx, "abc", |view, window, cx| {
            view.home(&Home, window, cx);
            view.delete(&Delete, window, cx);
            assert_eq!(view.buffer.text(), "bc");

            view.undo(&Undo, window, cx);
            assert_eq!(view.buffer.text(), "abc");
            assert_eq!(view.cursor(), at(1));

            view.redo(&Redo, window, cx);
            assert_eq!(view.buffer.text(), "bc");
        });
    }

    /// End-to-end: real keystrokes through GPUI's dispatch, proving the element
    /// registers its input handler on paint and the bindings reach the actions.
    #[gpui::test]
    fn keystrokes_reach_the_buffer_through_gpui_dispatch(cx: &mut TestAppContext) {
        cx.update(|cx| cx.bind_keys(key_bindings()));
        let window = cx.add_window(|_, cx| EditorView::new("", cx));
        window
            .update(cx, |view, window, cx| window.focus(&view.focus_handle, cx))
            .unwrap();

        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.simulate_input("héllo");
        window
            .update(&mut cx, |view, _, _| {
                assert_eq!(view.buffer.text(), "héllo");
                assert_eq!(view.cursor(), at(6));
            })
            .unwrap();

        cx.simulate_keystrokes("backspace shift-left");
        window
            .update(&mut cx, |view, _, _| {
                assert_eq!(view.buffer.text(), "héll");
                assert_eq!(view.selection, at(4)..at(5));
            })
            .unwrap();
    }
}
