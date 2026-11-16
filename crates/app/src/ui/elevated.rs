//! The surface a tooltip, a menu or a picker floats on, after Zed's `elevated`.

use gpui::{App, Styled};

use crate::theme;

/// How high a surface floats, which sets its shadow.
#[derive(Clone, Copy)]
pub(crate) enum Elevation {
    /// A tooltip.
    Low,
    /// A menu or a picker, in the shell's modal layer.
    High,
}

/// The elevated background, a 1 px border, the elevated radius and `elevation`'s shadow.
pub(crate) fn elevated<E: Styled>(surface: E, elevation: Elevation, cx: &App) -> E {
    let colors = theme::current(cx).colors;
    let surface = surface
        .bg(colors.elevated_background)
        .border_1()
        .border_color(colors.border)
        .rounded(theme::ELEVATED_RADIUS);
    match elevation {
        Elevation::Low => surface.shadow_md(),
        Elevation::High => surface.shadow_lg(),
    }
}
