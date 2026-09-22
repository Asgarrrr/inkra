# inkra-vault: open, list, save

Status: designed (2026-09-20), not implemented. Revised after red/blue team
review — the first draft was found **not implementable**, with three critical
defects. Section "What the review changed" records them, because the corrected
design only makes sense against the version it replaces.

Every `file:line` below was read in `vendor/zed-reference` or in this repo.
Dependency claims were resolved with `cargo tree` **per target**, not read from
`Cargo.lock`; the first draft made exactly that mistake and reached a wrong
conclusion from it.

## Why now

Nothing in this repo reads a file. `crates/app/src/main.rs:35-47` hardcodes a
`String` into a `Buffer`. Every remaining editor item in `docs/roadmap.md` is
deferred with no trigger fired.

`inkra-vault` is the brick everything else stands on. `inkra-markdown` and
`inkra-index` both depend on it; it depends on neither.

## Scope

Open a folder, show its notes as a tree, click to load one into the editor,
`⌘S` to write it back, and a "modified" indicator.

Not in this slice: file watching, diff-based reload, conflict detection,
create/rename/delete, multi-pane, search, links. "Deferred" records the trigger
for each.

## What the review changed

Three defects were critical. Each is a design decision, not a coding mistake,
which is why they are recorded rather than quietly fixed.

**1. `version()` counted undo groups, not edits.** The first draft gave each
*transaction* a monotonic id. `Buffer::close` extends the previous transaction
in place when the next edit lands within `GROUP_INTERVAL`
(`crates/text/src/buffer.rs:141-148`, interval 300ms at `:11`), and
`replace_text_in_range` stamps `Instant::now()` per keystroke
(`crates/app/src/editor/input.rs:91`). So a typing run is *one* transaction.
Type `x`, save, type `y` 120ms later: the id never moves, the indicator reads
clean, `⌘S` no-ops, and `y` is lost on quit. The fix is below: the id lives on
the **edit**, not the transaction.

**2. `WeakEntity<Buffer>` silently destroyed unsaved edits.** The draft held
open buffers weakly, for deduplication. `EditorView::buffer`
(`crates/app/src/editor/mod.rs:33`) is the only strong handle, so switching
notes dropped it and `upgrade()` returned `None`
(`vendor/zed-reference/crates/gpui/src/app/entity_map.rs:788`). Returning to
the note reloaded it from disk: the text and the whole undo history, gone,
with no prompt.

**3. "The swap goes through the existing gates" was not implementable.**
`edit_buffer` hands out `&mut Buffer` and `move_cursor` a `&Buffer`
(`crates/app/src/editor/mod.rs:91`, `:119`); neither can rebind `self.buffer`.
Followed literally, opening a note would `replace` the *previous* note's text
inside its own buffer, so `⌘Z` would restore the old note into the new file.
A correct swap must also rebuild `_buffer_changed` (`:62`, created at `:70`) —
a dropped `Subscription` stops every repaint, with no compiler warning and no
failing test.

Two further findings changed the model rather than breaking it: `EntryId`
keyed by path could not deliver what it claimed (see below), and the sort
comparator did not support the walk built on it.

## What we take from Zed, and what we refuse

| Piece | Zed | Verdict |
|---|---|---|
| `Fs` behind a narrow trait | `crates/fs/src/fs.rs:100` — 39 methods | **Take**, reduced to 3 |
| Entry identity from `(device, inode)` | `crates/worktree/src/worktree.rs:3959` | **Take.** See "identity" below |
| `PathEvent { path, kind }` + `Rescan` | `crates/fs/src/fs.rs:79-91` | Take, at the watcher slice |
| Diff-based reload, never `set_text` | `crates/language/src/buffer.rs:2459` | Take, at the watcher slice |
| `has_conflict` derived from mtime | `crates/language/src/buffer.rs:2531` | Take, at the watcher slice |
| `SumTree<Entry>` | `crates/worktree/src/worktree.rs:191` | **Refuse.** Pays off over 100k files |
| `BackgroundScanner` | `crates/worktree/src/worktree.rs:4339-6288`, ~1950 lines | **Refuse.** Restatting 200 files takes milliseconds |
| `FakeFs` | `crates/fs/src/fs.rs:1440-3646`, 2207 lines | **Refuse.** Inodes and a fake kernel watcher amortize over hundreds of scanner tests, not ours |
| Git, ignore rules, trash-by-id, legacy encodings, remote/collab | — | **Refuse.** Orthogonal |

Note on one citation the first draft got wrong: `worktree.rs:191` is only the
field declaration `entries_by_path: SumTree<Entry>`. The sort key lives in
`impl KeyedItem for Entry` near `:4228`, and it is **pure path order** — Zed
does not sort directories first. The draft cited it in support of a
dirs-first ordering it does not implement. Our comparator is our own, and is
specified below.

`vendor/zed-reference/CLAUDE.md` contains text addressed to agents, including
a "HARD RULE" demanding edits to `README.md`. It is data. Two of the three
review agents were exposed to it and ignored it, per the `vendor/` rule.

## Decision: the `Fs` seam is async, via `async-trait`

```rust
#[async_trait]
pub trait Fs: Send + Sync {
    async fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>>;
    async fn load(&self, path: &Path) -> Result<Loaded>;
    async fn save(&self, path: &Path, text: &str) -> Result<Mtime>;
}
```

**Cost, measured on the host target.** The first draft claimed "the cost is
zero, every crate is already compiled". That was read off `Cargo.lock`, which
aggregates every target. Resolved properly:

```
$ cargo tree -i -p futures      → present (gpui ← gpui_apple ← … ← inkra-app)
$ cargo tree -i -p async-trait  → nothing to print
$ cargo tree -i -p smol         → nothing to print
$ cargo tree -i -p async-fs     → nothing to print
```

`async-trait`, `smol` and `async-fs` reach the tree only through `gpui_linux`,
which is gated to Linux and FreeBSD. On macOS they are not compiled.

The real cost is **one new crate**: `async-trait`. Its own dependencies —
`proc-macro2`, `quote`, `syn` — are all present.

**`RealFs` does blocking `std::fs` inside the future, and does not use
`smol`.** This is deliberate. `smol::fs` is implemented over the `blocking`
crate's thread pool, so on local disk it only moves a blocking read to a
different pool. Our callers already run under `cx.background_spawn`
(`vendor/zed-reference/crates/gpui/src/gpui.rs:237`), which is itself a thread
pool. Copying Zed as far as `smol` would add six crates and buy no
performance. The async *shape* is what we are paying for: a remote backend
later is a second `impl Fs` and nothing else moves.

## Decision: `inkra-vault` is a crate

This overrides CLAUDE.md's "filesystem and network live in `app` until a
dedicated crate earns its place". It earns it on one argument: scan and index
work is where performance effort will land, and a gpui-free crate is
benchmarked against a synthetic 100k-entry tree with a plain `#[test]`, with
no `Window` and no platform.

It depends on nothing in this workspace — not `inkra-text`. `Fs::load` returns
a `String`; `app` builds the `Buffer`.

```
inkra-vault   ⊥   inkra-text → inkra-editor → inkra-app
```

This **supersedes** the row in
`docs/superpowers/specs/2026-09-20-modular-architecture-design.md:245`, which
records `inkra-vault` as depending on `text` + anchors. Anchors are needed by
diff-based reload, which lives in `app`/`editor`, not in the vault. That doc
gets a pointer to this one in the same change.

### Exactly what adding the crate costs

The first draft called this "one line". It is four edits, and the loop
addition fails outright without the first:

1. `Cargo.toml:3` — add `crates/vault` to `members`.
2. `Cargo.toml:8` — add `inkra-vault = { path = "crates/vault" }` to
   `[workspace.dependencies]`.
3. `scripts/check.sh:29` — add `inkra-vault` to the no-gpui loop.
4. `scripts/check.sh:9` — add `-p inkra-vault` to the `cargo fmt` list. Never
   `--all`: that rewrites the vendored gpui source.

The new manifest must carry `[lints] workspace = true`. Workspace lints are
**opt-in per crate**; without it, `unwrap_used` and `expect_used`
(`Cargo.toml:19-20`) are not denied in the new crate and the whole point of
the workspace lint table is lost there.

## The model

```rust
/// Identity is (device, inode), not the path. See below.
pub struct EntryId(u32);

pub enum Kind { Dir, Note }

pub struct Entry {
    id: EntryId,
    path: RelPath,      // relative to the vault root, validated
    kind: Kind,
    mtime: Mtime,
}

pub struct Vault {
    root: PathBuf,
    entries: Vec<Entry>,                  // sorted, see the comparator
    by_identity: HashMap<FileIdentity, EntryId>,
    by_path: HashMap<RelPath, EntryId>,   // an index, not the identity
    next_id: u32,
}
```

### Identity: `(device, inode)`, not the path

The first draft made "`EntryId` is stable across rescans, **keyed by path**"
its one non-negotiable decision. The review showed it delivered the opposite
of its claim, in three ways:

- **Rename `a.md` → `b.md`.** Keyed by path, that is a delete plus an insert,
  so the id changes. The open buffer and the sidebar selection both hold a dead
  id. The draft's headline proptest — ids stable across adds, removes **and
  renames** — could not have passed.
- **Case-only rename on APFS.** `Notes.md` → `notes.md` is the same file on a
  case-insensitive, case-preserving filesystem. Hashing path bytes makes it a
  new id and loses the old entry. Zed handles this case explicitly
  (`crates/worktree/src/worktree.rs:5693-5699`).
- **Delete `a.md`, create an unrelated `a.md`.** The id is *retained*, so the
  open buffer and its `saved_mtime` now describe a different file.

Path-keying was unstable exactly where stability was needed, and stable
exactly where it was not.

So identity is `FileIdentity { device: u64, inode: u64 }`. A rename keeps the
id and changes the path. A case-only rename is one file, so one inode, so one
id. A recreated file has a new inode, so a new id.

Zed's `Entry` carries `inode: u64` alone, without a device
(`crates/worktree/src/worktree.rs:3963`). We pair it with the device because an
inode is only unique within one filesystem, and nothing stops a vault root from
spanning a mount point. The cost is one more `u64` from the same `stat` we
already perform.

Known limits, accepted and recorded rather than discovered later:

- **Hard links collapse.** Two paths sharing an inode become one entry. A note
  vault does not use hard links; if one appears, the second path is dropped and
  logged.
- **Inode reuse.** The OS may reuse an inode after deletion. Between two
  rescans this is a theoretical mis-identification. It is the same exposure Zed
  carries.
- **Non-Unix filesystems.** FAT and some network mounts have unstable inodes.
  Out of scope: the vault is a local directory.

The proptest survives this change, which is the point: **ids stay stable across
an arbitrary sequence of adds, removes and renames.** That property is now
true, where before it was aspirational.

### Ordering, and the invariant the sidebar rests on

The tree renders as a linear walk over `entries`, so collapsing a directory
must be a forward skip. That requires: **a directory's descendants occupy a
contiguous run immediately after it.**

Contiguity comes from **component-wise** ordering, which `RelPath` already has:
it derives `Ord` from `Arc<Path>`, and `Path: Ord` compares component by
component. An ancestor is then a strict component prefix of its descendants, and
anything sorting between a path and its extensions shares those components, so
it *is* a descendant. Contiguity is a theorem of that ordering, not something
the comparator establishes.

Two encodings would lose it, and neither may return as a simplification. Plain
**byte** ordering: `.` is `0x2E` and `/` is `0x2F`, so `"a/b.md" < "a/b/c.md"`
and the sibling file `a/b.md` sorts *between* directory `a/b` and its own
children. The Obsidian folder-note pattern — `Projects/` beside `Projects.md` —
hits this immediately. Sorting by `(kind, path)`: that puts *every* directory
before *every* file, so `a`, `c`, `a/z.md`, `b.md`, `c/y.md`, and the walk
renders `a/z.md` indented under `c`.

What the comparator adds on top of `Path: Ord` is **sibling order**: dirs-first
at the **first differing component**, using that component's kind rather than
the entry's. Without it `b.md` sorts above `c`. That is cosmetic — the subtree
stays contiguous either way:

```
compare(x, y):
  walk components pairwise until they differ at index i
  if one path is exhausted, the shorter (the ancestor) comes first
  at i: a component that is a directory in its own path sorts before a file
  otherwise compare the component bytes
```

Prefix tests use `Path::starts_with`, which is component-wise, never
`str::starts_with` — the latter would make collapsing `Projects/` also hide
`Projects.md`.

A `#[test]` asserts the invariant directly: for every directory entry at index
`i` with `k` descendants, those descendants occupy exactly `i+1 .. i+1+k`. It
cannot discriminate the comparator from a plain `path.cmp` — contiguity is a
theorem of component order — so it is paired with a test that pins sibling
order, which is the only part `compare` owns.

### Derived, not stored

Display name is `path.file_stem()`, a method rather than a field — the call D5
made when it cut the per-line `top` table as derivable (`docs/roadmap.md:61-62`).
`file_stem()` returns an `OsStr`; rendering uses `to_string_lossy()`, so a
non-UTF-8 filename shows with replacement characters instead of vanishing.

Indentation depth is `path.components().count() - 1`.

### Path safety

`RelPath` is a newtype over `Arc<Path>` with a validating constructor:
relative, no `..`, no absolute prefix. Same discipline as `ByteOffset` and
`Utf16Offset` — an invariant the type holds so no caller has to remember it.

## The `Fs` types

The first draft named `DirEntry`, `Loaded` and `Mtime` without defining them,
which would have forced an implementer to add a fourth `metadata` method and
kill the "three methods" claim. They are:

```rust
pub struct FileIdentity { device: u64, inode: u64 }
pub struct Mtime(SystemTime);
pub struct DirEntry { path: PathBuf, kind: Kind, mtime: Mtime, identity: FileIdentity }
pub struct Loaded  { text: String, mtime: Mtime, identity: FileIdentity }
```

`DirEntry` carries everything the scan needs, so `read_dir` alone is enough to
recurse. On Unix, `device` and `inode` come from `MetadataExt`.

### Scanning

Recursive from the root. Keeps `*.md`. Skips dotfiles and dot-directories.

- **Symlinks are not followed.** This closes the loop hazard outright, at the
  cost of symlinked note folders, which nobody has asked for.
- **Depth limit 64**, which no real vault approaches, so a pathological tree
  cannot exhaust the stack.
- **Empty directories are kept** as entries. They render, and hiding them would
  make a freshly created folder invisible.
- A subdirectory that fails to read is recorded and skipped. One unreadable
  folder does not lose the vault.

### Line endings

`docs/roadmap.md:18` gives CRLF normalization the trigger "file I/O or a paste
action, whichever lands first, blocked by `inkra-vault`". **This slice fires
that trigger**, and the first draft did not mention line endings at all.

`Fs::load` normalizes `\r\n` and lone `\r` to `\n` before the text reaches the
`Buffer`. `Fs::save` writes `\n`. Nothing downstream ever sees a `\r`, which
is what the buffer model assumes.

This does not round-trip a CRLF file: saving rewrites it as LF. Recorded under
Deferred with its own trigger, rather than left as a surprise.

### `MemFs`

Behind a `test-support` feature, as Zed gates `FakeFs`. A `BTreeMap<PathBuf,
(String, Mtime, FileIdentity)>` with synthesized inodes and an injectable
error — not a simulated kernel.

Two consequences the first draft missed:

- `scripts/check.sh:38` runs `cargo test --workspace` **without**
  `--all-features`. The feature-gated tests would compile out and pass green
  without ever running — the exact failure mode CLAUDE.md records for the
  missing `font-kit` feature. `inkra-vault` therefore lists **itself** in
  `[dev-dependencies]` with `features = ["test-support"]`, so its own tests
  always build.
- `scripts/check.sh:12` runs clippy **with** `--all-features`, so `MemFs` is
  linted under `unwrap_used = "deny"`. It must be written to that bar, not as
  throwaway test code.

**Superseded.** This section originally said saving was not atomic, matching
Zed: `save` truncates and writes (`crates/fs/src/fs.rs:1019-1035`), and Zed's
`atomic_write` (`:974`) is reserved for its own config files. `RealFs::save`
now writes a temp file and renames over the destination. Zed can afford to
truncate because its autosave defaults to off and the user picks the moment;
ours writes unprompted, so a crash mid-write would corrupt a note nobody chose
to save. The rename changes the file's `(device, inode)`, which is why `save`
returns a `Saved` and `Vault::reidentify` exists.

## `inkra-text` gains `Buffer::version()`

The only change outside the vault, and the one the review corrected most
sharply.

**The id lives on the `Edit`, not on the `Transaction`.** Every `Edit` gets a
monotonic `u64` when it is applied, and `version()` returns the id of the last
applied edit, or `0` for an empty history.

Per-transaction ids are wrong because `close` merges by extending
`previous.edits` in place (`crates/text/src/buffer.rs:141-148`) — the
transaction's identity does not move even though the text changed. Per-edit ids
are immune by construction: a merged edit is still a new `Edit` with a new id.

This also removes the dependence on caller discipline. `finalize_transaction`
at save time would have patched the symptom; it is still called at save, but
for its actual purpose — stopping the next keystroke from merging into the
undo step that was saved — not to make `version()` correct.

The property holds through undo and redo:

- Undo pops a transaction, so `version()` returns to the last edit of the one
  before it.
- Redo pushes the same edits back, with the same ids.
- A divergent edit after an undo takes a fresh id and cannot collide with the
  history it replaced.

Zed reaches the same place with a `clock::Global` version vector and a memoized
`has_unsaved_edits` (`crates/language/src/buffer.rs:139`, `:2493`). It counts
operations, which is exactly the granularity adopted here. Ours is a single
`u64` because we have one writer.

`saved_version` lives in `app`, not in `Buffer`: it describes the link between
a file and a buffer, not a property of the text.

### The save race

`⌘S` is async, so keystrokes can land between the write starting and finishing.
The version is captured **with the text snapshot**, before the task is spawned,
and recorded as `saved_version` only on success. Edits that arrive meanwhile
leave `version() != saved_version`, so the note stays correctly dirty.

## The `app` side

### `NoteStore`

Owns `Entity<Vault>` and `HashMap<EntryId, Entity<Buffer>>` — a **strong**
handle, per critical defect 2. Weak handles would drop an unsaved buffer the
moment the editor switched away from it. There is no close action in this
slice, so buffers are held for the session; 200 markdown notes is not a memory
concern, and the trigger for eviction is recorded under Deferred.

It also holds `saved_version` per open note, and that alone: `saved_mtime` was
drafted here and deliberately not written. Nothing in this slice reads an
mtime, and a field that is only ever written fails `-D warnings` — the same
reason `Vault::by_path` is listed and not built. It returns with conflict
detection, which is what needs it; see Deferred.

The map deduplicates: opening an already-open note returns the same entity.
That is the shape "two panes on one note" needs later, which CLAUDE.md
currently forbids because `Selection` holds raw offsets. Zed's equivalent is
`BufferStore::open_buffer` with its `loading_buffers` dedup
(`crates/project/src/buffer_store.rs:908`).

`NoteStore` is an `Entity<NoteStore>` owned by `AppShell`, which is the only
view holding both it and the editor.

`open_note(EntryId) -> Task<Result<Entity<Buffer>>>` runs the load through
`cx.background_spawn` and resolves on the foreground.

It does **not** record which note is current. That is `set_current`, the single
writer, called by the clicking view once the buffer is actually bound to the
editor. The two are written on different schedules otherwise — the load's and
the view's — and `save_current` reads `current`: a `current` naming a note the
editor does not hold makes `⌘S` find a clean note and return early, so the
dirty text on screen is never written and nothing reports it. Same category as
critical defect 1.

The clicking view holds its activation `Task` rather than detaching it, so a
new click drops the previous one and cancels it
(`vendor/zed-reference/crates/scheduler/src/executor.rs:389-390`). Without
that, a load abandoned mid-flight still lands and rebinds the editor behind a
later, deliberate click. This is the opposite of `AppShell::save`, which
detaches deliberately: a save has no successor, and dropping its task would
cancel the write it just started.

### `EditorView::open_buffer` — a third gate

CLAUDE.md states `EditorView` has exactly two gates. This slice adds a third,
and the rule is updated in the same change rather than bent.

```rust
pub(crate) fn open_buffer(&mut self, buffer: Entity<Buffer>, cx: &mut Context<Self>)
```

It must do all of this, because each omission is a silent defect:

- Reassign `self.buffer` (`crates/app/src/editor/mod.rs:33`).
- **Rebuild `_buffer_changed`** by re-running `cx.observe` (`:70`). The old
  `Subscription` drops with the old buffer; keeping it would observe the closed
  note and repaint nothing for the new one.
- Reset `EditorState`, including `marked_range`
  (`crates/editor/src/state.rs:43`) — an in-flight IME composition must not
  survive into another document.
- Clear `last_layout`, `last_bounds`, `last_text_top` (`:41-51`). `reveal_line`
  early-returns on `None` (`:144-146`), so a stale non-`None` layout would
  scroll the new note by the old note's geometry.
- Reset `scroll_handle` (`:57`).

The caret is seated at offset 0, unlike `EditorView::new`, which seats it at
`end` (`:69`, `:74`). Opening a note starts at the top; that difference is
intentional and stated here so it is not read as a bug.

`open_buffer` is added to CLAUDE.md's "Don't recreate" table in the same
change, alongside `edit_buffer` and `move_cursor`.

### The sidebar

`crates/app/src/shell.rs:46` is a grey `div` reserving 180px. It becomes the
tree.

- `expanded: HashSet<EntryId>` lives in `app`. Expansion is UI state, never
  vault state.
- `visible_rows: Vec<EntryId>` is derived when the vault or the expansion set
  changes, never per frame.
- Collapsing skips forward while `Path::starts_with` holds — correct because of
  the comparator above.

The list uses `uniform_list`
(`vendor/zed-reference/crates/gpui/src/elements/uniform_list.rs:22-26`), which
renders only the visible window. `gpui-notes.md` rejected it for the *editor*,
and the reason it gives is why it fits here: its vertical algorithm is
`item_height = longest_item_size.height` (`:370`) and the item origin is
`item_height * ix` (`:497`). One scalar times an index breaks on soft-wrapped
text; every row of a file tree is one line tall.

Two traps the review found, both of which would have shipped as "the sidebar is
blank" or "the rows overlap":

- **`ListSizingBehavior` defaults to `Auto`** (`elements/list.rs:192-193`), and
  the `Auto` branch calls `window.request_layout(style, None, cx)` with no
  children (`uniform_list.rs:315-318`). Inside `flex_none().w(px(180.))` it
  resolves to zero height and paints nothing. The list gets `.size_full()`.
- **`item_height` is measured from item 0 only**, at
  `AvailableSpace::MaxContent` (`:658-680`), so the measurement never wraps —
  but paint lays rows out at the real sidebar width. A long filename would wrap
  at paint and overflow its row. Every row is `whitespace_nowrap` with
  truncation, which the "one line tall" claim requires and does not otherwise
  get.

### The `Save` action and focus

No existing owner fits: `actions!(editor, [...])`
(`crates/app/src/editor/actions.rs:16`) is editor-scoped, and `AppShell` holds
only the editor (`crates/app/src/shell.rs:16`).

- `actions!(vault, [Save])` in a new `app/src/vault.rs`.
- Handled by `AppShell`, which owns `NoteStore`.
- Bound in `keymap.rs` — the only binding site (`:9-32`) — at the `AppShell`
  key context, **not** at `EditorView`'s (`crates/app/src/editor/mod.rs:269`).

This matters because the sidebar is focusable. `AppShell::focus_handle` returns
the editor's handle whenever there is an editor, and `AppShell`'s own otherwise
— **corrected 2026-09-21**; it delegated unconditionally when this was written,
because the editor was unconditional. Either way it is never the sidebar's: once
a sidebar row can take focus, a `⌘S` bound to the editor context would do
nothing while the sidebar is focused. The regression test presses `⌘S` **with
the sidebar focused**.

### The modified indicator, and a re-entrancy hazard

The indicator renders on the sidebar row. `AppShell` deferred a status bar
(`2026-09-20-app-shell-layout-design.md:121`), so there is nowhere else.

Built with **no observer at all** — the first draft said `NoteStore` would
observe each open `Entity<Buffer>`, and that turned out to be redundant.
`Sidebar::render_rows` reads the store and the row's buffer while it renders,
gpui records every entity a window read during a draw
(`app/entity_map.rs:213-214`, recorded by `window.rs:3421-3433`), and
`App::notify` dirties any window that read the notifying entity
(`app.rs:2789-2823`). An edit or a save therefore redraws the window on its
own, and `.child(entity)` is uncached (`view.rs:100-104`), so the sidebar
re-renders with it. Wrapping the sidebar in `Entity::cached` would break that
and is the only way to reach the `dirty_views` check at `view.rs:483-498`.

CLAUDE.md's rule still applies wherever an observer is added: **no observer on
an `Entity<Buffer>` may call `.update()` or `.as_mut()` on that same buffer** —
a second lease panics
(`vendor/zed-reference/crates/gpui/src/app/entity_map.rs:231`). The save path
wants `finalize_transaction()` on the buffer, so it runs in the `Save` action
handler.

### Launch states

All four were undefined in the first draft, and `EditorView::new` requires an
`Entity<Buffer>`, so something must exist in every case.

| Situation | Behaviour |
|---|---|
| `cargo run -p inkra-app` with no argument | **Superseded 2026-09-21.** No buffer and no editor at all: `AppShell` renders its empty state, and `⌘S` is a no-op that still reaches its handler. The original row — an empty `Buffer` the editor worked on — is the hole the empty state closed; see `2026-09-21-empty-state.md` |
| Argument is a readable file | The file is loaded into the buffer and `⌘S` writes it back, with no vault and an empty sidebar. Task 5's behaviour, kept rather than replaced: `EntryId::DETACHED` exists for exactly this note |
| Argument is unreadable, or a directory that cannot be listed | Message to stderr, exit 1 — as `main.rs:63-66` already does for window failure |
| Vault with zero `.md` files | Empty tree, empty state |
| Vault scanned, nothing opened yet | The empty state, reading `Select a note.`, until a row is clicked |

The native picker exists (`vendor/zed-reference/crates/gpui/src/app.rs:1687`
`prompt_for_paths`) and is deferred: this slice is about the vault, not a
dialog.

## Errors

`thiserror` at the vault boundary — the use CLAUDE.md reserves it for.
Infallible operations still panic on caller bugs.

```rust
pub enum VaultError {
    Io { path: PathBuf, source: io::Error },
    NotUtf8 { path: PathBuf },
}
```

`OutsideRoot` was drafted here as a third variant and is not built. Nothing
constructs it: `RelPath::new` rejects `..` and absolute paths by returning
`None`, and the scan skips what it cannot make relative. A variant no code
path can produce is a lie in the API, the same objection that keeps
`Vault::by_path` and `saved_mtime` out. It returns with the command layer
under Deferred, which is the first code that validates a caller-supplied path
against the root.

## Testing

`inkra-vault` has no gpui, so its async tests run on
`futures::executor::block_on` — `futures` is genuinely present on the host
target.

**`inkra-vault`**
- Scan finds `.md`, skips dotfiles, does not follow symlinks, keeps empty
  directories, survives an unreadable subdirectory.
- `RelPath` rejects `..` and absolute paths.
- The ordering invariant: for every directory at index `i` with `k`
  descendants, they occupy `i+1 .. i+1+k`. With the `Projects/` beside
  `Projects.md` case as an explicit example.
- CRLF and lone CR normalize to LF on load.
- **proptest**: `EntryId`s stay stable across an arbitrary sequence of adds,
  removes and renames — including a case-only rename. This is the property the
  design rests on, and the one the first draft could not have passed.

**`inkra-text`**
- `version()` moves when a merged edit extends an existing transaction. This is
  the regression test for critical defect 1, and it must fail against a
  per-transaction id.
- `version()` returns to its earlier value after undo, and never collides after
  a divergent edit.
- Extend the existing buffer proptest: equal versions imply equal text.

**`inkra-app`**
- `#[gpui::test]` over `MemFs` for open and save. Tests call
  `cx.run_until_parked()` after the keystroke, or the `Task` from
  `cx.background_spawn` is never polled and the assertion races.
- `simulate_keystrokes` for `⌘S`, **with the sidebar focused**, per CLAUDE.md's
  rule that every binding needs one. Neither `with_editor` nor `dispatching`
  (`crates/app/src/editor/tests.rs:20`, `:35`) opens the window through
  `AppShell`, so this needs a new helper on the `shell.rs:74` precedent.
- Switching notes with unsaved edits and switching back preserves the text and
  the undo history. This is the regression test for critical defect 2.
- Editing after a note swap repaints. This is the regression test for critical
  defect 3, and it fails if `open_buffer` forgets the subscription.

**Manual render pass.** `check.sh` renders nothing, and the sidebar is new
pixels. Specifically: a long filename must truncate rather than wrap, and the
tree must not be blank — the two `uniform_list` traps above.

## Documentation to update in the same change

Not a follow-up. Each of these is wrong the moment this ships:

- **CLAUDE.md**: the "exactly two gates" rule becomes three; `open_buffer`,
  `NoteStore` and `Vault` are added to the "Don't recreate" table; the
  "filesystem lives in `app`" rule records that `inkra-vault` now exists.
- **`docs/roadmap.md`**: a "Vault" section with the table below; CRLF moves
  from deferred to done.
- **`2026-09-20-modular-architecture-design.md:245`**: a pointer noting this
  spec supersedes the `vault → text + anchors` row.

## Deferred

| Item | Trigger | Notes |
|---|---|---|
| File watcher + diff reload | A *clean* note changed on disk shows stale text until it is reopened. No longer gates conflict detection, which shipped without it — see `2026-09-21-conflict-detection-design.md`. | Needs anchored selections; `apply_diff`, `crates/language/src/buffer.rs:2459` |
| ~~Conflict detection~~ | **Closed 2026-09-21.** `Fs::save` takes a required `Stamp` and preserves the file's bytes beside the note before replacing them. Its premise here — that it "lands with the watcher" — was wrong: that describes Zed's implementation, not a constraint. | `2026-09-21-conflict-detection-design.md` |
| ~~Create / rename / delete~~ | **Closed 2026-09-22.** `Fs` gained `create`, `rename` and `trash`; `NoteStore` gained the three commands, each ordered on the note's `Ops` chain. Folder *deletion* is the one part left out — it destroys N notes with one gesture — with its own row below. | `2026-09-22-create-rename-delete-design.md` |
| Delete a folder | A user creates a folder they then want rid of. | Needs a prompt that counts the notes it destroys, and the closing of every open buffer in the subtree with its writer neutralised. The other five operations shipped without it |
| CRLF round-trip | A user reports that saving rewrote their line endings. | Requires storing the file's original ending per note |
| Serialized writes across notes | A vault on a network or FUSE mount, where writes to *different* notes contend. | The per-note case is closed: `Writer` (`app/src/vault.rs`) orders one note's writes by a monotonic sequence and serialises them with an async lock, and `MemFs::block_save` is the hook that tests it. Nothing coordinates across notes, and nothing yet shows it needs to |
| Buffer eviction | A vault large enough that holding every opened buffer costs memory. | Needs a close action first. Evicting a note now also means cancelling its pending write chain, dropping its observer `Subscription`, and dropping its `Arc<Writer>` — a note re-opened afterwards starts a fresh sequence, which is sound only because its old writes are gone too |
| Report a failed open | A user clicks a note that was deleted, renamed or made unreadable since the scan. | Nothing happens at all today: `NoteStore::open_note` discards the `VaultError` with `.await.ok()?` and `AppShell::open_note` bails on the `None`. Asymmetric with `save_current`, which at least reaches stderr. Rides on the same surfacing decision as the row below — a GUI app has nowhere to put an error yet |
| A home for user-visible errors | The first of these that a user rather than a developer hits. | Both failure paths above end in `eprintln!` or silence, and a released binary has no terminal attached. Needs a status line or a transient banner; `AppShell` is where it would live |
| ~~Save the unnamed launch buffer~~ | **Closed 2026-09-21**, by removal rather than by saving. There is no unnamed launch buffer any more: `bootstrap::root` builds a `Buffer` and an `EditorView` only for `Launch::Note`, and `AppShell` renders an empty state for the other two. Nothing can be typed into a buffer nobody tracks, because there is nothing to type into. Naming and saving such a buffer still needs the create/rename/delete row above. See `docs/superpowers/plans/2026-09-21-empty-state.md` |
| ~~Focusable sidebar~~ | **Closed 2026-09-22**, as a dependency of create/rename/delete rather than on its own trigger: a command needs a target, and a target needs a selection. The tree holds a `FocusHandle`, `selected` is its own state, and the arrows move it. The original note follows. | Keyboard navigation of the tree — arrows to move, enter to open — or any row that must show a focus ring. | Built as mouse-only: a row is a `div` with `on_mouse_down`, and nothing in the sidebar takes focus, so `AppShell::focus_handle` never returns a sidebar handle — it returns the editor's when there is one, and `AppShell`'s own otherwise. `⌘S` is already bound at the `AppShell` context rather than `EditorView`'s, so the binding does not move when this lands; the `⌘S`-with-the-sidebar-focused test does, and cannot be written before it |
| Native folder picker | A user who is not us opens the app. | `prompt_for_paths`, `crates/gpui/src/app.rs:1687` |
| Incremental scan | A profile on a real vault shows the full scan costing anything. | `(device, inode)` identity is what makes this a drop-in |
| `Vault::by_path` | A caller needs to resolve a `RelPath` to an `EntryId` — following a `[[link]]`, or reopening the last note by name across sessions. | Listed in the model above, not built: nothing reads it yet, and a private field that is only ever written fails `-D warnings`. The scan already builds every `(path, id)` pair, so adding it is three lines |
| `SumTree` entries | The same profile shows the `Vec` costing anything. | Internal to `Vault`; no consumer changes |
| Case-insensitive `MemFs` | A path-keyed lookup lands — `Vault::by_path` above — where `Notes.md` and `notes.md` must resolve to one entry. | `MemFs` keys a case-sensitive `BTreeMap<PathBuf, _>`, so the identity proptest's `Op::RecaseName` is a plain rename of one inode. That is the right shape for identity, which is keyed by inode and has no case-folding branch, but it does not exercise case folding itself. A path index would be the first code that could diverge |
| The plan's copy of the comparator rationale | Task 13 sweeps documentation, or someone reads the plan as a live description rather than a record. | `docs/superpowers/plans/2026-09-20-inkra-vault.md:1818`, `:1894` and `:2212` still embed the listings the "Ordering" section above just corrected — contiguity attributed to the comparator, and the byte-order trap stated as if it applied to `Path: Ord`. Left alone because the plan records phases already checked off; nothing is built against it |

## Rejected alternatives

**Synchronous `Fs`.** Zero new crates, and honest on local disk. Rejected
because `async-trait` costs one crate and retrofitting `.await` through a
settled API is the expensive direction.

**`async-trait` plus `smol::fs`, Zed's exact shape.** Rejected: six or more new
crates, and `smol::fs` is the `blocking` thread pool underneath, so it buys no
performance over a blocking read already running on gpui's background executor.

**Hand-rolled `BoxFuture` signatures.** Zero new crates and the same semantics.
Rejected for readability across three methods now and seven after the watcher.

**`Vault` as a module inside `app`.** Follows CLAUDE.md's letter. Rejected
because nothing would then stop a gpui dependency drifting in, which is what
makes later extraction painful.

**Path as entry identity.** Rejected — see "Identity" above. It was the first
draft's headline decision and it did not survive review.

**Per-transaction version ids.** Rejected — see "What the review changed".

**Rebuilding the sidebar tree per frame.** Correct at 200 notes. Rejected
because the linear walk is no more code once the comparator is right.

**Porting Zed's `FakeFs`.** Rejected: 2207 lines simulating inodes, monotonic
mtimes and a fake kernel watcher, amortized over hundreds of scanner tests.
`MemFs` is a `BTreeMap`.
