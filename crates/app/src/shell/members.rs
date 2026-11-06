//! The members of a layout tree: a split lays its children out by flex, a group draws its column (splits spec, P2).

use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement, Styled, div,
    prelude::FluentBuilder, px,
};
use inkra_layout::{Axis, GroupId, Member, SplitPath, Surface};

use super::AppShell;

/// Where a member sits: its parent's axis and its flex.
#[derive(Clone, Copy)]
pub(super) struct Slot {
    pub(super) axis: Axis,
    flex: f32,
}

impl Slot {
    /// The root, as its row's one member.
    pub(super) const ROOT: Self = Self {
        axis: Axis::Horizontal,
        flex: 1.0,
    };

    /// Basis 0, so the flexes alone share the parent's room; `min_*_0`, else content would widen it.
    pub(super) fn size<E: Styled>(self, element: E) -> E {
        let element = element.flex_basis(px(0.)).flex_grow(self.flex).relative();
        match self.axis {
            Axis::Horizontal => element.min_w_0().h_full(),
            Axis::Vertical => element.min_h_0().w_full(),
        }
    }
}

impl AppShell {
    /// `k` counts `path.surface`'s groups drawn so far, depth-first; `path` names `member`.
    pub(super) fn render_member(
        &self,
        member: &Member,
        slot: Slot,
        path: &SplitPath,
        top: &[GroupId],
        k: &mut usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match member {
            Member::Group(group) => {
                let lower = !top.contains(group);
                let column = match path.surface {
                    Surface::Center => self
                        .render_note_column(*k, *group, (slot, path), lower, cx)
                        .into_any_element(),
                    Surface::Dock(side) => self
                        .render_view_column(side, *k, *group, (slot, path), lower, cx)
                        .into_any_element(),
                };
                *k += 1;
                column
            }
            Member::Split {
                axis,
                children,
                flexes,
            } => {
                let separator = Self::render_separator(path, slot, *k, cx);
                let children =
                    self.render_split(*axis, children.iter().zip(flexes), path, top, k, cx);
                slot.size(div().flex().when(*axis == Axis::Vertical, Styled::flex_col))
                    .on_drag_move(Self::on_separator_drag(path.clone(), (px(0.), px(0.)), cx))
                    .children(children)
                    .children(separator)
                    .into_any_element()
            }
        }
    }

    /// `path` names the split.
    pub(super) fn render_split<'a>(
        &self,
        axis: Axis,
        children: impl Iterator<Item = (&'a Member, &'a f32)>,
        path: &SplitPath,
        top: &[GroupId],
        k: &mut usize,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        children
            .enumerate()
            .map(|(ix, (child, &flex))| {
                let mut child_path = path.clone();
                child_path.steps.push(ix);
                self.render_member(child, Slot { axis, flex }, &child_path, top, k, cx)
            })
            .collect()
    }
}
