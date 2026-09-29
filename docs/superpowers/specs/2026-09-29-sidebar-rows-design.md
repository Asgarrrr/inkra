# Sidebar rows own the edit slot

Date: 2026-09-29. Status: **implemented**.

A refactor from the architecture review. Behaviour stays identical.

## Context

`Visible` (`sidebar/visible.rs`) is pure and tested with plain `#[test]`s. The
inline name field is not. It is an overlay on `Visible` that lives on
`Sidebar`, behind `Context`, split across three files:

- `EditState` (`sidebar/edit.rs:11-22`) mixes gpui state (`field`,
  `_subscriptions`, `rejected`) with two list facts: `at`, the field's list
  index, and `target`, which also tells a create (`None`) from a rename.
- `row_at` and `row_count` (`sidebar/tree.rs:98-120`) map a list index to a
  row. A create inserts an item and shifts the rows below it by one. A rename
  replaces its row, so nothing shifts.
- `rebuild` (`sidebar/tree.rs:10-34`) re-places the field after each walk. A
  create clamps `at` to the row count. A rename moves to the row the walk
  found for `target`, or closes when the target has no row. Then it scrolls.
- `after_deleting` (`sidebar/tree.rs:69-78`) picks the next selection: the
  row below, else the row above. Its comment says it "must be computed before
  the delete". `step` (`:55-67`) moves the selection by one row.

None of this needs a window. The index mapping is tested only through
`#[gpui::test]`s (`the_rows_below_a_create_field_shift_down_by_one`,
`a_create_field_is_pulled_back_inside_the_tree`, and the rename-follow tests
in `sidebar/tests.rs`). The two facts `editing.is_some()` and "the list has a
field item" are kept in step by hand in four places.

## What Zed does

- The new entry is a phantom `Entry` with `NEW_ENTRY_ID`
  (`project_panel.rs:93`), inserted into `visible_entries` during
  `update_visible_entries` (`:4482`, `:4548`). A rename is found at render
  time by comparing each entry's id with `edit_state.leaf_entry_id`
  (`for_each_visible_entry`, `:5462-5480`).
- `EditState` (`:240-256`) tells a create from a rename by
  `leaf_entry_id.is_none()`, as ours does with `target`.
- `find_next_selection_after_deletion` (`:2993`) runs before the delete task
  is spawned (`:2891`), as ours does.

## Design

### The module: `Rows<E>` in `sidebar/rows.rs`

`Rows` is the list `uniform_list` draws: the tree's rows plus at most one
field item. It hides the index mapping and the re-placement rule after a
walk. It holds no gpui type; the field's gpui state is its payload `E`.

```rust
pub(super) struct Rows<E> {
    visible: Visible,
    slot: Option<Slot<E>>,
}

struct Slot<E> {
    item: usize,               // the field's item index
    place: Place,
    edit: E,
}

enum Place {
    Insert,                    // a create: shifts the rows below it
    Replace(EntryId),          // a rename: takes its target's row
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Item<'a, E> {
    Row(usize),                // a row index into `Visible`
    Field(&'a E),              // so a drawn field never lacks its edit
}

impl<E> Rows<E> {
    /// Walks, then re-places the field or closes it. Returns the field's item.
    /// Forwards the slot's target into `Visible::rewalk`'s target argument, in
    /// the same walk (rebuild spec S3): never a separate `index_of` pass.
    fn rewalk(&mut self, entries: Arc<[Entry]>, expanded: &HashSet<EntryId>) -> Option<usize>;
    fn visible(&self) -> &Visible;
    fn item_count(&self) -> usize;
    fn item(&self, item: usize) -> Option<Item<'_, E>>;
    /// Panics past the row count: a caller bug.
    fn insert(&mut self, item: usize, edit: E);
    /// `None`, and no slot, when `target` has no row.
    fn replace(&mut self, target: EntryId, edit: E) -> Option<usize>;
    fn close(&mut self) -> Option<E>;
    fn edit(&self) -> Option<&E>;
    fn edit_mut(&mut self) -> Option<&mut E>;
    fn target(&self) -> Option<EntryId>;
    fn field_item(&self) -> Option<usize>;
}
```

Three index spaces exist. An **item** indexes the drawn list, as
`uniform_list` and `scroll_to_item` name it. A **row** indexes `Visible`. A
**position** indexes the vault's entries. The names keep them apart.

**Invariants**, held after every method returns:

1. The field is open exactly when a slot exists. `Sidebar` keeps
   `rows: Rows<EditState>` and loses `editing`, so the pair cannot disagree.
2. A rename's slot has `item < visible.len()` and
   `visible.entry(item).id == target`.
3. A create's slot has `item <= visible.len()`.
4. For `item in 0..item_count()`, `item(item)` is `Some`. The `Row(k)` values
   rise strictly and cover every row once, except a rename's own row. Exactly
   one `Field` appears when a slot exists. Past `item_count()`, `item` is
   `None`.

**Ordering.** `rewalk` restores invariants 2 and 3 in the same call that
replaces `visible`. No caller sees a slot that points into the previous
walk. A target found under a collapsed folder, or gone, closes the slot, as
the rebuild spec (S3) requires.

`E` is a type parameter, not a trait. `Sidebar` uses `Rows<EditState>` and
the tests use `Rows<()>`. The parameter is what makes invariant 1 a property
of the type.

### Selection over rows: two methods on `Visible`

They read rows only; the slot does not take part.

```rust
/// The row that takes `id`'s place when it leaves: below, else above.
/// Call before removing the row: after removal this returns `None`.
fn neighbour(&self, id: EntryId) -> Option<EntryId>;
/// One step from `from`; no selection enters at the first or last row.
fn step(&self, from: Option<EntryId>, delta: isize) -> Option<EntryId>;
```

`neighbour` replaces `after_deleting`. Its answer is about the current rows,
so a caller asks before it removes anything; the name says so and needs no
ordering comment. `trash` calls it where it does today, before the prompt.

### What moves where

| From | To |
|---|---|
| `Sidebar::editing` | `Sidebar::rows` (with `visible`) |
| `EditState::at`, `EditState::target` | `Slot::item`, `Slot::place` |
| `row_at`, `row_count` mapping (`tree.rs`) | `Rows::item`, `Rows::item_count` |
| the clamp-or-close `match` in `rebuild` | `Rows::rewalk` |
| `after_deleting` (`tree.rs`) | `Visible::neighbour` |
| the index arithmetic of `step` (`tree.rs`) | `Visible::step` |

What stays on `Sidebar`: fetching the snapshot, `scroll_to_item`,
`cx.notify`, the editing guards on `step`, `trash` and `rename`, and
`insertion_point`, because it may expand a folder. `row_count` becomes
`item_count`, a one-line delegate: `shell/tests/renaming.rs` reads it. `render_rows` matches
on `Item`. `open_edit` splits into `open_create` calling `insert` and
`open_rename` calling `replace`; both then scroll to the field's item.

## Tests

The new interface is the test surface. New plain tests go in
`sidebar/rows_tests.rs`; the `Visible` additions go in
`sidebar/visible_tests.rs`. Both scan a `MemFs` as `visible_tests.rs` does.

Executable success criteria:

- **Mapping, proptest.** For a flat vault of 0 to 12 notes, and any slot (none,
  insert at `0..=n`, replace at `0..n`), invariant 4 holds for every item and
  `item_count()` is `n` plus one for an insert. A mapping bug (an off-by-one at
  the field, a rename that shifts rows) fails it.
- `rewalk` after a shrink clamps a create's item to the new row count.
- `rewalk` keeps a create's item when it still fits after the walk.
- `rewalk` moves a rename's item to its target's new row after a note sorts
  above it.
- `rewalk` closes a rename whose target moved under a collapsed folder, and one
  whose target is gone; `edit()` is then `None`.
- `replace` of a hidden id opens nothing.
- `neighbour`: a middle row gives the row below; the last row gives the row
  above; a lone row and an unknown id give `None`.
- `step`: no selection enters at the first row going down and at the last row
  going up; a step clamps at both ends; an empty tree gives `None`.

The `#[gpui::test]`s in `sidebar/tests.rs` and `shell/tests/` pass unchanged.
Only their helpers and direct field reads change. `sidebar/tests.rs` is
`#[cfg(test)]` as a whole, so every site below is test-only:

- `field_item` (was `field_row`), `is_editing`, `rejection_message` and `field_is_focused` read
  through `rows`.
- The private `rows()` helper (`tests.rs:61-65`) reads `self.visible`; it goes
  through `self.rows.visible()` and becomes `visible_rows()`. It backs `row_names`, `toggle_named`,
  `labels`, `row_labels` and `row`.
- `a_rename_field_follows_its_row_across_a_scans_swap` reads
  `editing...target` (`tests.rs:166`); it reads `rows.target()`.
- `a_file_appearing_in_the_vault_reaches_a_row` reads `visible.len()`
  (`tests.rs:210`, `:222`); it reads `rows.visible().len()`.
- A `#[cfg(test)]` `row_at` helper in `tests.rs` keeps
  `the_rows_below_a_create_field_shift_down_by_one` as it is.

`./scripts/test-names.sh` before and after shows additions only.

The scenes `sidebar-naming` and `sidebar-renaming` are rendered before and
after, and must look the same: the field on the item below `index`, and the
field on `index` with no duplicate row.

## Deviations from Zed

- **The field is an overlay index, not a phantom entry.** `Visible` holds
  positions into a shared `Arc<[Entry]>`. A phantom `Entry` would need a
  sentinel id and a copied snapshot, or a special case in every reader. The
  overlay costs one comparison per item.
- **A rename's row is found during the walk**, not per rendered row. This is
  unchanged from the rebuild spec (S3).

## Risks

- Dropping `Slot` drops `EditState` and its subscriptions. The cancel
  subscription already does that from inside its own callback today, so
  `close()` there keeps the current order.
- The generic parameter could read as a speculative abstraction. It has one
  production type and one test type, and it removes a two-place invariant.
- Three index names can drift. Review checks that every `usize` parameter in
  `rows.rs` is named `item` or `row`.

## Plan

Each step ends green on `./scripts/check.sh`.

1. **Rows and the slot.** Write `rows_tests.rs` first: the proptest and the
   `rewalk`/`replace` tests, red. Add `rows.rs`. Replace `visible` and
   `editing` on `Sidebar` with `rows`. Remove `at` and `target` from
   `EditState`. Move `row_at`, `row_count` and the rebuild `match` behind
   `Rows`. Update the test helpers and the direct reads listed under Tests
   (`rows()`, the `target` read, the two `visible.len()` reads). Run
   `test-names.sh` before and after.
2. **Selection on `Visible`.** Write the `neighbour` and `step` tests, red.
   Move `after_deleting` and `step`'s arithmetic into `Visible`. Delete the
   ordering comment.
3. **Look and measure.** Render `sidebar-naming`, `sidebar-renaming` and
   `sidebar-rejected`, and compare them with the baseline. Run
   `cargo mutants -f crates/app/src/sidebar/rows.rs -f crates/app/src/sidebar/visible.rs`
   with zero missed. Add this spec to the Docs list in `CLAUDE.md`. Set it to
   **implemented**.

## As built

- `Rows::field_item` is `#[cfg(test)]`. Production reads the field's item
  from the return of `insert`'s caller, `replace` and `rewalk`, never from a
  getter.
- `open_edit` became `edit_state`, which builds the `EditState`, and
  `show_field`, which scrolls and notifies. `open_create` and `open_rename`
  call both around `insert` and `replace`.
- `open_rename` finds its row twice: `index_of` for the name, depth and
  parent, then `replace`. The second pass is O(visible) once per rename
  gesture, not per walk or keystroke.
- `Visible::is_empty` is gone: `step` was its only caller, and
  `Visible::step` reads `len` instead.
- `step` also treats a selection with no row (a hidden or gone id) as no
  selection, as the old arithmetic did; `visible_tests.rs` pins it.
- The mapping proptest's fixture creates `/vault` itself: a scan of a vault
  with no root fails. The shrunk case, zero notes, is committed in
  `crates/app/proptest-regressions/sidebar/rows_tests.txt`.
- The three scenes rendered byte-identical before and after.
- `cargo mutants --in-diff` over `sidebar/*.rs` (with `--test-package
  inkra-app`) left one survivor, `show_field` replaced by `()`. The scroll on
  opening was never tested; `a_field_opened_far_below_the_drawn_rows_takes_keystrokes`
  in `shell/tests/creating.rs` now pins it.
- `.cargo/mutants.toml`'s equivalence entry for `step`'s `> 0` moved with the
  code, from `Sidebar::step` in `tree.rs` to `Visible::step` in `visible.rs`.
