use std::ops::Range;

use inkra_text::ByteOffset;
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag};

use crate::gutters::{cut, gutters, quote_conceals};
use crate::{Conceal, Gutter, Styles};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Markdown {
    /// Sorted, non-overlapping, never empty, never across a `\n`.
    pub spans: Vec<Span>,
    /// Sorted by `marker.start`; markers never overlap, never empty, never across a `\n`.
    pub conceals: Vec<Conceal>,
    /// In document order, never empty; a setext heading's underline is not part of it.
    pub headings: Vec<Heading>,
    /// Sorted by `range.start`; one per line inside a leading quote, lazy lines included.
    pub gutters: Vec<Gutter>,
    /// Sorted by `range.start`; blocks never nest or overlap.
    pub code_blocks: Vec<CodeBlock>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub range: Range<ByteOffset>,
    pub styles: Styles,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    pub range: Range<ByteOffset>,
    pub level: HeadingLevel,
    /// An ATX heading's opening sequence, its `#` run and the spaces or tabs after it; `None` for setext.
    pub marker: Option<Range<ByteOffset>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeBlock {
    /// The whole block, fences included, up to its last line's end (no `\n`).
    pub range: Range<ByteOffset>,
    /// The info string's first word; `None` without one, and for an indented block.
    pub language: Option<Range<ByteOffset>>,
}

/// One pass over pulldown-cmark's events. A byte inside an element that no
/// leaf covers is a marker (live-preview spec, D3). `text` is LF-only, as
/// `Buffer` holds it: a `\r` would be styled as a marker.
#[must_use]
pub fn parse(text: &str) -> Markdown {
    let mut out = Out {
        text,
        spans: Vec::new(),
        conceals: Vec::new(),
        cursor: 0,
    };
    let mut open: Vec<Open> = Vec::new();
    let mut headings = Vec::new();
    let mut quotes = Vec::new();
    let mut code_blocks = Vec::new();

    for (event, range) in Parser::new_ext(text, options()).into_offset_iter() {
        let current = open.last().cloned();
        let styles = current.as_ref().map_or(Styles::NONE, |open| open.styles);
        match event {
            Event::Start(tag) => {
                out.markers_until(range.start, current.as_ref());
                if let Tag::Item = tag {
                    out.bullet(range.start);
                }
                if let Tag::Heading { level, .. } = tag {
                    let heading = heading_text(text, &range);
                    headings.push(Heading {
                        marker: out.heading(heading.clone()),
                        range: to_offsets(heading),
                        level,
                    });
                }
                if let Tag::CodeBlock(kind) = &tag {
                    code_blocks.push(CodeBlock {
                        range: to_offsets(without_line_ending(text, &range)),
                        language: language(text, &range, kind).map(to_offsets),
                    });
                }
                // A quote leads its line's prefix when every ancestor is a quote; a list item breaks it.
                let leading = matches!(tag, Tag::BlockQuote(_))
                    && current.as_ref().is_none_or(|open| open.leading);
                if leading {
                    quotes.push(without_line_ending(text, &range));
                }
                open.push(Open {
                    styles: styles | tag_styles(&tag),
                    indent: if matches!(tag, Tag::CodeBlock(_)) {
                        styles
                    } else {
                        styles | tag_styles(&tag)
                    },
                    end: range.end,
                    element: concealable(text, &tag, &range),
                    leading,
                });
            }
            Event::End(_) => {
                out.markers_until(range.end, current.as_ref());
                open.pop();
            }
            Event::Text(_) | Event::Html(_) | Event::InlineHtml(_) => {
                out.markers_until(range.start, current.as_ref());
                out.leaf(range, styles);
            }
            Event::Code(_) => {
                let (content_start, content_end) = backtick_runs(text, &range);
                let code = Open {
                    styles: styles | Styles::CODE,
                    indent: styles | Styles::CODE,
                    end: range.end,
                    element: Some(range.clone()),
                    leading: false,
                };
                out.markers_until(range.start, current.as_ref());
                out.markers_until(content_start, Some(&code));
                out.leaf(content_start..content_end, code.styles);
                out.markers_until(range.end, Some(&code));
            }
            // At top level no element encloses it, so the gap rule alone would leave it unstyled.
            Event::Rule => {
                out.markers_until(range.start, current.as_ref());
                out.leaf(range, styles | Styles::MARKER);
            }
            // Breaks and task boxes are left to the next gap, which marks them.
            _ => {}
        }
    }
    let gutters = gutters(text, &quotes);
    cut(text, &mut out.spans, &gutters);
    let mut conceals = out.conceals;
    conceals.extend(quote_conceals(&gutters));
    // Two sorted runs: the stable sort merges them in one linear pass.
    conceals.sort_by_key(|conceal| conceal.marker.start);
    Markdown {
        spans: out.spans,
        conceals,
        headings,
        gutters,
        code_blocks,
    }
}

pub(crate) fn options() -> Options {
    Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_WIKILINKS
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_TABLES
}

fn tag_styles(tag: &Tag<'_>) -> Styles {
    match tag {
        Tag::Heading { level, .. } => Styles::heading(*level),
        Tag::Strong => Styles::STRONG,
        Tag::Emphasis => Styles::EMPHASIS,
        Tag::Strikethrough => Styles::STRIKETHROUGH,
        Tag::CodeBlock(_) => Styles::CODE | Styles::CODE_BLOCK,
        Tag::Link { .. } | Tag::Image { .. } => Styles::LINK,
        Tag::BlockQuote(_) => Styles::QUOTE,
        Tag::MetadataBlock(_) => Styles::FRONTMATTER,
        _ => Styles::NONE,
    }
}

/// The range whose touch reveals this element's markers, or `None` when they never hide (spec D2).
/// A fenced block's fences hide too (code-blocks spec, D2).
fn concealable(text: &str, tag: &Tag<'_>, range: &Range<usize>) -> Option<Range<usize>> {
    matches!(
        tag,
        Tag::Strong
            | Tag::Emphasis
            | Tag::Strikethrough
            | Tag::Link { .. }
            | Tag::CodeBlock(CodeBlockKind::Fenced(_))
    )
    .then(|| without_line_ending(text, range))
}

/// The first word of a fenced block's info string, as source text: the opening line from the
/// fence, past its run and the blanks after it. It splits at ASCII bytes only, so both ends stay on
/// char boundaries.
fn language(text: &str, block: &Range<usize>, kind: &CodeBlockKind<'_>) -> Option<Range<usize>> {
    if matches!(kind, CodeBlockKind::Indented) {
        return None;
    }
    let source = &text[block.clone()];
    let line = source
        .split_once('\n')
        .map_or(source, |(first, _)| first)
        .as_bytes();
    let fence = line.iter().take_while(|&&byte| byte == line[0]).count();
    let blanks = line[fence..]
        .iter()
        .take_while(|&&byte| matches!(byte, b' ' | b'\t'))
        .count();
    let start = fence + blanks;
    let len = line[start..]
        .iter()
        .take_while(|byte| !byte.is_ascii_whitespace())
        .count();
    (len > 0).then(|| block.start + start..block.start + start + len)
}

/// A container prefix or indent leads a line, not an element's syntax: the length of the leading
/// `[ \t>]*` run when `piece` starts a line, else 0.
fn line_start_prefix(text: &str, piece: &Range<usize>) -> usize {
    if piece.start != 0 && text.as_bytes()[piece.start - 1] != b'\n' {
        return 0;
    }
    text.as_bytes()[piece.clone()]
        .iter()
        .take_while(|&&byte| matches!(byte, b' ' | b'\t' | b'>'))
        .count()
}

/// The heading's text lines: an ATX heading is one line, and a setext heading's last line is its underline.
fn heading_text(text: &str, range: &Range<usize>) -> Range<usize> {
    let element = &text[range.clone()];
    let element = element.strip_suffix('\n').unwrap_or(element);
    let len = element.rfind('\n').unwrap_or(element.len());
    range.start..range.start + len
}

/// A block's range ends after its last `\n`; a gutter must not reach the line after it.
fn without_line_ending(text: &str, range: &Range<usize>) -> Range<usize> {
    range.start..range.end - usize::from(text[..range.end].ends_with('\n'))
}

/// The length of an ATX opening sequence: 1 to 6 `#`, then spaces and tabs up to the text or the
/// line end. `None` for setext; pulldown-cmark starts the range after any indent.
fn opening_sequence(heading: &str) -> Option<usize> {
    let hashes = heading.bytes().take_while(|&byte| byte == b'#').count();
    let blanks = heading.as_bytes()[hashes..]
        .iter()
        .take_while(|&&byte| matches!(byte, b' ' | b'\t'))
        .count();
    let atx = (1..=6).contains(&hashes) && (blanks > 0 || hashes == heading.len());
    atx.then_some(hashes + blanks)
}

/// A `Code` event's range includes its backticks; returns where the content starts and ends.
pub(crate) fn backtick_runs(text: &str, range: &Range<usize>) -> (usize, usize) {
    let code = &text.as_bytes()[range.clone()];
    let open = code.iter().take_while(|&&byte| byte == b'`').count();
    let close = code[open..]
        .iter()
        .rev()
        .take_while(|&&byte| byte == b'`')
        .count();
    (range.start + open, range.end - close)
}

/// An open element: the union of its and its ancestors' tags, its end, and its range when its
/// markers can hide.
#[derive(Clone)]
struct Open {
    styles: Styles,
    /// What a line-start prefix in this element's gaps carries: the parent's styles in a code
    /// block, so its indents match the opening line's.
    indent: Styles,
    end: usize,
    element: Option<Range<usize>>,
    /// A quote whose ancestors are all quotes.
    leading: bool,
}

struct Out<'a> {
    text: &'a str,
    spans: Vec<Span>,
    conceals: Vec<Conceal>,
    /// Everything before this byte is already emitted or known to be unstyled.
    cursor: usize,
}

impl Out<'_> {
    /// `current` is the innermost open element; `None` means outside every element, where the
    /// gap is unstyled, not a marker.
    fn markers_until(&mut self, end: usize, current: Option<&Open>) {
        let end = current.map_or(end, |open| end.min(open.end));
        if end <= self.cursor {
            return;
        }
        if let Some(open) = current {
            for piece in lines(self.text, self.cursor..end) {
                let split = piece.start + line_start_prefix(self.text, &piece);
                self.push(piece.start..split, open.indent | Styles::MARKER);
                self.push(split..piece.end, open.styles | Styles::MARKER);
                if let Some(element) = &open.element
                    && split < piece.end
                {
                    self.conceals.push(Conceal {
                        element: to_offsets(element.clone()),
                        marker: to_offsets(split..piece.end),
                        replacement: "",
                    });
                }
            }
        }
        self.cursor = end;
    }

    /// Conceals an ATX heading's opening sequence, and only it; `heading` excludes its line ending.
    fn heading(&mut self, heading: Range<usize>) -> Option<Range<ByteOffset>> {
        let len = opening_sequence(&self.text[heading.clone()])?;
        let marker = to_offsets(heading.start..heading.start + len);
        self.conceals.push(Conceal {
            element: to_offsets(heading),
            marker: marker.clone(),
            replacement: "",
        });
        Some(marker)
    }

    /// The element is the bullet byte alone, so a caret at the item's content does not reveal it.
    fn bullet(&mut self, item_start: usize) {
        // An indented top-level item's range starts at its indentation.
        let indent = self.text.as_bytes()[item_start..]
            .iter()
            .take_while(|&&byte| matches!(byte, b' ' | b'\t'))
            .count();
        let start = item_start + indent;
        // An ordered item's marker starts with a digit (CommonMark 5.2), so it keeps its number.
        if !matches!(self.text.as_bytes().get(start), Some(b'-' | b'*' | b'+')) {
            return;
        }
        let bullet = to_offsets(start..start + 1);
        self.conceals.push(Conceal {
            element: bullet.clone(),
            marker: bullet,
            replacement: "•",
        });
    }

    fn leaf(&mut self, range: Range<usize>, styles: Styles) {
        let start = range.start.max(self.cursor).min(range.end);
        self.emit(start..range.end, styles);
        self.cursor = self.cursor.max(range.end);
    }

    /// Splits at `\n`: a newline byte never carries a style, so no span crosses a line.
    fn emit(&mut self, range: Range<usize>, styles: Styles) {
        for line in lines(self.text, range) {
            self.push(line, styles);
        }
    }

    fn push(&mut self, range: Range<usize>, styles: Styles) {
        push(&mut self.spans, range, styles);
    }
}

/// Appends a span, merged into the last one when they touch and share their styles.
pub(crate) fn push(spans: &mut Vec<Span>, range: Range<usize>, styles: Styles) {
    if range.is_empty() || styles.is_empty() {
        return;
    }
    let range = to_offsets(range);
    if let Some(last) = spans.last_mut()
        && last.range.end == range.start
        && last.styles == styles
    {
        last.range.end = range.end;
        return;
    }
    spans.push(Span { range, styles });
}

/// The pieces of `range` between its `\n` bytes, which they exclude.
fn lines(text: &str, range: Range<usize>) -> impl Iterator<Item = Range<usize>> {
    let mut start = range.start;
    text.as_bytes()[range]
        .split(|&byte| byte == b'\n')
        .map(move |line| {
            let piece = start..start + line.len();
            start = piece.end + 1;
            piece
        })
}

fn to_offsets(range: Range<usize>) -> Range<ByteOffset> {
    ByteOffset::new(range.start)..ByteOffset::new(range.end)
}

#[cfg(test)]
mod tests;
