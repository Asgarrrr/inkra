use std::iter;
use std::sync::atomic::{AtomicBool, Ordering};

use inkra_markdown::{front_matter, parse};

use super::Excerpt;
use super::excerpt::{CANCEL_EVERY, EXCERPT_SCAN_BYTES};
use super::plain::{LINE_BYTES, Plain};
use crate::words::{WORD_MAX_CHARS, cap, fold, word_spans};

const PREVIEW_BYTES: usize = 160;

/// First prose line past front matter, blanks and a heading naming the note (S4 amendment).
pub(super) fn preview<'s>(
    text: &str,
    stem: impl Iterator<Item = &'s str>,
    cancel: &AtomicBool,
) -> Option<Excerpt> {
    let scanned = &text[..text.floor_char_boundary(EXCERPT_SCAN_BYTES)];
    // Taken by the first line with prose: only a leading heading can name the note.
    let mut stem = Some(stem);
    let mut from = front_matter_end(scanned, cancel)?;
    loop {
        let start = find(scanned, from, cancel, |c| !c.is_whitespace())?;
        if start == scanned.len() {
            return None;
        }
        let line = line_at(scanned, start);
        let mut plain = Plain::new(line, true, 0..line.len()).text;
        let blank = plain.trim().is_empty();
        let skipped = if blank {
            1
        } else if stem.take().is_some_and(|stem| names(&plain, stem)) {
            heading_lines(scanned, start, cancel)?
        } else {
            0
        };
        if skipped == 0 {
            plain.truncate(plain.floor_char_boundary(PREVIEW_BYTES));
            return Some(Excerpt {
                text: plain,
                highlights: Vec::new(),
            });
        }
        from = start;
        for _ in 0..skipped {
            from = find(scanned, from, cancel, |c| c == '\n')? + 1;
        }
    }
}

/// The line after a leading front matter block's closing fence, else 0; `None` when cancelled.
pub(super) fn front_matter_end(scanned: &str, cancel: &AtomicBool) -> Option<usize> {
    // Only `---` opens one, so a long first line is not read.
    if !scanned.starts_with("---") {
        return Some(0);
    }
    // The start of the line after the last one read; `None` past the text's last line.
    let mut next = Some(0);
    let lines = iter::from_fn(|| {
        let start = next?;
        let end = find(scanned, start, cancel, |c| c == '\n')?;
        next = (end < scanned.len()).then_some(end + 1);
        Some(&scanned[start..end])
    });
    let found = front_matter(lines);
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    Some(found.map_or(0, |_| next.unwrap_or(scanned.len())))
}

/// 1 for an ATX heading on the line at `start`, 2 for a setext one, 0 for none; `None` when cancelled.
fn heading_lines(scanned: &str, start: usize, cancel: &AtomicBool) -> Option<usize> {
    let line = line_at(scanned, start);
    let next = find(scanned, start, cancel, |c| c == '\n')? + 1;
    let lines = format!("{line}\n{}", line_at(scanned, next));
    Some(match parse(&lines).headings.first() {
        Some(heading) if !lines[..heading.range.start.0].contains('\n') => {
            if heading.marker.is_some() { 1 } else { 2 }
        }
        _ => 0,
    })
}

fn line_at(scanned: &str, at: usize) -> &str {
    let at = at.min(scanned.len());
    let head = &scanned[at..scanned.floor_char_boundary(at + LINE_BYTES)];
    head.find('\n').map_or(head, |end| &head[..end])
}

/// The first char at or past `from` where `is` holds, else the end; `None` when cancelled.
fn find(
    scanned: &str,
    mut from: usize,
    cancel: &AtomicBool,
    is: impl Fn(char) -> bool,
) -> Option<usize> {
    while from < scanned.len() {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        let to = scanned.ceil_char_boundary(from + CANCEL_EVERY);
        if let Some(at) = scanned[from..to].find(&is) {
            return Some(from + at);
        }
        from = to;
    }
    Some(scanned.len())
}

/// Whether `plain`'s words, folded and capped as stem words are, are `stem`: S3's exact name.
fn names<'s>(plain: &str, mut stem: impl Iterator<Item = &'s str>) -> bool {
    for (_, word) in word_spans(plain) {
        let folded = fold(word);
        let word = cap(&folded, WORD_MAX_CHARS);
        if !word.is_empty() && stem.next() != Some(word) {
            return false;
        }
    }
    stem.next().is_none()
}
