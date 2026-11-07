use std::path::Path;

use inkra_vault::{EntryId, FileIdentity, Stamp, VaultError};
use proptest::prelude::*;

use super::{ScrubJob, decode, encode, scrub_sample};
use crate::test_support::{ROOT, id, mtime, note_set, vault};
use crate::{FileRead, NoteContent, NoteIndex};

/// The stamp every note below is held and read at, whatever its text.
fn stamp() -> Stamp {
    Stamp {
        mtime: mtime(1),
        size: 5,
    }
}

fn at(text: &str, stamp: Stamp) -> NoteContent {
    NoteContent::parse(
        text,
        FileRead {
            stamp,
            identity: FileIdentity {
                device: 1,
                inode: 1,
            },
        },
    )
}

/// A job over `a.md`, held as `held`.
fn job(held: &str) -> ScrubJob {
    let (_fs, vault) = vault(&["a.md"]);
    let a = id(&vault, "a.md");
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    index.insert(a, at(held, stamp()));
    ScrubJob::new(vec![a], &vault, index.snapshot())
}

#[test]
fn golden_sample() {
    assert_eq!(scrub_sample(0xDEAD_BEEF, 1000, 4), [64, 193, 644, 677]);
}

#[test]
fn a_short_list_is_sampled_whole() {
    assert_eq!(scrub_sample(7, 3, 64), [0, 1, 2]);
    assert_eq!(scrub_sample(7, 0, 64), Vec::<usize>::new());
}

proptest! {
    #[test]
    fn a_sample_is_distinct_sorted_in_range_and_deterministic(
        crc in any::<u32>(),
        len in 0_usize..300,
        n in 0_usize..80,
    ) {
        let sample = scrub_sample(crc, len, n);
        prop_assert_eq!(sample.len(), n.min(len));
        prop_assert!(sample.windows(2).all(|pair| pair[0] < pair[1]));
        prop_assert!(sample.iter().all(|&at| at < len));
        prop_assert_eq!(sample, scrub_sample(crc, len, n));
    }
}

#[test]
fn the_sample_is_seeded_by_the_body_crc_the_header_holds() {
    let bytes = encode(&NoteIndex::default().snapshot(), Path::new(ROOT)).expect("under the cap");
    let Ok(decoded) = decode(&bytes, Path::new(ROOT)) else {
        panic!("an empty index decodes");
    };

    assert_eq!(decoded.crc, crc32fast::hash(&bytes[24..]));
}

#[test]
fn a_job_walks_each_pick_s_path() {
    let job = job("alpha");

    assert_eq!(
        job.paths().collect::<Vec<_>>(),
        [Path::new(ROOT).join("a.md")]
    );
}

#[test]
fn other_words_at_an_equal_stamp_mismatch() {
    assert!(job("alpha").mismatch(vec![Ok(at("bravo", stamp()))]));
}

#[test]
fn other_links_with_the_same_words_mismatch() {
    assert!(job("[[a]]").mismatch(vec![Ok(at("[a]", stamp()))]));
}

#[test]
fn a_file_at_another_stamp_is_skipped() {
    let later = Stamp {
        mtime: mtime(9),
        ..stamp()
    };

    assert!(!job("alpha").mismatch(vec![Ok(at("bravo", later))]));
}

#[test]
fn a_failed_read_is_skipped() {
    let failed = VaultError::NotUtf8 { path: ROOT.into() };

    assert!(!job("alpha").mismatch(vec![Err(failed)]));
}

#[test]
fn equal_content_matches() {
    assert!(!job("alpha [[b]]").mismatch(vec![Ok(at("alpha [[b]]", stamp()))]));
}

#[test]
#[should_panic(expected = "one read per pick")]
fn a_read_short_of_the_picks_panics() {
    let _ = job("alpha").mismatch(Vec::new());
}

#[test]
fn a_note_the_index_holds_no_read_for_is_skipped() {
    let (_fs, vault) = vault(&["a.md"]);
    let a = id(&vault, "a.md");
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    let job = ScrubJob::new(vec![a], &vault, index.snapshot());

    assert!(!job.mismatch(vec![Ok(at("bravo", stamp()))]));
}

#[test]
fn an_id_the_vault_does_not_list_is_dropped() {
    let (_fs, vault) = vault(&["a.md"]);
    let job = ScrubJob::new(
        vec![EntryId::DETACHED],
        &vault,
        NoteIndex::default().snapshot(),
    );

    assert_eq!(job.paths().count(), 0);
    assert!(!job.mismatch(Vec::new()));
}
