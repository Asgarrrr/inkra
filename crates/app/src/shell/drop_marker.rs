//! The edge of a bar's slot a drag over it lands on, and the marker drawn there (docks spec, Drag).

use std::cmp::Ordering;

use gpui::{StyleRefinement, Styled};

use crate::theme::{self, ThemeColors};

/// Where slot `ix` sits from the dragged item, at `own` in this bar: `Less` marks its left edge,
/// `Greater` its right. An item from another bar (`None`) lands before the slot.
pub(super) fn landing(own: Option<usize>, ix: usize) -> Ordering {
    own.map_or(Ordering::Less, |index| ix.cmp(&index))
}

/// The slot lit in the drop colour, with an edge on the side `landing` names.
pub(super) fn mark(
    style: StyleRefinement,
    landing: Ordering,
    colors: &ThemeColors,
) -> StyleRefinement {
    let style = style
        .bg(colors.accent_background)
        .border_color(colors.accent);
    match landing {
        Ordering::Less => style.border_l(theme::DROP_EDGE),
        Ordering::Greater => style.border_r(theme::DROP_EDGE),
        Ordering::Equal => style,
    }
}
