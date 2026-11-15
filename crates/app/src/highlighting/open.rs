//! The syntax layer's open function (code highlighting spec, D8).

use std::time::Instant;

use inkra_highlight::open_chunks;
use inkra_markdown::Markdown;

use super::colour;
use crate::decorations::Opened;

pub(super) fn open(text: &str, deadline: Instant) -> Opened {
    let chunks = open_chunks(
        text.len(),
        || {
            let Markdown {
                spans, code_blocks, ..
            } = inkra_markdown::parse(text);
            code_blocks.into_iter().map(move |block| {
                let marks = colour(text, &spans, &block);
                (block.range, block.shape, marks)
            })
        },
        || Instant::now() < deadline,
    );
    Opened {
        chunks,
        seed: Some(Box::new(())),
    }
}
