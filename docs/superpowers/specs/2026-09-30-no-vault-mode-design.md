# No vault, expressed once

Status: **proposed 2026-09-30, refreshed 2026-10-01** against
`arch/no-vault-tabs` at `240b442` (restore-tabs #47, index-status-once and
index-turn-effects landed). Refactor plus one behaviour fix: `⌘N` with no
vault open no longer opens a name field that can create nothing.

## Problem

The app launches in one of three modes (`crates/app/src/bootstrap.rs:23-31`):
nothing, one file opened by path, or a vault with its optional session file.
The mode is settled once, in `root` (`bootstrap.rs:35-61`), and never changes. The code does not record
that. Each vault-mode consumer takes the `NoteStore` and asks again whether a
vault exists.

`NoteStore.vault: Option<Entity<Vault>>` (`crates/app/src/vault/store.rs:16`)
is the one source. The store serves both modes, so its own guards are
legitimate: the save path, sessions and the quit flush are shared by a
detached note and a vault note. The friction is downstream, in modules that
have no meaning without a vault but are built in every mode:

| Module | Guards on "no vault" |
|---|---|
| `IndexSync` (`crates/app/src/index.rs`) | field `:53`, copy `:74`, subscribe `:76-78`, `status`'s no-vault arm `:122-128`, `turn`'s `?` `:136`, `saved`'s `let … else` on `turn`'s `Option` `:220-222` |
| `Sidebar` (`crates/app/src/sidebar/mod.rs:53,70-71`, `sidebar/tree.rs:11-15`, `sidebar/edit.rs:255,276`) | 4 |
| `BacklinksPanel` (`crates/app/src/backlinks/mod.rs:88`) | 1 |
| `AppShell` (`crates/app/src/shell.rs:158,223,236`, `shell/render.rs:13,109`) | 5 |

`IndexSync` copies the vault out of the store (`index.rs:74`). After
index-turn-effects every input reaches `LiveIndex` through one helper,
`IndexSync::turn` (`index.rs:130-146`), so the per-method guards of the old
`load_more`, `reconcile`, `apply_build` and `land` collapsed into `turn`'s
`self.vault.as_ref()?`. `turn` therefore returns `Option<R>`; `saved`
(`index.rs:218-232`) is the one caller that reads it. `loading.rs` and
`building.rs` hold no guard of their own now. Every path through `turn` is
dead with no vault: the shell never opens a picker (`shell.rs:223,236`) and
never draws the backlinks column (`render.rs:109`), and nothing emits
`NoteSaved` for the detached note. The remaining guard, `status`'s
`None => Status::Ready` arm (added by index-status-once), exists only so that
an index nobody reads does not report "scanning" forever.

The guards also hide a defect. With no vault, `⌘N` reaches `Sidebar::start_creating`
(`sidebar/edit.rs:105`), which opens a name field in an empty tree. Enter
calls `NoteStore::create`, which returns `Task::ready(None)` at
`vault/tree_ops.rs:77-79`. The
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
    pub(super) vault: Entity<Vault>,
    pub(super) links: Entity<IndexSync>,
    pub(super) sidebar: Entity<Sidebar>,
    pub(super) backlinks: Entity<BacklinksPanel>,
    /// The two panes' `NoteActivated`, and the vault: tab titles, the session file and a
    /// waiting restore follow it, as the sidebar's rows do.
    _subscriptions: [Subscription; 3],
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
(`shell.rs:86-88`) with `panes: Option<VaultPanes>`. `VaultPanes` keeps
the vault, so `⌘⇧F` reads it from `panes` and asks no second question. The
subscriptions to `backlinks` and `sidebar` for `NoteActivated`
(`shell.rs:122-125,129-132`) move into `VaultPanes::new`, held in
`_subscriptions` beside the vault observe: everything the shell wires only
with a vault lives in one place, with one lifetime.

The vault observe (`shell.rs:157-165`) moves into `VaultPanes::new`, the
third of `_subscriptions`. Since restore-tabs it does three things, not one: it saves
the session (`save_session_later`), retries a waiting restore
(`try_restore`, level-triggered on every vault notify, restore-tabs R5) and
redraws the tab titles. All three have meaning only with a vault, so the
move keeps them. `VaultPanes::new` takes `cx: &mut Context<AppShell>`, so
the closure calls the same shell methods it calls today.

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
  matches `Target::External` only; no index is built. Both are public
  (`crates/index/src/lib.rs:30`, `target.rs:8,34`).
- `tree_cursor` returns `Cursor::default()`; `set_shown` skips the sidebar and
  backlinks updates. `land_restore` (`shell/session.rs:215`) calls
  `tree_cursor` only in vault mode: a restore exists only with a vault.
- The session file and the restore are unchanged: `open_session`
  (`shell/session.rs:43-57`) already returns `(None, Restore::Done)` without
  a vault root.

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
        vault: Entity<Vault>,
        cx: &mut Context<Self>,
    ) -> Self;
}
```

Each stores `vault: Entity<Vault>`. The guards in the Problem table go.
`BacklinksPanel` drops its `notes: Entity<NoteStore>` argument and field: its
one read (`backlinks/mod.rs:88`) is the guard `self.vault` replaces, so a kept
field would be dead code and fail clippy.
index-status-once and index-turn-effects have landed (see the overlap
section), so in `IndexSync` this means three edits: `status` becomes
`self.live.status()`; `turn` returns `R`, not `Option<R>`, and reads
`self.vault.read(cx)`; `saved` drops its `let … else`. `reconcile`
(`building.rs:8-11`), `apply_landed` (`loading.rs:57-83`) and the build and
merge hand-backs in `run` (`index.rs:175,187`) already ignore `turn`'s
result and need no change. `Sidebar._vault_changed` (`sidebar/mod.rs:53`)
becomes a `Subscription`, not an `Option`.

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

- `rename` drops `id == EntryId::DETACHED` (`tree_ops.rs:123-125`); the
  `self.vault.is_none()` check below it covers the detached note.
- `write` emits `NoteSaved` for every landed write and drops the
  `id != EntryId::DETACHED` guard (`save.rs:144`). Its only subscriber,
  `IndexSync`, exists only beside a vault, where no detached note exists.
  `NoteSaved`'s doc line (`vault/events.rs:9`) changes to say so.

`EntryId::DETACHED` itself stays: it is the session key of the detached note,
and `inkra-vault` also uses it as the unminted placeholder
(`crates/vault/src/vault.rs:209`, `vault/identity.rs:20-22,63`). The vault
crate's own guards (`Vault::reidentify` `vault.rs:125`, `Claims::reclaim`
`scans/claims.rs:27`, `Vault::reclaim` `vault/reclaim.rs:30`) stay: `inkra-vault` cannot see the app's constructors, and
`scans/swap_tests.rs:189-203` pins them.

### What stays

`NoteStore.vault: Option<Entity<Vault>>` stays. Deletion test: remove it and
the detached note's save path, conflict check and quit flush need a second
store that duplicates `sessions`, `Queue` and `write_through`. The `Option`
there is the one honest statement of "no vault". Its guards in
`vault/scans.rs:18,28,43,55`, `vault/reload.rs:53`, `vault/tree_ops.rs`,
`vault/save.rs:95` and `store.rs:37-58,88,124` stay.

The session's reads of `notes.vault()` (`shell/session.rs:50-52,109,139`),
added by restore-tabs, also stay. `open_session` needs the root before any
pane exists, and it is the gate: without a vault it yields no
`SessionFile` and `Restore::Done`. `write_session`'s and `try_restore`'s
guards are then unreachable without a vault, but they read the store's
`Option`, as the store's own guards do. See Out of scope.

## What moves where

| From | To |
|---|---|
| `AppShell.{sidebar,links,backlinks}` (`shell.rs`) | `VaultPanes` (`shell/panes.rs`) |
| vault observe for tab titles, session save and restore retry (`shell.rs:157-165`) | `VaultPanes::new` |
| `notes.read(cx).vault()` in `IndexSync::new`, `Sidebar::new`, `BacklinksPanel::rebuild` | an `Entity<Vault>` argument |
| `IndexSync::turn`'s `Option<R>` (`index.rs:130-146`) | `R` |
| `NoteStore::new(fs, vault)` + `adopt` in one closure (`bootstrap.rs:53-59`) | `NoteStore::detached` for `Launch::Note`, `NoteStore::new` otherwise |
| detached link following via an empty index (`shell/activation.rs:146-153`) | `Target::parse` + `Target::External` |

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
  follow today (`following.rs` launches every test in a vault); slice 1
  replaces its implementation.
- Existing: `cmd_p_does_nothing_without_a_vault` (`shell/tests/quick_open.rs:63`),
  `cmd_shift_f_does_nothing_without_a_vault` (`shell/tests/vault_search.rs:102`),
  `the_backlinks_column_shows_for_an_empty_tab_in_a_vault_only`
  (`shell/tests/tab_bar.rs:192`), `opens_with_no_vault`
  (`shell/tests/command_palette.rs:260`),
  `root_builds_an_editor_only_for_a_named_file`
  (`shell/tests/activation.rs:248`), and from the tabs work
  `single_file_mode_titles_the_tab_with_the_file_name`
  (`shell/tests/tab_bar.rs:61`) and `single_file_mode_switches_back_to_its_file`
  (`shell/tests/tabs.rs:206`), which launch through `root` with
  `Launch::Note` and pin slice 4's `bootstrap.rs` move.
- Scenes `empty_no_vault` and the `Launch::Note` scenes in `shot/scenes.rs`:
  render before and after slice 1 and compare; the left column must not move.

### Tests that change

Counts below come from `grep -rn` over `crates/app/src` on
`arch/no-vault-tabs` at `240b442`; re-run the same grep when a slice starts,
and the slice's count must match it.

- Six test sites read the shell's fields directly (`grep -rnE
  "shell\.(sidebar|links|backlinks)\b" crates/app/src/shell/tests`):
  `mod.rs:92,450`, `tabs.rs:525`, `restore_refused.rs:22`,
  `sidebar_selection.rs:78`, `backlinks.rs:47`. Slice 1 changes each to read
  through `panes` (`VaultPanes`'s fields are `pub(super)`, visible to
  `shell::tests`); every one runs in a vault.
- `a_store_without_a_vault_is_ready` (`index/tests.rs:534`),
  index-status-once's criterion 3, dies in slice 2 with the arm it pins.
- `a_detached_note_emits_nothing` (`index/tests.rs:543`) dies in slice 2. Its
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
- `IndexSync::new(&notes, cx)`: 19 sites, 1 in `shell.rs:119`, 18 in tests
  (`grep -rn "IndexSync::new(&notes" crates/app/src`). Slice 2 updates 16
  and deletes 2 with their tests: `a_store_without_a_vault_is_ready`
  (`index/tests.rs:537`) and `a_detached_note_emits_nothing`
  (`index/tests.rs:563`).
  One helper in `test_support.rs`, `index_sync(&notes, cx) ->
  Entity<IndexSync>`, reads the vault from the store and `expect`s it, so
  each updated site stays one line.
- `Sidebar::new` at `sidebar/tests.rs:109,115` passes the vault, and
  `BacklinksPanel::new` at `backlinks/tests.rs:17,380` passes the vault in
  place of `notes.clone()` (slice 3). Both test sites keep their local
  `notes` for the other calls.
- `adopt`: 6 sites (`grep -rn "\.adopt(" crates/app/src`, excluding
  `vault/session/mod.rs:113`, which is `Writer::adopt`). Slice 4 moves
  `bootstrap.rs:56` and the four test sites `shell/tests/activation.rs:32`,
  `vault/tests/mod.rs:39`, `vault/tests/autosave.rs:47` and
  `vault/tests/trash.rs:142` to `NoteStore::detached`; all four build the
  store with `None`. The sixth, `index/tests.rs:554`, is gone with its test
  in slice 2.
- No test is disabled. Run `./scripts/test-names.sh` before and after; the
  only names that go are `a_store_without_a_vault_is_ready` and
  `a_detached_note_emits_nothing`, and the only name that comes from this
  section is `a_detached_write_emits_note_saved` (plus the three written
  first above).

## Plan

Each slice is green under `./scripts/check.sh` alone.

1. **`VaultPanes` and the no-vault shell.** Write the three tests above; see
   `cmd_n_does_nothing_without_a_vault` fail. Add `shell/panes.rs`; move the
   three fields, their subscriptions and the vault observe; `create`,
   `quick_open`, `search_vault`, `set_shown`, `tree_cursor`,
   `tree_focus_handle`, `follow`, `empty_lines` and `render` read `panes`.
   Update the six test field reads. Consumers keep their current
   constructors. Render the scenes and compare. About 130 lines.
2. **`IndexSync` takes the vault.** Constructor argument, field type, and
   the guards: `status` becomes `self.live.status()`, `turn` returns `R`,
   `saved` drops its `let … else`. Delete index-status-once's criterion 3,
   `a_store_without_a_vault_is_ready` (see the overlap section). Add
   `test_support::index_sync`; update 16 of the 18 test sites; the other two
   go with `a_store_without_a_vault_is_ready` and
   `a_detached_note_emits_nothing`. About 130 lines, most of it one-line
   test edits.
3. **`Sidebar` and `BacklinksPanel` take the vault.** Constructor arguments,
   the five guards, `_vault_changed` loses its `Option`. `BacklinksPanel`
   drops its `notes` argument and field in the same slice, since the guard
   was their only read; the call in `VaultPanes::new` and the two in
   `backlinks/tests.rs` pass the vault instead. About 60 lines.
4. **`NoteStore::detached`.** Write `a_detached_write_emits_note_saved`
   first and see it fail. New constructor, `adopt` private, the two
   `DETACHED` checks in `tree_ops.rs:123-125` and `save.rs:144` go,
   `NoteSaved`'s doc line. Move `bootstrap.rs:56` and the four detached test
   sites. About 90 lines.

Slices 2 and 3 depend on slice 1. Slice 4 depends on slice 2: before slice 2,
`a_detached_note_emits_nothing` still exists and asserts the rule slice 4
reverses. Ordering the slices removes the fork of "whichever lands first
deletes it".

### Overlap with index-status-once and index-turn-effects

**Both have landed on `arch/no-vault-tabs`** (index-status-once
`ffb9144`..`bdbdfe6`, index-turn-effects `ad33332`..`240b442`), in the
order all three specs state. The citations in this spec are to the code
after both.

Only slice 2 touches `app/src/index*`, and only `index.rs`: `IndexSync::new`,
the `vault` field's type, `status`, `turn` and `saved`. `loading.rs` and
`building.rs` need no edit: index-turn-effects moved their guards into
`turn`. index-turn-effects kept `IndexSync`'s own copies of `fs` and of the
vault (`index.rs:52-53`) and passes `vault.read(cx)` into `LiveIndex::turn`
(`index.rs:138`), so the rule "`IndexSync` holds `Entity<Vault>`" applies
as written; it did not shrink to a narrower handle.

index-status-once added `IndexSync::status` with a no-vault arm
(`index.rs:122-128`, "`Ready` without a vault") and its criterion 3,
`a_store_without_a_vault_is_ready` (`index/tests.rs:534`), which builds an
`IndexSync` over `NoteStore::new(fs, None, cx)`. Both contradict invariants
1 and 2 of this spec; index-status-once names the arm temporary. Slice 2
deletes the arm, making `status` `self.live.status()`, and deletes the test.
index-turn-effects' `turn` returns `Option<R>` only because of the same
`Option` field; slice 2 makes it return `R`. index-status-once already
deleted `is_building` and `awaits_first_build`.

No build carries both an `Entity<Vault>` field and a `match` on `Option`.

## Risks

- **Restore tabs (resolved).** restore-tabs landed on `main` as #47 before
  this refresh; the citations above include its changes to `bootstrap.rs`
  (`Launch::Vault` carries the session file), `shell.rs` (the vault observe
  now saves the session and retries the restore) and `shell/session.rs`.
  Restored tabs cannot open a detached file inside a vault session:
  `Plan::resolve` resolves saved paths against the vault's entries, and
  `open_session` reads a session only for a vault launch. Invariant 3 holds.
- **Layout drift with no vault.** The left column now has no child. If the
  sidebar's background or border came from `Sidebar::render` and not from the
  shell's wrapper `div` (`render.rs:84-90`), the column changes. Checked:
  `Sidebar::render` (`sidebar/mod.rs:94-130`) sets no background; the
  wrapper sets `theme::sidebar_background()`. The scene comparison in slice 1
  confirms it.
- **Focus with no vault.** `tree_focus_handle` (`shell.rs:258-261`, test and
  shot only) needs a vault; it panics without `panes`. No scene and no test
  focuses the tree without a vault: its 11 callers (`shot/stage.rs:162`,
  `shell/tests/`) all run in a vault. The `Launch::Note` tests in
  `tab_bar.rs` and `tabs.rs` read only `focus_handle`.
- **Performance.** No per-frame or per-keystroke work changes. The note and
  empty launches no longer build an `IndexSync` or a `Sidebar`, which removes
  a spawned drain task from their startup. Vault launches are unchanged; the
  first-frame and 8 ms turn budgets are not affected.

- **Overlap with index-status-once and index-turn-effects.** Both landed;
  see the overlap section. A reviewer of slice 2 greps for
  `a_store_without_a_vault`, `Option<Entity<Vault>>` and `-> Option<R>` in
  `crates/app/src/index*`; all must be absent.

## Out of scope

- A single-file `Vault` (see Deviations).
- Hiding vault-only commands (`New note`, `Quick open`, `Search vault`) from
  the command palette with no vault. They do nothing after this change; the
  palette's hide list (`is_hidden`, `command_palette/commands.rs:10`) could
  list them later.
- The session's `notes.vault()` reads (`shell/session.rs:50-52,109,139`).
  `VaultPanes` could carry the vault handle so `write_session` and
  `try_restore` read `panes` instead. That reshapes restore-tabs' state
  machine for no defect; its trigger is a second session consumer.
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

Refresh (2026-10-01): re-validated against `arch/no-vault-tabs` at `240b442`
(restore-tabs #47 on `main`, index-status-once and index-turn-effects on the
branch). Every file:line updated. `IndexSync`'s guards are now the field,
the copy, the subscribe, `status`'s no-vault arm and `turn`'s `?` (so `turn`
returns `Option<R>` and `saved` unwraps it); the `loading.rs` and
`building.rs` rows are gone and slice 2 makes `turn` return `R`. The vault
observe that moves into `VaultPanes` now also saves the session and retries
the restore; its field is renamed `_renamed` to `_vault_changed`. Slice 2
deletes two tests, not one (`a_store_without_a_vault_is_ready` is also an
`IndexSync::new` site): 16 sites updated, 2 deleted. Slice 1 also updates
six test reads of `shell.{sidebar,backlinks}`. `session.rs`'s three
`notes.vault()` reads stay, listed under What stays and Out of scope. The
restore-tabs risk is resolved: invariant 3 holds. The excluded `.adopt(` site is `Writer::adopt`, not `Lifecycle::adopt`. The design is unchanged;
no landed code makes it wrong.

Red/blue on the refresh (2026-10-01):

- **F1-backlinks-dead-notes-param**: fixed. Confirmed against the code:
  `self.notes` is read once in `backlinks/`, at `mod.rs:88`, the guard slice
  3 replaces. The Decision's `BacklinksPanel::new` drops `notes`; slice 3
  drops the field and updates `VaultPanes::new` and `backlinks/tests.rs:17,380`.
  Slice count unchanged: four.
