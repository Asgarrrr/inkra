//! What the cached words depend on beyond the format (persisted index spec, P3).

use std::time::SystemTime;

use crc32fast::Hasher;
use inkra_vault::{FileIdentity, Mtime, Stamp};

use super::notes::style_code;
use crate::words::fold;
use crate::{FileRead, NoteContent, Target};

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

/// Each probe note's words, as `NoteContent::parse` sorts them, then its link targets, sorted.
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
    for note in PROBE_NOTES {
        let content = NoteContent::parse(note, file);
        for (word, _) in content.words.iter() {
            terminated(&mut hasher, word);
        }
        let mut targets: Vec<_> = content
            .source
            .links
            .iter()
            .filter_map(|link| match &link.target {
                Target::Note {
                    path,
                    fragment,
                    style,
                } => Some((path, fragment, style_code(*style))),
                Target::External(_) | Target::SelfLink => None,
            })
            .collect();
        targets.sort_unstable();
        for (path, fragment, style) in targets {
            terminated(&mut hasher, path);
            if let Some(fragment) = fragment {
                hasher.update(&[1]);
                terminated(&mut hasher, fragment);
            } else {
                hasher.update(&[0]);
            }
            hasher.update(&[style]);
        }
    }
    hasher.finalize()
}

/// No word char is NUL, so the terminator keeps two splits of the same bytes apart.
fn terminated(hasher: &mut Hasher, word: &str) {
    hasher.update(word.as_bytes());
    hasher.update(&[0]);
}
