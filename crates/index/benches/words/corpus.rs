use std::num::NonZero;
use std::path::Path;
use std::sync::{Arc, LazyLock};
use std::thread;
use std::time::UNIX_EPOCH;

use futures::executor::block_on;
use inkra_index::NoteContent;
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Kind, Mtime, Vault};

pub(crate) const NOTES: usize = 50_000;
const FOLDERS: usize = 100;
/// Notes parsed at once, so parsed words never pile up far past a fresh tier.
const PARSE_CHUNK: usize = 1_024;
const VOCABULARY: usize = 30_000;
const SYLLABLES: &[&str] = &[
    "pro", "jet", "ré", "u", "nion", "é", "té", "ca", "fé", "plan", "da", "mi", "no", "ta", "lu",
    "ver", "sa", "ki", "ou", "ain", "ment", "tion", "ble", "que", "é", "è", "ç",
];
/// The startup vault's words (`benches/search.rs`).
const WORDS: &[&str] = &[
    "le", "la", "les", "de", "des", "et", "un", "une", "projet", "projets", "réunion", "été",
    "équipe", "idée", "café", "santé", "déjà", "très", "être", "après", "the", "and", "of", "to",
    "project", "meeting", "summer", "team", "idea", "coffee", "notes", "plan", "draft", "review",
    "roadmap", "budget", "voyage", "lecture", "journal", "archive",
];
/// One note in 1,000 holds it once.
const RARE: &str = "zanzibar";
/// Scanned once: a `MemFs` scan cost grows with files per folder times folders.
pub(crate) static ENTRIES: LazyLock<Vec<(EntryId, String, Kind, Mtime)>> = LazyLock::new(entries);
/// The notes of `ENTRIES`, in vault order: the order the startup load inserts them.
pub(crate) static NOTE_IDS: LazyLock<Vec<EntryId>> = LazyLock::new(|| {
    ENTRIES
        .iter()
        .filter(|(_, _, kind, _)| *kind == Kind::Note)
        .map(|(id, ..)| *id)
        .collect()
});
static VOCAB: LazyLock<Vec<String>> = LazyLock::new(|| (0..VOCABULARY).map(word).collect());
static CUMULATIVE: LazyLock<Vec<f64>> = LazyLock::new(cumulative);

/// A word drawn from a corpus's vocabulary.
pub(crate) type Pick = fn(&mut Lcg) -> &'static str;

/// A fixed LCG, so every run benches the same corpus.
pub(crate) struct Lcg(pub(crate) u64);

impl Lcg {
    pub(crate) fn next(&mut self, below: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        usize::try_from(self.0 >> 33).unwrap_or(0) % below
    }
}

fn entries() -> Vec<(EntryId, String, Kind, Mtime)> {
    let fs = MemFs::new();
    for note in 0..NOTES {
        fs.insert_file(format!("/vault/f{}/n{note}.md", note % FOLDERS), "");
    }
    let Ok(vault) = block_on(Vault::scan(&fs, Path::new("/vault"))) else {
        panic!("a MemFs scan cannot fail");
    };
    vault
        .entries()
        .map(|entry| {
            (
                entry.id,
                entry.path.as_path().to_string_lossy().into_owned(),
                entry.kind,
                entry.mtime,
            )
        })
        .collect()
}

/// Word `n` of the vocabulary: 1 to 4 syllables, some accented; word 0 is the most frequent.
fn word(n: usize) -> String {
    let mut lcg = Lcg(u64::try_from(n).unwrap_or(0) * 7 + 3);
    (0..=lcg.next(4))
        .map(|_| SYLLABLES[lcg.next(SYLLABLES.len())])
        .collect()
}

/// The running sum of the harmonic weights 1/1, 1/2, … 1/`VOCABULARY`.
fn cumulative() -> Vec<f64> {
    let mut sum = 0.0;
    (1..=VOCABULARY)
        .map(|rank| {
            sum += 1.0 / f64::from(u32::try_from(rank).unwrap_or(u32::MAX));
            sum
        })
        .collect()
}

/// A Zipf rank in `0..VOCABULARY`, by inverse transform on `cumulative`.
fn zipf(lcg: &mut Lcg, cumulative: &[f64]) -> usize {
    let total = cumulative[cumulative.len() - 1];
    let draw = u32::try_from(lcg.next(1 << 30)).unwrap_or(0);
    let x = f64::from(draw) / f64::from(1u32 << 30) * total;
    cumulative
        .partition_point(|&c| c < x)
        .min(cumulative.len() - 1)
}

pub(crate) fn zipf_word(lcg: &mut Lcg) -> &'static str {
    &VOCAB[zipf(lcg, &CUMULATIVE)]
}

pub(crate) fn forty_word(lcg: &mut Lcg) -> &'static str {
    WORDS[lcg.next(WORDS.len())]
}

/// Lines of 8 to 20 words until `bytes`, with `,` or `'` between some.
pub(crate) fn prose(lcg: &mut Lcg, bytes: usize, pick: Pick) -> String {
    let mut text = String::with_capacity(bytes + 128);
    while text.len() < bytes {
        for word in 0..8 + lcg.next(13) {
            if word > 0 {
                text.push(match lcg.next(12) {
                    0 => ',',
                    1 => '\'',
                    _ => ' ',
                });
            }
            text.push_str(pick(lcg));
        }
        text.push_str(".\n");
    }
    text
}

#[derive(Clone, Copy)]
pub(crate) enum Corpus {
    /// 30,000 generated words, Zipf-distributed: the stress case.
    Zipf,
    /// The startup vault's 40 words, uniform: the realistic case.
    Forty,
}

impl Corpus {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Zipf => "zipf",
            Self::Forty => "40 words",
        }
    }

    pub(crate) fn pick(self) -> Pick {
        match self {
            Self::Zipf => zipf_word,
            Self::Forty => forty_word,
        }
    }

    /// Every note's text: 1 to 5 KB, and `RARE` in one note of 1,000.
    pub(crate) fn texts(self) -> Vec<Arc<str>> {
        let mut lcg = Lcg(13);
        (0..NOTES)
            .map(|note| {
                let bytes = 1_024 + lcg.next(4_097);
                let mut text = prose(&mut lcg, bytes, self.pick());
                if note % 1_000 == 7 {
                    text.push_str(RARE);
                }
                text.into()
            })
            .collect()
    }

    /// `(case, query)`; the 40-word corpus has no word 1,000, and its words are equally common.
    pub(crate) fn queries(self) -> Vec<(&'static str, String)> {
        let (top, mid) = match self {
            Self::Zipf => (VOCAB[0].as_str(), Some(VOCAB[1_000].as_str())),
            Self::Forty => ("projet", None),
        };
        let second = mid.unwrap_or("réunion");
        let mut queries = vec![
            ("1 letter \"e\"", "e".to_owned()),
            ("top word", top.to_owned()),
        ];
        queries.extend(mid.map(|mid| ("word 1,000", mid.to_owned())));
        queries.extend([
            ("rare \"zanzibar\"", RARE.to_owned()),
            ("two words", format!("{top} {second}")),
            ("no match \"zzzz\"", "zzzz".to_owned()),
        ]);
        queries
    }
}

/// `texts` parsed on every core.
fn parse(texts: &[Arc<str>]) -> Vec<NoteContent> {
    let threads = thread::available_parallelism().map_or(1, NonZero::get);
    let part = texts.len().div_ceil(threads).max(1);
    thread::scope(|scope| {
        let parsing: Vec<_> = texts
            .chunks(part)
            .map(|texts| {
                scope.spawn(move || {
                    texts
                        .iter()
                        .map(|text| NoteContent::parse(Arc::clone(text), Mtime(UNIX_EPOCH)))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        parsing
            .into_iter()
            .flat_map(|parsing| {
                let Ok(contents) = parsing.join() else {
                    panic!("a parse thread panicked");
                };
                contents
            })
            .collect()
    })
}

/// `texts` parsed a chunk at a time, as loads land.
pub(crate) fn contents(texts: &[Arc<str>]) -> impl Iterator<Item = NoteContent> + '_ {
    texts.chunks(PARSE_CHUNK).flat_map(parse)
}
