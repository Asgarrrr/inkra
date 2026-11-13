//! What the shell needs to know about a dock pane (feature registry spec, D5).

use gpui::Action;
use inkra_layout::{Side, ViewId};

use crate::backlinks::BACKLINKS_PANE;
use crate::shell::FILES_PANE;
use crate::ui::IconName;

pub(crate) struct PaneDecl {
    pub(crate) id: ViewId,
    pub(crate) title: &'static str,
    pub(crate) icon: IconName,
    /// Stored, not formatted from `id`: `IconButton::new` and `PanelHeader::new` take `&'static str`.
    pub(crate) icon_id: &'static str,
    pub(crate) header_id: &'static str,
    /// Docks rule 6: where the pane goes when no dock holds it.
    pub(crate) side: Side,
    pub(crate) move_action: fn() -> Box<dyn Action>,
}

pub(crate) static CORE_PANES: &[&PaneDecl] = &[&FILES_PANE, &BACKLINKS_PANE];

pub(crate) fn boxed<A: Action + Default>() -> Box<dyn Action> {
    Box::new(A::default())
}

pub(crate) fn sides(decls: impl IntoIterator<Item = &'static PaneDecl>) -> Vec<(ViewId, Side)> {
    decls.into_iter().map(|decl| (decl.id, decl.side)).collect()
}

#[cfg(test)]
mod tests;
