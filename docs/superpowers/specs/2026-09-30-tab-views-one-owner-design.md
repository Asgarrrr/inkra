# Tab views: an inactive tab holds its own parked view

Status: **draft, reviewed** (2026-09-30), red/blue folded in (see "Review"). A refactor: behaviour
stays identical. **Blocked** until restore-tabs lands on `main` (see
"Depends on restore-tabs").

## Problem

A tab's view lives in one of three places, and the caller decides which:

- The active tab's view is in the one `EditorView`, reached through
  `Shown::Bound` (`crates/app/src/shell.rs:46-76`).
- An inactive tab's view is in `Parked`, a `HashMap<TabId, (EntryId,
  Option<ParkedView>)>` beside `Tabs` (`shell/parked.rs:13`,
  `shell.rs:87-89`).
- A stack or closed entry's view is plain offsets in `Place.view`, inside
  `Tabs<Place>` (`shell/tabs.rs:34-39`).

`Tabs<P>` stores one type `P` everywhere, so it cannot tell a parked place from
a plain one. The shell carries the difference by convention:

- Every op picks a `leaving` closure. `insert`, `activate` and `reopen` must
  pass `park` (`shell/tabs.rs:79`, `:100-102`, `:153`, `:212`); `commit`,
  `back`, `forward` and `close` must pass `capture` (`shell/activation.rs:31`,
  `shell/tabs.rs:126`, `:224`, `:238`). Both closures return `Place`, so a
  wrong pick compiles.
- A place `activate` or `close` returns must go through `arrive`, which
  unparks by tab id (`shell/tabs.rs:134`, `:214`, `:291-294`). A place
  `reopen`, `back` or `forward` returns must not (`shell/tabs.rs:142`).
- `close_tab` must unpark the closed id **before** `Tabs::close` and smuggle
  the view into `resolve` (`shell/tabs.rs:121-128`).
- A note leaving the tabs must hit both tables: `forget_note` calls
  `parked.drop_note` and `tabs.forget` (`shell/tabs.rs:283-288`); calling
  `tabs.forget` alone leaves a stale entry that panics on the next park
  (`shell/parked.rs:18-21`).
- I1 (an entry in `Parked` iff the tab is inactive and on a note) and I2
  (`Tabs` holds no view) are checked only by the shell tests' `check`, which
  reads `shell.tabs` and `shell.parked` directly
  (`shell/tests/mod.rs:292-321`, `Parked::tabs` at `shell/parked.rs:37-42`).

`.claude/rules/shell.md` spends its longest bullet stating these rules. Restore
tabs adds one more exception to them: "`restore` is the one place outside the
`leaving` closures of `insert` and `activate` that parks" (restore-tabs spec,
R7).

**Deletion test on `Parked`.** Delete the table and its complexity does not
spread: it collapses into the inactive tab's slot, which already exists
(`Tab.place: Option<P>`, `crates/tabs/src/lib.rs:66`). The table is a shallow
Module whose one invariant (I1) is kept by its callers. That is the friction,
and it is real.

## Decision

**`Tabs` takes a second place type, `Q`, for what an inactive tab holds.**
`Tabs<P, Q = P>`: stacks and closed entries hold `P` (plain), an inactive
tab's slot holds `Q` (parked). Each op's closure returns the type its op needs,
so the park-or-capture choice is made by the type checker, not by the caller.
The app instantiates `Tabs<Place, Parked>`, where `Parked` is no longer a table
but the value one inactive tab holds. `inkra-tabs` stays gpui-free: `Q` is a
type parameter.

The candidate proposed an app-side wrapper Module owning `Tabs<Place>`, the
parked table and the choice. It fails the deletion test once the types carry
the choice: each wrapper method would be two lines forwarding one fixed
closure. The depth belongs in the Interface of `inkra_tabs::Tabs`, which every
tab op already goes through.

### The Interface of `inkra-tabs`

```rust
/// What a tab shows, as far as history needs it.
pub trait Place: Default {
    type Note: Copy + Eq;
    /// `None`: an empty tab. Never pushed on a stack (invariant 5).
    fn note(&self) -> Option<Self::Note>;
}

pub struct Tab<P, Q = P> { /* place: Option<Q>, back: Stack<P>, forward: Stack<P> */ }

impl<P, Q> Tab<P, Q> {
    /// `None` for the active tab (H8): its place is on screen.
    pub fn place(&self) -> Option<&Q>;
}

pub struct Closed<P, Q = P> {
    /// The plain place the closed stack keeps.
    pub place: P,
    pub index: usize,
    /// The tab now active and the place it held, when the active tab closed.
    pub shown: Option<(TabId, Q)>,
}

impl<P: Place + Clone, Q: Place<Note = P::Note>> Tabs<P, Q> {
    /// Opens a tab at `at` and activates it; the tab left keeps what `park` returns.
    pub fn insert(&mut self, at: usize, park: impl FnOnce() -> Q) -> TabId;
    /// The arriving tab's parked place; `None`, calling no `park`, if `id` is active or no tab.
    pub fn activate(&mut self, id: TabId, park: impl FnOnce() -> Q) -> Option<Q>;
    /// `closing` gets the closed tab's slot, `None` for the active tab (H8),
    /// and returns the plain place the closed stack keeps. Called once.
    ///
    /// # Panics
    /// On a broken H8, before `closing` runs: an inactive tab with no slot, or
    /// the active tab with one. The app's I1 check used to catch this; it lives here now.
    pub fn close(&mut self, id: TabId, closing: impl FnOnce(Option<Q>) -> P)
        -> Option<Closed<P, Q>>;
    pub fn commit(&mut self, id: TabId, capture: impl FnOnce() -> P) -> bool;
    pub fn back(&mut self, showing: Option<P::Note>, capture: impl FnOnce() -> P) -> Option<P>;
    pub fn forward(&mut self, showing: Option<P::Note>, capture: impl FnOnce() -> P) -> Option<P>;
    /// The reopened tab's plain place; the tab left keeps what `park` returns.
    pub fn reopen(&mut self, park: impl FnOnce() -> Q) -> Option<P>;
    /// Replaces every inactive slot on `note` with `Q::default()`, dropping its value.
    pub fn forget(&mut self, note: P::Note);
}
```

The read-only methods (`active`, `active_index`, `count`, `iter`, `id_at`,
`last`, `step`, `move_tab`) keep their signatures. `Place` loses its `Clone`
bound; only `P` needs `Clone`, for the two copies `remember` keeps. `Q` needs
no `Clone`, so a parked view cannot be duplicated and forgotten twice.

`close` takes one closure instead of `leaving` and `resolve`. The two never
both run, and both need `&mut App` in the app: two closures would each borrow
it. The closure gets the tab's slot, but `close`
checks it against `was_active` first, so a broken H8 still panics
(`lib.rs:258-262` today) and never reaches the app as an active tab's `None`:

```rust
let slot = match (was_active, tab.place) {
    (true, None) => None,
    (false, Some(parked)) => Some(parked),
    _ => panic!("H8: a tab's slot is empty exactly when it is active"),
};
let place = closing(slot);
```

Without the check, an inactive tab whose slot was wrongly emptied would make
the app capture the active tab's view into the wrong closed entry.

`Q = P` by default, so the proptest `Fake` harness keeps `Tabs<Fake>`.

### The app's `Parked`

`shell/parked.rs` holds the value, not the table:

```rust
/// An inactive tab's place: its note, and its view kept so edits elsewhere move it (tabs spec, A2).
#[derive(Default)]
pub(super) struct Parked {
    pub(super) note: Option<EntryId>,
    view: Option<ParkedView>,
}

impl inkra_tabs::Place for Parked { type Note = EntryId; /* note */ }

impl Parked {
    /// The active tab's place, its view anchored; seals the undo group (A3).
    pub(super) fn park(shown: &Shown, cx: &mut App) -> Self;
    /// Resolves and forgets the anchors: the plain place to bind.
    pub(super) fn unpark(self, cx: &mut App) -> Place;
    /// The view where its anchors are now, forgetting none (restore-tabs R5).
    pub(super) fn peek(&self, cx: &App) -> Option<ViewState>;
}
```

What a restored tab's `Parked` holds before its buffer loads (restore-tabs
R7's `Kept`) is restore-tabs S2's decision; see "Depends on restore-tabs".

Dropping a `Parked` forgets no anchor. `Tabs::forget` is the one place that
drops one, and it does so on `NoteClosed`, when the buffer is gone, as
`drop_note` does today.

## What moves where

| Today | After |
|---|---|
| `AppShell.parked: Parked` (table) | gone; `AppShell.tabs: Tabs<Place, Parked>` |
| `shell/tabs.rs` `park(parked, tab, shown, cx)` | `Parked::park(shown, cx)`, no table, no tab id |
| `Parked::park/unpark/drop_note/peek` keyed by `TabId` | `Parked::park`, `unpark(self)`, `peek(&self)` on the value |
| `AppShell::arrive` | gone; `activate` and `Closed.shown` return `Parked`, and `show_place` takes a `Place`, so the caller must `unpark` |
| `close_tab`'s unpark before `close`, then `resolve` | one `closing` closure: `None` captures, `Some(parked)` unparks |
| `forget_note`'s `parked.drop_note` | gone; `tabs.forget` drops the value |
| `open_in_new_tab`'s `source` passed to `park` | still read for the T4 check, no longer passed to `park` |
| `Parked::tabs` (test only) and `check`'s I1, I2 | gone |
| restore-tabs `write_session`: `self.parked.peek(tab.id(), cx)` | `place.peek(cx)` |
| restore-tabs S2's restore path, whatever shape it lands in | generalized over `Q` like every other op; its shape stays restore-tabs' |

`capture` stays in `shell/tabs.rs`. `Place` and `ViewState` do not change.

## Invariants now held by types

- **I1** (an inactive tab on a note has a parked view, and no other tab has
  one): the view is the inactive slot's value. No second table can disagree.
- **I2** (no view in a stack or the closed stack is anchored): stacks and
  closed entries are `Stack<P>` and `ClosedTab<P>`; a `Parked` cannot be
  pushed.
- **Park only when a tab goes inactive:** `insert`, `activate` and `reopen`
  accept only a `FnOnce() -> Parked`; `commit`, `back` and `forward` only a
  `FnOnce() -> Place`.
- **Unpark every arrival from an inactive slot:** `activate` and
  `Closed.shown` yield `Parked`, which binds only through `unpark`.
- **Never park one tab twice:** one slot per tab; the `parked twice` panic
  (`shell/parked.rs:20`) goes.
- **Never unpark twice:** `unpark` takes `self` and `Parked` is not `Clone`.

H8 stays a runtime invariant (`Tab::place()` is `None` for the active tab).
The zipper lever in the tabs spec (As built S4) keeps its trigger; this change
removes one of its panic sites.

## Deviations from Zed

None new. Zed's `Pane` holds one item entity per tab
(`workspace/src/pane.rs:405`), and each item keeps its own state, so Zed has
no parked view at all. Inkra's one rebound `EditorView` is the recorded
deviation (tabs spec, A3). This change moves the parked view into the tab it
belongs to, which is closer to Zed's shape.

## Tests

### Executable success criterion, written first

`crates/tabs/src/tests/parked.rs`, a proptest over the existing op strategy
on `Tabs<Fake, Kept>`, where `Kept { note: Option<u8>, serial: u32 }` is
**not** `Clone`. The harness hands each `park` a fresh serial and records it
against the tab left. After every op:

- **K1.** Every inactive tab holds the `Kept` with the serial recorded when it
  was last left; a tab whose note was forgotten, or the fresh last tab, holds
  `Kept::default()`.
- **K2.** `activate(id)` returns the `Kept` recorded for `id`. `close` of an
  inactive `id` hands `closing` that `Kept`; `close` of the active tab hands
  `None`. `Closed.shown` carries the `Kept` recorded for the tab taking over.
- **K3.** `park` runs once per `insert`, per `activate` that changes the
  active tab, and per `reopen` that finds an entry; `capture` once per
  `commit` of the active tab and per `back`/`forward` that arrives;
  `closing` once per `close` that finds its tab. None runs otherwise.

Plus one white-box test outside the proptest, which by construction never
breaks H8:

- **K4.** `close_panics_on_a_broken_h8`: `tests.rs` is a child module, so it
  sets an inactive tab's `place` to `None` directly (no test-only
  constructor) and asserts `close` panics with the H8 message. A second case
  gives the active tab a slot. Deleting the `match` in `close` fails K4.

It fails before the change: `Tabs` takes one type parameter, so the file does
not compile. A plausible bug it then catches: `activate` taking the arriving
slot by the index read before `leave_active`, or `reopen` passing a capture
where the tab left needs a park. `cargo mutants -p inkra-tabs` judges it;
zero missed is the bar.

### Existing tests

- `inkra-tabs` proptests: the harness's `close` wraps its two closures into
  one `closing`; every property stays. No test dies.
- `shell/tests/mod.rs` `check`: I1 and I2 go in S2, not S1 (about 15
  lines); H8, T3 and the `Trashing` check stay. In S1 the `Parked` table
  still exists, so I1 still compares two tables and stays. In S2 there is no
  second table left to compare; the slot-versus-active half of I1 is K4 and
  the panic in `close`, which run always, not only in shell tests. Its doc
  comment says so.
- The behavioural suite is the regression net for slice 2 and does not
  change: `shell/tests/parked.rs` (edits, undo, reload and trash move or drop
  a parked view), `tabs.rs`, `reopen.rs`, `history.rs`, `failed_trash.rs`,
  `gone.rs` (`a_gone_then_restored_note_moves_its_parked_view`),
  `trashing.rs`, and restore-tabs' `restore.rs` (a restored inactive tab
  shows its saved view; the session file follows a parked view).
- `./scripts/test-names.sh` before and after: the only name added is the new
  proptest; none removed.

## Depends on restore-tabs

Design against `main` after `feat/restore-tabs` merges. It depends on:

- `Parked::peek` and `ParkedView::peek` (restore-tabs S1, `shell/parked.rs`
  and `editor/parked.rs` on that branch): they move onto the `Parked` value.
- `write_session`'s match on `tab.place()` (`shell/session.rs:30-36` on that
  branch): its inactive arm reads `place.peek(cx)`.
- restore-tabs S2 (R7: `Kept`, `Tabs::restored`, parking restored tabs) is
  not built; only S1 has landed on `feat/restore-tabs`. This spec freezes no
  shape for it: one hypothetical adapter is not a Seam. Restore-tabs S2's
  author designs it; this spec's author reviews that change and notes that
  a `Q` slot would let `restored` hand back the active `Q` and drop R7's
  "park outside a closure" exception. After it lands, S1 here generalizes
  whatever shape it took over `Q`, like every other op.
- `save_session_later` calls in `shell/tabs.rs`: they stay where they are.

## Plan

Each slice is green under `./scripts/check.sh` on its own.

| Slice | Data | Behaviour | Test | Size |
|---|---|---|---|---|
| S1 | `Tabs<P, Q = P>`, `Tab<P, Q>`, `Closed<P, Q>`; `Place` without `Clone`; `leave_active` generic over what `leaving` returns | `insert`/`activate`/`reopen` take `FnOnce() -> Q`; `close` takes one `closing`; `forget` drops a `Q`; `close` panics on a broken H8 before `closing`; restore-tabs' restore op, if any, generalized over `Q`. The app keeps `Tabs<Place>` (Q = P) and adapts `close_tab` to one closure | K1–K3 proptest and K4 first (red: does not compile); harness `close` adapted; `check`'s I1 stays; `cargo mutants -p inkra-tabs` | ~170 lines, most of it the new test |
| S2 | `Parked` becomes the per-tab value; `AppShell.parked` and `arrive` go; `Tabs<Place, Parked>` | every op passes `Parked::park` or `capture` as its type demands; arrivals `unpark`; `forget_note` drops `drop_note`; restore's parking goes through the slot; `write_session` reads `peek` on the value | `check` loses I1, I2; the behavioural suite unchanged and green; `test-names.sh` diff empty; `shell.md`'s park bullet shrinks to one sentence; tabs spec gains an "As built" note | ~150 lines, net negative |

## Risks

- **A dropped `Parked` leaks its anchor slots.** Today a missed `unpark`
  leaves a stale table entry that panics on the next park. After, a `Parked`
  dropped anywhere but `forget` leaks a few slots silently. The types leave
  one such path, `forget`, where the buffer is gone and nothing needs
  forgetting. A future op that drops a slot must `unpark` it; `#[must_use]`
  on `Parked` and on `activate`'s return flags the obvious case. Slots leak at
  human scale only (tabs spec, A2, Deferred).
- **`close`'s single closure** reads as less obvious than two. Its doc comment
  ties the `None` to H8; K2 pins both arms.
- **Conflict with restore-tabs: files.** Both touch
  `crates/tabs/src/lib.rs` and `shell/parked.rs`. Sequencing after the merge
  avoids it; nothing here is urgent.
- **Conflict with restore-tabs: authorship.** A separate risk from the files:
  who decides the shape of restore's op on `Tabs`. Restore-tabs S2 does; this
  spec only reviews it. Sequencing alone would not prevent this spec from
  pre-deciding it, so the Interface above leaves it out.
- **Performance.** A `HashMap` insert and remove per switch become a slot
  write. No budget moves.

## Out of scope

- **H8 readers branching** (`shell/tab_bar.rs:62`, `shell/tests/mod.rs:247`,
  restore-tabs `shell/session.rs:30`): each still maps `None` to the active
  tab. A shared reader saves three one-liners; the zipper lever is the real
  fix and keeps its trigger.
- **The pending open, T4** (`activation`, `shell/activation.rs:18-37`,
  `shell/tabs.rs:84-108`, `:310-314`): it is about which landing wins, not
  where a view lives. `open_note` and `open_in_new_tab` still duplicate the
  spawn; a separate candidate.
- **`Shown` against `NoteStore::current` and backlinks** (`set_shown` then
  `set_current` or `clear_current`, `shell/activation.rs:107-118`, `:165-173`,
  `shell/tabs.rs:318-332`): a separate candidate on what the screen shows.
- **`EditorView`'s view gates** (`open_buffer`, `restore`, `capture`,
  `view_state`, `park`) and `bind`'s choice among them
  (`shell/activation.rs:139-175`): unchanged; `Parked::park` still calls
  `EditorView::park`, and the A3 undo seal stays in `capture`.
- **An app-side wrapper Module over `Tabs`**: rejected in Decision.

## Review

Red/blue, 2026-09-30. All three findings confirmed; red and blue agreed on
each, checked against `crates/tabs/src/lib.rs:244-290` and
`feat/restore-tabs` (S2 not started).

| Finding | Outcome |
|---|---|
| F1-h8-invariant-loses-detection | **Fixed.** `close` matches `(was_active, slot)` and panics on a broken H8 before `closing` runs. Blue proposed `assert_eq!(slot.is_none(), was_active)`; the `match` is adopted instead because it yields the slot in the same expression. The closure signature stays. |
| F2-safety-net-removed-and-cant-catch-it | **Fixed.** K4 corrupts a slot directly from the child test module and asserts the panic. `check`'s I1 stays through S1 and goes in S2 only, when there is no second table to compare. |
| F3-restored-interface-frozen-ahead-of-restore-tabs-s2 | **Fixed.** `Tabs::restored`, `Kept` and `Parked::plain` leave this spec. Restore-tabs S2 owns them; this spec's author reviews that change. Risks names authorship apart from file overlap. |

Improvements: the H8 fix inside S1's `close`, K4, the doc comment on `close`
naming where I1's check went, dropping `restored`, S1's Interface limited to
the ops it ships, and the split risk entry are all adopted. The candidate
stands: the deletion test on `Parked` holds, and none of the findings touch
the `Q` type parameter itself.
