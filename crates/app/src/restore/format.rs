//! The session file: plain data, no gpui (restore-tabs spec, R1–R2).

use std::fmt;
use std::path::{Path, PathBuf};

use inkra_layout::{Axis, StoredDock, View, restore_views};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};

const VERSION: u32 = 2;

/// One vault's tree of tab groups, its docks, and the active group (splits spec, Session).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Session {
    pub(crate) version: u32,
    /// The vault's absolute root: a file whose root differs is not this vault's.
    pub(crate) root: PathBuf,
    /// The active group's depth-first index: the centre's groups, then the left dock's, then the right's.
    pub(crate) active: usize,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient_center"
    )]
    pub(crate) center: Option<SavedMember>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient_center"
    )]
    pub(crate) left: Option<SavedMember>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient_center"
    )]
    pub(crate) right: Option<SavedMember>,
    #[serde(default = "both_shown")]
    pub(crate) shown: [bool; 2],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum SavedMember {
    Split {
        split: SavedAxis,
        flexes: Vec<f32>,
        children: Vec<SavedMember>,
    },
    Group(SavedGroup),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SavedAxis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SavedGroup {
    pub(crate) tabs: Vec<SavedTab>,
    pub(crate) active: usize,
}

/// A view by name: an unknown one drops alone, where a serde enum would fail the whole file (docks spec).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum SavedTab {
    View { view: String },
    Note(SavedNoteTab),
}

/// `note` is vault-relative, `None` for an empty tab.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SavedNoteTab {
    pub(crate) note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) view: Option<SavedView>,
}

/// A view in byte offsets, without its goal.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(crate) struct SavedView {
    pub(crate) tail: usize,
    pub(crate) head: usize,
    pub(crate) top: usize,
    pub(crate) below: f32,
}

/// Version 1: the centre's one group and the docks (restore-tabs spec).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SessionV1 {
    pub(crate) version: u32,
    pub(crate) root: PathBuf,
    pub(crate) active: usize,
    pub(crate) tabs: Vec<SavedNoteTab>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient"
    )]
    pub(crate) docks: Option<SavedDocks>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SavedDocks {
    pub(crate) left: SavedDock,
    pub(crate) right: SavedDock,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SavedDock {
    pub(crate) views: Vec<String>,
    pub(crate) active: usize,
    pub(crate) shown: bool,
}

/// A malformed surface reads as absent, so the others still restore.
fn lenient<'de, D: Deserializer<'de>, T: DeserializeOwned>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).ok())
}

/// A split with no child is malformed: it would leave a surface a room with no group.
fn lenient_center<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<SavedMember>, D::Error> {
    Ok(lenient(deserializer)?.filter(SavedMember::is_whole))
}

fn both_shown() -> [bool; 2] {
    [true, true]
}

impl SavedMember {
    fn groups(&self) -> Vec<&SavedGroup> {
        match self {
            Self::Group(group) => vec![group],
            Self::Split { children, .. } => children.iter().flat_map(Self::groups).collect(),
        }
    }

    fn is_whole(&self) -> bool {
        match self {
            Self::Group(_) => true,
            Self::Split { children, .. } => {
                !children.is_empty() && children.iter().all(Self::is_whole)
            }
        }
    }
}

impl From<Axis> for SavedAxis {
    fn from(axis: Axis) -> Self {
        match axis {
            Axis::Horizontal => Self::Horizontal,
            Axis::Vertical => Self::Vertical,
        }
    }
}

impl From<SavedAxis> for Axis {
    fn from(axis: SavedAxis) -> Self {
        match axis {
            SavedAxis::Horizontal => Self::Horizontal,
            SavedAxis::Vertical => Self::Vertical,
        }
    }
}

impl SavedGroup {
    /// The views its tabs name, its active index following them: a note tab drops (L2).
    fn stored(&self) -> StoredDock {
        let (views, saved_ixs): (Vec<_>, Vec<_>) = self
            .tabs
            .iter()
            .enumerate()
            .filter_map(|(saved_ix, tab)| match tab {
                SavedTab::View { view } => Some((view.clone(), saved_ix)),
                SavedTab::Note(_) => None,
            })
            .unzip();
        // The saved active tab, else its nearest kept tab to the left, else the first.
        let active = saved_ixs
            .iter()
            .rposition(|&saved_ix| saved_ix <= self.active)
            .unwrap_or(0);
        StoredDock { views, active }
    }
}

/// Each dock group's views and active index, depth-first, left then right; each view in exactly one.
pub(crate) fn dock_views(docks: [Option<&SavedMember>; 2]) -> [Vec<(Vec<View>, usize)>; 2] {
    let stored = docks.map(|dock| {
        dock.map_or_else(Vec::new, |dock| {
            dock.groups().into_iter().map(SavedGroup::stored).collect()
        })
    });
    restore_views(stored.each_ref().map(Vec::as_slice))
}

impl SavedDock {
    fn group(self) -> SavedGroup {
        SavedGroup {
            tabs: self
                .views
                .into_iter()
                .map(|view| SavedTab::View { view })
                .collect(),
            active: self.active,
        }
    }
}

/// Its tabs become one centre group, active, and each dock one group.
impl From<SessionV1> for Session {
    fn from(v1: SessionV1) -> Self {
        let center = SavedMember::Group(SavedGroup {
            tabs: v1.tabs.into_iter().map(SavedTab::Note).collect(),
            active: v1.active,
        });
        let (left, right, shown) = v1.docks.map_or((None, None, both_shown()), |docks| {
            let shown = [docks.left.shown, docks.right.shown];
            let [left, right] =
                [docks.left, docks.right].map(|dock| Some(SavedMember::Group(dock.group())));
            (left, right, shown)
        });
        Self {
            version: VERSION,
            root: v1.root,
            active: 0,
            center: Some(center),
            left,
            right,
            shown,
        }
    }
}

/// Why a session file restores nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Rejected {
    Corrupt,
    Version,
    Root,
}

impl fmt::Display for Rejected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Corrupt => "corrupt",
            Self::Version => "other version",
            Self::Root => "other vault",
        })
    }
}

impl Session {
    pub(crate) fn new(
        root: PathBuf,
        active: usize,
        center: SavedMember,
        left: Option<SavedMember>,
        right: Option<SavedMember>,
        shown: [bool; 2],
    ) -> Self {
        Self {
            version: VERSION,
            root,
            active,
            center: Some(center),
            left,
            right,
            shown,
        }
    }

    /// Fails only on a root that is not UTF-8.
    pub(crate) fn encode(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    pub(crate) fn decode(text: &str, root: &Path) -> Result<Self, Rejected> {
        #[derive(Deserialize)]
        struct Versioned {
            version: u32,
        }
        let Versioned { version } = serde_json::from_str(text).map_err(|_| Rejected::Corrupt)?;
        let session = match version {
            1 => serde_json::from_str::<SessionV1>(text).map(Self::from),
            VERSION => serde_json::from_str(text),
            _ => return Err(Rejected::Version),
        }
        .map_err(|_| Rejected::Corrupt)?;
        if session.root != root {
            return Err(Rejected::Root);
        }
        Ok(session)
    }
}

/// FNV-1a 64, written out: `DefaultHasher`'s output may change between Rust releases.
fn fnv1a(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    bytes.iter().fold(OFFSET, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(PRIME)
    })
}

/// Where the session of the vault at `root`, absolute, lives under `home` (R1).
pub(crate) fn session_file(home: &Path, root: &Path) -> PathBuf {
    let key = fnv1a(root.as_os_str().as_encoded_bytes());
    home.join("Library/Application Support/Inkra/sessions")
        .join(format!("{key:016x}.json"))
}

#[cfg(test)]
mod tests;
