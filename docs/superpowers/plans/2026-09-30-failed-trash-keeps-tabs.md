# A failed trash keeps its tabs

Roadmap item: "A trash that fails after a tab switch reached its note leaves
the note open and its tabs empty." Spec of the area:
`docs/superpowers/specs/2026-09-29-tabs-design.md`.

## The defect

`AppShell::show` (`crates/app/src/shell/tabs.rs`) binds the arriving place's
note through `NoteStore::buffer_for`. For a note being trashed,
`buffer_for` returns `None` (`openable`, `vault/store.rs`), so `show` calls
`forget_note`: every tab, back, forward and closed entry on the note empties,
and its parked views drop. This happens when the trash **begins**. If the trash
then fails (`TrashFailed`), the store keeps the note open (`trash_failed` puts
it back to `Open`), but no tab, stack or parked view names it any more.

Reproduced by two failing tests, already in `shell/tests/tabs.rs`:

- `a_failed_trash_after_a_switch_reached_its_note_shows_it_again`: tabs
  `[b, a]`, `a` dirty with its save held, switch to `b`, trash `a` (made
  untrashable), switch back to `a`'s tab, release. Expected
  `([Some(b), Some(a)], 1)`, `showing() == Some(a)`, text `"alphax"`. Today
  `([Some(b), None], 1)`.
- `a_failed_trash_keeps_its_note_in_inactive_tabs_and_history`: `a` in tab 1's
  back stack and in tab 2, trash `a`, `⌘[` onto `a` in tab 1, fail. Expected
  both tabs on `a`, and `⌘]` still reaches `b`. Today `([None, None], 0)`.

## Facts the design rests on

- F1. A note's session closes only in `trash_done`, which emits `NoteClosed`
  (`vault/tree_ops.rs`). A tab place names a note that was bound, so its
  lifecycle is `Open` or `Trashing(Some(..))`, never `Closed` or
  `Trashing(None)`. Hence: for a place's note, `buffer_for == None` exactly
  when the note is being trashed.
- F2. `TrashFailed(id)` is emitted after `trash_failed` restored `Open`, so
  `buffer_for(id)` is `Some` in its handler. Nothing in `app` subscribes to it
  today.
- F3. `NoteClosed(id)` is emitted after `current` is cleared and the session
  closed.

## The rule

**A note leaves the tabs only on `NoteClosed`.** While its trash is pending, a
tab keeps the note. The active tab on such a note shows no editor, so nothing
can be typed into a note whose file is about to go (the reason `openable`
hides it). The outcome decides: `NoteClosed` forgets the note everywhere (as
today); `TrashFailed` shows it again in the active tab if that tab is still
on it.

## Data shape

Replace `AppShell.bound: Option<Bound>` with `shown: Shown`:

```rust
/// What the active tab shows; only `Bound` has an editor (T2).
enum Shown {
    Empty,
    Bound(Bound),
    /// The note's trash is pending: no editor until `NoteClosed` or `TrashFailed`.
    Trashing(EntryId),
}
```

The pair "no editor, but the tab is on a note" cannot be written any other
way, and `Bound` stays the only holder of an editor. Two readers:

- `showing()` keeps its meaning: the note **bound** in the editor (`Bound`
  only). It stays the one T3 compares with `NoteStore::current`, and the one
  for backlinks, recent, the sidebar highlight, the gone banner, following a
  link and quick open.
- New `active_note()`: `Bound` or `Trashing`. It is what the active tab is
  on: `capture`, `park`, `forget_note`'s active check, `tab_titles`, and the
  `showing` argument of `Tabs::back`/`forward`.

`set_showing(Option<Bound>, ..)` becomes `set_shown(Shown, ..)`, still the one
writer; its side effects (recent, sidebar follow) run for `Bound` only, and
backlinks get `showing()`.

Parked (I1): `park` has no view to park for a tab left while `Trashing`.
`Parked`'s entry becomes `(EntryId, Option<ParkedView>)`, so I1 stays an
exact "an entry iff the tab is inactive and shows a note". `unpark` flattens.
After a failed trash, a tab left while `Trashing` opens its note at the top:
its view was already resolved when the tab arrived and cannot be re-anchored
without an editor. Accepted: a failed trash is rare.

Rejected at the plan review: parking nothing and relaxing I1 to "unless the
note is trashing". After a failed trash the note is `Open` again while the tab
left still has no entry, so the relaxed I1 breaks exactly then. An enum
`NoteOnly | Viewed` holds the same information as the `Option` and adds a type.

`capture` and `park` take `&Shown` instead of `Option<&Bound>`, at their eight
call sites: `capture` in `close_tab`, `go_back`, `go_forward` and
`open_note`'s `commit`; `park` in `new_tab`, `open_in_new_tab`,
`reopen_closed_tab` and `activate_tab`.

## Behaviour, path by path

1. `show(place)`:
   - `place.note` with a buffer → `bind` (unchanged).
   - `place.note` with no buffer (F1: trashing) → `unfocus_editor`,
     `set_shown(Trashing(id))`, `clear_current`. **No `forget_note`.** The view
     `arrive` unparked is dropped.
   - no note → `Empty`, as today.
   `show` still drops `activation` first (T4, the previous change).
2. `NoteClosed(id)`: `forget_note(id)` (tabs, stacks, closed, parked), and if
   `active_note() == Some(id)` then `show(Place::default())`. Unchanged apart
   from `active_note`.
3. New `TrashFailed(id)` subscription (`subscribe_in`, as `NoteClosed`): if
   `shown` is `Trashing(id)`, `show(Place { note: Some(id), view: None })`.
   Focus, exactly: `if self.focus_handle.is_focused(window) ||
   self.modal.is_some()` then `land_focus` and focus it. The root holds the
   focus when the `Trashing` tab took it there through `show_place`; with a
   modal open, `land_focus` only retargets the dismissal. A trash started from
   the tree keeps the tree's focus. Otherwise nothing: inactive tabs already kept the
   note and their parked views.
4. Leaving a `Trashing` tab (switch, new tab, close, back, commit of a
   landing): `capture`/`park` yield `Place { note: Some(id), view: None }`;
   `park` stores `(id, None)`.
5. Arriving again while still trashing → `Trashing` again. After a failure →
   binds at the top.
6. Opening the trashing note (tree, picker, link): `open_note` resolves
   `None` for a trashing note, so the landing returns; `activation` is still
   replaced. Unchanged.

## Tests

Red/blue on this plan: red found no blocker; blue confirmed four claims.
Test-first. The two failing tests above are the criterion. Also:

- Update the three "being trashed shows it empty" tests
  (`switching_to_…`, `going_back_to_…`, `reopening_a_tab_…`): during the trash
  `tab_notes` now reports `Some(a)` for the active tab, `showing()` stays
  `None`; after `finish_trash`, `None` as before.
- `tab_notes` and `tab_titles` read `active_note()` for the active tab.
- `check` gains: `Shown::Trashing(id)` ⇒ `buffer_for(id).is_none()`. I1
  stays exact: an entry iff the tab is inactive and on a note.
- New: `back_from_a_trashing_tab_skips_its_note` (back `[X, A]`, active
  `Trashing(A)` reached by `⌘[`… or a switch; `⌘[` lands on `X`). Passing
  `showing()` instead of `active_note()` to `Tabs::back` fails it.
- New: `closing_onto_a_trashing_tab_shows_it_empty_then_again` (close the
  active tab so a `Trashing` tab becomes active; fail; it binds).
- New: `a_new_tab_from_a_trashing_tab_keeps_it` (`⌘`-click a note from a
  `Trashing` tab; fail; the tab left binds when switched to).
- New: `a_failed_trash_gives_the_editor_the_focus_the_root_held` (root focused
  after the `Trashing` switch → editor focused after the failure).
- New: `a_failed_trash_from_the_tree_keeps_the_tree_focused` (trash started
  from the tree while its note is bound: nothing moves).
- New: `leaving_a_trashing_tab_and_failing_opens_it_at_the_top` (I1 with a
  `None` view).
- New: the tab title of a `Trashing` tab is the note's name.
- Guard: removing the `TrashFailed` subscription fails the first test;
  restoring `forget_note` in `show` fails both.

## Out of scope

- The active tab bound to a note when its trash begins stays typable until
  the outcome (today's behaviour).
- The rename field's focus loss (separate roadmap item).

## Docs

- Roadmap: remove the item.
- Tabs spec, "As built": the rule, `Shown`, `active_note` vs `showing`, the
  `Option<ParkedView>` and its accepted loss.
