//! The syntax layer's open function (code highlighting spec, D8).

use std::time::Instant;

use inkra_highlight::open_chunks;

use super::styled;
use crate::decorations::Chunk;

pub(super) fn open(text: &str, deadline: Instant) -> Vec<Chunk> {
    let chunks = open_chunks(text, || Instant::now() < deadline);
    chunks.into_iter().map(styled).collect()
}
