//! Trampoline onto the library, which is what gives `examples/shot.rs` a
//! `[lib]` to link against. See `lib.rs`.

fn main() {
    inkra_app::run();
}
