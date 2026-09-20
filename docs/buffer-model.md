# Text model

Living reference for `crates/text`. Rules live in `CLAUDE.md`; this explains
why they are what they are. Update it when the model changes.

## Offsets

Two newtypes, never mixed:

- `ByteOffset` — position in the UTF-8 bytes. Storage coordinate, and the one
  GPUI's text shaping indexes by (`ShapedLine::x_for_index` and friends take
  byte indices).
- `Utf16Offset` — position in UTF-16 code units. The coordinate platform text
  input protocols speak.

There was a third, `CharOffset`, removed after the input spike. It bought
nothing byte offsets don't — every consumer wanted either storage positions or
protocol positions — and it added a conversion hop on a path already crossing
two coordinate systems.

Byte ↔ UTF-16 conversion lives in `text` rather than next to the protocol code
in `app`. It is a property of the text with no layout input, so the boundary
rule puts it there; keeping it in one place also means a future rope can make
it sub-linear without touching `app`.

`from_utf16` snaps forward when an offset lands inside a surrogate pair, so a
platform probing an arbitrary position never yields a split character.

## Editing

`replace(Range<ByteOffset>, &str)` is the single primitive. `insert` and
`delete` are wrappers. Every other operation in the system reduces to it.

An out-of-range or non-boundary offset is a caller bug, not a runtime
condition: `debug_assert!` and panic, no `Result`. The stdlib's own slicing
catches the same cases in release, so compiling the assertions out costs only
a less descriptive message.

## Undo

Each edit records `{ at, removed, inserted }` — both sides of the replacement.
Cost is proportional to the edit, not the buffer. The earlier design snapshotted
the whole text per edit, which is O(file) memory per keystroke.

A `Transaction` is a group of edits that undo and redo as one step.

### Grouping

`transact` makes one standalone step. `transact_at(now, …)` additionally merges
into the previous transaction when the two fall within `GROUP_INTERVAL`
(300 ms, matching Zed's default), so a typing run undoes as a unit rather than
one step per character. `finalize_transaction` seals the current step so the
next edit cannot merge into it — use it around a paste or a reformat, which
should not be swallowed by adjacent typing.

`text` never reads a clock. The caller supplies `now`. A buffer that calls
`Instant::now()` internally needs a `cfg(test)` override to be testable, and a
type that behaves differently under test than in production is exactly where
bugs hide. Zed takes the override approach (`text.rs:226-230`); we took the
injection one.

### Re-entrancy

`undo` and `redo` panic if called while a transaction is open. This is a real
`assert!`, not `debug_assert!`.

`apply` addresses edits by absolute byte offset, which is sound only while
"top of the undo stack" is also "most recent change to the text". An open
transaction breaks that: its edits have mutated the text but are not yet on the
stack, so the transaction below them describes an older state. Applying it
deletes the wrong bytes.

```rust
let mut buf = Buffer::new("abc");
buf.insert(ByteOffset(3), "1");        // "abc1"
buf.transact(|buf| {
    buf.insert(ByteOffset(0), "X");    // "Xabc1"
    buf.undo();                        // → "Xab1" without the guard
});
```

It is a real `assert!` because nothing else catches it. Unlike an out-of-range
offset, there is no stdlib bounds check underneath, so a release build would
silently corrupt text instead of panicking.

### The range undo and redo return

Both return the region touched by the **last edit they applied**, not a union
over the transaction.

For a multi-edit transaction, every earlier range has been shifted by the edits
applied after it — only the last one is still addressable in the resulting
text. A union across intermediate states can point past the end.

Undo and redo therefore report different positions for the same transaction.
That is correct, not an asymmetry to fix: they produce different texts.

A caller that needs the exact selection from before an edit has to remember it
itself. Zed does this with a side table in the editor layer keyed by
transaction id (`editor.rs:1437-1450`), deliberately not inside the buffer.

## Graphemes

Cursor movement and deletion step by grapheme cluster, via
`unicode-segmentation`. "e" plus a combining acute is one step; so is a ZWJ
emoji sequence. Stepping by char splits both.

## Cost

`previous_grapheme` and `next_grapheme` are O(length of the document), not
O(one step). Both build a `grapheme_indices` iterator over the whole text;
`previous_grapheme` then walks it backwards from the end until it passes the
offset. Every arrow key, backspace and delete pays one such scan.

Fine for a note. It stops being fine at the scale where one keypress scans tens
of kilobytes — the same trigger as the rope below, and they are fixed together:
a rope replaces the scan with a chunk-local `GraphemeCursor`.

## Deferred

Decisions made with reasons, so they are not relitigated from scratch:

- **Rope.** Not yet. `String` plus O(n) offset conversion has no measured
  problem at the sizes handled so far — a note, not a repository. The trigger
  is a measured one: a real vault, a real file. When it fires: `ropey` is the
  leading candidate,
  but it ships no grapheme segmentation, so `previous_grapheme`/`next_grapheme`
  become a manual chunk-walking `GraphemeCursor` (~40-60 lines) — not a
  dependency swap. Both `ropey` and `crop` were outside our 12-month freshness
  rule as of 2026-09-20; adopting either is a deliberate exception.
- **Caching offset conversion** short of a rope. Rejected. A cached last
  `(byte, utf16)` pair only helps sequential typing, already the cheap case,
  and does nothing for select-all, click-into-middle, or protocol queries at
  arbitrary points — while adding an invalidation invariant to maintain.
- **Anchors.** Not yet, but no longer open-ended: they are the named
  prerequisite for two panes on one note and for the vault's file watching.
  Every mutation currently goes through one path that re-derives the selection
  immediately, so there is no second actor to rot a raw offset. A second
  `EditorState` over the same `Buffer` would be exactly that actor — which is
  why multi-pane is wired but unsupported. The version needed is
  offset-shifting on edit (~25 lines), not Zed's CRDT `Anchor` with its Lamport
  clock and replica id.
