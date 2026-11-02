# Type safety and hygiene: batches 3 and 4 of the 2026-11-02 audit

Status: **S1 built 2026-11-02.** Later slices append their sections here.

## Method

- One stacked branch per slice, type slices first.
- Every type slice is test-first and changes no behaviour. Where practical, a
  `compile_fail` doctest shows the old misuse no longer compiles.
- A `compile_fail` doctest passes on any compile error, and rustdoc checks error
  codes only on nightly. So every `compile_fail` block is followed by a positive
  control that differs from it only in the forbidden token. The control proves
  that the token, not a typo, is what fails.
- No pixel changes: the off-screen shots before and after each slice are
  byte-identical (`cmp`).
- `cargo mutants` reports zero missed after each change to `text`, `editor`,
  `markdown`, `index` or `tabs`.

## Decisions

1. **Offset types name positions; counts and lengths stay `usize`.** An offset
   plus a length is an offset. Two offsets differ by a length. Two positions do
   not add.
2. **The gpui protocol keeps `Range<usize>`.** `EntityInputHandler` speaks
   UTF-16 positions as `usize`, so `text_input.rs` converts at that boundary
   and nowhere else.

## T1+T2 offsets

The audit found three ways to misuse an offset:

- A `pub` field let any crate build or read an offset from a bare `usize`,
  which bypassed the type.
- The UTF-16 range conversions took and returned `Range<usize>`, so a byte
  range could pass where a UTF-16 range was due.
- Code sliced the buffer text with `&text[a.0..b.0]`.

### As built (S1)

- `ByteOffset`, `Utf16Offset` (`text/src/offset.rs`) and `DisplayOffset`
  (`markdown/src/display.rs`) have a private field. `new` builds one, and `get`
  reads it.
- One local `macro_rules!` gives `ByteOffset` and `Utf16Offset` the same
  operators: `Add<usize>`, `Sub<usize>` and `Sub<Self>` with `Output = usize`.
  `Sub<Self>` panics in debug when `b > a`, exactly as the `usize` subtraction
  it replaces. That is a caller bug.
- `DisplayOffset` has no operators: no code does arithmetic on it. The one
  difference of two display offsets (`layout/runs.rs`) reads both with `get`.
- `Buffer::slice(Range<ByteOffset>) -> &str` replaces `&text[a.0..b.0]` on the
  buffer text. Slices of a `&str` that is not the buffer text (a markdown
  source, a line) index with `get()` values, as the local code already did.
- `Buffer::to_utf16_range` returns `Range<Utf16Offset>`, and
  `from_utf16_range` takes `&Range<Utf16Offset>`.
- `EditorState::target_range`, `replace_in_range` and
  `mark_and_replace_in_range` take `Option<&Range<Utf16Offset>>`.
  `mark_and_replace_in_range`'s `selected_utf16` stays `Option<Range<usize>>`:
  it is relative to the inserted text, so it holds lengths, not positions
  (decision 1).
- `app/src/text_input.rs` converts with two private free functions, `to_typed`
  and `to_raw`. `editor/tests/protocol.rs` is unchanged and pins the protocol.
- `compile_fail` doctests, each with its control: building an offset from a
  bare `usize`, reading `.0`, adding two offsets, mixing a byte and a UTF-16
  offset, building a `DisplayOffset` from a bare `usize`, and passing a
  `Range<usize>` to `from_utf16_range`.
- `.cargo/mutants.toml` matches the equivalent `transform` mutant by column.
  The column moved from 48 to 44 when `offset.0` became `offset`.

## Deviations from Zed

1. **Private fields.** Zed's offsets have a `pub` field: `MultiBufferOffset`,
   `BufferOffset` (`multi_buffer/src/multi_buffer.rs:226`, `:303`) and
   `OffsetUtf16` (`rope/src/offset_utf16.rs:4`). Inkra makes the field private,
   because the audit found a `pub` field to be the hole that let a bare `usize`
   pass as an offset.
2. **Fewer operators.** Zed also has `AddAssign`, `SubAssign` and
   `saturating_sub` (`multi_buffer.rs:415-470`). Inkra adds only the three
   operators that existing code needs. It adds no `AddAssign`, `SubAssign`,
   `From<usize>` or `Into<usize>`.
3. **One conversion site.** Zed wraps the protocol's `usize` at each use in
   `editor/src/input.rs` (`text_for_range` builds
   `MultiBufferOffsetUtf16(OffsetUtf16(..))` inline). Inkra converts in two
   named functions, because both hosts share one `TextInput`.

## Non-goals

- No change of behaviour or pixels.
- No new offset type, and no offset type for lengths.
- No operators on `DisplayOffset` until code does arithmetic on it.
