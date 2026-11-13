//! A read session mapped onto the layout before the first frame, then onto the vault walked whole,
//! then onto the notes that loaded (restore-tabs spec, R6; splits spec, Session).

use std::collections::HashMap;
use std::ffi::OsStr;

use inkra_layout::{GroupId, Layout, Shape, Side, ViewId, kept_active};
use inkra_vault::{EntryId, Vault};

use super::format::dock_views;
use super::{SavedFileTab, SavedGroup, SavedMember, SavedTab, Session};
use crate::editor::ViewState;
use crate::file_kind::FileKind;

/// What restores before the first frame: the tree, each dock group's views and its active index,
/// and the centre's saved groups still to load. No session restores docks rule 6's layout.
pub(crate) struct Surfaces {
    pub(crate) layout: Layout,
    pub(crate) docks: Vec<(GroupId, Vec<ViewId>, usize)>,
    pub(crate) pending: Option<Pending>,
}

/// The centre's saved groups by the ids the layout gave them, depth-first, and the saved active index.
pub(crate) struct Pending {
    groups: Vec<(GroupId, SavedGroup)>,
    active: usize,
}

/// A saved tab whose path names a file a tab can show.
struct Survivor {
    saved_ix: usize,
    file: EntryId,
    kind: FileKind,
    view: Option<ViewState>,
}

/// One centre group's survivors and its saved active index.
struct GroupPlan {
    group: GroupId,
    survivors: Vec<Survivor>,
    saved_active: usize,
}

/// Every centre group, depth-first, with the saved tabs whose path names a file, and the saved active index.
pub(crate) struct Plan {
    groups: Vec<GroupPlan>,
    active: usize,
}

/// A tab whose file's open landed.
pub(crate) struct Arrival<L> {
    pub(crate) file: EntryId,
    pub(crate) landing: L,
    pub(crate) view: Option<ViewState>,
}

/// The tabs whose file arrived: the one to show, the rest in bar order, and where the shown one stands.
pub(crate) struct Restored<L> {
    pub(crate) shown: Arrival<L>,
    pub(crate) parked: Vec<Arrival<L>>,
    pub(crate) active: usize,
}

/// Each group whose files arrived, and the groups that may take the saved active one's place, nearest first.
pub(crate) struct Landed<L> {
    pub(crate) groups: Vec<(GroupId, Restored<L>)>,
    pub(crate) active: Vec<GroupId>,
}

impl Surfaces {
    pub(crate) fn resolve(session: Option<Session>, catalog: &[(ViewId, Side)]) -> Self {
        let mut saved = Vec::new();
        let (center, [left, right], shown, active) = match session {
            None => (Shape::Group, [None, None], [true, true], None),
            Some(session) => (
                session
                    .center
                    .map_or(Shape::Group, |center| shape(center, &mut saved)),
                [session.left, session.right],
                session.shown,
                Some(session.active),
            ),
        };
        let views = dock_views([left.as_ref(), right.as_ref()], catalog);
        // A side saved empty is drawn only if it took a view no other dock kept.
        let dock_shape = |dock: Option<SavedMember>, views: &[(Vec<ViewId>, usize)]| match dock {
            Some(dock) => Some(shape(dock, &mut Vec::new())),
            None => (!views.is_empty()).then_some(Shape::Group),
        };
        let shapes = [dock_shape(left, &views[0]), dock_shape(right, &views[1])];
        let mut layout = Layout::restored(
            &center,
            shapes.each_ref().map(Option::as_ref),
            shown,
            active.unwrap_or(0),
        );
        let mut docks = Vec::new();
        let mut emptied = Vec::new();
        for (side, views) in [Side::Left, Side::Right].into_iter().zip(views) {
            for (group, (views, active)) in layout.dock_groups(side).into_iter().zip(views) {
                if views.is_empty() {
                    emptied.push(group);
                } else {
                    docks.push((group, views, active));
                }
            }
        }
        for group in emptied {
            layout.remove(group);
        }
        let pending = active.map(|active| Pending {
            groups: layout.center_groups().into_iter().zip(saved).collect(),
            active,
        });
        Self {
            layout,
            docks,
            pending,
        }
    }
}

/// Depth-first, as `Layout::restored` numbers the groups.
fn shape(member: SavedMember, groups: &mut Vec<SavedGroup>) -> Shape {
    match member {
        SavedMember::Group(group) => {
            groups.push(group);
            Shape::Group
        }
        SavedMember::Split {
            split,
            flexes,
            children,
        } => Shape::Split {
            axis: split.into(),
            children: children
                .into_iter()
                .map(|child| shape(child, groups))
                .collect(),
            flexes,
        },
    }
}

/// A view tab names no file: the centre drops it and the view returns to its default dock, since
/// the centre lands after the walk and Files must show during it.
fn path(tab: &SavedTab) -> Option<&OsStr> {
    match tab {
        SavedTab::File(SavedFileTab { file, .. }) => file.as_deref().map(OsStr::new),
        SavedTab::View { .. } => None,
    }
}

fn view(tab: &SavedTab) -> Option<ViewState> {
    match tab {
        SavedTab::File(SavedFileTab { view, .. }) => view.map(ViewState::from),
        SavedTab::View { .. } => None,
    }
}

impl Plan {
    /// Byte-exact on each path, in one pass over the entries; `None` if no tab names a file.
    pub(crate) fn resolve(pending: &Pending, vault: &Vault) -> Option<Self> {
        let tabs = || pending.groups.iter().flat_map(|(_, group)| &group.tabs);
        let mut found: HashMap<&OsStr, Option<(EntryId, FileKind)>> =
            tabs().filter_map(path).map(|path| (path, None)).collect();
        for entry in vault.entries().filter(|entry| entry.kind.is_file()) {
            if let Some(slot) = found.get_mut(entry.path.as_path().as_os_str())
                && let Some(kind) = FileKind::of(entry.path.as_path())
            {
                *slot = Some((entry.id, kind));
            }
        }
        let groups: Vec<_> = pending
            .groups
            .iter()
            .map(|(group, saved)| GroupPlan {
                group: *group,
                survivors: saved
                    .tabs
                    .iter()
                    .enumerate()
                    .filter_map(|(saved_ix, tab)| {
                        let (file, kind) = (*found.get(path(tab)?)?)?;
                        Some(Survivor {
                            saved_ix,
                            file,
                            kind,
                            view: view(tab),
                        })
                    })
                    .collect(),
                saved_active: saved.active,
            })
            .collect();
        groups
            .iter()
            .any(|group| !group.survivors.is_empty())
            .then_some(Self {
                groups,
                active: pending.active,
            })
    }

    /// The opens to join, depth-first, each group's in bar order.
    pub(crate) fn files(&self) -> impl Iterator<Item = (EntryId, FileKind)> + '_ {
        self.groups
            .iter()
            .flat_map(|group| &group.survivors)
            .map(|survivor| (survivor.file, survivor.kind))
    }

    /// Keeps the tabs whose open landed; `landings` holds one per survivor, in `files`' order.
    pub(crate) fn restored<L>(self, landings: Vec<Option<L>>) -> Landed<L> {
        let active = self
            .groups
            .iter()
            .enumerate()
            .rev()
            .filter(|&(ix, _)| ix <= self.active)
            .map(|(_, group)| group.group)
            .collect();
        let mut landings = landings.into_iter();
        let groups = self
            .groups
            .into_iter()
            .filter_map(|plan| {
                Some((
                    plan.group,
                    restored(plan.survivors, &mut landings, plan.saved_active)?,
                ))
            })
            .collect();
        Landed { groups, active }
    }
}

/// `None` if no survivor arrived; takes one landing per survivor from `landings`.
fn restored<L>(
    survivors: Vec<Survivor>,
    landings: &mut impl Iterator<Item = Option<L>>,
    saved_active: usize,
) -> Option<Restored<L>> {
    let (mut arrivals, saved_ixs): (Vec<_>, Vec<_>) = survivors
        .into_iter()
        .zip(landings)
        .filter_map(|(survivor, landing)| {
            let arrival = Arrival {
                file: survivor.file,
                landing: landing?,
                view: survivor.view,
            };
            Some((arrival, survivor.saved_ix))
        })
        .unzip();
    let active = kept_active(&saved_ixs, saved_active);
    (!arrivals.is_empty()).then(|| Restored {
        shown: arrivals.remove(active),
        parked: arrivals,
        active,
    })
}
