use super::Segment;
use crate::words::ordinal::{Alive, Ordinal};
use crate::words::{NoteWords, WORD_MAX_CHARS};

type Word = (String, Vec<(u32, u32)>);

fn built(old: &Segment, alive: &[u32], fresh: &[(u32, &str)]) -> Segment {
    let alive: Alive = alive.iter().copied().map(Ordinal::new).collect();
    let words: Vec<(Ordinal, NoteWords)> = fresh
        .iter()
        .map(|&(n, text)| (Ordinal::new(n), NoteWords::parse(text)))
        .collect();
    let mut fresh: Vec<(Ordinal, &NoteWords)> = words.iter().map(|(n, w)| (*n, w)).collect();
    fresh.sort_unstable_by_key(|&(n, _)| n);
    old.merge(&alive, &fresh)
}

fn words(segment: &Segment) -> Vec<Word> {
    segment
        .words()
        .map(|(word, postings)| {
            let postings = postings.map(|(n, count)| (n.get(), count)).collect();
            (word.to_owned(), postings)
        })
        .collect()
}

fn prefixed(segment: &Segment, term: &str) -> Vec<String> {
    segment
        .with_prefix(term)
        .map(|(word, _)| word.to_owned())
        .collect()
}

fn word(word: &str, postings: &[(u32, u32)]) -> Word {
    (word.to_owned(), postings.to_vec())
}

#[test]
fn a_segment_built_from_nothing_is_empty() {
    for segment in [Segment::default(), built(&Segment::default(), &[], &[])] {
        assert_eq!(words(&segment), []);
        assert_eq!(prefixed(&segment, ""), Vec::<String>::new());
        assert_eq!(prefixed(&segment, "a"), Vec::<String>::new());
        assert_eq!(segment.notes(), 0);
        assert_eq!(segment.heap_bytes(), Segment::default().heap_bytes());
    }
}

#[test]
fn a_build_sorts_the_words_and_each_word_s_postings_by_ordinal() {
    let first = built(
        &Segment::default(),
        &[],
        &[(2, "plan été plan"), (0, "plan zèbre")],
    );
    assert_eq!(
        words(&first),
        [
            word("ete", &[(2, 1)]),
            word("plan", &[(0, 1), (2, 2)]),
            word("zebre", &[(0, 1)]),
        ]
    );
    assert_eq!(first.notes(), 2);
    let second = built(&first, &[0, 2], &[(1, "autre plan")]);
    assert_eq!(
        words(&second),
        [
            word("autre", &[(1, 1)]),
            word("ete", &[(2, 1)]),
            word("plan", &[(0, 1), (1, 1), (2, 2)]),
            word("zebre", &[(0, 1)]),
        ]
    );
    assert_eq!(second.notes(), 3);
}

#[test]
fn counts_and_far_ordinals_stay_exact() {
    let many = "plan ".repeat(300);
    let segment = built(
        &Segment::default(),
        &[],
        &[
            (0, "plan"),
            (127, &many),
            (128, "plan"),
            (70_000, "plan"),
            (u32::MAX, "plan"),
        ],
    );
    assert_eq!(
        words(&segment),
        [word(
            "plan",
            &[(0, 1), (127, 300), (128, 1), (70_000, 1), (u32::MAX, 1)]
        )]
    );
}

#[test]
fn dead_ordinals_leave_the_postings_and_an_empty_word_goes() {
    let segment = built(
        &Segment::default(),
        &[],
        &[(0, "plan seul"), (1, "plan"), (2, "autre")],
    );
    let purged = built(&segment, &[1], &[]);
    assert_eq!(words(&purged), [word("plan", &[(1, 1)])]);
    assert_eq!(purged.notes(), 1);
    assert_eq!(words(&built(&purged, &[], &[])), []);
}

#[test]
fn a_rewritten_note_keeps_only_its_fresh_words() {
    let segment = built(
        &Segment::default(),
        &[],
        &[(0, "old shared"), (1, "shared")],
    );
    let rewritten = built(&segment, &[1], &[(0, "new shared shared")]);
    assert_eq!(
        words(&rewritten),
        [word("new", &[(0, 1)]), word("shared", &[(0, 2), (1, 1)])]
    );
    assert_eq!(rewritten.notes(), 2);
}

#[test]
fn with_prefix_returns_exactly_the_run() {
    let segment = built(
        &Segment::default(),
        &[],
        &[(0, "apro pro projet projets prune")],
    );
    assert_eq!(prefixed(&segment, "pro"), ["pro", "projet", "projets"]);
    assert_eq!(prefixed(&segment, "proj"), ["projet", "projets"]);
    assert_eq!(prefixed(&segment, "a"), ["apro"]);
    assert_eq!(
        prefixed(&segment, ""),
        ["apro", "pro", "projet", "projets", "prune"]
    );
    assert_eq!(prefixed(&segment, "q"), Vec::<String>::new());
    assert_eq!(prefixed(&segment, "zz"), Vec::<String>::new());
    assert_eq!(prefixed(&segment, "b"), Vec::<String>::new());
    let postings: Vec<Vec<(Ordinal, u32)>> = segment
        .with_prefix("proj")
        .map(|(_, postings)| postings.collect())
        .collect();
    assert_eq!(postings, [[(Ordinal::new(0), 1)], [(Ordinal::new(0), 1)]]);
}

#[test]
fn long_and_multibyte_words_keep_their_bytes_exact() {
    let long = "z".repeat(WORD_MAX_CHARS);
    let text = format!("{long} été 한국 naïve");
    let segment = built(&Segment::default(), &[], &[(0, text.as_str()), (1, "한")]);
    let stored: Vec<String> = NoteWords::parse(&text)
        .iter()
        .map(|(word, _)| word.to_owned())
        .collect();
    let [hangul] = &NoteWords::parse("한")
        .iter()
        .map(|(word, _)| word.to_owned())
        .collect::<Vec<_>>()[..]
    else {
        panic!("`한` is one word");
    };
    let found: Vec<String> = words(&segment).into_iter().map(|(word, _)| word).collect();
    let mut expected = [stored, vec![hangul.clone()]].concat();
    expected.sort_unstable();
    assert_eq!(found, expected);
    assert!(found.contains(&long));
    assert_eq!(prefixed(&segment, hangul).len(), 2, "`한` starts `한국`");
}

#[test]
fn heap_bytes_counts_the_words_bounds_starts_and_postings_exactly() {
    let segment = built(
        &Segment::default(),
        &[],
        &[(0, "plan été plan"), (200, "plan zèbre")],
    );
    let words = "ete".len() + "plan".len() + "zebre".len();
    // Three words: four `u32` offsets in `bounds`, four in `starts`.
    let offsets = 4 * size_of::<u32>();
    // Gap then count per posting, one byte each but gap 200, which takes two.
    let ete = 1 + 1;
    let plan = (1 + 1) + (2 + 1);
    let zebre = 2 + 1;
    assert_eq!(
        segment.heap_bytes(),
        words + offsets + offsets + ete + plan + zebre
    );
    assert_eq!(Segment::default().heap_bytes(), 2 * size_of::<u32>());
}
