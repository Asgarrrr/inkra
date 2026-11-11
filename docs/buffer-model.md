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

Both are opaque: `new` builds one, `get` reads it. An offset plus a length is an
offset, two offsets differ by a length, and a count or length stays `usize`
(`2026-11-02-type-safety-hygiene-design.md`).

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

Every edit happens inside `transact` or `transact_at`. The closure gets a
`Txn`, the only value with edit methods. `Txn::replace(Range<ByteOffset>, &str)`
is the single primitive, and `insert` and `delete` are wrappers. Every other
operation in the system reduces to it. A `Txn` derefs to `&Buffer`, so the
closure reads its own edits.

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

Re-entrancy is unrepresentable. A `Txn` derefs to `&Buffer` and never to
`&mut Buffer`, so the closure cannot call `undo`, `redo`, `transact` or
`anchor`. Each of these takes `&mut Buffer`. `Buffer` itself has no edit
method, so no edit happens outside a transaction.

The hazard this removes: `apply` addresses edits by absolute byte offset. That
is sound only while "top of the undo stack" is also "most recent change to the
text". An open transaction breaks that: its edits have mutated the text but are
not yet on the stack, so the transaction below them describes an older state.
An undo inside it would delete the wrong bytes:

```rust
let mut buf = Buffer::new("abc1");
buf.transact(|txn| {
    txn.insert(ByteOffset::new(0), "X"); // "Xabc1"
    txn.undo();                          // does not compile
});
```

Before the `Txn`, a depth counter joined nested transactions, and `undo` and
`redo` asserted that the depth was zero. The `compile_fail` doctests on
`Buffer::transact` replace those runtime checks.

### Version

`Buffer::version` returns a `Version`: the id of the last edit on the undo
stack. Equal versions mean equal text. A `Version` has no `Ord`: an undo moves
the version back to an older id, so a smaller version can name the newer text.

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

## Anchors

A position that survives edits. `Buffer::anchor(offset, bias)` returns an
`Anchor`; `resolve` reads it back as a `ByteOffset`, `forget` drops it. Every
mutation of the text — `Txn::replace`, and the edits `undo` and `redo` replay —
runs each live anchor through `transform` in `anchor.rs`. That function is the
whole model: shift past the edit, or, at the edit's own start, let `Bias` pick
a side.

Not Zed's `Anchor`. Zed's carries a Lamport timestamp and a replica id and
resolves by walking a `sum_tree` of insertion fragments. That machinery answers
a question we do not have — which of two concurrent inserts at the same offset
wins. `Buffer` is single-writer, one `String`, one edit-list undo stack. The
shape that fits is the classic one: Emacs markers, relocated in place on every
insertion and deletion.

`Anchor` is an opaque handle into a table `Buffer` owns, not an offset a caller
can read. So the representation can be swapped — for a CRDT, if collaborative
editing is ever built — without touching a call site. It is deliberately not
`Ord`: the handle is creation order, not document order, and comparing two
anchors means resolving both first. It also carries no buffer identity, so
resolving one against a *different* `Buffer` is silently wrong rather than a
panic. Every open note has its own `Buffer`, so ownership holds the risk: a
`ParkedView` owns its buffer, and its anchors resolve only there.

Slots are never reused. `forget` tombstones one, and resolving a tombstoned
handle panics — the caller-bug contract. Reuse without a generation counter
turns that panic into a confident answer about somebody else's anchor. The
price is that each edit walks every slot ever created rather than every live
one: the same "fine for a note" trade as the O(n) offset conversion below.

### What an anchor does not survive

An anchor strictly inside a deleted span collapses onto the edit's boundary and
does not come back inside the text on undo. That is scope, not a limit of the
model: Emacs restores markers through exactly this edit with a per-edit
adjustment in its undo list, and `Edit { at, removed, inserted }` is the same
shape of record and could carry the same adjustment. The cost is roughly
another transform pass over the undo path. Nothing needs it yet.

The boundaries are not symmetric, and the asymmetry is deliberate. A
`Right`-biased anchor at the exact end of a *pure deletion* does round-trip
through delete-then-undo. An anchor collapsed from *inside* a span moves
boundary to boundary instead, and never re-enters the restored text. Both are
pinned by tests, so a later "fix" to one cannot quietly be applied to the
other.

Whole-buffer reload is the case to watch. `replace(0..len, new_text)` puts
every anchor inside the touched span, so every one of them collapses. File
watching therefore needs anchors *and* diff-based reload, not anchors alone.

## Graphemes

Cursor movement and deletion step by grapheme cluster, via
`unicode-segmentation`. "e" plus a combining acute is one step; so is a ZWJ
emoji sequence. Stepping by char splits both.

## Cost

`previous_grapheme` and `next_grapheme` cost one step, not one document. Each
starts a `GraphemeCursor` at the offset and reads only the text around it.

One case is not local. A long run of regional indicators or emoji modifiers
makes the cursor look back through the whole run.

## Deferred

Decisions made with reasons, so they are not relitigated from scratch:

- **Rope.** Not yet. `String` plus O(n) offset conversion has no measured
  problem at the sizes handled so far — a note, not a repository. The trigger
  is a measured one: a real vault, a real file. When it fires: `ropey` is the
  leading candidate,
  but it ships no grapheme segmentation. `previous_grapheme`/`next_grapheme`
  already use a `GraphemeCursor`; under a rope, they must feed it chunks
  (~40-60 lines) — not a dependency swap. Both `ropey` and `crop` were outside our 12-month freshness
  rule as of 2026-09-20; adopting either is a deliberate exception.
- **Caching offset conversion** short of a rope. Rejected. A cached last
  `(byte, utf16)` pair only helps sequential typing, already the cheap case,
  and does nothing for select-all, click-into-middle, or protocol queries at
  arbitrary points — while adding an invalidation invariant to maintain.
