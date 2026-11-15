//! The syntax layer's open function (code highlighting spec, D8).

use std::time::Instant;

use inkra_highlight::open_chunks;

use super::styled;
use crate::decorations::Opened;

pub(super) fn open(text: &str, deadline: Instant) -> Opened {
    let chunks = open_chunks(text, || Instant::now() < deadline);
    Opened {
        chunks: chunks.into_iter().map(styled).collect(),
        seed: Some(Box::new(())),
    }
}
