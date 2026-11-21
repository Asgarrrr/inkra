//! What the shell needs to know about a dock pane (feature registry spec, D5).

use gpui::{Action, AnyView, App, Context, Entity, FocusHandle, Render};
use inkra_layout::{Side, ViewId};

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

/// What the shell asks of a pane's view. Not dyn-compatible: `PaneEntry` erases it.
pub(crate) trait Pane: Render {
    fn header(&self, cx: &App) -> PaneHeader;

    /// The handle a pane's tab focuses when it activates.
    fn focus(&self, _cx: &App) -> Option<FocusHandle> {
        None
    }

    /// The pane's tab stopped showing, its dock hid, or its feature switched off.
    fn hidden(&mut self, _cx: &mut Context<Self>) {}

    fn shown(&mut self, _cx: &mut Context<Self>) {}
}

pub(crate) enum PaneHeader {
    Title { count: Option<usize> },
    Buttons(&'static [HeaderButton]),
}

pub(crate) struct HeaderButton {
    pub(crate) id: &'static str,
    pub(crate) icon: IconName,
    pub(crate) tooltip: &'static str,
    pub(crate) action: fn() -> Box<dyn Action>,
}

/// A button a feature adds to `pane`'s header, drawn after the pane's own while the feature is on.
pub(crate) struct PaneButton {
    pub(crate) pane: ViewId,
    pub(crate) button: HeaderButton,
}

type Read<R> = Box<dyn Fn(&App) -> R>;
type Hook = Box<dyn Fn(&mut App)>;

/// A registered pane, its `Pane` methods bound to its entity.
pub(crate) struct PaneEntry {
    view: AnyView,
    header: Read<PaneHeader>,
    focus: Read<Option<FocusHandle>>,
    hidden: Hook,
    shown: Hook,
}

impl PaneEntry {
    pub(crate) fn new<T: Pane>(entity: Entity<T>) -> Self {
        let header = entity.clone();
        let focus = entity.clone();
        let hidden = entity.clone();
        let shown = entity.clone();
        Self {
            view: entity.into(),
            header: Box::new(move |cx| header.read(cx).header(cx)),
            focus: Box::new(move |cx| focus.read(cx).focus(cx)),
            hidden: Box::new(move |cx| hidden.update(cx, T::hidden)),
            shown: Box::new(move |cx| shown.update(cx, T::shown)),
        }
    }

    pub(crate) fn view(&self) -> AnyView {
        self.view.clone()
    }

    pub(crate) fn header(&self, cx: &App) -> PaneHeader {
        (self.header)(cx)
    }

    pub(crate) fn focus(&self, cx: &App) -> Option<FocusHandle> {
        (self.focus)(cx)
    }

    pub(crate) fn hidden(&self, cx: &mut App) {
        (self.hidden)(cx);
    }

    pub(crate) fn shown(&self, cx: &mut App) {
        (self.shown)(cx);
    }
}

pub(crate) static CORE_PANES: &[&PaneDecl] = &[&FILES_PANE];

pub(crate) fn boxed<A: Action + Default>() -> Box<dyn Action> {
    Box::new(A::default())
}

pub(crate) fn sides(decls: impl IntoIterator<Item = &'static PaneDecl>) -> Vec<(ViewId, Side)> {
    decls.into_iter().map(|decl| (decl.id, decl.side)).collect()
}

#[cfg(test)]
mod tests;
