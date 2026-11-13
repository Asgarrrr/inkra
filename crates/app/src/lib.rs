//! Inkra: a note editor on GPUI. A library with two entry points, `run` and `shot`, and
//! `seed_session` for the screenshot script, so every view stays `pub(crate)`.

mod app_files;
mod background;
mod backlinks;
mod bootstrap;
mod buffer_events;
mod command_palette;
mod count;
mod editor;
mod field;
mod file_kind;
mod images;
mod index;
mod keymap;
mod last_vault;
mod logger;
mod one_line;
mod open;
mod pane;
mod picker;
mod quick_open;
mod restore;
mod row_text;
mod shell;
mod sidebar;
#[cfg(test)]
mod test_support;
mod text_input;
mod theme;
mod timeline;
mod ui;
mod vault;
mod vault_search;

#[cfg(feature = "shot")]
pub mod shot;

use gpui::actions;

actions!(inkra, [Quit]);

pub use bootstrap::{run, seed_session};
