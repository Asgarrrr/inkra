use inkra_editor::{HeardLayer, TextChange};
use inkra_text::{Buffer, ByteOffset, EditMark, Splice};
use proptest::prelude::*;

/// The splices the buffer's log keeps.
const KEPT: usize = 256;

#[derive(Debug, Clone)]
enum Step {
    Land,
    Edit,
    /// More splices than the log keeps, in one transaction.
    Overflow,
    /// A publish computed at an edit mark picked among those seen so far.
    Publish(usize),
    CatchUp,
    Hear,
    Switch(bool),
}

fn step_strategy() -> impl Strategy<Value = Step> {
    prop_oneof![
        3 => Just(Step::Land),
        2 => Just(Step::Edit),
        3 => Just(Step::Overflow),
        3 => any::<usize>().prop_map(Step::Publish),
        1 => Just(Step::CatchUp),
        2 => Just(Step::Hear),
        1 => any::<bool>().prop_map(Step::Switch),
    ]
}

/// What a first hearing reads, from counts of splices instead of the log.
#[derive(Clone, Copy)]
enum Heard {
    Unheard { opened: Option<usize> },
    Lost,
    At(usize),
}

/// `HeardLayer`'s rules, with every edit mark as the number of splices before it.
struct Model {
    now: usize,
    layer_at: usize,
    heard: Heard,
}

impl Model {
    /// Brings the layer to `now`; past the log, it is cleared and `true` returned, so a heard
    /// listener hears it lost and an unheard one's open is no longer fresh.
    fn catch_up(&mut self, since: usize) -> bool {
        let lost = self.now - self.layer_at.min(since) > KEPT;
        if lost {
            self.heard = match self.heard {
                Heard::Unheard { .. } => Heard::Unheard { opened: None },
                Heard::Lost | Heard::At(_) => Heard::Lost,
            };
        }
        self.layer_at = self.now;
        lost
    }

    /// The change heard, with an edit's splices counted.
    fn hear(&mut self) -> Option<TextChange> {
        let change = match self.heard {
            Heard::Unheard { opened } => Some(TextChange::Opened {
                fresh: opened == Some(self.now),
            }),
            Heard::Lost => Some(TextChange::Lost),
            Heard::At(mark) if mark == self.now => None,
            Heard::At(mark) if self.now - mark > KEPT => Some(TextChange::Lost),
            Heard::At(mark) => Some(TextChange::Edited(vec![TYPED; self.now - mark])),
        };
        self.heard = Heard::At(self.now);
        change
    }
}

/// Every edit inserts one byte at the start.
const TYPED: Splice = Splice {
    at: ByteOffset::new(0),
    removed_len: 0,
    inserted_len: 1,
};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    #[test]
    fn a_hearing_follows_the_model(steps in prop::collection::vec(step_strategy(), 1..40)) {
        let mut buffer = Buffer::new("");
        let mut marks: Vec<(EditMark, usize)> = vec![(buffer.edit_mark(), 0)];
        let mut layer = HeardLayer::<u8>::new(buffer.edit_mark());
        let mut model = Model { now: 0, layer_at: 0, heard: Heard::Unheard { opened: None } };
        for step in steps {
            match step {
                Step::Land => {
                    let lost = model.catch_up(model.now);
                    if let (false, Heard::Unheard { opened }) = (lost, &mut model.heard) {
                        *opened = Some(model.now);
                    }
                    layer.land(&buffer, Vec::new());
                }
                Step::Edit | Step::Overflow => {
                    let splices = if matches!(step, Step::Edit) { 1 } else { KEPT + 1 };
                    buffer.transact(|buffer| {
                        for _ in 0..splices {
                            buffer.insert(ByteOffset::new(0), "x");
                        }
                    });
                    model.now += splices;
                    marks.push((buffer.edit_mark(), model.now));
                }
                Step::Publish(pick) => {
                    let (at, since) = marks[pick % marks.len()];
                    let lost = model.catch_up(since);
                    let published = layer.publish(&buffer, Vec::new(), Vec::new(), at);
                    prop_assert_eq!(published.is_err(), lost);
                }
                Step::CatchUp => {
                    let lost = model.catch_up(model.now);
                    prop_assert_eq!(layer.catch_up(&buffer).is_err(), lost);
                }
                Step::Hear => prop_assert_eq!(layer.hear(&buffer), model.hear()),
                Step::Switch(on) => {
                    model.heard = Heard::Lost;
                    layer.switch(on);
                }
            }
        }
    }
}
