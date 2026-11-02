//! `NoteIndex` against a model that scans every note and every link, sharing only `Key`
//! folding and `Target::parse` with the code under test.

use inkra_vault::{EntryId, Kind, Mtime, Vault};
use proptest::prelude::*;
use unicode_normalization::UnicodeNormalization;

use super::NoteIndex;
use crate::key::{Key, strip_md};
use crate::test_support::{id, mtime, note_set, try_rename, vault};
use crate::{CorpusSnapshot, NoteContent, Resolution, Source, Style, Target};

#[derive(Debug, Clone)]
enum Op {
    Create(String),
    Delete(String),
    Rename(String, String),
    /// A load or a save: the text and its mtime.
    Load(String, String, u64),
    Unload(String),
    Build,
    Hold,
    Merge,
    StartMerge,
    ApplyMerge,
    DropMerge,
}

fn small() -> impl Strategy<Value = String> {
    prop::sample::select(&["a.md", "b.md", "f/a.md", "F/b.md", "cafe\u{301}.md", "x.md"][..])
        .prop_map(str::to_owned)
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        small().prop_map(Op::Create),
        small().prop_map(Op::Delete),
        (small(), small()).prop_map(|(a, b)| Op::Rename(a, b)),
        (small(), "[a-céÉ' ]{0,12}", 0..4u64).prop_map(|(a, t, m)| Op::Load(a, t, m)),
        (small(), "[a-céÉ' ]{0,12}", 0..4u64).prop_map(|(a, t, m)| Op::Load(a, t, m)),
        small().prop_map(Op::Unload),
        Just(Op::Build),
        Just(Op::Hold),
        Just(Op::Merge),
        Just(Op::StartMerge),
        Just(Op::ApplyMerge),
        Just(Op::DropMerge),
    ]
}

proptest! {
    #[test]
    fn the_corpus_agrees_with_a_model_across_vault_changes(ops in prop::collection::vec(op(), 0..40)) {
        use std::collections::HashMap;
        use crate::corpus::{SLOT_BYTES, TEXT_HEADER_BYTES};
        use crate::test_support::{ROOT, texts};
        use crate::words::{NoteWords, Words};
        let (fs, mut vault) = vault(&["a.md"]);
        let mut index = NoteIndex::default();
        let _ = index.set_notes(note_set(&vault));
        let mut table: Vec<EntryId> = Model::new(&vault).notes.iter().map(|n| n.0).collect();
        let mut model: HashMap<EntryId, String> = HashMap::new();
        let mut modified: HashMap<EntryId, Mtime> = scanned(&vault).collect();
        let mut held = Vec::new();
        let mut held_words = Vec::new();
        let mut running = None;
        let find = |vault: &Vault, path: &str| {
            vault.entries().find(|e| e.path.as_path() == std::path::Path::new(path) && e.kind == Kind::Note).map(|e| e.id)
        };
        for op in ops {
            match op {
                Op::Create(path) => {
                    fs.insert_file(format!("{ROOT}/{path}"), "");
                    let _ = futures::executor::block_on(vault.rescan(&fs));
                }
                Op::Delete(path) => {
                    fs.remove(std::path::Path::new(&format!("{ROOT}/{path}")));
                    let _ = futures::executor::block_on(vault.rescan(&fs));
                }
                Op::Rename(from, to) => {
                    let _ = try_rename(&fs, &mut vault, &from, &to);
                }
                Op::Load(path, text, at) => {
                    if let Some(id) = find(&vault, &path) {
                        index.insert(id, NoteContent::parse(text.as_str().into(), mtime(at)));
                        if table.contains(&id) {
                            model.insert(id, text);
                            modified.insert(id, mtime(at));
                        }
                    }
                }
                Op::Unload(path) => {
                    if let Some(id) = find(&vault, &path) {
                        index.remove(id);
                        model.remove(&id);
                    }
                }
                Op::Build => {
                    let _ = index.set_notes(note_set(&vault));
                    table = Model::new(&vault).notes.iter().map(|n| n.0).collect();
                    model.retain(|id, _| table.contains(id));
                    // A note keeping its text keeps that text's mtime; the others take the scan's.
                    modified = scanned(&vault)
                        .map(|(id, scan)| (id, if model.contains_key(&id) { modified[&id] } else { scan }))
                        .collect();
                }
                Op::Hold => {
                    let snap = index.corpus();
                    let seen = (texts(&snap), mtimes(&snap));
                    held.push((snap, seen));
                    let words = index.words();
                    let seen = words.entries();
                    held_words.push((words, seen));
                }
                Op::Merge => index.merge_all(),
                Op::StartMerge => {
                    let started = index.words.start_merge();
                    prop_assert!(running.is_none() || started.is_none(), "one merge at a time");
                    running = running.or(started);
                }
                Op::ApplyMerge => {
                    if let Some(merge) = running.take() {
                        let _ = index.words_apply(merge.run());
                    }
                }
                Op::DropMerge => drop(running.take()),
            }
            let corpus = index.corpus();
            let expected: Vec<(EntryId, String)> = table
                .iter()
                .filter_map(|id| Some((*id, model.get(id)?.clone())))
                .collect();
            prop_assert_eq!(texts(&corpus), expected);
            prop_assert_eq!(corpus.len(), table.len());
            let expected: Vec<Option<Mtime>> = table.iter().map(|id| modified.get(id).copied()).collect();
            prop_assert_eq!(mtimes(&corpus), expected);
            let bytes: usize = model.values().map(String::len).sum();
            prop_assert_eq!(corpus.heap_bytes(), table.len() * SLOT_BYTES + model.len() * TEXT_HEADER_BYTES + bytes);
            for (snap, seen) in &held {
                prop_assert_eq!(&(texts(snap), mtimes(snap)), seen);
            }
            let fresh = {
                let mut words = Words::default();
                for (id, text) in &model {
                    words.set(*id, NoteWords::parse(text));
                }
                words.snapshot().entries()
            };
            prop_assert_eq!(index.words().entries(), fresh);
            for (snap, seen) in &held_words {
                prop_assert_eq!(&snap.entries(), seen);
            }
        }
    }
}

#[test]
fn external_edits_keep_each_old_mtime_until_the_reload() {
    use crate::test_support::ROOT;
    let (fs, mut vault) = vault(&["a.md", "b.md", "c.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    let (a, b, c) = (id(&vault, "a.md"), id(&vault, "b.md"), id(&vault, "c.md"));
    for (note, at) in [(a, 0), (b, 1), (c, 2)] {
        index.insert(note, NoteContent::parse("old".into(), mtime(at)));
    }

    for file in ["a.md", "b.md", "c.md", "d.md"] {
        fs.insert_file(format!("{ROOT}/{file}"), "edited");
    }
    let Ok(()) = futures::executor::block_on(vault.rescan(&fs)) else {
        panic!("a MemFs rescan cannot fail");
    };
    let _ = index.set_notes(note_set(&vault));
    let d = id(&vault, "d.md");
    let scan: std::collections::HashMap<EntryId, Mtime> = scanned(&vault).collect();
    assert_eq!(
        mtimes(&index.corpus()),
        [
            Some(mtime(0)),
            Some(mtime(1)),
            Some(mtime(2)),
            Some(scan[&d])
        ],
        "each text keeps its mtime; d has no text yet"
    );

    index.insert(c, NoteContent::parse("edited".into(), scan[&c]));
    index.insert(a, NoteContent::parse("edited".into(), scan[&a]));
    let _ = index.set_notes(note_set(&vault));
    assert_eq!(
        mtimes(&index.corpus()),
        [
            Some(scan[&a]),
            Some(mtime(1)),
            Some(scan[&c]),
            Some(scan[&d])
        ],
        "b's reload has not landed"
    );

    index.insert(b, NoteContent::parse("edited".into(), scan[&b]));
    assert_eq!(
        mtimes(&index.corpus()),
        [
            Some(scan[&a]),
            Some(scan[&b]),
            Some(scan[&c]),
            Some(scan[&d])
        ]
    );
}

fn scanned(vault: &Vault) -> impl Iterator<Item = (EntryId, Mtime)> {
    vault
        .entries()
        .filter(|entry| entry.kind == Kind::Note)
        .map(|entry| (entry.id, entry.mtime))
}

fn mtimes(corpus: &CorpusSnapshot) -> Vec<Option<Mtime>> {
    (0..corpus.len()).map(|at| corpus.modified(at)).collect()
}

struct Model {
    /// Notes in vault order: id and NFC path without `.md`.
    notes: Vec<(EntryId, String)>,
}

impl Model {
    fn new(vault: &Vault) -> Self {
        let notes = vault
            .entries()
            .filter(|entry| entry.kind == Kind::Note)
            .map(|entry| {
                let path = entry.path.as_path().to_string_lossy();
                (entry.id, strip_md(&path).nfc().collect())
            })
            .collect();
        Self { notes }
    }

    fn resolve(&self, source: EntryId, target: &Target) -> Resolution {
        let Target::Note { path, style, .. } = target else {
            return match target {
                Target::External(url) => Resolution::External(url.clone()),
                _ => Resolution::SelfLink,
            };
        };
        if path.ends_with('/')
            || path.ends_with("/.")
            || path.ends_with("/..")
            || path == "."
            || path == ".."
        {
            return Resolution::Unresolved;
        }
        let written: String = path.nfc().collect();
        let written = match style {
            Style::Relative => match self.join(source, &written) {
                Some(joined) => joined,
                None => return Resolution::Unresolved,
            },
            _ => written,
        };
        let wanted = Key::path(&written);
        let last = |text: &str| text.rsplit('/').next().unwrap_or(text).to_owned();
        let mut best: Option<(bool, usize, usize, EntryId)> = None;
        for (order, (id, path)) in self.notes.iter().enumerate() {
            let key = Key::path(path);
            let matches = match style {
                Style::Name => Key::fold(&last(path)) == Key::fold(&last(&written)),
                Style::Suffix => {
                    key == wanted || key.as_str().ends_with(&format!("/{}", wanted.as_str()))
                }
                Style::Relative => key == wanted,
            };
            if !matches {
                continue;
            }
            let exact = *path == written || path.ends_with(&format!("/{written}"));
            let rank = (!exact, path.matches('/').count(), order, *id);
            if best.is_none_or(|current| rank < current) {
                best = Some(rank);
            }
        }
        best.map_or(Resolution::Unresolved, |(.., id)| Resolution::Note(id))
    }

    fn join(&self, source: EntryId, path: &str) -> Option<String> {
        let mut stack: Vec<String> = Vec::new();
        if !path.starts_with('/') {
            let (_, own) = self.notes.iter().find(|(id, _)| *id == source)?;
            let mut folders: Vec<&str> = own.split('/').collect();
            folders.pop();
            stack.extend(folders.into_iter().map(str::to_owned));
        }
        for part in path
            .split('/')
            .filter(|part| !part.is_empty() && *part != ".")
        {
            if part == ".." {
                stack.pop()?;
            } else {
                stack.push(part.to_owned());
            }
        }
        Some(stack.join("/"))
    }
}

const NAMES: &[&str] = &["a", "A", "b", "caf\u{e9}", "cafe\u{301}", "v1.2", "x.md"];
const FOLDERS: &[&str] = &["", "f/", "F/", "f/g/"];
const PREFIXES: &[&str] = &["", "./", "../", "/", "/f/", "f/", "g/"];

fn file() -> impl Strategy<Value = String> {
    (prop::sample::select(FOLDERS), prop::sample::select(NAMES))
        .prop_map(|(folder, name)| format!("{folder}{name}.md"))
}

fn link() -> impl Strategy<Value = String> {
    (
        0..4u8,
        prop::sample::select(PREFIXES),
        prop::sample::select(NAMES),
    )
        .prop_map(|(form, prefix, name)| match form {
            0 => format!("[[{name}]]"),
            1 => format!("[[{prefix}{name}]]"),
            2 => format!("[x]({prefix}{name}.md)"),
            _ => format!(
                "[x]({prefix}{name}{})",
                ["/", "/.", "/..", "/../x.md"][name.len() % 4]
            ),
        })
}

fn check(index: &NoteIndex, model: &Model, vault: &Vault, sources: &[(EntryId, Source)]) {
    for (source, parsed) in sources {
        for link in &parsed.links {
            assert_eq!(
                index.resolve(Some(*source), &link.target),
                model.resolve(*source, &link.target),
                "{:?} from {source:?}",
                link.target
            );
        }
    }
    for (note, _) in &model.notes {
        let mut expected: Vec<(usize, EntryId, usize)> = Vec::new();
        for (source, parsed) in sources {
            // The index drops the source of a note gone from the vault.
            let Some(order) = model.notes.iter().position(|(id, _)| id == source) else {
                continue;
            };
            for link in &parsed.links {
                if model.resolve(*source, &link.target) == Resolution::Note(*note) {
                    expected.push((order, *source, link.range.start.get()));
                    let Target::Note { path, .. } = &link.target else {
                        panic!("only note links resolve to a note");
                    };
                    let stem = model
                        .notes
                        .iter()
                        .find(|(id, _)| id == note)
                        .map(|(_, p)| Key::stem(p));
                    assert_eq!(Some(Key::stem(path)), stem, "the stem property");
                }
            }
        }
        expected.sort_unstable();
        let actual: Vec<(usize, EntryId, usize)> = index
            .backlinks(*note)
            .into_iter()
            .flat_map(|backlink| {
                let order = model
                    .notes
                    .iter()
                    .position(|(id, _)| *id == backlink.source)
                    .unwrap_or(usize::MAX);
                backlink
                    .links
                    .into_iter()
                    .map(move |link| (order, backlink.source, link.range.start.get()))
            })
            .collect();
        assert_eq!(
            actual,
            expected,
            "backlinks of {note:?} in {vault_len} entries",
            vault_len = vault.entries().count()
        );
    }
}

/// `PROPTEST_CASES` when it parses, else 96: fewer than proptest's default, as each case is slow.
fn cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|cases| cases.parse().ok())
        .unwrap_or(96)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(cases()))]

    #[test]
    fn the_index_agrees_with_a_scan_of_every_note(
        files in prop::collection::btree_set(file(), 1..10),
        texts in prop::collection::vec(prop::collection::vec(link(), 0..5), 10),
        renames in prop::collection::vec((any::<prop::sample::Index>(), file()), 0..3),
    ) {
        let files: Vec<&str> = files.iter().map(String::as_str).collect();
        let (fs, mut vault) = vault(&files);
        let mut index = NoteIndex::default();
        let _ = index.set_notes(note_set(&vault));
        let loaded: Vec<(EntryId, Source, String)> = files
            .iter()
            .zip(&texts)
            .map(|(file, links)| {
                let text = links.join(" ");
                (id(&vault, file), Source::parse(&text), text)
            })
            .collect();
        for (source, _, text) in &loaded {
            index.insert(*source, NoteContent::parse(text.as_str().into(), mtime(0)));
        }
        let sources: Vec<(EntryId, Source)> = loaded
            .into_iter()
            .map(|(source, parsed, _)| (source, parsed))
            .collect();
        check(&index, &Model::new(&vault), &vault, &sources);

        for (from, to) in renames {
            let notes: Vec<String> = vault
                .entries()
                .filter(|entry| entry.kind == Kind::Note)
                .map(|entry| entry.path.as_path().to_string_lossy().into_owned())
                .collect();
            if notes.is_empty() {
                break;
            }
            let from = from.get(&notes);
            if notes.contains(&to) {
                continue;
            }
            if !try_rename(&fs, &mut vault, from, &to) {
                continue;
            }
            let _ = index.set_notes(note_set(&vault));
            check(&index, &Model::new(&vault), &vault, &sources);
        }
    }
}
