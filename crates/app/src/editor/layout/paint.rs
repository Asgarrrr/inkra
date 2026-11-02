use gpui::{App, Pixels, Point, TextAlign, Window, fill, point, px};

use super::DocumentLayout;
use super::prefix::Prefix;
use crate::theme;

impl DocumentLayout {
    /// One `WrappedLine::paint` per line; gpui steps rows down at each wrap. A
    /// failed line is skipped and logged, not fatal, so one bad glyph run doesn't blank the page.
    pub(crate) fn paint(&self, origin: Point<Pixels>, window: &mut Window, cx: &mut App) {
        for bar in &self.bars {
            window.paint_quad(fill(*bar + origin, theme::quote_bar()));
        }
        for (index, line) in self.lines.iter().enumerate() {
            let line_origin = origin + point(px(0.), line.top);
            if let Err(err) = line.wrapped.paint(
                line_origin + point(line.indent(), px(0.)),
                line.row_height,
                TextAlign::Left,
                None,
                window,
                cx,
            ) {
                log::warn!("failed to paint line {index}: {err}");
            }
            if let Some((shaped, x)) = line.prefix.as_ref().and_then(Prefix::shown)
                && let Err(err) = shaped.paint(
                    line_origin + point(x, px(0.)),
                    line.row_height,
                    TextAlign::Left,
                    None,
                    window,
                    cx,
                )
            {
                log::warn!("failed to paint the prefix of line {index}: {err}");
            }
        }
    }
}
