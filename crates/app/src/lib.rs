//! Inkra: a note editor on GPUI.
//!
//! A library with two entry points rather than a bare binary: `run` opens the
//! real window, and `shot` renders the same views into an off-screen one. Both
//! live inside the crate so every view stays `pub(crate)` — a second entry
//! point is not a reason to widen the public surface. That surface is `run`,
//! `shot::run`, and `Quit` — the last unavoidably, since `actions!` expands to
//! `pub struct`.
//!
//! Editing semantics live in `inkra-editor`, the text model in `inkra-text`,
//! the editor view in `editor/`, and the window's outer layout in `shell.rs`.

mod bootstrap;
mod editor;
mod field;
mod keymap;
mod shell;
mod sidebar;
mod theme;
mod vault;

#[cfg(feature = "shot")]
pub mod shot;

use gpui::actions;

actions!(inkra, [Quit]);

pub use bootstrap::run;
