//! Trampoline onto `inkra_app::shot`, which lives inside the library so it can
//! reach the `pub(crate)` views. Why an example and not a test or a bin:
//! `docs/superpowers/specs/2026-09-21-offscreen-shots-design.md`.
//!
//!   cargo run -p inkra-app --features shot --example shot

fn main() {
    inkra_app::shot::run();
}
