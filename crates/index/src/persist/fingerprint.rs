//! What the cached words depend on beyond the format (persisted index spec, P3).

use std::time::SystemTime;

use crc32fast::Hasher;
use inkra_vault::{FileIdentity, Mtime, Stamp};

use super::notes::put_links;
use crate::words::{WORD_MAX_CHARS, fold};
use crate::{FileRead, NoteContent};

/// Accents, a ligature, final sigma, Turkish i, CJK and emoji.
const FOLD_PROBES: [&str; 9] = [
    "Été",
    "e\u{301}te",
    "ﬁnal",
    "ΣΑΣ",
    "İı",
    "Straße",
    "東京タワー",
    "😀👍🏽",
    "ǅ",
];

const PROBE_NOTES: [&str; 3] = [
    "# Café notes\n\n[[zeta/one#Top]] See [[Other note]] and [a link](b.md). Word word WORD.",
    "- [ ] task_one\n> a quote, `code` and ﬁnal Straße\n\n```\nfenced block\n```",
    "日本語のテキスト 😀 x² e\u{301}te l'été snake_case 3.14",
];

/// The Unicode versions the fold and the word rule read, then the fold of each probe.
pub(crate) fn fold_fingerprint() -> u32 {
    let (normalization, segmentation, chars) = (
        unicode_normalization::UNICODE_VERSION,
        unicode_segmentation::UNICODE_VERSION,
        char::UNICODE_VERSION,
    );
    let mut hasher = Hasher::new();
    for part in [
        u64::from(normalization.0),
        u64::from(normalization.1),
        u64::from(normalization.2),
        segmentation.0,
        segmentation.1,
        segmentation.2,
        u64::from(chars.0),
        u64::from(chars.1),
        u64::from(chars.2),
    ] {
        hasher.update(&part.to_le_bytes());
    }
    for probe in FOLD_PROBES {
        terminated(&mut hasher, &fold(probe));
    }
    hasher.finalize()
}

/// The probe notes, plus a word past `WORD_MAX_CHARS` and a link deep inside a long line.
fn probe_notes() -> Vec<String> {
    let mut notes: Vec<String> = PROBE_NOTES.iter().map(|note| (*note).to_owned()).collect();
    notes.push(format!("{} short", "w".repeat(WORD_MAX_CHARS + 5)));
    notes.push(format!(
        "{} [[far/target#Part]] {}",
        "x ".repeat(90),
        "y ".repeat(90)
    ));
    notes
}

/// Each probe note's words and counts, then its links as the cache file encodes them.
pub(crate) fn derivation_fingerprint() -> u32 {
    let file = FileRead {
        stamp: Stamp {
            mtime: Mtime(SystemTime::UNIX_EPOCH),
            size: 0,
        },
        identity: FileIdentity {
            device: 0,
            inode: 0,
        },
    };
    let mut hasher = Hasher::new();
    let mut links = Vec::new();
    for note in probe_notes() {
        let content = NoteContent::parse(&note, file);
        for (word, count) in content.words.iter() {
            terminated(&mut hasher, word);
            hasher.update(&count.to_le_bytes());
        }
        links.clear();
        let Some(()) = put_links(&mut links, &content.source.links) else {
            panic!("a probe link fits the format");
        };
        hasher.update(&links);
    }
    hasher.finalize()
}

/// No word char is NUL, so the terminator keeps two splits of the same bytes apart.
fn terminated(hasher: &mut Hasher, word: &str) {
    hasher.update(word.as_bytes());
    hasher.update(&[0]);
}
