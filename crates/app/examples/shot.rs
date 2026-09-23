//! Trampoline onto `inkra_app::shot`, which lives inside the library so it
//! can reach the `pub(crate)` views.
//!
//!   cargo run -p inkra-app --features shot --example shot

fn main() {
    inkra_app::shot::run();
}
