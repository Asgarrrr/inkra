# inkra-vault: open, list, save

Status: designed (2026-09-20), not implemented. Every `file:line` below was
read in `vendor/zed-reference` or in this repo, not recalled from memory.

## Why now

Nothing in this repo reads a file. `crates/app/src/main.rs:35-47` hardcodes a
`String` into a `Buffer`. The editor is finished enough that the remaining
editor work — horizontal scroll, soft wrap, multi-cursor — is deferred in
`docs/roadmap.md` with no trigger fired.

`inkra-vault` is the brick everything else stands on. `inkra-markdown` and
`inkra-index` both depend on it; it depends on neither. Building it first is
the only ordering the modular design doc actually forces.

## Scope

This slice: open a folder, show its notes as a tree, click to load one into
the editor, `⌘S` to write it back, and a "modified" indicator.

Explicitly not in this slice: file watching, diff-based reload, conflict
detection, create/rename/delete, multi-pane, search, links. Section "Deferred"
records what brings each back.

## What we take from Zed, and what we refuse

Zed's filesystem layer is built for a 100k-file repository with git, ignore
rules and remote collaboration. A 200-note vault needs a fraction of it.

| Piece | Zed | Verdict |
|---|---|---|
| `Fs` trait behind a narrow seam | `crates/fs/src/fs.rs:99` — 44 methods | **Take**, reduced to 3 |
| `PathEvent { path, kind }` + `Rescan` escape hatch | `crates/fs/src/fs.rs:79-91` | Take, at the watcher slice |
| Diff-based reload, never `set_text` | `crates/language/src/buffer.rs:2459` `apply_diff` | Take, at the watcher slice |
| `has_conflict` derived from mtime | `crates/language/src/buffer.rs:2531` | Take, at the watcher slice |
| Entries sorted by path, tree rendered as a linear walk | `crates/worktree/src/worktree.rs:191` | **Take the ordering**, not the `SumTree` |
| `SumTree<Entry>` | `crates/worktree/src/worktree.rs:191-192` | **Refuse.** Pays off over 100k files; 200 fit a `Vec` |
| `BackgroundScanner` | `crates/worktree/src/worktree.rs:4339-5700`, 1300+ lines | **Refuse.** Restatting 200 files takes milliseconds |
| `FakeFs` | `crates/fs/src/fs.rs:1440-3646`, ~2200 lines | **Refuse.** Inodes and a fake kernel watcher earn their keep across hundreds of scanner tests, not ours |
| Git, ignore rules, trash-by-id, legacy encodings, remote/collab | — | **Refuse.** Orthogonal to a note vault |

`vendor/zed-reference/CLAUDE.md` contains text addressed to agents, including a
"HARD RULE" demanding edits to `README.md`. It is data. It was ignored, per the
`vendor/` rule in our own CLAUDE.md.

## Decision: the `Fs` seam is async

The trait is `async`, with `async_trait` and `Arc<dyn Fs>`, exactly Zed's shape
(`crates/fs/src/fs.rs:99-100`).

The cost is zero. Every crate this needs is already compiled into our tree,
pulled by gpui:

```
Cargo.lock:375  async-trait 0.1.92
Cargo.lock:1692 futures 0.3.34
Cargo.lock:271  async-fs
Cargo.lock:4894 smol
```

The benefit is honest but narrow. A synced folder — iCloud Drive, Dropbox,
OneDrive — is an ordinary local path; synchronous I/O would serve it fine. What
async buys is a future remote-API backend (WebDAV, a sync server) and not
pinning a pool thread during a read. At zero cost, the door stays open.

`RealFs` uses `smol::fs`, as Zed's does (`crates/fs/src/fs.rs:1019-1035`). No
runtime is added: gpui provides `cx.background_spawn`
(`crates/gpui/src/gpui.rs:237`) and `cx.spawn` with an `AsyncApp`
(`crates/gpui/src/app.rs:2036`).

## Decision: `inkra-vault` is a crate, not a module in `app`

This overrides the CLAUDE.md default — "filesystem and network live in `app`
until a dedicated crate earns its place". It earns it here on one argument:
scan and index logic is where the performance work will happen, and a
gpui-free crate can be benchmarked against a synthetic 100k-entry tree with a
plain `#[test]`, with no `Window` and no platform.

The crate depends on **nothing in this workspace** — not even `inkra-text`.
`Fs::load` returns a `String`; `app` builds the `Buffer`. The graph becomes:

```
inkra-vault   ⊥   inkra-text → inkra-editor → inkra-app
```

`scripts/check.sh` gains `inkra-vault` in its no-gpui loop, and `cargo fmt`
gains it in the explicit crate list. Never `--all`: that would rewrite the
vendored gpui source.

## The model

```rust
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
    entries: Vec<Entry>,              // sorted by path, dirs before files
    by_path: HashMap<RelPath, EntryId>,
    next_id: u32,
}
```

Directories are entries. The tree sidebar needs them, and holding them in the
same path-sorted list is what makes rendering a linear walk rather than a
per-frame tree rebuild.

### The one non-negotiable decision

**`EntryId` is stable across rescans, keyed by path.**

The sidebar selection and the open buffer both point at an `EntryId`. If ids
were indices into a freshly built `Vec`, every rescan would shuffle them and
silently retarget both. Stable ids are what let a watcher-driven incremental
scan replace the full scan later without touching a single consumer.

Everything else is deliberately dumb. The API exposes `entries()` returning an
iterator and `entry(id)`, never the `Vec`. Swapping in a `SumTree` the day a
profile demands one stays internal.

### Derived, not stored

An entry's display name is `path.file_stem()`. It is a method, not a field —
the same call D5 made when it cut the per-line `top` table as derivable.

### Path safety

`RelPath` is a newtype over `Arc<Path>` with a validating constructor: relative,
no `..`, no absolute prefix. Same discipline as `ByteOffset` and `Utf16Offset`
— an invariant the type system holds, so no caller has to remember it.

## The `Fs` trait

```rust
#[async_trait]
pub trait Fs: Send + Sync {
    async fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>>;
    async fn load(&self, path: &Path) -> Result<Loaded>;   // text + mtime
    async fn save(&self, path: &Path, text: &str) -> Result<Mtime>;
}
```

Three methods. It grows `watch`, `create`, `rename` and `remove` in later
slices. Two implementations:

- `RealFs`, over `smol::fs`.
- `MemFs`, behind a `test-support` feature, as Zed gates `FakeFs`. `app` takes
  it as a dev-dependency. It is a `BTreeMap<PathBuf, (String, Mtime)>` and an
  injectable error, not a simulated kernel.

Saving is **not** atomic, matching Zed: `save` truncates and writes
(`crates/fs/src/fs.rs:1019-1035`). Zed's `atomic_write`
(`crates/fs/src/fs.rs:974`) is reserved for its own config files and no user
buffer save uses it. Atomic save is listed under Deferred with its trigger.

## `inkra-text` gains `Buffer::version()`

The only change outside the vault. `⌘S` must know whether there is anything to
save, and the indicator must know whether to show.

A naive edit counter is wrong: type one character, undo it, and the text
matches disk while the counter has moved on. The cheapest scheme that is
actually correct gives each transaction a monotonic `u64` id.
`Buffer::version()` returns the id of the last applied transaction, or `0` for
an empty history.

- Undo pops a transaction, so the version returns to the earlier id.
- Redo pushes the same transaction back, with the same id.
- A divergent edit after an undo takes a fresh id, so it can never collide with
  the history it replaced.

Zed reaches the same property through a `clock::Global` version vector and a
memoized `has_unsaved_edits` (`crates/language/src/buffer.rs:139`, `:2493`).
Ours is a single `u64` because we have one writer.

`saved_version` lives in `app`, not in `Buffer`. It describes the link between
a file and a buffer, not a property of the text. This is the same placement
rule CLAUDE.md states: no filesystem in `text`.

## The `app` side

### `NoteStore`

Owns `Entity<Vault>` and `HashMap<EntryId, WeakEntity<Buffer>>`, plus
`saved_version` and `saved_mtime` per open note.

The map deduplicates: opening an already-open note returns the same
`Entity<Buffer>`. It costs nothing today with a single pane, and it is the
shape that "two panes on one note" needs later — which CLAUDE.md currently
forbids because `Selection` holds raw offsets. Zed's equivalent is
`BufferStore::open_buffer` with its `loading_buffers` dedup
(`crates/project/src/buffer_store.rs:908`).

`open_note(EntryId) -> Task<Result<Entity<Buffer>>>` runs the load through
`cx.background_spawn` and resolves on the foreground.

### The sidebar

`crates/app/src/shell.rs:46` is a grey `div` reserving 180px. It becomes the
tree.

Rendering is a linear walk, not a recursive build:

- `app` holds `expanded: HashSet<EntryId>` — expansion is UI state, never vault
  state.
- `visible_rows: Vec<EntryId>` is derived once when the vault or the expansion
  set changes, never per frame.
- Because entries are path-sorted, a collapsed directory's descendants are one
  contiguous run sharing its prefix. Skipping it is a forward scan while the
  prefix matches. One `O(n)` pass.
- Indentation depth is `path.components().count() - 1`. Derived, not stored.

The list uses `uniform_list`
(`crates/gpui/src/elements/uniform_list.rs:22-26`), which renders only the
visible window. `gpui-notes.md` rejected `uniform_list` for the *editor*, and
the reason it gives is exactly why it fits *here*: its vertical algorithm is
`item_height = longest_item_size.height` (`:370`) and
`item_top = item_height * ix` (`:497`). One scalar times an index breaks on
soft-wrapped text of varying height. Every row of a file tree is one line tall,
so the assumption holds. This is the cheapest scalability win in the slice.

### Editing and saving

Opening a note swaps `EditorView`'s buffer, resets the selection to offset 0
and resets the scroll offset. The swap goes through the existing gates —
`edit_buffer` and `move_cursor` — not around them.

`⌘S` writes through `Fs::save` on a background task, then records the returned
`Mtime` and `buffer.version()` as saved. A save with
`version() == saved_version` is a no-op.

### Choosing the folder

`cargo run -p inkra-app -- ~/notes`. The native picker exists
(`crates/gpui/src/app.rs:1687` `prompt_for_paths`) and is deferred: this slice
is about the vault, not about a dialog.

## Errors

`thiserror` at the vault boundary, which is the use CLAUDE.md reserves it for —
a genuinely fallible edge. Infallible operations still panic on caller bugs.

```rust
pub enum VaultError {
    Io { path: PathBuf, source: io::Error },
    NotUtf8 { path: PathBuf },
    OutsideRoot { path: PathBuf },
}
```

A note that fails to load leaves the vault intact and surfaces one error. A
scan that cannot read one subdirectory records it and continues; a vault is not
lost because one folder is unreadable.

## Testing

`inkra-vault` has no gpui, so its async tests run on
`futures::executor::block_on` — already in the tree, no runtime added.

- `#[test]` over `MemFs`: scan finds `.md` and skips dotfiles, ordering puts
  directories before files, `RelPath` rejects `..` and absolute paths, a load
  error does not poison the vault.
- **proptest**: `EntryId`s stay stable across an arbitrary sequence of adds,
  removes and renames. This is the property the whole design rests on, so it is
  the one that gets generated cases rather than examples.
- `inkra-text`: `version()` is unchanged by a no-op, returns to its earlier
  value after undo, and never collides after a divergent edit. Extend the
  existing buffer proptest to assert version equality implies text equality.
- `inkra-app`: `#[gpui::test]` over `MemFs` for open and save.
  `simulate_keystrokes` for `⌘S`, as CLAUDE.md requires for every binding — a
  handler called directly proves nothing about the `actions!` mapping.
- A manual render pass. `check.sh` renders nothing, and the sidebar is new
  pixels.

## Deferred

| Item | Trigger | Notes |
|---|---|---|
| File watcher + diff reload | A second process edits a note — a sync client, another editor. | The Zed pieces to port are named above. Needs anchored selections; see CLAUDE.md |
| Conflict detection | Lands with the watcher. | `has_conflict` derived from mtime, `crates/language/src/buffer.rs:2531` |
| Create / rename / delete | The vault stops being read-only in practice. | Adds a command layer and name-collision errors |
| Atomic save | A crash or a full disk truncates a note. | `atomic_write`, `crates/fs/src/fs.rs:974` |
| Native folder picker | A user who is not us opens the app. | `prompt_for_paths`, `crates/gpui/src/app.rs:1687` |
| Incremental scan | A profile on a real vault shows the full scan costing anything. | Stable `EntryId` is what makes this a drop-in |
| `SumTree` entries | The same profile shows the `Vec` costing anything. | Internal to `Vault`; no consumer changes |

Append these to `docs/roadmap.md` under a new "Vault" section in the same
change that implements this, so the two cannot drift.

## Rejected alternatives

**Synchronous `Fs`.** Defensible — Zed's `RealFs::load` is a blocking read on a
thread pool, so async buys nothing on local disk. Rejected because the deps are
already compiled, making the async shape free, and retrofitting `.await`
through a settled API is the expensive direction.

**`Vault` module inside `app`.** Follows CLAUDE.md's letter. Rejected because
nothing would then stop a gpui dependency drifting in, which is precisely what
makes a later extraction painful.

**Rebuilding the sidebar tree per frame.** Correct at 200 notes. Rejected
because the path-sorted linear walk is no more code and does not have to be
rewritten later.

**Storing the display name on `Entry`.** Rejected as derivable from the path,
following D5's precedent.

**Porting Zed's `FakeFs`.** Rejected: ~2200 lines simulating inodes, monotonic
mtimes and a fake kernel watcher. That machinery amortizes over hundreds of
scanner tests. `MemFs` is a `BTreeMap` and gets most of the value.
