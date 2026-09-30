# No vault, expressed once

Status: **proposed 2026-09-30.** Refactor plus one behaviour fix: `⌘N` with
no vault open no longer opens a name field that can create nothing.

## Problem

The app launches in one of three modes (`crates/app/src/bootstrap.rs:21-29`):
nothing, one file opened by path, or a vault. The mode is settled once, in
`root` (`bootstrap.rs:39-58`), and never changes. The code does not record
that. Each vault-mode consumer takes the `NoteStore` and asks again whether a
vault exists.

`NoteStore.vault: Option<Entity<Vault>>` (`crates/app/src/vault/store.rs:16`)
is the one source. The store serves both modes, so its own guards are
legitimate: the save path, sessions and the quit flush are shared by a
detached note and a vault note. The friction is downstream, in modules that
have no meaning without a vault but are built in every mode:

| Module | Guards on "no vault" |
|---|---|
| `IndexSync` (`crates/app/src/index.rs`) | field `:40`, copy `:60`, subscribe `:62-64`, `is_building` `:109`, `awaits_first_build` `:127`, `saved` `:190`, `land` `:211` |
| `index/loading.rs` | `load_more` `:26` |
| `index/building.rs` | `reconcile` `:12`, `apply_build` `:47` |
| `Sidebar` (`crates/app/src/sidebar/mod.rs:70-71`, `sidebar/tree.rs:11-15`, `sidebar/edit.rs:255,276`) | 4 |
| `BacklinksPanel` (`crates/app/src/backlinks/mod.rs:88`) | 1 |
| `AppShell` (`crates/app/src/shell.rs:150,207,220`, `shell/render.rs:12,102`) | 5 |

`IndexSync` copies the vault out of the store (`index.rs:60`) and then guards
it at eight sites. Seven of them are dead in practice: with no vault the shell
never opens a picker (`shell.rs:207,220`) and never draws the backlinks
column (`render.rs:102`). The eighth, `vault.is_some() &&` in `is_building`,
exists only so that an index nobody reads does not report "building" forever.

The guards also hide a defect. With no vault, `⌘N` reaches `Sidebar::start_creating`
(`sidebar/edit.rs:105`), which opens a name field in an empty tree. Enter
calls `NoteStore::create`, which returns `None` at `vault/tree_ops.rs:78`. The
user types a name and nothing happens. No test covers it.

A second encoding of the same fact: `EntryId::DETACHED` and
`Location::Detached` (`vault/session/lifecycle.rs:16`). A detached note exists
only when there is no vault, but only `root` holds that: `NoteStore::adopt`
(`store.rs:176`) is callable on a vault store. So `rename` checks both
`id == EntryId::DETACHED` and `self.vault.is_none()` (`tree_ops.rs:123,129`),
and `write` checks `id != EntryId::DETACHED` before it emits `NoteSaved`
(`save.rs:144`).

## Decision

Vault-mode modules take `Entity<Vault>`, not `Option`, and exist only in vault
mode. The shell builds them once, in one place, from the one `Option` the
store holds. A detached note is built by its own constructor, so "detached
implies no vault" holds by construction.

### `VaultPanes`: the shell's vault-mode half

A new file `crates/app/src/shell/panes.rs`. It holds what the shell has only
when a vault is open. It is a data shape, not a layer: the Leverage is that
every consumer below it drops its `Option`, and the shell asks once per
action instead of each consumer asking per call.

```rust
/// What the shell has only with a vault: built once in `AppShell::new`, never later.
pub(super) struct VaultPanes {
    pub(super) links: Entity<IndexSync>,
    pub(super) sidebar: Entity<Sidebar>,
    pub(super) backlinks: Entity<BacklinksPanel>,
    /// Tab titles follow a rename, as the sidebar's rows do.
    _renamed: Subscription,
}

impl VaultPanes {
    pub(super) fn new(
        vault: Entity<Vault>,
        notes: &Entity<NoteStore>,
        cx: &mut Context<AppShell>,
    ) -> Self;
}
```

`AppShell` replaces its three fields `sidebar`, `links`, `backlinks`
(`shell.rs:110-112`) with `panes: Option<VaultPanes>`. The subscriptions to
`sidebar` and `backlinks` for `NoteActivated` (`shell.rs:114-117,122-125`)
move into `AppShell::new`'s `if let Some(panes)` branch.

Interface of the shell with no vault, all behaviour as today except `⌘N`:

- `⌘P`, `⌘⇧F`: do nothing (`panes` is `None`).
- `⌘N`, `⌘⇧N`: do nothing. Today they open a dead name field.
- The left column is drawn at `SIDEBAR_WIDTH` with the sidebar background and
  no child, so the layout does not move (the `empty_no_vault` and note scenes
  stay pixel-identical).
- The backlinks column is not drawn (as today).
- Following a link in a detached note opens a URL and ignores every other
  target. Today it resolves against an empty index, which gives the same
  result. After the change it reads `inkra_index::Target::parse(link)` and
  matches `Target::External` only; no index is built.
- `tree_cursor` returns `Cursor::default()`; `set_shown` skips the sidebar and
  backlinks updates.

### Consumers take the vault

```rust
impl IndexSync {
    pub(crate) fn new(notes: &Entity<NoteStore>, vault: Entity<Vault>, cx: &mut Context<Self>) -> Self;
}
impl Sidebar {
    pub(crate) fn new(notes: Entity<NoteStore>, vault: Entity<Vault>, cx: &mut Context<Self>) -> Self;
}
impl BacklinksPanel {
    pub(crate) fn new(
        links: Entity<IndexSync>,
        notes: Entity<NoteStore>,
        vault: Entity<Vault>,
        cx: &mut Context<Self>,
    ) -> Self;
}
```

Each stores `vault: Entity<Vault>`. The guards in the Problem table go.
index-status-once lands first (see the overlap section), so the two
`vault.is_some() &&` guards are by then one no-vault arm in
`IndexSync::status`; `status` becomes `self.live.status()`.
`Sidebar._vault_changed` becomes a `Subscription`, not an `Option`.

### The detached note has its own constructor

```rust
impl NoteStore {
    /// A vault's store, or the empty launch's with `None`.
    pub(crate) fn new(fs: Arc<dyn Fs>, vault: Option<Entity<Vault>>, cx: &mut Context<Self>) -> Self;
    /// One file opened by path, outside any vault; it is `current`.
    pub(crate) fn detached(
        fs: Arc<dyn Fs>,
        buffer: Entity<Buffer>,
        path: PathBuf,
        stamp: Stamp,
        cx: &mut Context<Self>,
    ) -> Self;
}
```

`adopt` stops being `pub(crate)`; `detached` is its only caller. Because no
constructor gives a store both a vault and a detached note:

- `rename` drops `id == EntryId::DETACHED` (`tree_ops.rs:123`); the
  `self.vault.is_none()` check below it covers the detached note.
- `write` emits `NoteSaved` for every landed write and drops the
  `id != EntryId::DETACHED` guard (`save.rs:144`). Its only subscriber,
  `IndexSync`, exists only beside a vault, where no detached note exists.
  `NoteSaved`'s doc line (`vault/events.rs:9`) changes to say so.

`EntryId::DETACHED` itself stays: it is the session key of the detached note,
and `inkra-vault` also uses it as the unminted placeholder
(`crates/vault/src/vault.rs:209`). The vault crate's own guards
(`Vault::reidentify` `vault.rs:125`, `Claims::reclaim` `scans/claims.rs:27`)
stay: `inkra-vault` cannot see the app's constructors, and
`scans/swap_tests.rs:189-203` pins them.

### What stays

`NoteStore.vault: Option<Entity<Vault>>` stays. Deletion test: remove it and
the detached note's save path, conflict check and quit flush need a second
store that duplicates `sessions`, `Queue` and `write_through`. The `Option`
there is the one honest statement of "no vault". Its guards in
`vault/scans.rs:18,28,43,55`, `vault/reload.rs:53`, `vault/tree_ops.rs`,
`vault/save.rs:95` and `store.rs:40-58,88,124` stay.

## What moves where

| From | To |
|---|---|
| `AppShell.{sidebar,links,backlinks}` (`shell.rs`) | `VaultPanes` (`shell/panes.rs`) |
| vault observe for tab titles (`shell.rs:150-152`) | `VaultPanes::new` |
| `notes.read(cx).vault()` in `IndexSync::new`, `Sidebar::new`, `BacklinksPanel::rebuild` | an `Entity<Vault>` argument |
| `NoteStore::new(fs, None)` + `adopt` (`bootstrap.rs:52-55`) | `NoteStore::detached` |
| detached link following via an empty index (`shell/activation.rs:130-131`) | `Target::parse` + `Target::External` |

## Invariants now held by types

1. `IndexSync`, `Sidebar` and `BacklinksPanel` always have a vault. Held by
   their constructors' non-optional argument.
2. They exist only in vault mode. Held by `AppShell.panes: Option<VaultPanes>`,
   built once in `AppShell::new`.
3. A detached note never shares a store with a vault. Held by `NoteStore`'s
   two constructors and `adopt` becoming private.

## Deviations from Zed

- Zed opens a single file as a single-file worktree
  (`vendor/zed-reference/crates/worktree/src/worktree.rs:1274`,
  `is_single_file`), so a project always holds zero or more worktrees and its
  panels iterate them. We keep a detached note beside no vault. A single-file
  `Vault` would need `Scans` and `Watcher` on a file root, and an atomic save
  replaces the inode a file watch holds. That is an `inkra-vault` change with
  no roadmap trigger.
- Zed's project panel exists with zero worktrees and shows an empty list. Our
  sidebar entity does not exist without a vault; the shell draws its empty
  column. Reason: with no folder, `⌘N` has nowhere to create, and an existing
  but empty tree is what let the dead name field happen.

## Tests

### Executable success criterion (written first, fails before the change)

- `shell/tests/creating.rs`:
  `cmd_n_does_nothing_without_a_vault`. `no_vault(cx)`, simulate `cmd-n`,
  assert the shell's own focus handle is still focused. Today the name
  field takes focus, so it fails. A plausible bug it catches: `create` routed
  to a sidebar that exists in every mode.

### Guards (pass before and after; written first to pin behaviour)

- `shell/tests/following.rs`:
  `a_url_in_a_detached_note_opens_outside_the_app` and
  `a_wikilink_in_a_detached_note_changes_nothing`. Launch with
  `Launch::Note` over `[x](https://x.org/p)` and `[[b]]`; assert
  `opened_url` and that `showing` is unchanged. Nothing covers the detached
  follow today; slice 1 replaces its implementation.
- Existing: `cmd_p_does_nothing_without_a_vault` (`shell/tests/quick_open.rs:63`),
  `cmd_shift_f_does_nothing_without_a_vault` (`shell/tests/vault_search.rs:102`),
  `the_backlinks_column_shows_for_an_empty_tab_in_a_vault_only`
  (`shell/tests/tab_bar.rs:192`), `opens_with_no_vault`
  (`shell/tests/command_palette.rs:260`),
  `root_builds_an_editor_only_for_a_named_file`
  (`shell/tests/activation.rs:247`).
- Scenes `empty_no_vault` and the `Launch::Note` scenes in `shot/scenes.rs`:
  render before and after slice 1 and compare; the left column must not move.

### Tests that change

Counts below come from `grep -rn` over `crates/app/src` on `main`; re-run
the same grep when a slice starts, and the slice's count must match it.

- `a_detached_note_emits_nothing` (`index/tests.rs:556`) dies in slice 2. Its
  `IndexSync` half cannot be built without a vault. Its `NoteSaved` half
  pins `seen == 0`, the emission rule slice 4 reverses. It is the only test
  that pins detached emission (no file under `vault/tests/` names
  `NoteSaved`), so slice 4 adds its replacement:
  `vault/tests/autosave.rs`: `a_detached_write_emits_note_saved`. Build
  `NoteStore::detached` over `/loose.md`, subscribe to `NoteSaved` on the
  store (no `IndexSync`: none exists beside a detached note), type, advance
  `AUTOSAVE_DELAY`, assert one event and the file text. It fails before
  slice 4 (zero events). A plausible bug it catches: the `DETACHED` guard
  in `write` coming back, which silently makes `NoteSaved` mode-dependent
  again.
- `IndexSync::new(&notes, cx)`: 19 sites, 1 in `shell.rs:111`, 18 in tests
  (`grep -rn "IndexSync::new(&notes" crates/app/src`). Slice 2 updates 17
  and deletes 1 with `a_detached_note_emits_nothing` (`index/tests.rs:577`).
  One helper in `test_support.rs`, `index_sync(&notes, cx) ->
  Entity<IndexSync>`, reads the vault from the store and `expect`s it, so
  each updated site stays one line.
- `Sidebar::new` at `sidebar/tests.rs:109,115` and `BacklinksPanel::new` at
  `backlinks/tests.rs:17,380` pass the vault (slice 3).
- `adopt`: 6 sites (`grep -rn "\.adopt(" crates/app/src`, excluding
  `vault/session/mod.rs:113`, which is `Lifecycle::adopt`). Slice 4 moves
  `bootstrap.rs:54` and the four test sites `shell/tests/activation.rs:32`,
  `vault/tests/{mod,autosave,trash}.rs` to `NoteStore::detached`. The sixth,
  `index/tests.rs:568`, is gone with its test in slice 2.
- No test is disabled. Run `./scripts/test-names.sh` before and after; the
  only name that goes is `a_detached_note_emits_nothing`, and the only name
  that comes from this section is `a_detached_write_emits_note_saved` (plus
  the three written first above).

## Plan

Each slice is green under `./scripts/check.sh` alone.

1. **`VaultPanes` and the no-vault shell.** Write the three tests above; see
   `cmd_n_does_nothing_without_a_vault` fail. Add `shell/panes.rs`; move the
   three fields and their subscriptions; `create`, `quick_open`,
   `search_vault`, `set_shown`, `tree_cursor`, `follow` and `render` read
   `panes`. Consumers keep their current constructors. Render the scenes and
   compare. About 120 lines.
2. **`IndexSync` takes the vault.** Constructor argument, field type, and
   the guards; the two in `is_building` and `awaits_first_build` are, after
   index-status-once, the no-vault arm of `IndexSync::status`. Delete that
   arm and index-status-once's criterion 3,
   `a_store_without_a_vault_is_ready` (see the overlap section). Add
   `test_support::index_sync`; update 17 of the 18 test sites; delete
   `a_detached_note_emits_nothing`, the eighteenth. About 145 lines, most of
   it one-line test edits.
3. **`Sidebar` and `BacklinksPanel` take the vault.** Constructor arguments,
   the five guards, `_vault_changed` loses its `Option`. About 60 lines.
4. **`NoteStore::detached`.** Write `a_detached_write_emits_note_saved`
   first and see it fail. New constructor, `adopt` private, the two
   `DETACHED` checks in `tree_ops.rs:123` and `save.rs:144` go, `NoteSaved`'s
   doc line. Move `bootstrap.rs:54` and the four detached test sites. About
   90 lines.

Slices 2 and 3 depend on slice 1. Slice 4 depends on slice 2: before slice 2,
`a_detached_note_emits_nothing` still exists and asserts the rule slice 4
reverses. Ordering the slices removes the fork of "whichever lands first
deletes it".

### Overlap with index-status-once and index-turn-effects

**Merge order: index-status-once lands before this spec and before
index-turn-effects; this spec's slice 2 lands after index-turn-effects.**
All three specs state this order. The Problem section cites the code before
index-status-once.

Only slice 2 touches `app/src/index*`, and only `IndexSync::new`, the
`vault` field's type, and the early returns in `index.rs`, `loading.rs` and
`building.rs`. The rule it applies, "`IndexSync` holds `Entity<Vault>`", does
not depend on how index-turn-effects shapes `IndexSync`. That spec moves
`reconcile`, `load_more` and `land` into turns; slice 2 rebases onto the new
shape and deletes the same guards there. If index-turn-effects removes
`IndexSync`'s copy of `fs` or of the vault (for example by reading them
through a narrower handle), slice 2 shrinks to passing the vault into that
handle.

index-status-once (`2026-09-30-index-status-once-design.md`) adds
`IndexSync::status` with a no-vault arm ("`Ready` without a vault") and
its criterion 3, `a_store_without_a_vault_is_ready`, which builds an
`IndexSync` over `NoteStore::new(fs, None, cx)`. Both contradict invariants
1 and 2 of this spec. index-status-once names the arm temporary and lands
first. Slice 2 here then deletes the no-vault arm, making `status`
`self.live.status()`, and deletes `a_store_without_a_vault_is_ready`. It has
no `is_building` or `awaits_first_build` guard left to edit:
index-status-once deleted those accessors.

The no-vault arm exists for at most the PRs between the two specs, and no
build carries both an `Entity<Vault>` field and a `match` on `Option`.

## Risks

- **Merge conflicts with `feat/restore-tabs`.** That branch restores tabs on
  launch and probably touches `bootstrap.rs` and `shell.rs`. Land this after
  it, or rebase slice 1 and 4 onto it. If restored tabs can open a detached
  file inside a vault session, invariant 3 breaks; slice 4 then needs that
  branch's design first.
- **Layout drift with no vault.** The left column now has no child. If the
  sidebar's background or border came from `Sidebar::render` and not from the
  shell's wrapper `div` (`render.rs:78-83`), the column changes. The scene
  comparison in slice 1 catches it.
- **Focus with no vault.** `tree_focus_handle` (test and shot only) needs a
  vault; it `expect`s `panes`. No scene focuses the tree without a vault.
- **Performance.** No per-frame or per-keystroke work changes. The note and
  empty launches no longer build an `IndexSync` or a `Sidebar`, which removes
  a spawned drain task from their startup. Vault launches are unchanged; the
  first-frame and 8 ms turn budgets are not affected.

- **Overlap with index-status-once.** See the overlap section: it lands
  first, and its no-vault arm and `a_store_without_a_vault_is_ready` go in
  slice 2. A reviewer of slice 2 greps for `a_store_without_a_vault`
  and `vault.is_some()` in `crates/app/src/index*`; both must be absent.

## Out of scope

- A single-file `Vault` (see Deviations).
- Hiding vault-only commands (`New note`, `Quick open`, `Search vault`) from
  the command palette with no vault. They do nothing after this change; the
  palette's hide list (`command_palette/commands.rs:10`) could list them later.
- `EntryId::DETACHED` as the vault crate's unminted placeholder
  (`vault.rs:209`).
- An "open folder" action that turns a no-vault shell into a vault shell. If
  it lands, `panes` is built when the vault arrives; invariant 2 becomes "built
  once per vault", and this spec's shape already allows it.

## Review

Red/blue pass, 2026-09-30.

- **F1-overlap-with-index-status-once**: fixed. The overlap section now
  covers index-status-once, and slice 2 names the arm and test it deletes
  when index-status-once lands first. Red's claim that index-status-once
  "does not anticipate" this is not borne out: its Decision and Risks
  already defined both orders. Blue's "hard rule, no-vault-mode first" is
  rejected: the work is the same in either order, and index-status-once
  prefers itself first. This spec adopts that order so the two documents
  agree.
- **F2-untested-detached-notesaved-emission**: fixed. Confirmed: no test
  under `vault/tests/` names `NoteSaved`. Slice 4 adds
  `a_detached_write_emits_note_saved`, written first. Slice 4 now depends on
  slice 2, so the old test and the new rule never coexist.
- **F3-call-site-count-off-by-one**: fixed. 18 test sites, grep-verified; 17
  are updated and 1 is deleted with its test, which is why the old text said
  17. The `adopt` count ("six test sites") was also wrong: it is five sites
  (one production, four tests) after slice 2, now stated per site.
- Improvement "name the replacement in the same bullet": adopted, in "Tests
  that change".
- Improvement "merge slice 2 with index-status-once's slice 2": rejected.
  Its slices carry `inkra-index` work (`live::Status`) and picker changes
  this spec does not touch; merging couples two independent candidates into one PR over
  ~250 lines. The shared edit is two guard lines, and the ordering above
  already makes it happen once.
- Improvement "derive counts by grep": adopted; each count cites its grep.

Verdict: proceed. No finding questions the Decision; all three were plan
defects.

Cross-spec reconciliation (2026-09-30): candidate numbers are replaced by
spec slugs (index-turn-effects, index-status-once); the third candidate
cited as reshaping `IndexSync` matched no live spec and is dropped; the
overlap section states the fixed order, index-status-once first, and drops
the "this spec first" fallback; the Decision and slice 2 assume
`IndexSync::status` exists.
