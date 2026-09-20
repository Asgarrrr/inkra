# Anchors — plan

Status: revised after red and blue review. Changes forced by review are
marked **[red]** / **[blue]** where they replace something the draft got
wrong; agreements that shaped wording without changing behavior are folded in
silently.

`docs/buffer-model.md`'s "Deferred" section already named the trigger and the
shape: anchors are the prerequisite for two panes on one note and for the
vault's file watching, and "the version needed is offset-shifting on edit
(~25 lines), not Zed's CRDT `Anchor` with its Lamport clock and replica id."
This plan sizes that sentence.

Scope is **`inkra-text` only**. No consumer exists yet — multi-pane and
`inkra-vault` are both unbuilt — so this plan adds the primitive and nothing
that uses it. `editor` and `app` are untouched.

**What this does not unlock, stated up front [blue]**: file watching needs
more than anchors. The obvious reload implementation —
`replace(ByteOffset(0)..len, new_text)` — puts every anchor inside the
touched span, so every one of them collapses per D2. Anchors serve the
two-pane consumer completely; they serve file watching only once reload is
diffed into minimal edits, which is unscoped here. Carried into the Docs
section below so `inkra-vault`'s eventual author finds "prerequisite: anchors
+ diff-based reload," not "prerequisite met."

## Why not Zed's model

Read against `vendor/zed-reference/crates/text/src/anchor.rs` and `text.rs`
(`anchor_before`/`anchor_after` at `text.rs:2621-2628`, resolution in
`offset_for_anchor` at `text.rs:2509`).

Zed's `Anchor` is `{ timestamp: Lamport, offset, bias, buffer_id }`, resolved
by walking a `sum_tree` of insertion fragments that are marked invisible
rather than deleted. That machinery exists to answer one question we do not
have: two replicas insert at the same offset concurrently, which one "wins."
`inkra_text::Buffer` is single-writer, single-thread, backed by one `String`
with an edit-list undo stack (`buffer.rs:47-54`) — there is no second replica
to disagree with.

What we need is the classic single-writer version: a position that is
transformed through each edit as it happens, forward or in reverse. No
fragments, no timestamps, no tombstones. Review found real prior art for this
exact shape: Emacs markers, relocated in place on every insertion/deletion,
with per-edit adjustments recorded in `buffer-undo-list` so undo restores
them too — decades of precedent for "shift in place," not an invention.

## Decisions

### D1 — `Anchor` is opaque, so the internals stay swappable

`Anchor` is a handle (`Anchor(usize)`, private field) into a table `Buffer`
owns, not a `ByteOffset` a caller can read directly. Every anchor is created
and resolved through `Buffer`:

```rust
pub fn anchor(&mut self, offset: ByteOffset, bias: Bias) -> Anchor;
#[must_use]
pub fn resolve(&self, anchor: Anchor) -> ByteOffset;
pub fn forget(&mut self, anchor: Anchor);
```

`Anchor` derives `Clone, Copy, PartialEq, Eq, Hash, Debug` and deliberately
**not** `PartialOrd`/`Ord` **[blue]**: an index is creation order, not
document order, and ordering two anchors is meaningless without resolving
both first. Zed makes the identical point by requiring a buffer for
comparison (`Anchor::cmp(&self, other, buffer)`,
`vendor/zed-reference/crates/text/src/anchor.rs:91-103`). If a future caller
needs "which of these comes first," that is a `Buffer`-taking method, not a
derive — adding `Ord` later is easy, removing it once something depends on it
is not.

`#[must_use]` goes on `anchor()` as well as `resolve()` **[blue]**: under
D3's never-reuse policy, a discarded handle leaks its slot for the life of
the buffer.

This is the one piece of insurance against the multi-author question the
user raised: if collaborative editing is ever built, `Buffer`'s internals can
be swapped for a CRDT (offset+bias becomes timestamp+fragment) without
touching a single call site in `editor` or `app`, because none of them ever
see anything but the opaque handle. It does not buy collaboration itself.
Merge semantics (replica ids, version vectors, concurrent-insert ordering)
are a separate, unscoped, much larger initiative.

**One honest limit on that claim, not fixed here [red]**: `Anchor` carries no
buffer identity. `bufferB.resolve(anchor_from_buffer_A)` is not a type error
— it either panics on an out-of-range or tombstoned slot, or, worse, silently
answers with whatever buffer B happens to have at that index. Zed guards this
with `buffer_id` (`anchor.rs:27`, checked at `text.rs:2515` and
`panic_bad_anchor`, `text.rs:2545-2563`). Review considered adding a
debug-only buffer id and declined it: today there is exactly one `Buffer` and
zero anchor consumers, so the field would be paid for by nobody. The doc
comment on `resolve` says anchors are valid only against the buffer that
created them, and this is recorded as a risk below — revisit the moment
`inkra-vault` introduces a second `Buffer`.

### D2 — the transform is a pure function, in its own file

`crates/text/src/anchor.rs`, mirroring the "pure geometry in its own file"
precedent from `2026-09-20-scroll-to-cursor-plan.md` D2.

**The draft's boundary condition was wrong at the trailing edge of a
non-empty replaced span, and it is exactly the kind of off-by-one the scroll
plan's own red team warned this plan to expect [red, blue — found
independently from two lenses].** Concrete counter-example from review:
`"abcde"`, replace `[1, 4)` (`"bcd"`) with `"XY"` → `"aXYe"`. An anchor at
offset 4 — sitting immediately before `'e'`, never inside the removed
text — is `Left`-biased. The draft's condition treated the whole closed
range `[at, touched_end]` as "inside the edit, bias decides," so `Left`
collapsed it to `1`: *before* the inserted `"XY"`, when it started out
*after* the entire replaced span. `Right` happened to land correctly (`3`,
right before `'e'`) by the same rule, which is exactly what let this pass
every listed test — both outcomes looked plausible.

The fix separates two boundaries that the draft conflated:

- `at`, the edit's start, is the **only** boundary a pure insertion has. Bias
  must decide there — that is the whole point of `Bias`, and typing at a
  live anchor is the load-bearing case.
- `touched_end`, the edit's end, is **exclusive** of what was removed —
  Rust's own half-open range convention, matching `replace`'s own
  `range.start..range.end`. An offset sitting exactly there was never inside
  the deleted text. It has no more claim to "bias decides" than an offset
  further right; it should simply shift, the same as anything past the edit.

```rust
pub enum Bias { Left, Right }

fn transform(
    offset: ByteOffset,
    bias: Bias,
    at: ByteOffset,
    removed_len: usize,
    inserted_len: usize,
) -> ByteOffset {
    let touched_end = at.0 + removed_len;
    if offset.0 < at.0 {
        offset
    } else if offset.0 == at.0 || (removed_len > 0 && offset.0 < touched_end) {
        // At the edit's start, or strictly inside a non-empty removed span:
        // bias decides.
        match bias {
            Bias::Left => at,
            Bias::Right => ByteOffset(at.0 + inserted_len),
        }
    } else {
        // Past the edit — either a pure insertion's far side, or at-or-beyond
        // the exclusive end of a removed span. Shifts; bias plays no role,
        // because nothing here was ever ambiguous about before/after.
        ByteOffset(offset.0 - removed_len + inserted_len)
    }
}
```

Re-verified by hand against every case the draft's version was checked
against (composition with delete-then-insert, no underflow — the shift
branch's `offset.0 - removed_len` only runs where `offset.0 >= touched_end
>= removed_len`, so it cannot wrap — round-trip symmetry through
`apply`'s inverted edit, and the pure-deletion case where the two branches
already agreed and still do). The fixed example: offset 4 now takes the
shift branch (`4 - 3 + 2 = 3`) for **both** biases — `"aXY|e"` either way,
which is the only defensible answer for a position that was never inside the
deleted text.

`Bias::Left` still means "stays attached to the text before the edit" and
`Bias::Right` "stays attached to the text after," same convention as Zed's
`anchor_before`/`anchor_after` — confirmed against the vendored source,
restricted deliberately to the insertion-point meaning; Zed's fragment model
resolves replace-boundary behavior differently (via tombstone ordering), so
the equivalence claim does not extend there, and this plan does not lean on
it for the fix above.

**Accepted, not a bug, and not for the reason the draft gave [red]**: an
anchor strictly inside a deleted span collapses to `at` or `at +
inserted_len` and does not return inside the text on undo. The draft claimed
this "would need the fragment/tombstone model D1 explicitly declines" — that
is false. Emacs restores markers through exactly this kind of edit with a
per-edit adjustment entry in its undo list, no CRDT involved; `Buffer`'s
`Edit { at, removed, inserted }` (`buffer.rs:16-20`) is the same shape of
undo representation and could carry the same adjustment. The real reason to
decline it now is scope, not impossibility — recorded here so a future
reader relitigates from the true cost (roughly another transform pass over
the undo path) rather than a false ceiling.

The asymmetry is worth stating precisely, since "collapses and stays there"
oversimplifies: a `Right`-biased anchor at the exact end of a **pure
deletion** (`inserted_len == 0`) *does* round-trip through delete-then-undo —
traced `"abc"`, `Right` anchor at 1, `delete(0..1)` → anchor at 0, undo
(insert `"a"` back at 0) → anchor at `0 + 1 = 1`, restored exactly. That is
different from a `Right`-biased anchor collapsed from *inside* a span, which
moves boundary-to-boundary (`at` → `at + inserted_len` on undo) but is never
inside the restored text. Both are correct; they are not the same case, and
the tests below pin both so a later "fix" cannot silently invert one to match
the other's shape.

### D3 — no separate registry type; `Buffer` gets one field

**The draft's `AnchorRegistry` was a wrapper around a single caller [blue] —
`Buffer` is that caller, and `Buffer` already sets the opposite precedent in
this exact file.** `undo_stack`, `redo_stack`, `open`, `depth`, and
`grouping_blocked` are bare fields on `Buffer` with all logic (`record`,
`close`, `transact_inner`) as private methods directly on it — no `History`
type, even though Zed has one. `create`/`resolve`/`forget` on the draft's
registry were 1:1 delegations of `Buffer`'s own public methods, and `shift`
was a three-line loop: no capability was gained by the extra type, and it
pushed the design further from `buffer-model.md`'s own size estimate. `Bias`,
`Anchor`, `AnchorState`, and the pure `transform` function still get their
own file — that part of the precedent (pure logic, no `Buffer` dependency,
testable standalone, per the scroll plan's D2) holds and is kept.

```rust
// anchor.rs — public/pub(crate) surface only, transform above
struct AnchorState { offset: ByteOffset, bias: Bias }
```

```rust
// buffer.rs
pub struct Buffer {
    text: String,
    undo_stack: Vec<Transaction>,
    redo_stack: Vec<Transaction>,
    open: Option<Vec<Edit>>,
    depth: usize,
    grouping_blocked: bool,
    anchors: Vec<Option<AnchorState>>,
}

impl Buffer {
    pub fn anchor(&mut self, offset: ByteOffset, bias: Bias) -> Anchor { ... }
    #[must_use]
    pub fn resolve(&self, anchor: Anchor) -> ByteOffset { ... }
    pub fn forget(&mut self, anchor: Anchor) { ... }

    fn shift_anchors(&mut self, at: ByteOffset, removed_len: usize, inserted_len: usize) {
        for state in self.anchors.iter_mut().flatten() {
            state.offset = transform(state.offset, state.bias, at, removed_len, inserted_len);
        }
    }
}
```

**Never-reuse is the answer, not an open question [blue] — the draft's own
framing of this as unresolved undersold it.** Any reuse scheme without a
generation counter turns `forget`-then-`resolve` from the deterministic panic
D5 requires into a silent resolve against whatever anchor now occupies the
slot — under this crate's "infallible operations panic on caller bugs"
convention, that is strictly worse than the leak it would fix. The real cost
is not only memory: `shift_anchors` walks every tombstoned slot too
(`iter_mut().flatten()` still visits `None` entries), so per-edit cost is
O(anchors ever created in this buffer's life), not O(live anchors). Same
accepted-cost shape as `previous_grapheme`/`next_grapheme` being O(document)
— "fine for a note, not a repository" (`buffer-model.md:108-117`) — with the
same caveat: there is no consumer yet to generate real load, and revisiting
this is explicitly `inkra-vault`'s or multi-pane's problem to reopen, not
this plan's to solve pre-emptively.

### D4 — two call sites, not one

`Buffer` mutates `self.text` in exactly two places, verified against the
real file: `replace()` (`buffer.rs:153`, `replace_range` at `:172`) and
`apply()` (`buffer.rs:240`, `replace_range` at `:242`), the latter used by
both `undo` and `redo`, with `edit.inverted()` on the undo side. Both already
compute `(at, removed_len, inserted_len)` before mutating `self.text`; call
`self.shift_anchors(at, removed_len, inserted_len)` from both. `replace()`
skips it on its existing no-op early return
(`removed.is_empty() && text.is_empty()`, `buffer.rs:168-170`).

Review hand-traced this against the repo's own multi-edit transaction shape
(`buffer.rs`'s bracket-insertion test: insert `'<'` at 0, insert `'>'` at 5
on `"abcd"`) with an anchor on `'c'`: forward `2 → 3 → 3`; undo replays
inverted edits in `.rev()` order (`buffer.rs:211`), `3 → 3 → 2`, landing
back on `'c'` in `"abcd"` exactly; redo returns it to 3, on `'c'` in
`"<abcd>"`. Confirmed correct, and confirmed **not proven by any test in the
draft's list** — see Tests below.

### D5 — creation asserts, it does not clamp, and it is a real `assert!`

**The draft claimed `Buffer::anchor` uses "the same `debug_assert!` contract
as `replace()`," and that parity claim is false [red — found independently
by both red-team reviews].** `replace()`'s `debug_assert!` is safe *because*
`self.text[range]` (`buffer.rs:167`) and `replace_range` (`:172`) panic
unconditionally in release too — the stdlib slicing is the real backstop,
and `buffer-model.md:34-37` says exactly this: "the stdlib's own slicing
catches the same cases in release, so compiling the assertions out costs
only a less descriptive message." `Buffer::anchor` only stores an offset; it
touches no slice. A `debug_assert!` there fails *open* in release — a
garbage offset is accepted silently, and `resolve` later hands back a
value past `len()` or mid-character with no panic at the point of the actual
bug. This crate already has the correct precedent for that situation:
`undo`/`redo`'s re-entrancy guard is a real `assert!` specifically "because
nothing else catches it" (`buffer-model.md:82-84`). `Buffer::anchor` follows
that rule instead:

```rust
pub fn anchor(&mut self, offset: ByteOffset, bias: Bias) -> Anchor {
    assert!(offset.0 <= self.text.len(), "anchor offset {offset} past end of buffer");
    assert!(self.text.is_char_boundary(offset.0), "anchor offset {offset} not a char boundary");
    ...
}
```

Both checks are O(1); the cost is nil on a creation path. This is
deliberately *not* `Buffer::clip`'s behavior: `clip` exists for readers
known to lag a frame — `DocumentLayout`'s clamped line lookups, per
`CLAUDE.md`'s "a stale read is a caret one frame late, never an index out of
bounds." An anchor is created against the live, current buffer,
synchronously, same as a `replace` call — there is no frame lag to forgive.

## The change

### `crates/text/src/anchor.rs` (new)

`Bias`, `Anchor`, `AnchorState`, `transform`, and the pure unit tests for
`transform` (no `Buffer` needed).

### `crates/text/src/buffer.rs`

- New field `anchors: Vec<Option<AnchorState>>` on `Buffer`, empty in
  `Buffer::new`.
- New public methods `anchor`, `resolve`, `forget` (D1, D5).
- New private method `shift_anchors`, called from `replace()` and `apply()`
  (D4).

### `crates/text/src/lib.rs`

`mod anchor;` and `pub use anchor::{Anchor, Bias};`.

## Tests

Per `CLAUDE.md`'s testing rule for `text`: `#[test]` plus `proptest` over
edit sequences, `.proptest-regressions` committed.

**Pure, in `anchor.rs`** (`transform` directly, no `Buffer`):

1. An offset strictly before the touched span is unchanged.
2. An offset strictly after shifts by `inserted_len - removed_len`, both for
   growing and shrinking edits.
3. A pure insertion (`removed_len == 0`) at an anchor: `Left` stays, `Right`
   moves to `at + inserted_len`.
4. A replace that fully spans an anchor strictly *inside* the removed span:
   `Left` collapses to `at`, `Right` collapses to `at + inserted_len`, for
   both a shrinking and a growing replacement.
5. **`Left` and `Right` at `offset == at` of a non-empty replace** — pins the
   boundary D2's fix touches: `Left → at`, `Right → at + inserted_len`
   (`Right` jumping over the inserted text is the surprising-enough-to-pin
   case).
6. **`Left` and `Right` at `offset == touched_end` of a non-empty replace**
   — the case the draft got wrong: **both** biases give
   `at + inserted_len`, unconditionally. This is the test that would have
   caught D2's original bug on its own.
7. **`Right` at `offset == touched_end` of a pure deletion** — the
   exact-tracking case D4's review trace depends on: gives `at`, matching
   the pure-deletion "both branches agree" property.

Dropped from the draft: the no-op-edit case (`removed_len == 0 &&
inserted_len == 0`) is an identity in every branch by construction and
passes even against a stub that returns its input unchanged — tautological
by this repo's own standard (see the scroll plan's "Dropped as
tautological"). Folded into proptest invariant 13 instead of standing alone.

**On `Buffer`, in `buffer.rs`** (`#[test]`):

8. **An edit strictly before a live anchor shifts it by `inserted_len -
   removed_len`, through the public `Buffer` API.** This is the actual
   wiring test the draft's version of this case ("survives unchanged")
   lacked — an edit placed *after* the anchor would also satisfy "survives
   unchanged" while proving `shift_anchors` is never called from `replace`
   at all.
9. An anchor at the exact insertion point of typed text: `Right` grows with
   it (the common "end of a live selection" case), `Left` does not.
10. **An anchor positioned between two edits inside one `transact` block: undo
    restores its offset.** Pins D4's hand-traced multi-edit case (the
    `<abcd>` bracket example above), which the draft's single-edit-only test
    list never exercised.
11. Undo restores an anchor's offset when the undone edit is strictly before
    it (mirrors test 8: the edit must actually touch the anchor's position
    for the assertion to prove anything).
12. Undo/redo round trip: anchor created, edits applied, undone, redone —
    `resolve` never panics and always returns an offset `<= buffer.len()`
    **and on a char boundary** (tightened to match the proptest invariant
    below; the draft's version asserted less than its own property test).
13. **The accepted-limitation pair, both halves pinned** — not just the
    negative: (a) an anchor strictly inside a deleted span does not return
    inside the text after delete-then-undo (checks both `at` after delete,
    and the correct boundary-to-boundary value after undo, per bias); (b) a
    `Right`-biased anchor at the exact *end* of a pure deletion **does**
    round-trip exactly through delete-then-undo (D2's asymmetry note). Both
    in one test module so a reader sees them as a contrasting pair, not two
    unrelated facts.
14. `forget` then `resolve` panics — the caller-bug contract.
15. `forget` then `forget` again panics too — same contract, not a silent
    no-op; otherwise the behavior depends on which internal representation
    ships, which is exactly what D1's opacity is supposed to prevent from
    mattering.

**Proptest**, over a random sequence of edits interleaved with anchor
creation:

16. The generator produces `insert`/`delete`/`replace` calls at random valid
    positions **wrapped in `transact` for random sub-sequences**, using
    **non-ASCII, multi-byte text** (the char-boundary property is vacuous
    over ASCII-only input), and **interleaving `forget`** on some fraction
    of live anchors. The draft's generator produced only single-edit
    transactions, which never exercises `apply`'s multi-edit reverse-replay
    path — the one `buffer-model.md`'s "range undo and redo return" section
    exists to describe.
17. Per step: every live anchor's `resolve` stays `<= buffer.len()` and on a
    char boundary (subsumes the draft's invariant 12/13, plus the dropped
    no-op case).
18. **A differential check per edit**, not just a bounds check: before the
    edit, record each anchor's offset; after, assert the offset moved
    exactly as `transform` predicts for its recorded `(at, removed_len,
    inserted_len, bias)` — bounds-only checking would pass a build that
    forgot to wire `shift_anchors` into one of the two call sites, as long
    as the buffer only ever grows.
19. **Full-undo round trip**: after undoing every applied edit, every anchor
    that was never inside a removed range resolves to its creation offset
    exactly.
20. **Order preservation**: for two anchors created at `a1 <= a2`, `resolve`
    never inverts their relative order across any sequence of edits and
    undo/redo.

## Docs

- `CLAUDE.md`: add `Buffer::anchor` / `resolve` / `forget` and `Bias` to the
  don't-recreate table, in the same change.
- **`CLAUDE.md:95` and
  `docs/superpowers/specs/2026-09-20-modular-architecture-design.md:228` need
  rewording, not just the additions above [red].** Both currently read as
  "needs anchors" / "not supported until anchors land in `inkra-text`" —
  true today, but this plan makes the literal condition true while the
  actual prerequisite (anchored selections wired through `editor`) stays
  unmet. Reword both to name what's still missing, e.g. "needs anchored
  selections wired through `editor`," so a future session doesn't read
  `Buffer::anchor` existing in the don't-recreate table as license to enable
  a second pane on raw-offset selections.
- `docs/buffer-model.md`: move "Anchors" out of "Deferred" — it is no longer
  that — into a new top-level section alongside Offsets/Editing/Undo,
  describing the shipped shape, D2's corrected accepted-limitation note (the
  Emacs-marker framing, not the false impossibility claim), and the
  file-watching caveat from this plan's opening.
- `docs/roadmap.md`: drop the "Anchors" row from the Editor table; it has no
  estimate today because this plan sizes it.

## Verification

`./scripts/check.sh`. No manual render pass needed — this is `text`, no
`Window`, nothing to look at; the proptest and unit suite are the whole
verification surface for this change.

## Risks

- **`Anchor` carries no buffer identity (D1).** Accepted for now — zero
  consumers, one `Buffer` in the whole tree. Revisit the instant a second
  `Buffer` exists: `inkra-vault` and multi-pane are both multi-buffer by
  definition, and a stray cross-buffer `resolve` is silently wrong, not a
  panic, unless the slot happens to be out of range or tombstoned.
- **Unbounded per-edit iteration over dead anchor slots (D3).** Accepted on
  the same cost philosophy as `previous_grapheme`, with no consumer yet to
  prove or disprove it against. Fix, if it's ever needed, is a
  monotonically-increasing-id design (sorted `Vec<(u64, AnchorState)>`,
  binary search) rather than a generation counter — both were considered and
  are more machinery than this plan's trigger currently justifies.
- **File watching is not actually unlocked by this plan alone.** Stated up
  front and in the Docs updates: whole-buffer reload collapses every anchor;
  `inkra-vault` needs diff-based reload on top of this to keep them.
- **The boundary math in `transform` already had one real bug caught by
  review (D2).** Treat the seven pure-function cases above as a minimum, not
  a ceiling, if this is touched again.
