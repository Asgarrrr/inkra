//! A concealed item part's slots: its blank slots, one per mark, then the box, and what each mark
//! paints where (task boxes spec, D3, D11).

use std::collections::HashMap;
use std::rc::Rc;

use gpui::{Pixels, ShapedLine};
use inkra_markdown::{ItemMarker, ItemPart, WidthKey};

use crate::theme::{LIST_SLOT_GAP, TASK_BOX_SIZE};

const DIGITS: &str = "0123456789";

/// The standard slot: the box and its gap.
fn standard_slot() -> Pixels {
    TASK_BOX_SIZE + LIST_SLOT_GAP
}

pub(in crate::editor::layout) struct Mark {
    /// From the line's start.
    pub(in crate::editor::layout) x: Pixels,
    pub(in crate::editor::layout) paint: Paint,
}

pub(in crate::editor::layout) enum Paint {
    /// A `•` or a number, at the body size.
    Text(Rc<ShapedLine>),
    Box {
        checked: bool,
    },
}

/// What marker lines share within one `shape_document`: each mark text's shape and each width
/// key's slot, measured once.
#[derive(Default)]
pub(super) struct Marks<'a> {
    shapes: HashMap<&'a str, Rc<ShapedLine>>,
    numbers: HashMap<WidthKey, Pixels>,
    /// The digit with the widest advance, from one shape of `0123456789`.
    widest_digit: Option<char>,
}

impl<'a> Marks<'a> {
    /// The marks of `item`, whose text is `part` and whose width keys are `keys`, from `x`, and
    /// the text start past its slots. `shape` shapes a mark's text.
    pub(super) fn layout(
        &mut self,
        item: &ItemPart,
        keys: &[WidthKey],
        part: &'a str,
        x: Pixels,
        mut shape: impl FnMut(&str) -> ShapedLine,
    ) -> (Vec<Mark>, Pixels) {
        let marked: Vec<ItemMarker> = item.marks(part).collect();
        let numbered = marked
            .iter()
            .filter(|mark| matches!(mark, ItemMarker::Number(_)))
            .count();
        // The blank slots' keys come first, then one per number mark, in order (D1).
        let (blank, mut marked_keys) = keys.split_at(keys.len() - numbered);
        let blank_slots: Pixels =
            std::iter::repeat_n(standard_slot(), item.indent - blank.len()).sum();
        let blank_numbers: Pixels = blank
            .iter()
            .map(|&key| self.slot(key, &mut |text| advances(&shape(text))))
            .sum();
        let mut left = x + blank_slots + blank_numbers;
        let mut marks = Vec::new();
        for mark in marked {
            let (text, slot) = match &mark {
                ItemMarker::Bullet => ("•", standard_slot()),
                ItemMarker::Number(range) => {
                    let text = &part[range.start - item.start..range.end - item.start];
                    let [key, rest @ ..] = marked_keys else {
                        unreachable!("the parse gives each number mark its key (D1)");
                    };
                    marked_keys = rest;
                    (text, self.slot(*key, &mut |text| advances(&shape(text))))
                }
            };
            let shaped = Rc::clone(
                self.shapes
                    .entry(text)
                    .or_insert_with(|| Rc::new(shape(text))),
            );
            // A `•` centres on the box's axis; a number ends a gap left of its slot's end.
            let at = match mark {
                ItemMarker::Bullet => left + (TASK_BOX_SIZE - shaped.width()) / 2.,
                ItemMarker::Number(_) => left + slot - LIST_SLOT_GAP - shaped.width(),
            };
            marks.push(Mark {
                x: at,
                paint: Paint::Text(shaped),
            });
            left += slot;
        }
        if let Some(task) = &item.task {
            marks.push(Mark {
                x: left,
                paint: Paint::Box {
                    checked: task.checked,
                },
            });
            left += standard_slot();
        }
        (marks, left)
    }

    /// A number slot of `key`: its widest number and the gap, never narrower than a standard slot.
    /// `advances` gives each glyph's advance in a text.
    fn slot(&mut self, key: WidthKey, advances: &mut impl FnMut(&str) -> Vec<Pixels>) -> Pixels {
        if let Some(&slot) = self.numbers.get(&key) {
            return slot;
        }
        let digit = *self
            .widest_digit
            .get_or_insert_with(|| widest_digit(&advances(DIGITS)));
        let mut widest: String = std::iter::repeat_n(digit, usize::from(key.digits)).collect();
        widest.push(char::from(key.delimiter));
        let width: Pixels = advances(&widest).into_iter().sum();
        let slot = (width + LIST_SLOT_GAP).max(standard_slot());
        self.numbers.insert(key, slot);
        slot
    }
}

/// Each glyph's advance in `shaped`, for an ASCII text.
fn advances(shaped: &ShapedLine) -> Vec<Pixels> {
    let x = |index| shaped.x_for_index(index);
    (0..shaped.len())
        .map(|index| x(index + 1) - x(index))
        .collect()
}

/// The digit of `DIGITS` with the widest of `advances`, one per digit.
fn widest_digit(advances: &[Pixels]) -> char {
    let widest = DIGITS
        .chars()
        .zip(advances)
        .max_by_key(|&(_, &advance)| advance);
    widest.map_or('0', |(digit, _)| digit)
}

#[cfg(test)]
mod tests;
