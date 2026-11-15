//! The syntax layer's open function (code highlighting spec, D8).

use std::time::Instant;

use super::colour;
use crate::decorations::Opened;

/// Above this many bytes, a parse would take more than half the landing deadline (D8).
pub(super) const OPEN_PARSE_LIMIT: usize = 500_000;

pub(super) fn open(text: &str, deadline: Instant) -> Opened {
    let chunks = if text.len() > OPEN_PARSE_LIMIT {
        Vec::new()
    } else {
        let markdown = inkra_markdown::parse(text);
        markdown
            .code_blocks
            .iter()
            .take_while(|_| Instant::now() < deadline)
            .map(|block| (block.range.clone(), colour(text, &markdown.spans, block)))
            .collect()
    };
    Opened {
        chunks,
        seed: Some(Box::new(())),
    }
}
