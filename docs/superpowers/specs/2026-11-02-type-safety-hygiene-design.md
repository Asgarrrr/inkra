# Type safety and hygiene: batches 3 and 4 of the 2026-11-02 audit

Status: **S1, S2 and S3 built 2026-11-02.** Later slices append their sections here.

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

## T3 Txn and T5 Version

The audit found two holes in `Buffer`:

- `replace`, `insert` and `delete` were `pub` on `Buffer`. An edit outside any
  transaction recorded its own step, and an `undo` inside an open transaction
  needed a runtime `assert!` to stop it applying stale offsets.
- `version()` returned a `u64`. Code could order two versions, but an undo
  moves the version back, so the order meant nothing.

### As built (S2)

- `Txn<'a>` (`text/src/buffer.rs`) holds `&mut Buffer` and the transaction's
  edits. It has `replace`, `insert` and `delete`, and derefs to `&Buffer` for
  reads. It has no `DerefMut`, so the closure cannot call `undo`, `redo`,
  `transact` or `anchor`.
- `transact` and `transact_at` pass `&mut Txn<'_>` to the closure. After the
  closure, `close` records the edits with today's merge rule: within
  `GROUP_INTERVAL` unless `finalize_transaction` blocked grouping.
- The private `Buffer::edit` holds the old `replace` body and returns `None`
  for a no-op edit. An empty transaction records no step and does not move the
  version.
- The `open` and `depth` fields, `record`, and the `assert!`s in `undo` and
  `redo` are gone. `compile_fail` doctests on `Buffer::transact` replace three
  tests: nested transactions, undo inside a transaction, and redo inside a
  transaction.
- A former bare edit now runs in an untimed `transact`. Both record one step
  that no later edit merges into, so the undo history is unchanged.
- `Version(u64)` derives `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq` and
  `Default`. `Version::default()` is the version of a buffer with no edit,
  which was `0`. It has no `Ord`, `Display` or `Hash`: no holder needs them.
- Retyped holders: `Diff.base_version` and `diff()`, `ParkedSelection.version`,
  `DocumentLayout.version`, `EditorView.parsed_at`, `OpenNote.saved_version`,
  and the save and reload paths in `app/src/vault`. The session file's
  `version: u32` and the index revision are different things and stay.
- `version_moves_whenever_a_timed_edit_changes_the_text` keeps every earlier
  version and asserts that the new one is not among them, instead of `>`.

## T4 DETACHED and T6 OnNote

The audit found two sentinels:

- `EntryId::DETACHED` had three meanings: the id of a note opened by path, a
  found row that `mint` has not given an id yet, and "no source" for a link
  resolved from a detached note.
- `Place` and `Parked` held a `note: Option<EntryId>` beside a
  `view: Option<..>`, so a view with no note was representable.

### As built (S3)

- `Vault::applied` builds private `Row`s (`vault.rs`) with `id: Option<EntryId>`.
  A kept entry has `Some(id)`. A found file has `None` until `mint` gives it an
  id. `sorted` takes and returns `Vec<Row>`.
- `mint` (`vault/identity.rs`) consumes the rows and returns
  `(Vec<(Entry, FileIdentity)>, u32)`: the entries and the next id to mint.
  It keeps the old order and the old decisions: a `None` row is dropped when
  its identity is already claimed (hard links), else it takes the known id,
  else the replaced one, else a new one. `replaced` filters `row.id.is_none()`.
- `Key::of(entry)` is gone. `Key::new(path, kind)` builds every key, so
  `reclaim.rs` and the oracle tests build no `Entry` to sort.
- `NoteIndex::resolve` and `resolve_link` take `source: Option<EntryId>`, and
  so do the table's `resolve`, `query` and `join`. A source outside the table,
  `None` or an id the table does not hold, resolves names and absolute paths
  but no relative link. A test pins that `None` and `Some(EntryId::DETACHED)`
  resolve the same. `backlinks` passes `Some(source)`.
- `AppShell::follow` passes `self.showing()`, not `unwrap_or(DETACHED)`.
- `DETACHED` now means only the id of a note opened by path. The guards in
  `reidentify`, `reclaim` and `Scans`'s claims stay for that meaning.
- `OnNote<V> { note, view: Option<V> }` (`shell/tabs.rs`) is a tab on a note.
  `Place(Option<OnNote<ViewState>>)` and `Parked(Option<OnNote<ParkedView>>)`
  wrap it, so a view with no note does not compile. `Place::default()` is the
  empty tab. The tabs spec's "`Parked` holds `(EntryId, Option<ParkedView>)`"
  now holds by type.
- The session file format and the restore are unchanged.

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
4. **A closure guard, not a depth counter.** Zed's `History` counts
   `transaction_depth` in `start_transaction` and `end_transaction`
   (`text/src/text.rs:238-275`), so nested editor operations join the
   outermost transaction. A note app has no nested editor operation that needs
   the counter. Inkra hands the closure a `Txn` instead, and nesting does not
   compile.
5. **An opaque stamp, not a vector clock.** Zed's `version()` is a
   `clock::Global` (`text/src/text.rs:829`) with a partial order, for
   concurrent replicas. `Buffer` has one writer, so a `Version` compares only
   for equality.

## Non-goals

- No change of behaviour or pixels.
- No new offset type, and no offset type for lengths.
- No operators on `DisplayOffset` until code does arithmetic on it.
