//! The note vault: a directory of markdown files, and the seam to the
//! filesystem that holds it.
//!
//! No gpui, no workspace dependency — `Fs::load` returns a `String` and `app`
//! builds the `Buffer`. See
//! `docs/superpowers/specs/2026-09-20-inkra-vault-design.md`.
