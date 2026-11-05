//! A dock view on the move: what a drag carries and the preview under the pointer (docks spec, Drag).

use gpui::{Context, IntoElement, Render, Window};
use inkra_layout::{Side, View};

use super::docks::{icon, title};
use crate::ui::IconButton;

/// The view under the pointer; also its preview. `side` and `index` are where it stood when the drag began.
#[derive(Clone)]
pub(super) struct DraggedView {
    pub(super) view: View,
    pub(super) side: Side,
    pub(super) index: usize,
}

impl Render for DraggedView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        IconButton::new("dock-view-preview", icon(self.view), title(self.view)).toggled(true)
    }
}
