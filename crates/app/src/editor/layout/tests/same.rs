//! What the oracle compares: every line's placement, rows, display, prefix, marks and label, and
//! the document's rows, bars and code blocks (incremental layout spec, D10).

use proptest::prelude::*;
use proptest::test_runner::TestCaseError;

use super::super::line::{LineLayout, Placed};
use super::super::{DocumentLayout, prefix};

fn assert_line(carried: &Placed, fresh: &Placed) -> Result<(), TestCaseError> {
    let at = carried.start;
    prop_assert_eq!(carried.start, fresh.start);
    prop_assert_eq!(carried.top, fresh.top, "top at {}", at);
    prop_assert_eq!(carried.first_row, fresh.first_row, "first row at {}", at);
    prop_assert_eq!(carried.block, fresh.block, "block at {}", at);
    let (carried, fresh) = (&*carried.line, &*fresh.line);
    prop_assert_eq!(carried.len, fresh.len, "len at {}", at);
    prop_assert_eq!(carried.row_height, fresh.row_height, "row height at {}", at);
    let rows = |line: &LineLayout| {
        let rows = line.rows.iter();
        rows.map(|row| (row.start, row.caret_end, row.start_x))
            .collect::<Vec<_>>()
    };
    prop_assert_eq!(rows(carried), rows(fresh), "rows at {}", at);
    prop_assert_eq!(
        format!("{:?}", carried.display),
        format!("{:?}", fresh.display),
        "display at {}",
        at
    );
    let prefix = |line: &LineLayout| {
        line.prefix.as_ref().map(|prefix| {
            let shown: Vec<_> = prefix
                .shown()
                .map(|(shaped, x)| (shaped.text.to_string(), x))
                .collect();
            (
                prefix.range(),
                prefix.indent(),
                prefix.bars().to_vec(),
                shown,
            )
        })
    };
    let marks = |line: &LineLayout| {
        let marks = line.prefix.iter().flat_map(prefix::Prefix::marks);
        marks
            .map(|mark| match &mark.paint {
                prefix::Paint::Text(shaped, align) => {
                    (mark.x, Some((shaped.text.to_string(), *align)), None)
                }
                prefix::Paint::Box { checked } => (mark.x, None, Some(*checked)),
            })
            .collect::<Vec<_>>()
    };
    prop_assert_eq!(marks(carried), marks(fresh), "marks at {}", at);
    prop_assert_eq!(prefix(carried), prefix(fresh), "prefix at {}", at);
    let label = |line: &LineLayout| line.label.as_ref().map(|label| label.text.to_string());
    prop_assert_eq!(label(carried), label(fresh), "label at {}", at);
    prop_assert!(carried.shape.same_args(&fresh.shape), "shape at {}", at);
    Ok(())
}

pub(super) fn assert_same(
    carried: &DocumentLayout,
    fresh: &DocumentLayout,
) -> Result<(), TestCaseError> {
    prop_assert_eq!(carried.lines.len(), fresh.lines.len());
    for (carried, fresh) in carried.lines.iter().zip(&fresh.lines) {
        assert_line(carried, fresh)?;
    }
    prop_assert_eq!(carried.row_count, fresh.row_count);
    prop_assert_eq!(carried.height, fresh.height);
    prop_assert_eq!(&carried.bars, &fresh.bars);
    let blocks = |layout: &DocumentLayout| {
        let blocks = layout.code_blocks.iter().map(|block| {
            let label = block.label.as_ref();
            let label = label.map(|(at, shaped)| (*at, shaped.text.to_string()));
            (block.start, block.max, block.background, label)
        });
        blocks.collect::<Vec<_>>()
    };
    prop_assert_eq!(blocks(carried), blocks(fresh));
    Ok(())
}
