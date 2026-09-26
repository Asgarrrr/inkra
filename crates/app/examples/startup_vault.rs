//! Writes the 50,000-note startup vault, 100 folders or `-- flat` in the root, and prints its path.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

const NOTES: usize = 50_000;
const FOLDERS: usize = 100;
const SEED: u64 = 13;
/// Bumped when `note_text` or the layout changes, so the vault is written again.
const FORMAT: u32 = 1;
const WORDS: &[&str] = &[
    "le", "la", "les", "de", "des", "et", "un", "une", "projet", "projets", "réunion", "été",
    "équipe", "idée", "café", "santé", "déjà", "très", "être", "après", "the", "and", "of", "to",
    "project", "meeting", "summer", "team", "idea", "coffee", "notes", "plan", "draft", "review",
    "roadmap", "budget", "voyage", "lecture", "journal", "archive",
];
/// One note in 1,000 holds it once.
const RARE: &str = "zanzibar";
/// Written last, so a vault cut short is written again; hidden, so the scan skips it.
const WRITTEN: &str = ".written";

/// A fixed LCG, so every run writes the same vault.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, below: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        usize::try_from(self.0 >> 33).unwrap_or(0) % below
    }
}

/// 1 to 5 KB of prose: lines of 8 to 20 words, `.` or `,` between some.
fn note_text(lcg: &mut Lcg, note: usize) -> String {
    let target = 1_024 + lcg.next(4_097);
    let mut text = String::with_capacity(target + 128);
    while text.len() < target {
        for word in 0..8 + lcg.next(13) {
            if word > 0 {
                text.push(if lcg.next(9) == 0 { ',' } else { ' ' });
            }
            text.push_str(WORDS[lcg.next(WORDS.len())]);
        }
        text.push_str(".\n");
    }
    if note % 1_000 == 7 {
        text.push_str(RARE);
    }
    text
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
enum Layout {
    Nested,
    Flat,
}

fn parameters(layout: Layout) -> String {
    let mut hasher = DefaultHasher::new();
    (FORMAT, NOTES, FOLDERS, SEED, WORDS, RARE, layout).hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn write_vault(dir: &Path, layout: Layout, parameters: &str) -> std::io::Result<()> {
    let mut lcg = Lcg(SEED);
    for note in 0..NOTES {
        let folder = match layout {
            Layout::Nested => dir.join(format!("f{}", note % FOLDERS)),
            Layout::Flat => dir.to_path_buf(),
        };
        std::fs::create_dir_all(&folder)?;
        std::fs::write(
            folder.join(format!("n{note}.md")),
            note_text(&mut lcg, note),
        )?;
    }
    std::fs::write(dir.join(WRITTEN), parameters)
}

fn main() -> std::io::Result<()> {
    let (layout, name) = match std::env::args().nth(1).as_deref() {
        None => (Layout::Nested, "inkra-startup-50k"),
        Some("flat") => (Layout::Flat, "inkra-startup-50k-flat"),
        Some(other) => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("unknown layout {other:?}, expected `flat` or none"),
            ));
        }
    };
    let dir = std::env::temp_dir().join(name);
    let parameters = parameters(layout);
    let written = std::fs::read_to_string(dir.join(WRITTEN)).unwrap_or_default();
    if written != parameters {
        // An older layout would leave its notes beside the new ones.
        match std::fs::remove_dir_all(&dir) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error),
            _ => {}
        }
        write_vault(&dir, layout, &parameters)?;
    }
    println!("{}", dir.display());
    Ok(())
}
