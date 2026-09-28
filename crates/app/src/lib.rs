//! Inkra: a note editor on GPUI. A library with two entry points, `run` and
//! `shot`, so every view stays `pub(crate)` instead of widening the public surface.

mod backlinks;
mod bootstrap;
mod buffer_events;
mod count;
mod editor;
mod field;
mod index;
mod keymap;
mod one_line;
mod picker;
mod quick_open;
mod shell;
mod sidebar;
#[cfg(test)]
mod test_support;
mod theme;
mod timeline;
mod vault;

#[cfg(feature = "shot")]
pub mod shot;

use gpui::actions;

actions!(inkra, [Quit]);

pub use bootstrap::run;
