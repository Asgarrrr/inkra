# Markdown slice 1 — styled source — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The editor styles markdown — headings, strong, emphasis, strikethrough, code, links, quotes, frontmatter — with every syntax marker visible and dimmed. Nothing is concealed yet.

**Architecture:** A new gpui-free crate, `inkra-markdown`, turns `&str` into sorted, flat `Span`s tagged with a `Styles` bitset (spec D1–D3). `inkra-app` maps `Styles` to a `TextStyle` in `theme.rs` (D9). `EditorView` keeps the parse, tagged with `Buffer::version()` (D10), and `shape_document` merges the spans with the IME range into `TextRun`s per line (D9). `layout.rs` is split into `editor/layout/` first, as a pure move (D11).

**Tech Stack:** Rust 2024, gpui (vendored), pulldown-cmark 0.13.4, proptest.

**Spec:** `docs/superpowers/specs/2026-09-23-markdown-live-preview-design.md`. Read D1, D2, D3, D9, D10, D11 before starting. This plan is slice 1 of 6.

**Branch:** `markdown-live-preview` (exists; the spec is uncommitted on it — Task 0 commits it).

**Rules that bite here** (from `CLAUDE.md` and `.claude/rules/`):
- Never `cargo fmt --all`. Use `cargo fmt -p inkra-text -p inkra-editor -p inkra-app -p inkra-vault -p inkra-markdown` (after Task 2 adds the crate; before that, without `-p inkra-markdown`).
- Clippy runs `pedantic` with `-D warnings`, plus `unwrap_used`/`expect_used` denied outside tests. Public functions returning a value need `#[must_use]`.
- Comments: one line, only for a why the code cannot show.
- Tests live in their own file: `foo.rs` declares `#[cfg(test)] mod tests;`, and the tests go in `foo/tests.rs`.
- `./scripts/check.sh` green is the definition of done.

---

## File structure

| File | Status | Responsibility |
|---|---|---|
| `crates/app/src/editor/layout.rs` | delete (moved) | — |
| `crates/app/src/editor/layout/mod.rs` | create (moved) | `DocumentLayout`, rows geometry, offset↔pixel conversions |
| `crates/app/src/editor/layout/shape.rs` | create (moved, then modified) | `shape_document` |
| `crates/app/src/editor/layout/runs.rs` | create (moved, then rewritten) | `line_runs`, `RunStyles` |
| `crates/app/src/editor/layout/runs/tests.rs` | create | plain `#[test]`s for `line_runs` |
| `crates/app/src/editor/layout/rows.rs` | create (moved) | `rows`, grapheme snapping |
| `crates/markdown/Cargo.toml` | create | crate manifest |
| `crates/markdown/src/lib.rs` | create | re-exports only |
| `crates/markdown/src/styles.rs` | create | `Styles` bitset |
| `crates/markdown/src/styles/tests.rs` | create | `Styles` tests |
| `crates/markdown/src/parse.rs` | create | `parse`, `Markdown`, `Span` |
| `crates/markdown/src/parse/tests.rs` | create | per-construct tests + proptest |
| `crates/app/src/theme.rs` | modify | markdown colours, `CODE_FONT_FAMILY`, `markdown_style` |
| `crates/app/src/theme/tests.rs` | create | `markdown_style` tests |
| `crates/app/src/editor/mod.rs` | modify | `markdown` field, `refresh_markdown`, `spans` |
| `crates/app/src/editor/element.rs` | modify | pass spans to `shape_document` |
| `crates/app/src/field.rs`, `crates/app/src/field/tests.rs` | modify | pass `&[]` |
| `crates/app/src/editor/tests/markdown.rs` | create | `#[gpui::test]`s for the parse refresh |
| `crates/app/src/shot/scenes.rs` | modify | `editor-markdown` scene |
| `Cargo.toml`, `crates/app/Cargo.toml`, `scripts/check.sh`, `.cargo/mutants.toml`, `CLAUDE.md` | modify | wire the crate in |

---

### Task 0: Commit the spec

- [ ] **Step 1: Commit**

```bash
git add docs/superpowers/specs/2026-09-23-markdown-live-preview-design.md docs/superpowers/plans/2026-09-23-markdown-slice-1-styled-source.md
git commit -m "Spec markdown live preview; plan slice 1"
```

---

### Task 1: Split `layout.rs` into `editor/layout/` (pure move)

No behaviour change. The existing suite is the test; `test-names.sh` proves no test was dropped.

**Files:**
- Delete: `crates/app/src/editor/layout.rs`
- Create: `crates/app/src/editor/layout/{mod.rs,shape.rs,runs.rs,rows.rs}`

- [ ] **Step 1: Record the test names before the move**

Run: `./scripts/test-names.sh > /tmp/names-before.txt && wc -l /tmp/names-before.txt`
Expected: a count greater than 0.

- [ ] **Step 2: Move the file and split it**

```bash
mkdir -p crates/app/src/editor/layout
git mv crates/app/src/editor/layout.rs crates/app/src/editor/layout/mod.rs
```

Then, in `crates/app/src/editor/layout/mod.rs`, cut these items verbatim (with their doc comments) into new files:

- `fn line_runs` (the function whose doc starts "Runs indexed from zero within the line") → `layout/runs.rs`, made `pub(super)`.
- `pub(crate) fn shape_document` (doc starts "Takes `style` explicitly") → `layout/shape.rs`, unchanged visibility.
- `fn rows`, `fn grapheme_at_or_after`, `fn grapheme_before` → `layout/rows.rs`; `rows` made `pub(super)`.

Add to the top of `layout/mod.rs`, after the module doc comment:

```rust
mod rows;
mod runs;
mod shape;

pub(crate) use shape::shape_document;
```

Each new file gets only the imports it uses. `shape.rs` needs `use super::{DocumentLayout, LineLayout, rows::rows, runs::line_runs};` — child modules can read the parent's private fields, so `LineLayout` and `DocumentLayout` keep private fields. `rows.rs` needs `use super::Row;`.

- [ ] **Step 3: Build and let clippy find stale imports**

Run: `cargo clippy -p inkra-app --all-targets --all-features -- -D warnings`
Expected: no warnings. Fix any unused or missing import it reports; change nothing else.

- [ ] **Step 4: Check no test was dropped**

Run: `./scripts/test-names.sh > /tmp/names-after.txt && diff /tmp/names-before.txt /tmp/names-after.txt && echo SAME`
Expected: `SAME`.

- [ ] **Step 5: Run the suite and fmt**

Run: `cargo fmt -p inkra-app && cargo test -p inkra-app`
Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add -A crates/app/src/editor/layout crates/app/src/editor/layout.rs
git commit -m "Split editor layout into rows, runs and shape modules"
```

---

### Task 2: Create `inkra-markdown` with `Styles`, wired into the workspace

**Files:**
- Create: `crates/markdown/Cargo.toml`, `crates/markdown/src/lib.rs`, `crates/markdown/src/styles.rs`, `crates/markdown/src/styles/tests.rs`
- Modify: `Cargo.toml`, `scripts/check.sh`, `.cargo/mutants.toml`, `CLAUDE.md`

- [ ] **Step 1: Write the manifest**

`crates/markdown/Cargo.toml`:

```toml
[package]
name = "inkra-markdown"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
publish.workspace = true

[dependencies]
inkra-text = { workspace = true }
# Pure Rust with runtime wikilinks; tree-sitter-md measured 20x slower (live-preview spec, X1).
pulldown-cmark = { version = "0.13.4", default-features = false }

[dev-dependencies]
proptest = { workspace = true }

[lints]
workspace = true
```

- [ ] **Step 2: Wire the workspace**

In `Cargo.toml`, change `members` and add the path dependency:

```toml
members = ["crates/text", "crates/editor", "crates/app", "crates/vault", "crates/markdown"]
```

```toml
inkra-markdown = { path = "crates/markdown" }
```

(the second line goes under `[workspace.dependencies]`, after `inkra-vault`).

In `scripts/check.sh`, line 6 becomes:

```bash
cargo fmt -p inkra-text -p inkra-editor -p inkra-app -p inkra-vault -p inkra-markdown -- --check
```

and the isolation loop header becomes:

```bash
for crate in inkra-text inkra-editor inkra-vault inkra-markdown; do
```

In `.cargo/mutants.toml`:

```toml
examine_globs = ["crates/text/src/**/*.rs", "crates/editor/src/**/*.rs", "crates/markdown/src/**/*.rs"]
```

```toml
test_package = ["inkra-text", "inkra-editor", "inkra-markdown"]
```

and change the comment above `examine_globs` from "these globs mutate the same two crates" to "these globs mutate the same three crates".

In `CLAUDE.md`:
- The fmt command line: add ` -p inkra-markdown` after `-p inkra-vault`.
- Architecture, first bullet, append: "`inkra-markdown` depends on `inkra-text` only; `app` depends on it."
- Architecture, second bullet: "`text`, `editor`, `vault` and `markdown` have no gpui dependency".
- "Where code goes", add a bullet after the `text`/`editor` one: "- parsing markdown into styled ranges → `markdown`; never a colour or a pixel".
- The mutants bullet: "after changing `text`, `editor` or `markdown` logic".
- Docs list, add first: "- `docs/superpowers/specs/2026-09-23-markdown-live-preview-design.md` — markdown styling and concealment".

- [ ] **Step 3: Write the failing `Styles` tests**

`crates/markdown/src/styles/tests.rs`:

```rust
use super::Styles;

#[test]
fn a_union_contains_each_part() {
    let styles = Styles::STRONG | Styles::MARKER;
    assert!(styles.contains(Styles::STRONG));
    assert!(styles.contains(Styles::MARKER));
    assert!(!styles.contains(Styles::EMPHASIS));
}

#[test]
fn heading_levels_round_trip() {
    for level in 1..=6 {
        assert_eq!(Styles::heading(level).heading_level(), Some(level));
    }
    assert_eq!(Styles::STRONG.heading_level(), None);
    assert_eq!((Styles::heading(2) | Styles::MARKER).heading_level(), Some(2));
}

#[test]
fn a_heading_level_does_not_read_as_another_tag() {
    for level in 1..=6 {
        let heading = Styles::heading(level);
        for tag in [
            Styles::STRONG,
            Styles::EMPHASIS,
            Styles::STRIKETHROUGH,
            Styles::CODE,
            Styles::LINK,
            Styles::MARKER,
            Styles::QUOTE,
            Styles::FRONTMATTER,
        ] {
            assert!(!heading.contains(tag));
        }
    }
}

#[test]
#[should_panic(expected = "heading level")]
fn heading_level_zero_panics() {
    let _ = Styles::heading(0);
}

#[test]
#[should_panic(expected = "heading level")]
fn heading_level_seven_panics() {
    let _ = Styles::heading(7);
}

#[test]
fn only_none_is_empty() {
    assert!(Styles::NONE.is_empty());
    assert!(!Styles::MARKER.is_empty());
    assert!(!Styles::heading(1).is_empty());
}

#[test]
fn or_assign_matches_or() {
    let mut styles = Styles::CODE;
    styles |= Styles::QUOTE;
    assert_eq!(styles, Styles::CODE | Styles::QUOTE);
}
```

`crates/markdown/src/lib.rs`:

```rust
mod styles;

pub use styles::Styles;
```

`crates/markdown/src/styles.rs` (stub so the tests compile and fail):

```rust
use std::ops::{BitOr, BitOrAssign};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Styles(u16);

impl Styles {
    pub const NONE: Self = Self(0);
    pub const STRONG: Self = Self(0);
    pub const EMPHASIS: Self = Self(0);
    pub const STRIKETHROUGH: Self = Self(0);
    pub const CODE: Self = Self(0);
    pub const LINK: Self = Self(0);
    pub const MARKER: Self = Self(0);
    pub const QUOTE: Self = Self(0);
    pub const FRONTMATTER: Self = Self(0);

    #[must_use]
    pub fn heading(_level: u8) -> Self {
        Self(0)
    }

    #[must_use]
    pub fn heading_level(self) -> Option<u8> {
        None
    }

    #[must_use]
    pub fn contains(self, _other: Self) -> bool {
        false
    }

    #[must_use]
    pub fn is_empty(self) -> bool {
        false
    }
}

impl BitOr for Styles {
    type Output = Self;

    fn bitor(self, _rhs: Self) -> Self {
        self
    }
}

impl BitOrAssign for Styles {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = *self | rhs;
    }
}

#[cfg(test)]
mod tests;
```

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p inkra-markdown`
Expected: FAIL — `a_union_contains_each_part`, `heading_levels_round_trip`, `heading_level_zero_panics`, `heading_level_seven_panics`, `only_none_is_empty` among the failures.

- [ ] **Step 5: Implement `Styles`**

Replace `crates/markdown/src/styles.rs` with:

```rust
use std::ops::{BitOr, BitOrAssign};

/// Semantic tags only; colour, font and size belong to `app`'s theme.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Styles(u16);

/// The low three bits hold a heading level, 1 to 6; zero means no heading.
const LEVEL: u16 = 0b111;

impl Styles {
    pub const NONE: Self = Self(0);
    pub const STRONG: Self = Self(1 << 3);
    pub const EMPHASIS: Self = Self(1 << 4);
    pub const STRIKETHROUGH: Self = Self(1 << 5);
    pub const CODE: Self = Self(1 << 6);
    pub const LINK: Self = Self(1 << 7);
    pub const MARKER: Self = Self(1 << 8);
    pub const QUOTE: Self = Self(1 << 9);
    pub const FRONTMATTER: Self = Self(1 << 10);

    /// # Panics
    ///
    /// When `level` is not 1 to 6: a caller bug, since pulldown-cmark has six levels.
    #[must_use]
    pub fn heading(level: u8) -> Self {
        assert!((1..=6).contains(&level), "heading level {level} out of 1..=6");
        Self(u16::from(level))
    }

    #[must_use]
    pub fn heading_level(self) -> Option<u8> {
        match self.0 & LEVEL {
            0 => None,
            level => u8::try_from(level).ok(),
        }
    }

    #[must_use]
    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    #[must_use]
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl BitOr for Styles {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for Styles {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = *self | rhs;
    }
}

#[cfg(test)]
mod tests;
```

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p inkra-markdown`
Expected: 7 passed.

- [ ] **Step 7: Check the crate stays gpui-free and clean**

Run: `cargo fmt -p inkra-markdown && cargo clippy -p inkra-markdown --all-targets -- -D warnings && cargo tree -p inkra-markdown -e normal,build,dev --target=all | grep -c "gpui v" || true`
Expected: clippy clean; the `grep -c` prints `0`.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml Cargo.lock crates/markdown scripts/check.sh .cargo/mutants.toml CLAUDE.md
git commit -m "Add inkra-markdown with the Styles tag set"
```

---

### Task 3: `parse` — inline constructs and the marker rule

**Files:**
- Create: `crates/markdown/src/parse.rs`, `crates/markdown/src/parse/tests.rs`
- Modify: `crates/markdown/src/lib.rs`

The expected values below were produced by a prototype of this exact algorithm against pulldown-cmark 0.13.4; they are not guesses.

- [ ] **Step 1: Write the failing tests**

`crates/markdown/src/parse/tests.rs`:

```rust
use super::parse;
use crate::Styles;

const S: Styles = Styles::STRONG;
const E: Styles = Styles::EMPHASIS;
const X: Styles = Styles::STRIKETHROUGH;
const C: Styles = Styles::CODE;
const M: Styles = Styles::MARKER;

/// Each span as the text it covers, so expectations read like the source.
fn styled(text: &str) -> Vec<(&str, Styles)> {
    parse(text)
        .spans
        .iter()
        .map(|span| (&text[span.range.start.0..span.range.end.0], span.styles))
        .collect()
}

#[test]
fn plain_text_has_no_spans() {
    assert_eq!(styled("plain text"), vec![]);
}

#[test]
fn strong_marks_its_delimiters() {
    assert_eq!(styled("**a**"), vec![("**", S | M), ("a", S), ("**", S | M)]);
    assert_eq!(styled("__a__"), vec![("__", S | M), ("a", S), ("__", S | M)]);
}

#[test]
fn emphasis_marks_its_delimiters() {
    assert_eq!(styled("*a*"), vec![("*", E | M), ("a", E), ("*", E | M)]);
    assert_eq!(styled("_a_"), vec![("_", E | M), ("a", E), ("_", E | M)]);
}

#[test]
fn strikethrough_marks_its_delimiters() {
    assert_eq!(styled("~~a~~"), vec![("~~", X | M), ("a", X), ("~~", X | M)]);
}

#[test]
fn inline_code_marks_its_backtick_runs() {
    assert_eq!(styled("`a`"), vec![("`", C | M), ("a", C), ("`", C | M)]);
    assert_eq!(
        styled("``a ` b``"),
        vec![("``", C | M), ("a ` b", C), ("``", C | M)]
    );
}

#[test]
fn nesting_flattens_into_unions() {
    assert_eq!(
        styled("**b *i***"),
        vec![
            ("**", S | M),
            ("b ", S),
            ("*", S | E | M),
            ("i", S | E),
            ("*", S | E | M),
            ("**", S | M),
        ]
    );
    assert_eq!(
        styled("***x***"),
        vec![
            ("*", E | M),
            ("**", S | E | M),
            ("x", S | E),
            ("**", S | E | M),
            ("*", E | M),
        ]
    );
}

#[test]
fn offsets_are_bytes_across_multibyte_text() {
    assert_eq!(
        styled("héllo **wörld**"),
        vec![("**", S | M), ("wörld", S), ("**", S | M)]
    );
}

#[test]
fn an_escape_backslash_is_a_marker() {
    assert_eq!(styled("a\\*b"), vec![("\\", M)]);
}
```

`crates/markdown/src/parse.rs` (stub):

```rust
use std::ops::Range;

use inkra_text::ByteOffset;

use crate::Styles;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Markdown {
    /// Sorted, non-overlapping, never empty, never across a `\n`.
    pub spans: Vec<Span>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub range: Range<ByteOffset>,
    pub styles: Styles,
}

#[must_use]
pub fn parse(_text: &str) -> Markdown {
    Markdown::default()
}

#[cfg(test)]
mod tests;
```

`crates/markdown/src/lib.rs`:

```rust
mod parse;
mod styles;

pub use parse::{Markdown, Span, parse};
pub use styles::Styles;
```

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p inkra-markdown parse`
Expected: FAIL — every test except `plain_text_has_no_spans`.

- [ ] **Step 3: Implement `parse`**

Replace `crates/markdown/src/parse.rs` with:

```rust
use std::ops::Range;

use inkra_text::ByteOffset;
use pulldown_cmark::{Event, Options, Parser, Tag};

use crate::Styles;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Markdown {
    /// Sorted, non-overlapping, never empty, never across a `\n`.
    pub spans: Vec<Span>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub range: Range<ByteOffset>,
    pub styles: Styles,
}

/// One pass over pulldown-cmark's events. A byte inside an element that no
/// leaf covers is a marker (live-preview spec, D3).
#[must_use]
pub fn parse(text: &str) -> Markdown {
    let mut out = Out {
        text,
        spans: Vec::new(),
        cursor: 0,
    };
    // Each entry is the union of every open element's tags, innermost last.
    let mut open: Vec<Styles> = Vec::new();

    for (event, range) in Parser::new_ext(text, options()).into_offset_iter() {
        let current = open.last().copied();
        match event {
            Event::Start(tag) => {
                out.markers_until(range.start, current);
                open.push(current.unwrap_or_default() | tag_styles(&tag));
            }
            Event::End(_) => {
                out.markers_until(range.end, current);
                open.pop();
            }
            Event::Text(_) | Event::Html(_) | Event::InlineHtml(_) => {
                out.markers_until(range.start, current);
                out.leaf(range, current.unwrap_or_default());
            }
            Event::Code(_) => {
                let code = current.unwrap_or_default() | Styles::CODE;
                let (content_start, content_end) = backtick_runs(text, &range);
                out.markers_until(range.start, current);
                out.markers_until(content_start, Some(code));
                out.leaf(content_start..content_end, code);
                out.markers_until(range.end, Some(code));
            }
            // Breaks and task boxes are left to the next gap, which marks them.
            _ => {}
        }
    }
    Markdown { spans: out.spans }
}

fn options() -> Options {
    Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_WIKILINKS
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_TABLES
}

fn tag_styles(tag: &Tag<'_>) -> Styles {
    match tag {
        Tag::Heading { level, .. } => Styles::heading(*level as u8),
        Tag::Strong => Styles::STRONG,
        Tag::Emphasis => Styles::EMPHASIS,
        Tag::Strikethrough => Styles::STRIKETHROUGH,
        Tag::CodeBlock(_) => Styles::CODE,
        Tag::Link { .. } | Tag::Image { .. } => Styles::LINK,
        Tag::BlockQuote(_) => Styles::QUOTE,
        Tag::MetadataBlock(_) => Styles::FRONTMATTER,
        _ => Styles::NONE,
    }
}

/// A `Code` event's range includes its backticks; returns where the content starts and ends.
fn backtick_runs(text: &str, range: &Range<usize>) -> (usize, usize) {
    let code = &text.as_bytes()[range.clone()];
    let open = code.iter().take_while(|&&byte| byte == b'`').count();
    let close = code[open..]
        .iter()
        .rev()
        .take_while(|&&byte| byte == b'`')
        .count();
    (range.start + open, range.end - close)
}

struct Out<'a> {
    text: &'a str,
    spans: Vec<Span>,
    /// Everything before this byte is already emitted or known to be unstyled.
    cursor: usize,
}

impl Out<'_> {
    /// `None` means outside every element: the gap is unstyled, not a marker.
    fn markers_until(&mut self, end: usize, current: Option<Styles>) {
        if end <= self.cursor {
            return;
        }
        if let Some(styles) = current {
            self.emit(self.cursor..end, styles | Styles::MARKER);
        }
        self.cursor = end;
    }

    fn leaf(&mut self, range: Range<usize>, styles: Styles) {
        let end = range.end;
        self.emit(range, styles);
        self.cursor = self.cursor.max(end);
    }

    /// Splits at `\n`: a newline byte never carries a style, so no span crosses a line.
    fn emit(&mut self, range: Range<usize>, styles: Styles) {
        let mut start = range.start;
        for (index, byte) in self.text.as_bytes()[range.clone()].iter().enumerate() {
            if *byte == b'\n' {
                self.push(start..range.start + index, styles);
                start = range.start + index + 1;
            }
        }
        self.push(start..range.end, styles);
    }

    fn push(&mut self, range: Range<usize>, styles: Styles) {
        if range.is_empty() || styles.is_empty() {
            return;
        }
        let range = ByteOffset(range.start)..ByteOffset(range.end);
        if let Some(last) = self.spans.last_mut()
            && last.range.end == range.start
            && last.styles == styles
        {
            last.range.end = range.end;
            return;
        }
        self.spans.push(Span { range, styles });
    }
}

#[cfg(test)]
mod tests;
```

If clippy flags `*level as u8` (`cast_possible_truncation` does not apply to fieldless enums, but check), replace it with a `match` over `HeadingLevel::H1..H6` returning `1..=6`.

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p inkra-markdown parse`
Expected: 8 passed.

- [ ] **Step 5: Clippy and fmt**

Run: `cargo fmt -p inkra-markdown && cargo clippy -p inkra-markdown --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add crates/markdown
git commit -m "Parse inline markdown into flat styled spans"
```

---

### Task 4: `parse` — block constructs, links, and thematic breaks

**Files:**
- Modify: `crates/markdown/src/parse.rs`, `crates/markdown/src/parse/tests.rs`

- [ ] **Step 1: Write the failing tests**

Append to `crates/markdown/src/parse/tests.rs`:

```rust
const L: Styles = Styles::LINK;
const Q: Styles = Styles::QUOTE;
const F: Styles = Styles::FRONTMATTER;

fn h(level: u8) -> Styles {
    Styles::heading(level)
}

#[test]
fn atx_headings_mark_their_hashes() {
    assert_eq!(styled("# T"), vec![("# ", h(1) | M), ("T", h(1))]);
    assert_eq!(
        styled("# T ##"),
        vec![("# ", h(1) | M), ("T", h(1)), (" ##", h(1) | M)]
    );
}

#[test]
fn a_heading_stops_at_its_line_ending() {
    assert_eq!(styled("## T\nx"), vec![("## ", h(2) | M), ("T", h(2))]);
}

#[test]
fn a_setext_heading_marks_its_underline() {
    assert_eq!(styled("T\n==="), vec![("T", h(1)), ("===", h(1) | M)]);
}

#[test]
fn a_heading_carries_its_inline_styles() {
    assert_eq!(
        styled("# **b**"),
        vec![
            ("# ", h(1) | M),
            ("**", h(1) | S | M),
            ("b", h(1) | S),
            ("**", h(1) | S | M),
        ]
    );
}

#[test]
fn links_mark_everything_but_their_text() {
    assert_eq!(
        styled("[t](http://u)"),
        vec![("[", L | M), ("t", L), ("](http://u)", L | M)]
    );
    assert_eq!(
        styled("[t][r]\n\n[r]: http://x"),
        vec![("[", L | M), ("t", L), ("][r]", L | M)]
    );
    assert_eq!(
        styled("<http://a>"),
        vec![("<", L | M), ("http://a", L), (">", L | M)]
    );
    assert_eq!(
        styled("![alt](u)"),
        vec![("![", L | M), ("alt", L), ("](u)", L | M)]
    );
}

#[test]
fn wikilinks_show_the_alias_or_the_target() {
    assert_eq!(
        styled("[[Page]]"),
        vec![("[[", L | M), ("Page", L), ("]]", L | M)]
    );
    assert_eq!(
        styled("[[Page|alias]]"),
        vec![("[[Page|", L | M), ("alias", L), ("]]", L | M)]
    );
}

#[test]
fn every_quote_prefix_is_a_marker() {
    assert_eq!(
        styled("> a\n> b"),
        vec![("> ", Q | M), ("a", Q), ("> ", Q | M), ("b", Q)]
    );
    assert_eq!(
        styled("> - a\n> - b"),
        vec![("> - ", Q | M), ("a", Q), ("> - ", Q | M), ("b", Q)]
    );
}

#[test]
fn a_fence_inside_a_quote_keeps_both_styles() {
    assert_eq!(
        styled("> ```\n> code\n> ```"),
        vec![
            ("> ", Q | M),
            ("```", C | Q | M),
            ("> ", C | Q | M),
            ("code", C | Q),
            ("> ```", C | Q | M),
        ]
    );
}

#[test]
fn list_markers_are_dimmed() {
    assert_eq!(styled("- a\n- b"), vec![("- ", M), ("- ", M)]);
    assert_eq!(styled("1. a\n2) b"), vec![("1. ", M), ("2) ", M)]);
    assert_eq!(
        styled("- [ ] t\n- [x] u"),
        vec![("- [ ] ", M), ("- [x] ", M)]
    );
}

#[test]
fn code_blocks_mark_their_fences() {
    assert_eq!(
        styled("```rs\nlet x;\n```"),
        vec![("```rs", C | M), ("let x;", C), ("```", C | M)]
    );
    assert_eq!(styled("    code"), vec![("code", C)]);
}

#[test]
fn frontmatter_marks_its_fences() {
    assert_eq!(
        styled("---\ntitle: x\n---\nbody"),
        vec![("---", F | M), ("title: x", F), ("---", F | M)]
    );
}

#[test]
fn a_thematic_break_is_a_marker() {
    assert_eq!(styled("a\n\n---\n\nb"), vec![("---", M)]);
}

#[test]
fn table_pipes_are_markers() {
    assert_eq!(
        styled("| a | b |\n|---|---|\n| 1 | 2 |"),
        vec![
            ("| ", M),
            (" | ", M),
            (" |", M),
            ("|---|---|", M),
            ("| ", M),
            (" | ", M),
            (" |", M),
        ]
    );
}
```

- [ ] **Step 2: Run the tests to see which fail**

Run: `cargo test -p inkra-markdown parse`
Expected: FAIL on `a_thematic_break_is_a_marker` only (`Rule` sits outside every element, so the gap rule leaves it unstyled). Every other new test passes already: they exercise the same rule. If any other fails, stop and compare against the spec's D2/D3 before changing code.

- [ ] **Step 3: Handle `Rule`**

In `parse`, add an arm before the `_ => {}` arm:

```rust
            // Outside every element, so the gap rule alone would leave it unstyled.
            Event::Rule => {
                out.markers_until(range.start, current);
                out.leaf(range, Styles::MARKER);
            }
```

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p inkra-markdown`
Expected: all passed (7 styles + 21 parse).

- [ ] **Step 5: Commit**

```bash
git add crates/markdown
git commit -m "Style headings, links, quotes, lists, code blocks and frontmatter"
```

---

### Task 5: `parse` invariants under proptest

**Files:**
- Modify: `crates/markdown/src/parse/tests.rs`

- [ ] **Step 1: Write the property tests**

Append to `crates/markdown/src/parse/tests.rs`:

```rust
use proptest::prelude::*;

fn check_invariants(text: &str) {
    let spans = parse(text).spans;
    for span in &spans {
        let (start, end) = (span.range.start.0, span.range.end.0);
        assert!(start < end, "empty span {span:?} in {text:?}");
        assert!(end <= text.len(), "span {span:?} past the end of {text:?}");
        assert!(text.is_char_boundary(start) && text.is_char_boundary(end));
        assert!(!text[start..end].contains('\n'), "span {span:?} crosses a line");
        assert!(!span.styles.is_empty());
    }
    for pair in spans.windows(2) {
        assert!(pair[0].range.end <= pair[1].range.start, "unsorted or overlapping: {pair:?}");
        if pair[0].range.end == pair[1].range.start {
            assert_ne!(pair[0].styles, pair[1].styles, "unmerged neighbours: {pair:?}");
        }
    }
}

proptest! {
    #[test]
    fn spans_hold_their_invariants_on_any_text(text in any::<String>()) {
        check_invariants(&text);
    }

    #[test]
    fn spans_hold_their_invariants_on_syntax_heavy_text(
        text in "[a-zé *_`\\[\\]()#>!|~:=+.0-9\n-]{0,120}"
    ) {
        check_invariants(&text);
    }
}
```

- [ ] **Step 2: Run them**

Run: `cargo test -p inkra-markdown`
Expected: all pass. If proptest finds a failure, it writes `crates/markdown/proptest-regressions/parse/tests.txt`; fix `parse` (not the test), keep the regressions file, and re-run.

- [ ] **Step 3: Commit (including any regressions file)**

```bash
git add crates/markdown
git commit -m "Hold parse's span invariants under proptest"
```

---

### Task 6: `theme::markdown_style`

**Files:**
- Modify: `crates/app/Cargo.toml`, `crates/app/src/theme.rs`
- Create: `crates/app/src/theme/tests.rs`

- [ ] **Step 1: Add the dependency**

In `crates/app/Cargo.toml`, under `[dependencies]` after `inkra-vault`:

```toml
inkra-markdown = { workspace = true }
```

- [ ] **Step 2: Write the failing tests**

`crates/app/src/theme/tests.rs`:

```rust
use gpui::{FontStyle, FontWeight, Hsla, TextStyle};
use inkra_markdown::Styles;

use super::{CODE_FONT_FAMILY, code_text, link_text, markdown_style, marker_text, quote_text};

#[test]
fn no_tag_keeps_the_base_style() {
    let base = TextStyle::default();
    assert_eq!(markdown_style(&base, Styles::NONE), base);
}

#[test]
fn strong_is_bold_and_emphasis_is_italic() {
    let base = TextStyle::default();
    assert_eq!(markdown_style(&base, Styles::STRONG).font_weight, FontWeight::BOLD);
    assert_eq!(markdown_style(&base, Styles::EMPHASIS).font_style, FontStyle::Italic);
}

#[test]
fn strikethrough_is_struck() {
    let style = markdown_style(&TextStyle::default(), Styles::STRIKETHROUGH);
    assert!(style.strikethrough.is_some());
}

#[test]
fn a_heading_is_bold_at_the_body_size() {
    let base = TextStyle::default();
    let style = markdown_style(&base, Styles::heading(1));
    assert_eq!(style.font_weight, FontWeight::BOLD);
    assert_eq!(style.font_size, base.font_size);
}

#[test]
fn code_is_monospace() {
    let style = markdown_style(&TextStyle::default(), Styles::CODE);
    assert_eq!(style.font_family.as_ref(), CODE_FONT_FAMILY);
    assert_eq!(style.color, Hsla::from(code_text()));
}

#[test]
fn links_and_quotes_take_their_colours() {
    let base = TextStyle::default();
    assert_eq!(markdown_style(&base, Styles::LINK).color, Hsla::from(link_text()));
    assert_eq!(markdown_style(&base, Styles::QUOTE).color, Hsla::from(quote_text()));
}

#[test]
fn a_marker_is_dimmed_but_keeps_its_weight() {
    let style = markdown_style(&TextStyle::default(), Styles::STRONG | Styles::MARKER);
    assert_eq!(style.color, Hsla::from(marker_text()));
    assert_eq!(style.font_weight, FontWeight::BOLD);
}

#[test]
fn a_marker_inside_a_link_is_dimmed_not_linked() {
    let style = markdown_style(&TextStyle::default(), Styles::LINK | Styles::MARKER);
    assert_eq!(style.color, Hsla::from(marker_text()));
}

#[test]
fn frontmatter_is_dimmed() {
    let style = markdown_style(&TextStyle::default(), Styles::FRONTMATTER);
    assert_eq!(style.color, Hsla::from(marker_text()));
}
```

At the end of `crates/app/src/theme.rs` add:

```rust
#[cfg(test)]
mod tests;
```

- [ ] **Step 3: Run to see them fail**

Run: `cargo test -p inkra-app theme`
Expected: compile error — `markdown_style`, `CODE_FONT_FAMILY`, `code_text`, `link_text`, `marker_text`, `quote_text` not found.

- [ ] **Step 4: Implement**

In `crates/app/src/theme.rs`, change the `use gpui::…` line to:

```rust
use gpui::{
    FontStyle, FontWeight, Pixels, Rgba, StrikethroughStyle, TextStyle, px, rgb, rgba,
};
use inkra_markdown::Styles;
```

After `BODY_COLUMN_CHARS`, add:

```rust
/// Ships with macOS, the only platform today; a second platform makes this per-platform.
pub(crate) const CODE_FONT_FAMILY: &str = "Menlo";
```

Before `#[cfg(test)]`, add:

```rust
pub(crate) fn heading_text() -> Rgba {
    rgb(0xffffff)
}

/// Not reused from `placeholder_text`: a marker sits inside body text, not on its own.
pub(crate) fn marker_text() -> Rgba {
    rgb(0x6a6a6a)
}

pub(crate) fn code_text() -> Rgba {
    rgb(0xe5c07b)
}

pub(crate) fn link_text() -> Rgba {
    rgb(0x4ea1ff)
}

pub(crate) fn quote_text() -> Rgba {
    rgb(0xa8a8a8)
}

/// `Marker` goes last so a marker stays dimmed inside a link or a heading; it
/// keeps weight and style so its width matches the text around it.
pub(crate) fn markdown_style(base: &TextStyle, styles: Styles) -> TextStyle {
    let mut style = base.clone();
    if styles.heading_level().is_some() {
        style.font_weight = FontWeight::BOLD;
        style.color = heading_text().into();
    }
    if styles.contains(Styles::STRONG) {
        style.font_weight = FontWeight::BOLD;
    }
    if styles.contains(Styles::EMPHASIS) {
        style.font_style = FontStyle::Italic;
    }
    if styles.contains(Styles::STRIKETHROUGH) {
        style.strikethrough = Some(StrikethroughStyle {
            thickness: px(1.),
            color: None,
        });
    }
    if styles.contains(Styles::CODE) {
        // `HighlightStyle` has no family field, so this cannot go through `TextStyle::highlight`.
        style.font_family = CODE_FONT_FAMILY.into();
        style.color = code_text().into();
    }
    if styles.contains(Styles::LINK) {
        style.color = link_text().into();
    }
    if styles.contains(Styles::QUOTE) {
        style.color = quote_text().into();
    }
    if styles.contains(Styles::FRONTMATTER) || styles.contains(Styles::MARKER) {
        style.color = marker_text().into();
    }
    style
}
```

- [ ] **Step 5: Run to see them pass**

Run: `cargo test -p inkra-app theme`
Expected: 9 passed. (`TextStyle` implements `PartialEq`; if `no_tag_keeps_the_base_style` fails to compile for lack of it, compare `font_weight`, `font_style`, `color` and `font_family` instead.)

- [ ] **Step 6: Commit**

```bash
git add crates/app/Cargo.toml Cargo.lock crates/app/src/theme.rs crates/app/src/theme
git commit -m "Map markdown styles to text styles in the theme"
```

---

### Task 7: `line_runs` merges spans and the IME range

**Files:**
- Modify: `crates/app/src/editor/layout/runs.rs`
- Create: `crates/app/src/editor/layout/runs/tests.rs`

- [ ] **Step 1: Write the failing tests**

`crates/app/src/editor/layout/runs/tests.rs`:

```rust
use std::ops::Range;

use gpui::{FontWeight, Hsla, TextStyle};
use inkra_markdown::parse;
use inkra_text::ByteOffset;

use super::{RunStyles, line_runs};
use crate::theme;

fn range(start: usize, end: usize) -> Range<ByteOffset> {
    ByteOffset(start)..ByteOffset(end)
}

fn lens(runs: &[gpui::TextRun]) -> Vec<usize> {
    runs.iter().map(|run| run.len).collect()
}

#[test]
fn runs_follow_the_spans_and_cover_the_line() {
    let text = "a **b** c";
    let spans = parse(text).spans;
    let base = TextStyle::default();
    let mut styles = RunStyles::new(&base);

    let runs = line_runs(&range(0, text.len()), &spans, None, &mut styles);

    assert_eq!(lens(&runs), vec![2, 2, 1, 2, 2]);
    assert_eq!(runs[0].font.weight, base.font_weight);
    assert_eq!(runs[1].color, Hsla::from(theme::marker_text()));
    assert_eq!(runs[2].font.weight, FontWeight::BOLD);
    assert!(runs.iter().all(|run| run.underline.is_none()));
}

#[test]
fn a_composition_inside_bold_stays_bold_and_gains_the_underline() {
    let text = "a **b** c";
    let spans = parse(text).spans;
    let base = TextStyle::default();
    let mut styles = RunStyles::new(&base);

    let runs = line_runs(&range(0, text.len()), &spans, Some(&range(3, 6)), &mut styles);

    assert_eq!(lens(&runs), vec![2, 1, 1, 1, 1, 1, 2]);
    let underlined: Vec<bool> = runs.iter().map(|run| run.underline.is_some()).collect();
    assert_eq!(underlined, vec![false, false, true, true, true, false, false]);
    assert_eq!(runs[3].font.weight, FontWeight::BOLD);
}

#[test]
fn runs_are_indexed_from_the_line_start() {
    let text = "ab\n**c**";
    let spans = parse(text).spans;
    let base = TextStyle::default();
    let mut styles = RunStyles::new(&base);

    let runs = line_runs(&range(3, 8), &spans[..], None, &mut styles);

    assert_eq!(lens(&runs), vec![2, 1, 2]);
}

#[test]
fn a_plain_line_is_one_run_and_an_empty_line_one_empty_run() {
    let base = TextStyle::default();
    let mut styles = RunStyles::new(&base);

    assert_eq!(lens(&line_runs(&range(0, 5), &[], None, &mut styles)), vec![5]);
    assert_eq!(lens(&line_runs(&range(4, 4), &[], None, &mut styles)), vec![0]);
}

#[test]
fn a_stale_span_past_the_line_is_clipped_not_fatal() {
    let base = TextStyle::default();
    let mut styles = RunStyles::new(&base);
    let spans = parse("**long bold text**").spans;

    let runs = line_runs(&range(0, 4), &spans, None, &mut styles);

    assert_eq!(runs.iter().map(|run| run.len).sum::<usize>(), 4);
}
```

At the end of `crates/app/src/editor/layout/runs.rs` add:

```rust
#[cfg(test)]
mod tests;
```

- [ ] **Step 2: Run to see them fail**

Run: `cargo test -p inkra-app runs`
Expected: compile error — `RunStyles` not found, `line_runs` has the wrong signature.

- [ ] **Step 3: Rewrite `runs.rs`**

Replace the body of `crates/app/src/editor/layout/runs.rs` (keep the `#[cfg(test)] mod tests;` at the end) with:

```rust
use std::collections::HashMap;
use std::ops::Range;

use gpui::{HighlightStyle, TextRun, TextStyle, UnderlineStyle, px};
use inkra_markdown::{Span, Styles};
use inkra_text::ByteOffset;

use crate::theme;

/// Resolves each `(Styles, composing)` once per `shape_document` call: that
/// runs every frame, and a `TextStyle` clone per run would be per-frame waste.
pub(super) struct RunStyles<'a> {
    base: &'a TextStyle,
    resolved: HashMap<(Styles, bool), TextStyle>,
}

impl<'a> RunStyles<'a> {
    pub(super) fn new(base: &'a TextStyle) -> Self {
        Self {
            base,
            resolved: HashMap::new(),
        }
    }

    fn run(&mut self, styles: Styles, composing: bool, len: usize) -> TextRun {
        let base = self.base;
        self.resolved
            .entry((styles, composing))
            .or_insert_with(|| {
                let style = theme::markdown_style(base, styles);
                if composing { underlined(style) } else { style }
            })
            .to_run(len)
    }
}

fn underlined(style: TextStyle) -> TextStyle {
    let color = style.color;
    style.highlight(HighlightStyle {
        underline: Some(UnderlineStyle {
            color: Some(color),
            thickness: px(1.0),
            wavy: false,
        }),
        ..HighlightStyle::default()
    })
}

/// Runs indexed from zero within the line. Every range is clamped to the line,
/// so a stale span mis-colours one frame instead of breaking the run lengths.
pub(super) fn line_runs(
    line: &Range<ByteOffset>,
    spans: &[Span],
    marked: Option<&Range<ByteOffset>>,
    styles: &mut RunStyles<'_>,
) -> Vec<TextRun> {
    let clamp = |offset: ByteOffset| offset.clamp(line.start, line.end);

    let mut cuts = vec![line.start, line.end];
    for span in spans {
        cuts.extend([clamp(span.range.start), clamp(span.range.end)]);
    }
    if let Some(marked) = marked {
        cuts.extend([clamp(marked.start), clamp(marked.end)]);
    }
    cuts.sort_unstable();
    cuts.dedup();

    let mut spans = spans.iter().peekable();
    let mut runs = Vec::with_capacity(cuts.len());
    for pair in cuts.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        while spans.next_if(|span| span.range.end <= start).is_some() {}
        let span_styles = spans
            .peek()
            .filter(|span| span.range.start <= start)
            .map_or(Styles::NONE, |span| span.styles);
        let composing = marked.is_some_and(|marked| marked.start <= start && start < marked.end);
        runs.push(styles.run(span_styles, composing, end.0 - start.0));
    }
    if runs.is_empty() {
        runs.push(styles.run(Styles::NONE, false, 0));
    }
    runs
}
```

- [ ] **Step 4: Update the caller in `shape.rs`**

In `crates/app/src/editor/layout/shape.rs`, change `shape_document`'s signature and body. The new function:

```rust
/// Takes `style` explicitly since the editor also shapes outside any draw,
/// where `window.text_style()` doesn't apply. `wrap_width: None` never wraps.
pub(crate) fn shape_document(
    buffer: &Buffer,
    spans: &[Span],
    marked: Option<&Range<ByteOffset>>,
    style: &TextStyle,
    wrap_width: Option<Pixels>,
    window: &mut Window,
) -> DocumentLayout {
    let mut styles = RunStyles::new(style);
    let font_size = style.font_size.to_pixels(window.rem_size());

    let mut row_count = 0;
    // Spans never cross a `\n`, so each line's spans are the next ones in order.
    let mut rest = spans;
    let lines = buffer
        .lines()
        .map(|(start, text)| {
            let range = start..ByteOffset(start.0 + text.len());
            let own = rest
                .iter()
                .take_while(|span| span.range.start < range.end)
                .count();
            let (line_spans, next) = rest.split_at(own);
            rest = next;
            let runs = line_runs(&range, line_spans, marked, &mut styles);
            let wrapped = window
                .text_system()
                .shape_text(
                    SharedString::from(text.to_string()),
                    font_size,
                    &runs,
                    wrap_width,
                    None,
                )
                .map_err(|err| eprintln!("failed to shape line at {start}: {err}"))
                .ok()
                .and_then(|mut shaped| shaped.pop())
                .unwrap_or_default();
            let rows = rows(text, &wrapped, &range);
            let first_row = row_count;
            row_count += rows.len();
            LineLayout {
                wrapped,
                range,
                first_row,
                rows,
            }
        })
        .collect();

    DocumentLayout {
        lines,
        row_count,
        line_height: style.line_height_in_pixels(window.rem_size()),
        style: style.clone(),
        wrap_width,
    }
}
```

Imports in `shape.rs`: add `use inkra_markdown::Span;` and `use super::runs::{RunStyles, line_runs};`; remove `HighlightStyle`, `UnderlineStyle` and `px` if now unused (clippy will say).

- [ ] **Step 5: Update the three other callers**

`crates/app/src/editor/element.rs` — both calls (in `request_layout`'s measure closure and in `prepaint`'s fallback) gain `editor.spans()` as the second argument:

```rust
                let layout = shape_document(
                    editor.buffer.read(cx),
                    editor.spans(),
                    marked.as_ref(),
                    &text_style,
                    Some(width),
                    window,
                );
```

```rust
            shape_document(
                editor.buffer.read(cx),
                editor.spans(),
                marked.as_ref(),
                &style,
                Some(bounds.size.width),
                window,
            )
```

`crates/app/src/editor/mod.rs`, in `reshape`:

```rust
        self.last_layout = Some(layout::shape_document(
            self.buffer.read(cx),
            self.spans(),
            marked.as_ref(),
            &style,
            wrap_width,
            window,
        ));
```

and add, temporarily returning nothing (Task 8 fills it), next to `buffer()`:

```rust
    fn spans(&self) -> &[inkra_markdown::Span] {
        &[]
    }
```

`crates/app/src/field.rs:357` and `crates/app/src/field/tests.rs:188`: pass `&[]` as the second argument (a file name is not markdown):

```rust
        let layout = shape_document(buffer, &[], marked.as_ref(), &style, None, window);
```

- [ ] **Step 6: Run the tests**

Run: `cargo test -p inkra-app`
Expected: all pass, including the 5 new `runs` tests. Nothing is styled yet in the app: `spans()` is empty.

- [ ] **Step 7: Clippy, fmt, commit**

```bash
cargo fmt -p inkra-app && cargo clippy -p inkra-app --all-targets --all-features -- -D warnings
git add crates/app/src
git commit -m "Merge markdown spans and the IME range into text runs"
```

---

### Task 8: `EditorView` keeps the parse, tagged with the buffer version

**Files:**
- Modify: `crates/app/src/editor/mod.rs`, `crates/app/src/editor/tests/mod.rs`
- Create: `crates/app/src/editor/tests/markdown.rs`

- [ ] **Step 1: Write the failing tests**

In `crates/app/src/editor/tests/mod.rs`, add `mod markdown;` after `mod dispatch;`.

`crates/app/src/editor/tests/markdown.rs`:

```rust
use gpui::{TestAppContext, prelude::*};
use inkra_markdown::parse;
use inkra_text::{Buffer, ByteOffset};

use super::{dispatching, with_editor, window_on};

#[gpui::test]
fn a_note_is_styled_before_any_keystroke(cx: &mut TestAppContext) {
    with_editor(cx, "**b**", |view, _, _| {
        assert_eq!(view.spans(), parse("**b**").spans.as_slice());
    });
}

#[gpui::test]
fn typing_refreshes_the_spans_and_undo_restores_them(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "*b");
    let before = parse("*b").spans;

    cx.simulate_input("*");
    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(view.buffer.read(cx).text(), "*b*");
            assert_eq!(view.spans(), parse("*b*").spans.as_slice());
        })
        .unwrap();

    cx.simulate_keystrokes("cmd-z");
    window
        .update(&mut cx, |view, _, _| assert_eq!(view.spans(), before.as_slice()))
        .unwrap();
}

#[gpui::test]
fn opening_another_note_replaces_the_spans(cx: &mut TestAppContext) {
    let other = cx.update(|cx| cx.new(|_| Buffer::new("# T")));
    with_editor(cx, "**b**", move |view, _, cx| {
        view.open_buffer(other, cx);
        assert_eq!(view.spans(), parse("# T").spans.as_slice());
    });
}

#[gpui::test]
fn a_write_outside_the_gates_refreshes_the_spans(cx: &mut TestAppContext) {
    let buffer = cx.update(|cx| cx.new(|_| Buffer::new("b")));
    let window = window_on(cx, buffer.clone());

    buffer.update(cx, |buffer, cx| {
        buffer.transact_at(std::time::Instant::now(), |buffer| {
            buffer.insert(ByteOffset(1), "**");
            buffer.insert(ByteOffset(0), "**");
        });
        cx.notify();
    });
    cx.run_until_parked();

    window
        .update(cx, |view, _, _| {
            assert_eq!(view.spans(), parse("**b**").spans.as_slice());
        })
        .unwrap();
}
```

Note: the last test writes the way `crates/app/src/shell/tests/renaming.rs:57-61` does, through `transact_at` inside `Entity::update`, and calls `cx.notify()` because a direct `update` does not notify observers on its own.

- [ ] **Step 2: Run to see them fail**

Run: `cargo test -p inkra-app markdown`
Expected: FAIL — `spans()` returns `&[]`, so all four assertions on spans fail.

- [ ] **Step 3: Implement**

In `crates/app/src/editor/mod.rs`:

Add `use inkra_markdown::{Markdown, Span};` to the imports.

Add the field to `EditorView`, after `state`:

```rust
    /// Tagged with the `Buffer::version()` it was parsed from (live-preview spec, D10).
    markdown: Option<(u64, Markdown)>,
```

Replace `EditorView::new`'s body with:

```rust
        let end = buffer.read(cx).len();
        let buffer_changed = cx.observe(&buffer, Self::on_buffer_changed);

        let mut view = Self {
            buffer,
            state: EditorState::new(Selection::at(end)),
            markdown: None,
            focus_handle: cx.focus_handle(),
            is_selecting: false,
            last_layout: None,
            last_bounds: None,
            last_text_top: None,
            scroll_handle: ScrollHandle::new(),
            _buffer_changed: buffer_changed,
        };
        view.refresh_markdown(cx);
        view
```

In `open_buffer`, replace the observer line and add the refresh right after `self.buffer = buffer;`:

```rust
        self._buffer_changed = cx.observe(&buffer, Self::on_buffer_changed);
        self.buffer = buffer;
        // Reset, not compared: versions are per buffer, so two notes can share one.
        self.markdown = None;
        self.refresh_markdown(cx);
```

Replace the temporary `spans` from Task 7 and add the refresh and the observer, next to `buffer()`:

```rust
    fn spans(&self) -> &[Span] {
        self.markdown
            .as_ref()
            .map_or(&[], |(_, markdown)| markdown.spans.as_slice())
    }

    /// Parses only when the buffer moved since the last parse; a caret move costs nothing.
    fn refresh_markdown(&mut self, cx: &App) {
        let buffer = self.buffer.read(cx);
        let version = buffer.version();
        if self.markdown.as_ref().is_some_and(|(tag, _)| *tag == version) {
            return;
        }
        self.markdown = Some((version, inkra_markdown::parse(buffer.text())));
    }

    /// Covers writes that bypass `edit_buffer`; tests make them today.
    fn on_buffer_changed(&mut self, _: Entity<Buffer>, cx: &mut Context<Self>) {
        self.refresh_markdown(cx);
        cx.notify();
    }
```

In `reshape`, make the refresh the first line, before the `let Some(last)` early return:

```rust
    fn reshape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // First: the observer only runs when effects flush, after this reshape.
        self.refresh_markdown(cx);
        let Some(last) = self.last_layout.as_ref() else {
            return;
        };
```

- [ ] **Step 4: Run to see them pass**

Run: `cargo test -p inkra-app`
Expected: all pass, including the 4 new `markdown` tests.

- [ ] **Step 5: Clippy, fmt, commit**

```bash
cargo fmt -p inkra-app && cargo clippy -p inkra-app --all-targets --all-features -- -D warnings
git add crates/app/src/editor
git commit -m "Parse the note once per buffer version and style the editor with it"
```

---

### Task 9: Render and look

**Files:**
- Modify: `crates/app/src/shot/scenes.rs`

- [ ] **Step 1: Add the scene**

In `crates/app/src/shot/scenes.rs`, after the `WRAPPED` constant:

```rust
const MARKDOWN: &str = "---\n\
                        title: Styled source\n\
                        ---\n\
                        # Heading one\n\
                        \n\
                        ## Heading two ##\n\
                        \n\
                        Plain, **strong**, *emphasis*, ***both***, ~~struck~~ and `code`.\n\
                        \n\
                        A [link](https://example.com), an <https://auto.link>, a [[Wiki Page|wikilink]], and ![an image](a.png).\n\
                        \n\
                        > A quote that runs onto\n\
                        > a second line.\n\
                        \n\
                        - a bullet with **bold**\n\
                        - [ ] a task\n\
                        1. a numbered item\n\
                        \n\
                        ```rust\n\
                        let x = 1;\n\
                        ```\n\
                        \n\
                        ---\n\
                        \n\
                        | a | b |\n\
                        |---|---|\n\
                        | 1 | 2 |";
```

In `catalogue()`, after the `editor-wrapped-selection` entry:

```rust
        Scene {
            name: "editor-markdown",
            shows: "styled source: every construct, markers dimmed, nothing concealed",
            open: editor_markdown,
        },
```

After `fn editor_wrapped_selection`:

```rust
fn editor_markdown(stage: &mut Stage) -> WindowHandle<AppShell> {
    stage.open_note(MARKDOWN)
}
```

- [ ] **Step 2: Render**

Run: `cargo run -p inkra-app --features shot --example shot editor-markdown`
Expected: writes `target/shots/editor-markdown.png`.

- [ ] **Step 3: Look at it**

Open `target/shots/editor-markdown.png` (the Read tool shows images). Check, and write down what you see for each:
- headings bold; `#`, ` ##` dimmed;
- `**`, `*`, `~~`, backticks dimmed; strong bold, emphasis italic, `***both***` bold italic, struck text struck;
- `code` and the fenced block in Menlo, code colour; fence lines dimmed;
- link text blue; `[`, `](…)`, `<`, `>`, `[[Wiki Page|`, `]]` dimmed;
- quote text grey; both `> ` dimmed;
- `- `, `- [ ] `, `1. ` dimmed;
- frontmatter and both `---` fences dimmed; the thematic break dimmed; table pipes dimmed;
- rows evenly spaced; nothing overlaps; the column is centred.

If anything is off, fix it before continuing, and re-render.

- [ ] **Step 4: Commit**

```bash
git add crates/app/src/shot/scenes.rs
git commit -m "Add the editor-markdown shot scene"
```

---

### Task 10: Mutants, full check, and the spec

- [ ] **Step 1: Mutants on the new crate**

Run: `cargo mutants --package inkra-markdown` (if the flag is rejected by the config, run `cargo mutants` and read only the `crates/markdown` lines).
Expected: zero missed in `crates/markdown`. For each missed mutant, add a test in `parse/tests.rs` or `styles/tests.rs` that kills it; do not exclude it unless it is provably equivalent, in which case add it to `exclude_re` in `.cargo/mutants.toml` with a comment proving equivalence, as the existing entry does.

- [ ] **Step 2: Full verification**

Run: `./scripts/check.sh; echo "exit $?"`
Expected: `exit 0`.

- [ ] **Step 3: Mark slice 1 in the spec**

In `docs/superpowers/specs/2026-09-23-markdown-live-preview-design.md`, change the status line to:

```markdown
Status: approved (2026-09-23), after two adversarial reviews. Slice 1 (styled
source) implemented. The first draft was a source-mode design; "What the
review changed" records what survives from it.
```

If building disagreed with the spec anywhere, add a "Corrected in implementation" section at the end, one bullet per disagreement, as `2026-09-23-soft-wrap-design.md` does.

- [ ] **Step 4: Commit**

```bash
git add -A docs .cargo crates
git commit -m "Kill the surviving mutants in inkra-markdown; record slice 1"
```
