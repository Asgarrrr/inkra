use gpui::{App, IntoElement, RenderOnce, Styled, Window, div};

use crate::theme;

/// The dirty dot: a note edited but not written.
#[derive(IntoElement)]
pub(crate) struct Indicator;

impl RenderOnce for Indicator {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .size(theme::DOT_SIZE)
            .rounded_full()
            .bg(theme::dirty_dot())
    }
}
