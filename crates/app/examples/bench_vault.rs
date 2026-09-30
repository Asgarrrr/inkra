//! Writes the `-- <notes>` vault the Obsidian comparison opens, and prints its path.
//! Content and layout: docs/benchmarks.md.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{Error, ErrorKind};
use std::path::Path;

const SEED: u64 = 29;
/// Bumped when the content or the layout changes, so the vault is written again.
const FORMAT: u32 = 1;
const MAX_FOLDERS: usize = 100;
const NOTES_PER_FOLDER: usize = 50;
/// Written last, so a vault cut short is written again; hidden, so neither app lists it.
const WRITTEN: &str = ".written";
/// Obsidian's per-vault state: the benchmark decides when it goes, not the generator.
const KEEP: &str = ".obsidian";
const COMMON: &[&str] = &[
    "the", "of", "and", "to", "a", "in", "is", "that", "for", "it", "as", "with", "on", "be", "at",
    "by", "this", "from", "or", "an", "are", "not", "but", "we", "can", "all", "will", "one",
    "which", "their", "more", "about", "when", "into", "some", "than", "then", "only", "also",
    "after", "before", "because", "project", "meeting", "team", "idea", "plan", "draft", "review",
    "budget", "design", "release", "customer", "question", "answer", "reading", "book", "chapter",
    "summary", "decision", "risk", "goal", "week", "month", "morning", "evening", "coffee",
    "travel", "health", "garden", "music", "recipe", "research", "paper", "method", "result",
    "data", "model", "system", "change", "problem", "reason", "example", "process", "feedback",
    "roadmap", "journal", "archive", "habit", "workout", "sleep", "focus", "writing", "course",
    "lecture", "invoice", "contract",
];
/// Rare words are built from these, so the vocabulary has a long tail like real prose.
const SYLLABLES: &[&str] = &[
    "ka", "lo", "mi", "ne", "ra", "so", "tu", "vel", "dor", "an", "is", "ber", "cor", "fen", "gal",
    "hol", "mar", "pen", "ril", "sta", "tor", "wen", "zu", "qua",
];
const RARE_WORDS: usize = 20_000;
const AREAS: &[&str] = &[
    "Projects",
    "Areas",
    "Resources",
    "Archive",
    "Journal",
    "People",
    "Reading",
    "Meetings",
    "Ideas",
    "Work",
];
const TOPICS: &[&str] = &[
    "General", "Planning", "Research", "Personal", "Clients", "Learning", "Health", "Finance",
    "Travel", "Home",
];
const TAGS: &[&str] = &[
    "todo",
    "idea",
    "meeting",
    "book",
    "review",
    "draft",
    "project/alpha",
    "project/beta",
    "person",
    "reference",
    "question",
    "someday",
];
const FENCES: &[&str] = &["rust", "js", "sh", "python", ""];

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

fn folder_count(notes: usize) -> usize {
    (notes / NOTES_PER_FOLDER).clamp(1, MAX_FOLDERS)
}

fn folder_path(folder: usize) -> String {
    format!(
        "{}/{}",
        AREAS[folder % AREAS.len()],
        TOPICS[folder / AREAS.len() % TOPICS.len()]
    )
}

struct Vault {
    lcg: Lcg,
    titles: Vec<String>,
    folders: usize,
}

impl Vault {
    /// Titles first, so a link can name a note written later.
    fn new(notes: usize) -> Self {
        let mut vault = Vault {
            lcg: Lcg(SEED),
            titles: Vec::with_capacity(notes),
            folders: folder_count(notes),
        };
        for note in 0..notes {
            let mut title = String::new();
            vault.words(&mut title, 2, 3);
            if let Some(first) = title.get_mut(..1) {
                first.make_ascii_uppercase();
            }
            title.push(' ');
            title.push_str(&note.to_string());
            vault.titles.push(title);
        }
        vault
    }

    /// Half the words from the common list, half from a skewed long tail.
    fn word(&mut self, text: &mut String) {
        if self.lcg.next(2) == 0 {
            text.push_str(COMMON[self.lcg.next(COMMON.len())]);
            return;
        }
        let skew = self.lcg.next(RARE_WORDS) + 1;
        let mut rank = self.lcg.next(skew) + SYLLABLES.len();
        while rank > 0 {
            text.push_str(SYLLABLES[rank % SYLLABLES.len()]);
            rank /= SYLLABLES.len();
        }
    }

    /// `least` words, plus up to `spread - 1` more.
    fn words(&mut self, text: &mut String, least: usize, spread: usize) {
        for index in 0..least + self.lcg.next(spread) {
            if index > 0 {
                text.push(' ');
            }
            self.word(text);
        }
    }

    /// Half the links stay in the note's folder, as notes on one topic link each other.
    fn link(&mut self, text: &mut String, note: usize) {
        let notes = self.titles.len();
        let target = if self.lcg.next(2) == 0 {
            self.lcg.next(notes / self.folders) * self.folders + note % self.folders
        } else {
            self.lcg.next(notes)
        };
        let target = if target == note {
            (target + 1) % notes
        } else {
            target
        };
        text.push_str("[[");
        text.push_str(&self.titles[target]);
        if self.lcg.next(4) == 0 {
            text.push('|');
            self.words(text, 1, 2);
        }
        text.push_str("]]");
    }

    fn sentence(&mut self, text: &mut String, note: usize) {
        for index in 0..6 + self.lcg.next(11) {
            if index > 0 {
                text.push(' ');
            }
            match self.lcg.next(60) {
                0 => self.link(text, note),
                1 => {
                    text.push_str("**");
                    self.words(text, 1, 2);
                    text.push_str("**");
                }
                2 => {
                    text.push('*');
                    self.word(text);
                    text.push('*');
                }
                3 if self.lcg.next(3) == 0 => {
                    text.push('#');
                    text.push_str(TAGS[self.lcg.next(TAGS.len())]);
                }
                _ => self.word(text),
            }
        }
        text.push('.');
    }

    fn list(&mut self, text: &mut String, note: usize, marker: impl Fn(usize) -> String) {
        for item in 0..2 + self.lcg.next(5) {
            text.push_str(&marker(item));
            self.sentence(text, note);
            text.push('\n');
        }
    }

    fn frontmatter(&mut self, text: &mut String) {
        let created = format!(
            "2026-{:02}-{:02}",
            1 + self.lcg.next(12),
            1 + self.lcg.next(28)
        );
        let first = TAGS[self.lcg.next(TAGS.len())];
        let second = TAGS[self.lcg.next(TAGS.len())];
        for part in [
            "---\ncreated: ",
            &created,
            "\ntags: [",
            first,
            ", ",
            second,
            "]\n---\n\n",
        ] {
            text.push_str(part);
        }
    }

    fn fence(&mut self, text: &mut String) {
        text.push_str("```");
        text.push_str(FENCES[self.lcg.next(FENCES.len())]);
        text.push('\n');
        for _ in 0..2 + self.lcg.next(5) {
            text.push_str("let ");
            self.word(text);
            text.push_str(" = ");
            self.word(text);
            text.push_str("(1);\n");
        }
        text.push_str("```\n");
    }

    /// 0.5 to 5 KB of blocks, mostly paragraphs.
    fn note_text(&mut self, note: usize) -> String {
        let target = 512 + self.lcg.next(4_609);
        let mut text = String::with_capacity(target + 512);
        if self.lcg.next(5) == 0 {
            self.frontmatter(&mut text);
        }
        if self.lcg.next(3) != 0 {
            text.push_str("# ");
            text.push_str(&self.titles[note]);
            text.push_str("\n\n");
        }
        while text.len() < target {
            match self.lcg.next(20) {
                0..=8 => {
                    for _ in 0..2 + self.lcg.next(4) {
                        self.sentence(&mut text, note);
                        text.push(' ');
                    }
                    text.pop();
                    text.push('\n');
                }
                9..=11 => self.list(&mut text, note, |_| "- ".to_owned()),
                12 | 13 => self.list(&mut text, note, |item| format!("{}. ", item + 1)),
                14 | 15 => {
                    let done = self.lcg.next(2) == 0;
                    self.list(&mut text, note, |item| {
                        if done ^ (item % 3 == 0) {
                            "- [x] "
                        } else {
                            "- [ ] "
                        }
                        .to_owned()
                    });
                }
                16..=18 => {
                    text.push_str("## ");
                    self.words(&mut text, 2, 3);
                    text.push('\n');
                }
                _ => self.fence(&mut text),
            }
            text.push('\n');
        }
        text
    }
}

fn parameters(notes: usize) -> String {
    let mut hasher = DefaultHasher::new();
    (
        FORMAT, notes, SEED, COMMON, SYLLABLES, RARE_WORDS, AREAS, TOPICS, TAGS, FENCES,
    )
        .hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Removes an older vault's notes, which would stay beside the new ones, but keeps `.obsidian`.
fn clear(dir: &Path) -> std::io::Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        entries => entries?,
    };
    for entry in entries {
        let entry = entry?;
        if entry.file_name() == KEEP {
            continue;
        }
        if entry.file_type()?.is_dir() {
            std::fs::remove_dir_all(entry.path())?;
        } else {
            std::fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

fn write_vault(dir: &Path, notes: usize, parameters: &str) -> std::io::Result<()> {
    let mut vault = Vault::new(notes);
    for folder in 0..vault.folders {
        std::fs::create_dir_all(dir.join(folder_path(folder)))?;
    }
    for note in 0..notes {
        let path = dir
            .join(folder_path(note % vault.folders))
            .join(format!("{}.md", vault.titles[note]));
        let text = vault.note_text(note);
        std::fs::write(path, text)?;
    }
    std::fs::write(dir.join(WRITTEN), parameters)
}

fn main() -> std::io::Result<()> {
    let notes = std::env::args()
        .nth(1)
        .and_then(|arg| arg.parse::<usize>().ok())
        .filter(|&notes| notes > 0)
        .ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidInput,
                "expected a note count, e.g. `-- 5000`",
            )
        })?;
    let dir = std::env::temp_dir().join(format!("inkra-bench-{notes}"));
    let parameters = parameters(notes);
    let written = std::fs::read_to_string(dir.join(WRITTEN)).unwrap_or_default();
    if written != parameters {
        clear(&dir)?;
        write_vault(&dir, notes, &parameters)?;
    }
    println!("{}", dir.display());
    Ok(())
}
