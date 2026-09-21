//! Renders the real views into an off-screen window and writes the frame to a
//! PNG, so a change to what the app *looks like* can be checked without a human
//! at the screen. `check.sh` rasterizes nothing, so a view can stop drawing
//! text entirely while every test stays green.
//!
//! A tool, not a gate: no baselines, nothing compared, never run by `check.sh`.
//! See `docs/superpowers/specs/2026-09-21-offscreen-shots-design.md`.
//!
//!   cargo run -p inkra-app --features shot --example shot
//!   cargo run -p inkra-app --features shot --example shot -- editor-selection

mod scenes;
mod stage;

use std::path::{Path, PathBuf};

use scenes::{Scene, catalogue};
use stage::Stage;

/// Under `target/`, which is already gitignored: shots are looked at and
/// thrown away, never committed.
const OUTPUT: &str = "target/shots";

/// Entry point behind `examples/shot.rs`.
///
/// Panics on any failure rather than propagating it. This is run by hand: a
/// backtrace names the failing scene and line, where a `Result` would print a
/// message with nobody positioned to act on it.
///
/// # Panics
///
/// If a window cannot be opened, a frame cannot be captured, or a PNG cannot be
/// written.
pub fn run() {
    // `args_os`, not `args`: the latter panics on an argument that is not valid
    // UTF-8, and this path is reserved for defects — a mistyped name is user
    // error and owes the catalogue, not a backtrace. `bootstrap::launch` takes
    // the same care, for the same reason.
    //
    // `nth(1)`, not a parser: one optional scene name is the whole surface.
    let requested = std::env::args_os()
        .nth(1)
        .map(|argument| argument.to_string_lossy().into_owned());

    let wanted: Vec<&Scene> = match requested.as_deref() {
        None => catalogue().iter().collect(),
        Some("--list") => {
            list();
            return;
        }
        Some(name) => {
            let Some(scene) = catalogue().iter().find(|scene| scene.name == name) else {
                // Exit non-zero: a typo must not read as a pass.
                eprintln!("no scene named `{name}`");
                list();
                std::process::exit(2);
            };
            vec![scene]
        }
    };

    let out = PathBuf::from(OUTPUT);
    std::fs::create_dir_all(&out)
        .unwrap_or_else(|error| panic!("create {}: {error}", out.display()));

    for scene in wanted {
        shoot(scene, &out);
    }
}

fn shoot(scene: &Scene, out: &Path) {
    // One `Stage` per scene, so no scene inherits another's app state: a
    // shared one would carry over clipboard, focus and every open window.
    let mut stage = Stage::new();
    let window = (scene.open)(&mut stage);
    let image = stage.capture(window);

    let path = out.join(format!("{}.png", scene.name));
    image
        .save(&path)
        .unwrap_or_else(|error| panic!("save {}: {error}", path.display()));

    println!(
        "{:<18} {}x{}  {}",
        scene.name,
        image.width(),
        image.height(),
        path.display()
    );
}

fn list() {
    println!("scenes:");
    for scene in catalogue() {
        println!("  {:<18} {}", scene.name, scene.shows);
    }
}
