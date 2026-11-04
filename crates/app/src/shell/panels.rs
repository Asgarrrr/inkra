//! Showing and hiding the sidebar and the backlinks, and the toggles that do it (shell chrome spec, S2a-D3).

use gpui::{Context, Focusable, IntoElement, Window, actions};

use super::AppShell;
use crate::sidebar::Sidebar;
use crate::ui::{IconButton, IconName};

actions!(panels, [ToggleSidebar, ToggleBacklinks]);

/// Both shown at launch. A hidden panel keeps its entity, so it comes back as it was left.
#[derive(Clone, Copy)]
pub(super) struct PanelsShown {
    pub(super) sidebar: bool,
    pub(super) backlinks: bool,
}

impl Default for PanelsShown {
    fn default() -> Self {
        Self {
            sidebar: true,
            backlinks: true,
        }
    }
}

impl AppShell {
    /// A hidden tree keeps neither focus nor an open name field: either would take keys out of sight.
    pub(super) fn toggle_sidebar(
        &mut self,
        _: &ToggleSidebar,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(panes) = &mut self.panes else {
            return;
        };
        panes.shown.sidebar = !panes.shown.sidebar;
        if !panes.shown.sidebar {
            let sidebar = panes.sidebar.clone();
            let focused = sidebar.focus_handle(cx).contains_focused(window, cx);
            sidebar.update(cx, Sidebar::cancel_edit);
            if focused && let Some(focus) = self.land_focus(cx) {
                window.focus(&focus, cx);
            }
        }
        cx.notify();
    }

    /// The backlinks take no focus, so a hide lands none.
    pub(super) fn toggle_backlinks(
        &mut self,
        _: &ToggleBacklinks,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(panes) = &mut self.panes {
            panes.shown.backlinks = !panes.shown.backlinks;
            cx.notify();
        }
    }

    /// `None` with no vault: there is no panel to toggle.
    pub(super) fn render_panel_toggles(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<(impl IntoElement + use<>, impl IntoElement + use<>)> {
        let shown = self.panes.as_ref()?.shown;
        let sidebar = IconButton::new(
            "panel-toggle-sidebar",
            IconName::PanelLeft,
            "Toggle sidebar",
        )
        .toggled(shown.sidebar)
        .on_click(cx.listener(|shell, _, window, cx| {
            shell.toggle_sidebar(&ToggleSidebar, window, cx);
        }));
        let backlinks = IconButton::new(
            "panel-toggle-backlinks",
            IconName::PanelRight,
            "Toggle backlinks",
        )
        .toggled(shown.backlinks)
        .on_click(cx.listener(|shell, _, window, cx| {
            shell.toggle_backlinks(&ToggleBacklinks, window, cx);
        }));
        Some((sidebar, backlinks))
    }
}
