# Tab views: an inactive tab holds its own parked view

Status: **draft, reviewed** (2026-09-30), red/blue folded in (see "Review").
Refreshed 2026-10-01 against `arch/no-vault-tabs` (restore-tabs #47 and
no-vault-mode landed). A refactor: behaviour stays identical. Unblocked.

## Problem

A tab's view lives in one of three places, and the caller decides which:

- The active tab's view is in the one `EditorView`, reached through
  `Shown::Bound` (`crates/app/src/shell.rs:47-77`).
- An inactive tab's view is in `Parked`, a `HashMap<TabId, (EntryId,
  Option<ParkedView>)>` beside `Tabs` (`shell/parked.rs:13`,
  `shell.rs:87-89`).
- A stack or closed entry's view is plain offsets in `Place.view`, inside
  `Tabs<Place>` (`shell/tabs.rs:34-39`).

`Tabs<P>` stores one type `P` everywhere, so it cannot tell a parked place from
a plain one. The shell carries the difference by convention:

- Every op picks a `leaving` closure. `insert`, `activate` and `reopen` must
  pass `park` (`shell/tabs.rs:79`, `:100-102`, `:154`, `:214`); `commit`,
  `back`, `forward` and `close` must pass `capture` (`shell/activation.rs:31`,
  `shell/tabs.rs:126`, `:226`, `:240`). Both closures return `Place`, so a
  wrong pick compiles.
- A place `activate` or `close` returns must go through `arrive`, which
  unparks by tab id (`shell/tabs.rs:134`, `:216`, `:285-288`). A place
  `reopen`, `back` or `forward` returns must not (`shell/tabs.rs:143`).
- `close_tab` must unpark the closed id **before** `Tabs::close` and smuggle
  the view into `resolve` (`shell/tabs.rs:121-128`).
- A note leaving the tabs must hit both tables: `forget_note` calls
  `parked.drop_note` and `tabs.forget` (`shell/tabs.rs:277-282`); calling
  `tabs.forget` alone leaves a stale entry that panics on the next park
  (`shell/parked.rs:18-21`).
- `land_restore` writes the table directly, outside any `leaving` closure: it
  replaces `self.tabs` with `Tabs::restored`, then parks each inactive tab by
  id (`shell/session.rs:205-214`). Its correctness rests on the table being
  empty first, which `holds_nothing` guarantees.
- I1 (an entry in `Parked` iff the tab is inactive and on a note) and I2
  (`Tabs` holds no view) are checked only by the shell tests' `check`, which
  reads `shell.tabs` and `shell.parked` directly
  (`shell/tests/mod.rs:295-328`, `Parked::tabs` at `shell/parked.rs:42-47`).

`.claude/rules/shell.md` spends its longest bullet stating these rules. Restore
tabs added one more exception to them: park "in `land_restore`
(`shell/session.rs`), which parks each restored inactive tab".

**Deletion test on `Parked`.** Delete the table and its complexity does not
spread: it collapses into the inactive tab's slot, which already exists
(`Tab.place: Option<P>`, `crates/tabs/src/lib.rs:28`). The table is a shallow
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

The ops live in three files today: `lib.rs` (`restored`, `insert`, `activate`,
`leave_active`), `closed.rs` (`Closed`, `ClosedTab`, `close`, `commit`,
`reopen`) and `history.rs` (`back`, `forward`, `forget`, `Stack`).

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

impl<P: Place, Q: Place<Note = P::Note>> Tabs<P, Q> {
    /// Tabs on `places` in bar order; the caller shows `active`'s (H8), and its `Q` is dropped.
    pub fn restored(places: Vec<Q>, active: usize) -> Self;
    /// Opens a tab at `at` and activates it; the tab left keeps what `park` returns.
    pub fn insert(&mut self, at: usize, park: impl FnOnce() -> Q) -> TabId;
    /// The arriving tab's parked place; `None`, calling no `park`, if `id` is active or no tab.
    pub fn activate(&mut self, id: TabId, park: impl FnOnce() -> Q) -> Option<Q>;
    pub fn commit(&mut self, id: TabId, capture: impl FnOnce() -> P) -> bool;
    pub fn back(&mut self, showing: Option<P::Note>, capture: impl FnOnce() -> P) -> Option<P>;
    pub fn forward(&mut self, showing: Option<P::Note>, capture: impl FnOnce() -> P) -> Option<P>;
    /// The reopened tab's plain place; the tab left keeps what `park` returns.
    pub fn reopen(&mut self, park: impl FnOnce() -> Q) -> Option<P>;
    /// Replaces every inactive slot on `note` with `Q::default()`, dropping its value.
    pub fn forget(&mut self, note: P::Note);
}

impl<P: Place + Clone, Q: Place<Note = P::Note>> Tabs<P, Q> {
    /// `closing` gets the closed tab's slot, `None` for the active tab (H8),
    /// and returns the plain place the closed stack keeps. Called once.
    ///
    /// # Panics
    /// On a broken H8, before `closing` runs: an inactive tab with no slot, or
    /// the active tab with one. The app's I1 check used to catch this; it lives here now.
    pub fn close(&mut self, id: TabId, closing: impl FnOnce(Option<Q>) -> P)
        -> Option<Closed<P, Q>>;
}
```

`restored` keeps the shape restore-tabs gave it (R7, `lib.rs:154-192`); only
its element type moves from `P` to `Q`, since every place it stores is an
inactive tab's slot. The read-only methods (`active`, `active_index`, `count`,
`iter`, `id_at`, `last`, `step`, `move_tab`, `has_closed`) keep their
signatures. `Place` loses its `Clone` bound (`lib.rs:15`); only `P` needs
`Clone`, for the two copies `remember` keeps, so the bound sits on `close`'s
impl block alone (`closed.rs:61` is the one clone of a `P`). `Q` needs no `Clone`, so a parked
view cannot be duplicated and forgotten twice. The fresh tab `close` pushes
when the last tab closes holds `Q::default()` (`closed.rs:67-74` today pushes
`P::default()`); the same call takes it back out as `Closed.shown`, so no
caller sees that tab inactive.

`close` takes one closure instead of `leaving` and `resolve`. The two never
both run, and both need `&mut App` in the app: two closures would each borrow
it. The closure gets the tab's slot, but `close`
checks it against `was_active` first, so a broken H8 still panics
(`closed.rs:55-59` today) and never reaches the app as an active tab's `None`:

```rust
let slot = match (was_active, tab.place) {
    (true, None) => None,
    (false, Some(parked)) => Some(parked),
    _ => panic!("H8: a tab's slot is empty exactly when it is active"),
};
let place = closing(slot);
```

Without the check, an inactive tab whose slot was wrongly emptied would make
the app capture the active tab's view into the wrong closed entry. Today's
`(Some(left), _)` arm also ignores a slot on the active tab; the `match`
rejects it.

`Q = P` by default, so the proptest `Fake` harness keeps `Tabs<Fake>`, and
`restored.rs`'s proptests keep `Tabs::restored(Vec<Fake>, _)`.

### The app's `Parked`

`shell/parked.rs` holds the value, not the table:

```rust
/// An inactive tab's place: its note, and its view kept so edits elsewhere move it (tabs spec, A2).
#[derive(Default)]
pub(super) struct Parked {
    note: Option<EntryId>,
    view: Option<ParkedView>,
}

impl inkra_tabs::Place for Parked { type Note = EntryId; /* note */ }

impl Parked {
    /// A place on `note`, its view already anchored; `None` if its note was being trashed.
    pub(super) fn new(note: EntryId, view: Option<ParkedView>) -> Self;
    /// The active tab's place, its view anchored; seals the undo group (A3).
    pub(super) fn park(shown: &Shown, cx: &mut App) -> Self;
    /// Resolves and forgets the anchors: the plain place to bind.
    pub(super) fn unpark(self, cx: &mut App) -> Place;
    /// The view where its anchors are now, forgetting none (restore-tabs R5).
    pub(super) fn peek(&self, cx: &App) -> Option<ViewState>;
}
```

`Parked::new` is the one constructor with a view. `park` calls it with
`EditorView::park`'s anchors; `land_restore` calls it with
`ParkedView::new(arrival.buffer, view, cx)` (`editor/parked.rs:19`), as it
builds the table entry today. Restore-tabs R7's first design, a restored
inactive tab with a plain view and no buffer, was dropped in its "As built
(S2)": every restored note loads before landing, so every restored `Parked`
is anchored like any other. No `Kept` type or second slot state exists, and
this spec needs none.

`note` is private, and readers call `inkra_tabs::Place::note` (S2 review,
Q1). The first sketch made it `pub(super)`. Then code in `shell` could
change the note and leave the view anchored in the old note's buffer.

Dropping a `Parked` forgets no anchor. `Tabs::forget` is the one place that
drops an anchored one, and it does so on `NoteClosed`, when the buffer is
gone, as `drop_note` does today. `Tabs::restored` drops the active entry's
`Q`; `land_restore` puts a `Parked::default()` there, which holds no anchor.

## What moves where

| Today | After |
|---|---|
| `AppShell.parked: Parked` (table, `shell.rs:88-89`, `:154`) | gone; `AppShell.tabs: Tabs<Place, Parked>` |
| `shell/tabs.rs` `park(parked, tab, shown, cx)` (`:59-72`) | `Parked::park(shown, cx)`, no table, no tab id |
| `Parked::park/unpark/drop_note/peek` keyed by `TabId` | `Parked::new`, `park`, `unpark(self)`, `peek(&self)` on the value |
| `AppShell::arrive` (`shell/tabs.rs:284-288`) | gone; `activate` and `Closed.shown` return `Parked`, and `show_place` takes a `Place`, so the caller must `unpark` |
| `close_tab`'s unpark before `close`, then `resolve` | one `closing` closure: `None` captures, `Some(parked)` unparks |
| `forget_note`'s `parked.drop_note` | gone; `tabs.forget` drops the value |
| `open_in_new_tab`'s `source` passed to `park` | still read for the T4 check, no longer passed to `park` |
| `land_restore`: `Place`s into `Tabs::restored`, then `self.parked.park(tab.id(), ..)` per inactive tab (`shell/session.rs:196-214`) | one `Vec<Parked>` into `Tabs::restored`: `Parked::new(arrival.note, view)` per inactive tab, `Parked::default()` at `active`; no table write, no id lookup |
| `write_session`'s inactive arm: `parked.peek(tab.id(), cx)` (`shell/session.rs:115`) | `place.peek(cx)`; the `parked` borrow at `:107` goes |
| `Parked::tabs` (test only) and `check`'s I1, I2 | gone |

`capture` stays in `shell/tabs.rs`. `Place` and `ViewState` do not change.
`ParkedView` (`editor/parked.rs`) does not change.

## Invariants now held by types

- **I1** (an inactive tab on a note has a parked view, and no other tab has
  one): the view is the inactive slot's value. No second table can disagree.
- **I2** (no view in a stack or the closed stack is anchored): stacks and
  closed entries are `Stack<P>` and `ClosedTab<P>`; a `Parked` cannot be
  pushed.
- **Park only when a tab goes inactive:** `insert`, `activate` and `reopen`
  accept only a `FnOnce() -> Parked`; `commit`, `back` and `forward` only a
  `FnOnce() -> Place`. `restored` accepts only `Parked` values.
- **Unpark every arrival from an inactive slot:** `activate` and
  `Closed.shown` yield `Parked`, which binds only through `unpark`.
- **Never park one tab twice:** one slot per tab; the `parked twice` panic
  (`shell/parked.rs:20`) goes.
- **Never unpark twice:** `unpark` takes `self` and `Parked` is not `Clone`.

H8 stays a runtime invariant (`Tab::place()` is `None` for the active tab).
The zipper lever in the tabs spec (As built S4) keeps its trigger, "a third H8
panic site". This change does not fire it: `close` keeps its two H8 panics
(`closed.rs:58` becomes the `match`, `:81` stays), and `restored` sets H8 by
construction.

## Deviations from Zed

None new. Zed's `Pane` holds one item entity per tab
(`workspace/src/pane.rs:405`), and each item keeps its own state, so Zed has
no parked view at all. Inkra's one rebound `EditorView` is the recorded
deviation (tabs spec, A3). This change moves the parked view into the tab it
belongs to, which is closer to Zed's shape.

## Tests

### Executable success criterion, written first

`crates/tabs/src/tests/parked.rs`, a proptest over the existing op strategy
(`tests.rs`, `op()`) on `Tabs<Fake, Kept>`, where `Kept { note: Option<u8>,
serial: u32 }` is **not** `Clone`. The test-only `Kept` is local to this file;
no production type has that name. The harness starts from `Tabs::new()` or
from `Tabs::restored` over fresh `Kept`s, and records each restored inactive
tab's serial. It hands each `park` a fresh serial and records it against the
tab left. After every op:

- **K1.** Every inactive tab holds the `Kept` with the serial recorded when it
  was last left or restored; a tab whose note was forgotten holds
  `Kept::default()`. The harness records `Kept::default()` for a tab id it
  never parked, so K2 has an expected value when the fresh last tab takes
  over; `close` makes that tab active before it returns, so K1 never checks
  it inactive.
- **K2.** `activate(id)` returns the `Kept` recorded for `id`. `close` of an
  inactive `id` hands `closing` that `Kept`; `close` of the active tab hands
  `None`. `Closed.shown` carries the `Kept` recorded for the tab taking over.
- **K3.** `park` runs once per `insert`, per `activate` that changes the
  active tab, and per `reopen` that finds an entry; `capture` once per
  `commit` of the active tab and per `back`/`forward` that arrives;
  `closing` once per `close` that finds its tab. None runs otherwise.

Plus one white-box test outside the proptest, which by construction never
breaks H8:

- **K4.** `close_panics_on_a_broken_h8`: `tests` is a child module of the
  crate root, so it sets an inactive tab's private `place` to `None` directly
  (no test-only constructor) and asserts `close` panics with the H8 message.
  A second case gives the active tab a slot. Deleting the `match` in `close`
  fails K4.

It fails before the change: `Tabs` takes one type parameter, so the file does
not compile. A plausible bug it then catches: `activate` taking the arriving
slot by the index read before `leave_active`, or `reopen` passing a capture
where the tab left needs a park. `cargo mutants -p inkra-tabs` judges it;
zero missed is the bar.

### Existing tests

- `inkra-tabs` proptests: the harness's `close` (`tests/harness.rs:69-81`)
  wraps its `leaving` and `resolved` into one `closing`; `Tabs::check`
  (`tests.rs:58-91`) is generalized over `Q`; every property stays. No test
  dies.
- `shell/tests/mod.rs` `check` (`:295-328`): I1 (`:311-317`) and I2
  (`:318-325`) go in S2, not S1 (about 15 lines); H8, T3 and the `Trashing`
  check stay. In S1 the `Parked` table still exists, so I1 still compares two
  tables and stays. In S2 there is no second table left to compare; the
  slot-versus-active half of I1 is K4 and the panic in `close`, which run
  always, not only in shell tests. Its doc comment says so.
- The behavioural suite is the regression net for slice 2 and does not
  change: `shell/tests/parked.rs` (edits, undo, reload and trash move or drop
  a parked view), `tabs.rs`, `reopen.rs`, `history.rs`, `failed_trash.rs`,
  `gone.rs` (`a_gone_then_restored_note_moves_its_parked_view`),
  `trashing.rs`; restore-tabs' `restore_lands.rs`
  (`a_restored_inactive_tab_shows_its_saved_view_when_activated`,
  `a_restored_inactive_view_follows_an_edit_made_in_another_tab`,
  `two_inactive_tabs_on_one_note_keep_their_own_views`) and
  `session_writes.rs`
  (`a_parked_view_is_saved_after_an_edit_made_in_another_tab`,
  `an_inactive_tab_closed_is_saved`,
  `an_inactive_tab_emptied_by_a_trash_is_saved`).
- `./scripts/test-names.sh` before and after: the only names added are the
  new proptest and K4; none removed.

## Landed dependencies

Restore-tabs merged as #47; no-vault-mode landed on `arch/no-vault-tabs`.
What this spec builds on, as the code stands:

- `Parked::peek` (`shell/parked.rs:32-34`) and `ParkedView::peek`
  (`editor/parked.rs:49`): `Parked::peek` moves onto the value.
- `write_session`'s match on `tab.place()` (`shell/session.rs:111-116`): its
  inactive arm reads `place.peek(cx)`.
- `Tabs::restored(places: Vec<P>, active)` (`lib.rs:154-192`) and
  `land_restore` (`shell/session.rs:177-220`). Restore-tabs S2 decided the
  shape; S1 here only changes the element type to `Q`, and S2 here moves the
  parks into the `Vec`. The shape stays restore-tabs'.
- `Restore::Landing` holds the loads outside `activation`. It touches no
  parked view; nothing here changes it.
- `save_session_later` calls in `shell/tabs.rs`: they stay where they are.
- No-vault mode: `AppShell.panes` is `Option<VaultPanes>`, and a launch with
  no vault has no `SessionFile`, so `land_restore` never runs there. Tab ops,
  `Parked` and `forget_note` do not read `panes`; nothing here changes for a
  detached note.

## Plan

Each slice is green under `./scripts/check.sh` on its own.

| Slice | Data | Behaviour | Test | Size |
|---|---|---|---|---|
| S1 | `Tabs<P, Q = P>`, `Tab<P, Q>`, `Closed<P, Q>` (`closed.rs`); `Place` without `Clone`; `leave_active` generic over what `leaving` returns | `insert`/`activate`/`reopen` take `FnOnce() -> Q`; `restored` takes `Vec<Q>`; `close` takes one `closing`; `forget` and `close`'s fresh last tab use `Q::default()`; `close` panics on a broken H8 before `closing`. The app keeps `Tabs<Place>` (Q = P) and adapts `close_tab` to one closure | K1–K3 proptest and K4 first (red: does not compile); harness `close` and `Tabs::check` adapted; `check`'s I1 stays; `cargo mutants -p inkra-tabs` | ~180 lines, most of it the new test |
| S2 | `Parked` becomes the per-tab value with `new`; `AppShell.parked` and `arrive` go; `Tabs<Place, Parked>` | every op passes `Parked::park` or `capture` as its type demands; arrivals `unpark`; `forget_note` drops `drop_note`; `land_restore` hands `Tabs::restored` a `Vec<Parked>`; `write_session` reads `peek` on the value | `check` loses I1, I2; the behavioural suite unchanged and green; `test-names.sh` diff empty; `shell.md`'s park bullet shrinks to one sentence and its `land_restore` exception goes; tabs spec and restore-tabs spec gain an "As built" note | ~150 lines, net negative |

## Risks

- **A dropped `Parked` leaks its anchor slots.** Today a missed `unpark`
  leaves a stale table entry that panics on the next park. After, a `Parked`
  dropped anywhere but `forget` leaks a few slots silently. The types leave
  two drop paths: `forget`, where the buffer is gone and nothing needs
  forgetting, and `restored`'s active entry, which `land_restore` fills with
  `Parked::default()`. A future op that drops a slot must `unpark` it;
  `#[must_use]` on `Parked` and on `activate`'s return flags the obvious case.
  Slots leak at human scale only (tabs spec, A2, Deferred).
- **`close`'s single closure** reads as less obvious than two. Its doc comment
  ties the `None` to H8; K2 pins both arms.
- **Conflict with restore-tabs (resolved).** Restore-tabs landed first, as
  planned. Its S2 decided `restored`'s shape; this spec only generalizes it,
  so the authorship risk is gone. Both specs' "As built" notes record the
  move of `land_restore`'s parks.
- **Performance.** A `HashMap` insert and remove per switch become a slot
  write. No budget moves.

## Out of scope

- **H8 readers branching** (`shell/tab_bar.rs:62`, `shell/tests/mod.rs:251`,
  `shell/session.rs:111-116`): each still maps `None` to the active tab. A
  shared reader saves three one-liners; the zipper lever is the real fix and
  keeps its trigger.
- **The pending open, T4** (`activation`, `shell/activation.rs:18-37`,
  `shell/tabs.rs:84-108`, `:304-308`): it is about which landing wins, not
  where a view lives. `open_note` and `open_in_new_tab` still duplicate the
  spawn; a separate candidate.
- **`Shown` against `NoteStore::current` and backlinks** (`set_shown` then
  `set_current` or `clear_current`, `shell/activation.rs:119-138`, `:197-206`,
  `shell/tabs.rs:312-326`): a separate candidate on what the screen shows.
- **`EditorView`'s view gates** (`open_buffer`, `restore`, `capture`,
  `view_state`, `park`) and `bind`'s choice among them
  (`shell/activation.rs:170-207`): unchanged; `Parked::park` still calls
  `EditorView::park`, and the A3 undo seal stays in `capture`.
- **`restored` handing back the active `Q`** instead of dropping it: the
  active place reaches `bind` through `Restored.shown`, never through `Tabs`,
  so a returned `Q` would have no reader.
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

Refresh (2026-10-01): re-validated against `arch/no-vault-tabs` after
restore-tabs #47 and no-vault-mode. Every file:line updated: `close`,
`commit` and `reopen` now live in `crates/tabs/src/closed.rs`, `back`,
`forward` and `forget` in `history.rs`. Restore-tabs S2 landed as
`Tabs::restored(Vec<P>, usize)` plus direct table parks in `land_restore`;
R7's `Kept` was dropped there (every restored note loads first), so no `Kept`
or second slot state exists. The Interface now includes `restored` over
`Vec<Q>` (shape unchanged); `Parked` gains `new` for `park` and
`land_restore`; `land_restore`'s table parks move into the `Vec` in S2, and
its `shell.md` exception goes. The fresh last tab in `close` holds
`Q::default()`. Corrected the claim that this change removes an H8 panic site:
`close` keeps two, so the zipper trigger does not fire. Added `restored`'s
dropped active entry as a second drop path in Risks, and a restored start to
the K1 harness. No-vault mode touches no tab or parked code. The design holds
unchanged: `Tabs<P, Q = P>`, the single `closing`, and the two slices.

Refresh red/blue (2026-10-01). Both findings confirmed; red and blue agreed,
checked against `crates/tabs/src/closed.rs:45-89`.

| Finding | Outcome |
|---|---|
| K1-fresh-last-tab-unobservable | **Fixed.** K1 drops the fresh last tab: `close` pushes it and takes it back as `Closed.shown` in one call. The harness's default for a never-parked id now carries that case, for K2. The Interface prose says so. |
| interface-clone-bound-overbroad | **Fixed.** The Interface splits into two impl blocks; `P: Clone` bounds only `close`, the one op that clones a `P` (`closed.rs:61`). |

Neither finding changes the slicing: both edit S1's Interface and test text only.
