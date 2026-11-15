use std::ops::Range;

use inkra_editor::{Moved, RevisionLog};
use inkra_text::{Buffer, ByteOffset, EditMark};
use proptest::prelude::*;
use proptest::sample::Index;

#[derive(Debug, Clone)]
enum Step {
    /// Replaces the bytes between two points with `inserted`.
    Edit(Index, Index, String),
    /// More splices than the buffer's log keeps, leaving the text as it was.
    Overflow,
    Shift,
    /// A recolour between two points of the text at a mark picked among those seen so far.
    Range(Index, Index, Index),
    Whole,
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        4 => (any::<Index>(), any::<Index>(), "[ab\n]{0,4}").prop_map(|(a, b, text)| Step::Edit(a, b, text)),
        1 => Just(Step::Overflow),
        3 => Just(Step::Shift),
        4 => (any::<Index>(), any::<Index>(), any::<Index>()).prop_map(|(mark, a, b)| Step::Range(mark, a, b)),
        1 => Just(Step::Whole),
    ]
}

/// The text's bytes by identity: a byte keeps its id until an edit removes it.
struct Ids {
    ids: Vec<usize>,
    next: usize,
}

impl Ids {
    fn replace(&mut self, range: Range<usize>, inserted: usize) {
        let fresh = self.next..self.next + inserted;
        self.next = fresh.end;
        self.ids.splice(range, fresh);
    }
}

/// One bump, as the log should answer it from any revision before it.
enum Bumped {
    Shift,
    /// The mark the recolour was computed at, and the ids of the bytes it covered.
    Range(EditMark, Vec<usize>),
    Whole,
}

fn sorted(text_len: usize, a: Index, b: Index) -> Range<usize> {
    let [a, b] = [a, b].map(|at| at.index(text_len + 1));
    a.min(b)..a.max(b)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    /// From any revision it reaches with no `Whole` since, the log answers ranges holding every
    /// byte recoloured since that is still in the text.
    #[test]
    fn every_recoloured_byte_lies_in_an_answered_range(
        text in "[ab\n]{0,12}",
        steps in prop::collection::vec(step(), 1..24),
        since in any::<Index>(),
    ) {
        let mut buffer = Buffer::new(text.as_str());
        let mut ids = Ids { ids: (0..text.len()).collect(), next: 1000 };
        // Every mark seen, with the ids of its text's bytes.
        let mut marks = vec![(buffer.edit_mark(), ids.ids.clone())];
        let mut log = RevisionLog::new();
        let mut bumps: Vec<(u64, Bumped)> = Vec::new();
        for step in steps {
            let revision = log.revision() + 1;
            match step {
                Step::Edit(a, b, inserted) => {
                    let range = sorted(buffer.text().len(), a, b);
                    let start = ByteOffset::new(range.start);
                    buffer.transact(|txn| txn.replace(start..ByteOffset::new(range.end), &inserted));
                    ids.replace(range, inserted.len());
                    marks.push((buffer.edit_mark(), ids.ids.clone()));
                }
                Step::Overflow => {
                    buffer.transact(|txn| {
                        for _ in 0..129 {
                            txn.insert(ByteOffset::new(0), "a");
                            txn.delete(ByteOffset::new(0)..ByteOffset::new(1));
                        }
                    });
                    marks.push((buffer.edit_mark(), ids.ids.clone()));
                }
                Step::Shift => {
                    log.bump(revision, Moved::Shift);
                    bumps.push((revision, Bumped::Shift));
                }
                Step::Range(mark, a, b) => {
                    let (mark, then) = &marks[mark.index(marks.len())];
                    let range = sorted(then.len(), a, b);
                    let moved = Moved::Range(*mark, ByteOffset::new(range.start)..ByteOffset::new(range.end));
                    log.bump(revision, moved);
                    bumps.push((revision, Bumped::Range(*mark, then[range].to_vec())));
                }
                Step::Whole => {
                    log.bump(revision, Moved::Whole);
                    bumps.push((revision, Bumped::Whole));
                }
            }
        }
        let first = since.index(bumps.len() + 1);
        let revision = first.checked_sub(1).map_or(0, |at| bumps[at].0);
        let after = &bumps[first..];
        let answer = log.changed_since(revision, &buffer);
        let whole = after.iter().any(|(_, bumped)| matches!(bumped, Bumped::Whole));
        let followed = after.iter().all(|(_, bumped)| match bumped {
            Bumped::Range(mark, _) => buffer.edits_since(*mark).is_some(),
            Bumped::Shift | Bumped::Whole => true,
        });
        if whole || !followed {
            prop_assert_eq!(answer, None);
        } else {
            // 24 steps stay far inside the bumps the log keeps.
            let ranges = answer.expect("a reachable revision answers");
            for (_, bumped) in after {
                let Bumped::Range(_, recoloured) = bumped else { continue };
                for id in recoloured {
                    let Some(now) = ids.ids.iter().position(|kept| kept == id) else { continue };
                    let now = ByteOffset::new(now);
                    prop_assert!(
                        ranges.iter().any(|range| range.start <= now && now < range.end),
                        "byte {} at {:?} outside {:?}", id, now, ranges
                    );
                }
            }
        }
    }

    /// A revision the log never held answers `None`, however far it lies.
    #[test]
    fn a_revision_never_bumped_answers_none(bumps in 1..20u64, gap in 1..5u64, missing in 0..200u64) {
        let buffer = Buffer::new("ab");
        let mut log = RevisionLog::new();
        for bump in 1..=bumps {
            log.bump(bump * gap * 2, Moved::Shift);
        }
        let held = missing == 0 || (missing % (gap * 2) == 0 && missing <= bumps * gap * 2);
        prop_assert_eq!(log.changed_since(missing, &buffer).is_some(), held);
    }
}
