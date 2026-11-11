use gpui::{
    App, Context, DispatchPhase, Entity, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, Point, Window, px,
};
use inkra_markdown::{Link, links};

use super::{EditorView, LinkFollowed};
use crate::open::Open;
use crate::text_input::InputHost;

impl EditorView {
    /// The innermost link whose drawn span holds the pointer. Not `link_at` on the click's
    /// offset: that is the nearest boundary, which past a line's end, beside a link, or inside
    /// its hidden syntax lands on the link's edge.
    fn link_under(&self, position: Point<Pixels>, cx: &App) -> Option<Link> {
        let (layout, local) = self.input.hit_test(position)?;
        if local.y < px(0.) || local.y >= layout.height() {
            return None;
        }
        let row = layout.row_for_offset(layout.offset_for_position(local));
        let spans = |link: &Link| {
            let (start, end) = (link.range.start, link.range.end);
            let (first, last) = (layout.row_for_offset(start), layout.row_for_offset(end));
            (first..=last).contains(&row)
                && (row > first || local.x >= layout.x_for_offset(start))
                && (row < last || local.x < layout.x_for_offset(end))
        };
        links(self.input.buffer().read(cx).text())
            .into_iter()
            .filter(spans)
            .min_by_key(|link| link.range.end - link.range.start)
    }

    pub(super) fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(self.input.focus_handle(), cx);
        let Some(offset) = self.input.offset_at(event.position) else {
            self.is_selecting = true;
            return;
        };
        // Before `is_selecting`: a ⌘-click follows and must not start a drag selection.
        if event.modifiers.platform
            && let Some(link) = self.link_under(event.position, cx)
        {
            // `⇧` adds the new tab: a mouse event keeps it, only a key's character clears it.
            let open = if event.modifiers.alt {
                Open::SplitRight
            } else if event.modifiers.shift {
                Open::NewTab
            } else {
                Open::Here
            };
            cx.emit(LinkFollowed { link, open });
            return;
        }
        self.is_selecting = true;
        let extend = event.modifiers.shift;
        self.move_cursor(window, cx, |state, buffer| {
            state.move_to(buffer, offset, extend);
        });
    }

    // On the window, not the div: a div's move listener stops when the pointer leaves it (div.rs:303).
    pub(super) fn track_drag(editor: &Entity<Self>, window: &mut Window) {
        let editor = editor.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble && event.pressed_button == Some(MouseButton::Left) {
                editor.update(cx, |view, cx| view.on_mouse_move(event, window, cx));
            }
        });
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.is_selecting {
            return;
        }
        let Some(offset) = self.input.offset_at(event.position) else {
            return;
        };
        self.move_cursor(window, cx, |state, buffer| {
            state.move_to(buffer, offset, true);
        });
    }

    pub(super) fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }
}
