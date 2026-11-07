//! A sideways wheel over a code block scrolls that block (code block scroll spec, D6), and a
//! caret in it scrolls it into view (D5).

use std::time::{Duration, Instant};

use gpui::{App, Context, Pixels, Point, ScrollWheelEvent, TouchPhase, px};
use inkra_text::{Bias, ByteOffset};

use super::EditorView;
use super::layout::DocumentLayout;
use crate::theme::CODE_BLOCK_SCROLL_MARGIN;

/// How long a block's bar stays after it scrolls.
pub(crate) const BAR_LINGER: Duration = Duration::from_secs(1);

/// gpui's `SCROLL_EVENT_SEPARATION` (private): a longer pause starts a new gesture.
pub(crate) const GESTURE_GAP: Duration = Duration::from_millis(28);
/// gpui's unlock thresholds in `OngoingScroll::filter_at`.
const UNLOCK_RATIO: f32 = 1.9;
const UNLOCK_LOWER_BOUND: Pixels = px(6.);

#[derive(Clone, Copy, PartialEq)]
enum Taker {
    Page,
    Block,
}

/// One wheel gesture: gpui's `OngoingScroll` on the executor's clock, plus who took its first
/// event that moved something; the other refuses the rest, whatever the axis filter unlocks.
/// Only a sideways lock is kept: a gesture that starts vertical is the page's from its first event.
#[derive(Default)]
pub(super) struct Gesture {
    last: Option<Instant>,
    sideways: bool,
    taker: Option<Taker>,
}

impl Gesture {
    /// Drops a precise `delta`'s y while the gesture is locked sideways; false when `phase` ends
    /// the gesture. An imprecise event, a mouse wheel tick, is a gesture of its own.
    fn filter(
        &mut self,
        delta: &mut Point<Pixels>,
        phase: TouchPhase,
        precise: bool,
        now: Instant,
    ) -> bool {
        if matches!(phase, TouchPhase::Ended | TouchPhase::Cancelled) {
            *self = Self::default();
            return false;
        }
        let new = !precise
            || phase == TouchPhase::Started
            || self
                .last
                .is_none_or(|last| now.duration_since(last) >= GESTURE_GAP);
        if new {
            *self = Self::default();
        }
        if !precise || *delta == Point::default() {
            return true;
        }
        self.last = Some(now);
        let (x, y) = (delta.x.abs(), delta.y.abs());
        if new {
            self.sideways = x > y;
        } else if y >= UNLOCK_LOWER_BOUND && y >= x * UNLOCK_RATIO {
            self.sideways = false;
        }
        if self.sideways {
            delta.y = px(0.);
        }
        true
    }

    fn take(&mut self, wants: Option<Taker>) -> Option<Taker> {
        self.taker = self.taker.or(wants);
        self.taker
    }
}

impl EditorView {
    /// `over` is the block under the pointer, as its start and `max`. True when the block took
    /// the event, at its limit too, so the page never takes the rest of the gesture.
    pub(super) fn on_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        over: Option<(ByteOffset, Pixels)>,
        line_height: Pixels,
        cx: &mut Context<Self>,
    ) -> bool {
        let raw = event.delta.pixel_delta(line_height);
        let mut delta = raw;
        let now = cx.background_executor().now();
        // Fed over prose too, so a gesture keeps its axis across a block (div.rs:3339).
        if !self
            .gesture
            .filter(&mut delta, event.touch_phase, event.delta.precise(), now)
        {
            return false;
        }
        // The hitbox may come from a frame painted before an edit.
        let over = over.filter(|&(start, _)| {
            self.refresh(cx);
            self.is_block_start(start)
        });
        let wants = if delta.y != px(0.) {
            Some(Taker::Page)
        } else if over.is_some() && raw != Point::default() {
            Some(Taker::Block)
        } else {
            None
        };
        if self.gesture.take(wants) != Some(Taker::Block) {
            return false;
        }
        let Some((start, max)) = over else {
            return true;
        };
        let stored = self.scrolls.x(start);
        let new = (stored.min(max) - delta.x).clamp(px(0.), max);
        if new != stored {
            self.scroll_block(start, new, cx);
            cx.notify();
        }
        true
    }

    /// Scrolls the caret's block the least that shows the caret, a margin inside the right edge.
    /// Never notifies: its callers already do, and prepaint runs it mid-frame.
    pub(super) fn reveal_block(&mut self, layout: &DocumentLayout, cx: &mut Context<Self>) {
        let Some(span) = layout.block_span(self.cursor()) else {
            return;
        };
        self.refresh(cx);
        if !self.is_block_start(span.start) {
            return;
        }
        let rel = span.caret - span.left;
        let x = self.scrolls.x(span.start).min(span.max);
        let new = if rel < x {
            rel
        } else if rel + CODE_BLOCK_SCROLL_MARGIN > x + span.visible {
            rel + CODE_BLOCK_SCROLL_MARGIN - span.visible
        } else {
            x
        };
        let new = new.clamp(px(0.), span.max);
        if new != x {
            self.scroll_block(span.start, new, cx);
        }
    }

    /// Stores block `start`'s new `x` and repaints once when its bar's linger ends.
    fn scroll_block(&mut self, start: ByteOffset, x: Pixels, cx: &mut Context<Self>) {
        let now = cx.background_executor().now();
        self.input
            .buffer()
            .update(cx, |buffer, _| self.scrolls.set(start, x, now, buffer));
        self.expire_bar(cx);
    }

    /// A later scroll replaces the task, which cancels it.
    fn expire_bar(&mut self, cx: &mut Context<Self>) {
        self.bar_expiry = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(BAR_LINGER).await;
            this.update(cx, |_, cx| cx.notify()).ok();
        }));
    }

    /// Before the view leaves its buffer: the anchors resolve only there.
    pub(super) fn forget_scrolls(&self, cx: &mut App) {
        self.input
            .buffer()
            .update(cx, |buffer, _| self.scrolls.forget_all(buffer));
    }

    /// Clips each start to the bound buffer; the next `refresh` drops any non-block start. Each
    /// entry is a scroll from zero, so its bar shows as after a swipe.
    pub(super) fn anchor_scrolls(
        &mut self,
        entries: &[(ByteOffset, Pixels)],
        cx: &mut Context<Self>,
    ) {
        let now = cx.background_executor().now();
        self.input.buffer().update(cx, |buffer, _| {
            let clipped: Vec<_> = entries
                .iter()
                .map(|&(start, x)| (buffer.clip_grapheme(start, Bias::Left), x))
                .collect();
            self.scrolls.anchor_all(&clipped, now, buffer);
        });
        if !entries.is_empty() {
            self.expire_bar(cx);
        }
    }

    pub(super) fn is_block_start(&self, offset: ByteOffset) -> bool {
        self.markdown
            .code_blocks
            .binary_search_by_key(&offset, |block| block.range.start)
            .is_ok()
    }

    #[cfg(test)]
    pub(crate) fn block_x(&self, index: usize) -> Pixels {
        self.drawn_layout().block_scroll(index)
    }

    /// Block `index`'s centre in window coordinates; `None` before it is drawn.
    #[cfg(any(test, feature = "shot"))]
    pub(crate) fn block_center(&self, index: usize) -> Option<Point<Pixels>> {
        let bounds = self.input.last_bounds()?;
        let background = *self.input.last_layout()?.block_backgrounds().get(index)?;
        Some(bounds.origin + background.center())
    }
}
