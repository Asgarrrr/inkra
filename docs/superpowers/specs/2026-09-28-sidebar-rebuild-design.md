# Sidebar rebuild: skip what is hidden, build what is drawn

Status: **implemented.**

Item 7 of the roadmap's "Index and search: the 2026-09-27 audit".

## Why

`Sidebar::rebuild` (`crates/app/src/sidebar/tree.rs`) runs on every notify of
the `Vault` entity: each create, rename, trash, and each watcher batch that
changes the tree. It costs the whole vault on the foreground, whatever is
drawn. Release, Apple M5 Pro, `#[gpui::test]` over a 50,000-note `MemFs`,
8 rebuilds each (2026-09-28):

| Tree | Rows | Before |
|---|---|---|
| Flat, 50,000 notes at the root | 50,000 | 2.30–2.35 ms |
| 100 collapsed folders of 500 notes | 100 | 1.22–1.27 ms |
| 100 folders, one open | 600 | 1.28–1.33 ms |

The budget is 0.1 ms of foreground work per vault change (watcher spec,
"Performance"). Two costs make up the figures:

1. **A collapsed folder is skipped one entry at a time.** The loop tests
   `RelPath::starts_with` on each of its descendants. `Path::starts_with`
   parses components, so 49,900 hidden notes cost about 1.2 ms.
2. **Every visible row is materialized.** Each `Row` owns a `String` name from
   `display_name` and a depth from `components().count()`. A flat vault pays
   50,000 allocations and 50,000 component walks, for the 30 or so rows
   `uniform_list` draws.

## What Zed does

`ProjectPanel::update_visible_entries`
(`vendor/zed-reference/crates/project_panel/src/project_panel.rs:4504`) walks
the worktree with a cursor and calls `advance_to_sibling` (`:4744`) at a
collapsed directory: a logarithmic jump over the subtree on its `SumTree`. The
visible list holds entries, and the file name is derived when a row renders.

We take both ideas. Our entries are a sorted `Arc<[Entry]>`, not a `SumTree`,
so the jump is a binary search.

## Design

### S1. Skip a collapsed folder by binary search

The vault spec guarantees that a directory's descendants occupy a contiguous
run immediately after it (`2026-09-20-inkra-vault-design.md`, "Ordering").
So `starts_with(dir)` is true on a prefix of `entries[i + 1..]` and false
after it. That is exactly the shape `slice::partition_point` needs:

```rust
i += entries[i..].partition_point(|entry| entry.path.starts_with(&dir.path));
```

Here `i` is the folder's own index: the folder satisfies the predicate too,
so the jump lands on the first entry past its subtree. The cost per collapsed
folder drops from its subtree size to about `log2(n)` comparisons.

### S1b. A level's notes are copied as one run

The first probe after S1 and S2 measured the flat vault at 0.105–0.119 ms:
reading 50,000 entries' kinds still cost about 2 ns each. The vault's order
removes that read. Folders sort before notes at each level (`vault.rs`,
`compare`), and a note has no subtree. So the first note of a level starts a
run that ends where the enclosing folder's subtree ends, or at the end of the
entries at the root.

The walk keeps a stack of the subtree ends of the expanded folders around the
current entry. At a note, it copies the whole run with `Vec::extend(i..end)`
and jumps to `end`. A folder costs one binary search, whether expanded or
collapsed. A `debug_assert!` checks that every entry of the run is a note.

The target of an open rename can sit inside a run. The walk then finds its
position with a linear search of that run only. This runs only while a name
is being edited.

### S2. The visible list holds indices, not rows

`Sidebar.visible` is a `Visible` (`sidebar/visible.rs`): the snapshot it
walked and positions into it.

```rust
struct Visible {
    entries: Arc<[Entry]>,  // the snapshot `positions` indexes
    positions: Vec<usize>,  // positions in `entries`, in draw order
}
```

**Invariant:** every position is a valid index into `entries`. The fields are
private, and only `Visible::rewalk` sets them, always both. A new snapshot
therefore never pairs with stale positions. The type makes this a property
of the code, not a comment, and lets the walk be tested with plain `#[test]`
over a scanned `MemFs`, without gpui.

Two index spaces exist: a row index `ix` (into `positions`) and an entry
`position` (into `entries`). The names keep them apart.

`Visible` reads the list with:

- `entry(ix) -> &Entry` returns `entries[positions[ix]]`. It builds nothing.
  `insertion_point`, `trash` and `step` read it, since they only need `id`,
  `kind` or depth.
- `parent_of(ix)` finds the nearest row above whose path is the row's
  parent path. It used to find the nearest folder row one level up. Both
  find the same row, because a folder's subtree follows it; path equality
  states the rule directly and needs no depth per candidate.
- `row(ix) -> Row` builds the owned `Row` (`id`, `kind`, `depth`, `name`)
  from `entry(ix)`. `Row` stays the value `render_row`, `activate` and the
  edit code pass around, so only drawn rows and gestures allocate a name.

`row_at` keeps its create and rename branching: it maps a list index past the
edit field to a `visible` index, then calls `row`. `rebuild` itself computes
no name and no depth.

`index_of(id)` stays a linear scan over `positions`, comparing
`entries[position].id`. It runs per gesture only: `rebuild` no longer calls
it (S3).

### S3. The edit field's row is found during the walk

Today, while a name is edited, `rebuild` calls `index_of(target)` after the
walk: a second O(visible) pass on every vault change. Instead, the walk
records the target's position when it pushes the target's entry. A target
under a collapsed folder is never pushed, so it resolves to `None` and the
edit closes, as today. Any other rule would put the field on an undrawn row,
and gpui would swallow its keystrokes.

### S4. A broken order fails loudly in debug builds

`partition_point` returns an unspecified index when the slice is not
partitioned. The vault's ordering is tested, but the sidebar should not fail
silently if that ever regresses. After each jump, a `debug_assert!` checks
the landing point: the entry before it is in the subtree, and the entry at
it, if any, is not. The check is O(1).

Holding the `Arc` keeps the previous snapshot alive until the next rebuild.
The vault already shares it with background tasks, so this adds no copy.

## Budget and measurement

| Tree | Target | Before | After |
|---|---|---|---|
| Flat, 50,000 rows | under 0.1 ms | 2.30–2.35 ms | 0.005 ms |
| Flat, a rename open | under 0.1 ms | — | 0.028–0.030 ms |
| 100 collapsed folders | under 0.1 ms | 1.22–1.27 ms | 0.040–0.051 ms |
| 100 folders, one open (600 rows) | under 0.1 ms | 1.28–1.33 ms | 0.039–0.041 ms |

Release, Apple M5 Pro, 8 rebuilds each, 2026-09-28; the first rebuild of a
run is excluded as warm-up. The collapsed case is now the most expensive: 100
binary searches of about 16 `Path::starts_with` calls each. `rebuild`
reserves `entries.len()` in `visible` after the clear, so expanding a large
folder never reallocates mid-walk. Measure with the probe above, in release,
before and after.

## Tests

Walk shapes, target positions and parents are plain `#[test]`s in
`sidebar/visible_tests.rs`. The edit field's behaviour stays in the
`#[gpui::test]`s of `sidebar/tests.rs`.

- Existing sidebar tests cover the behaviour: order, collapse, expand, the
  edit field across a shrinking tree, selection, rename and create positions.
  They must pass unchanged, except for the test helpers that read `Row` from
  `visible`.
- New: a collapsed folder followed by siblings whose names extend the
  folder's (`Projects/`, then `Projects 2/`, then the note `Projects.md`,
  since folders sort first) shows the siblings. `partition_point` with a byte-prefix predicate would swallow them;
  the component-wise `starts_with` does not.
- New: nested collapse — an expanded folder containing a collapsed folder,
  followed by a note of the outer folder. The skip must stop at the inner
  subtree's end, not the outer one's.
- New: a collapsed folder as the last entry, and a collapsed empty folder
  followed by a note. Both must show every row after the folder. Then, with
  folders expanded, a level's notes resume after a nested subtree and stop at
  their folder's end (S1b).
- New: a rename whose note moves under a collapsed folder closes the field
  (S3). The move keeps the note's id, so the test pins the hidden case, not
  the deleted one.

## Rejected alternatives

**Apply the scan's changes to the rows as a delta.** It would make a change
cost O(changed) instead of O(visible). But the vault publishes whole
snapshots, and the delta would need its own diff of two `Arc<[Entry]>`. With
S1 and S2 the whole rebuild fits the budget, so the delta buys nothing
measurable.

**A subtree-end index stored in the vault.** It makes the skip O(1), but
moves a view concern into `inkra-vault` and grows every entry. The binary
search already meets the budget.
