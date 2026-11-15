//! Inkra: a note editor on GPUI. A library with two entry points, `run` and `shot`, plus
//! `seed_session` for the screenshot script and `bench` for the layout bench, so every view stays
//! `pub(crate)`.

mod app_files;
mod background;
mod backlinks;
#[cfg(feature = "bench")]
pub mod bench;
mod bootstrap;
mod buffer_events;
mod command_palette;
mod count;
mod decorations;
mod editor;
mod feature;
mod field;
mod file_kind;
mod highlighting;
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
mod settings;
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
