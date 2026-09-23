//! Renders the real views off-screen to PNG for a human to look at.
//!
//!   cargo run -p inkra-app --features shot --example shot
//!   cargo run -p inkra-app --features shot --example shot -- editor-selection

mod scenes;
mod stage;

use std::path::{Path, PathBuf};

use scenes::{Scene, catalogue};
use stage::Stage;

const OUTPUT: &str = "target/shots";

/// Entry point behind `examples/shot.rs`.
///
/// # Panics
///
/// If a window cannot be opened, a frame cannot be captured, or a PNG cannot be
/// written.
pub fn run() {
    // `args_os`: `args` panics on non-UTF-8, and a mistyped name owes the catalogue, not a backtrace.
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
    // One `Stage` per scene: a shared one carries over clipboard, focus and open windows.
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
