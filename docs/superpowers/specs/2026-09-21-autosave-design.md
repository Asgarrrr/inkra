# Autosave, atomic writes, and flushing on quit

Status: **implemented, and corrected in four places by what implementing it
found.** Slices 1-3 shipped on 2026-09-21; the empty state (slice 4) was split
into `docs/superpowers/plans/2026-09-21-autosave-quit-flush.md`. Each
correction below is marked **Corrected** inline rather than edited away, so the
reasoning that turned out wrong stays legible.

Every `file:line` below was read in `vendor/zed-reference` or in this repo.

## Why now

`inkra-vault` shipped open, list and save on 2026-09-20. Saving is one gesture,
`⌘S`, and nothing else writes. Three consequences, all recorded as Deferred in
`2026-09-20-inkra-vault-design.md`:

- `⌘Q` discards every dirty buffer with no prompt and no write.
- Nothing writes when the editor switches notes.
- `Fs::save` truncates before it writes (`crates/vault/src/real_fs.rs:106`), so
  a crash or a full disk mid-write leaves a note half-written or empty.

The three are one decision, not three: how often the file on disk is allowed to
lag the buffer. Deciding it as "rarely" makes the quit prompt unnecessary and
makes the truncating write dangerous, which is why atomic save comes in the
same slice rather than staying deferred behind it.

## The decision, and the evidence

Obsidian's cadence: a write about two seconds after input starts, then every two
seconds while the text keeps changing, plus a write on note switch, tab close
and quit. There is no prompt on quit, because nothing is ever more than about
two seconds dirty. The interval is not configurable; a community plugin exists
solely to turn it into an idle-debounce, and its stated motivation is the real
cost of the default — sync conflicts on Drive and Dropbox, latency on network
mounts, battery on laptops.

Obsidian pairs that with a safety net: the *File recovery* core plugin, on by
default, keeps full snapshots at least five minutes apart for seven days,
outside the vault, keyed by absolute path and deliberately not synced.

Zed sits at the other end. `AutosaveSetting`
(`vendor/zed-reference/crates/settings_content/src/workspace.rs:640-649`) offers
`Off`, `AfterDelay { milliseconds }`, `OnFocusChange` and `OnWindowChange`, and
**defaults to `Off`** — `ctrl-s` only. `AfterDelay` is an idle-debounce, not a
periodic write: each `ItemEvent::Edit` cancels the pending save and restarts the
timer (`crates/workspace/src/item.rs:936-952`, driving
`DelayedDebouncedEditAction` at `crates/workspace/src/workspace.rs:1445-1490`).

We take Obsidian's cadence and Zed's mechanism. Periodic, not debounced: an
idle-debounce bounds nothing, because ten minutes of continuous typing never
reaches idle. Periodic bounds the loss at one interval regardless of how the
user types, which is the only property that makes the quit prompt droppable.

We do **not** take Obsidian's snapshot store. Atomic writes make each individual
write safe, which is the part that matters when writes become frequent; a local
history is a feature, not a safety net for this slice, and it would need its own
storage, retention policy and UI.

**Interval: 2000 ms**, a constant in `app`, not a setting. This repo has no
settings system and adding one to hold a single number is the wrong order.

## What we take from Zed, and what we refuse

| Piece | Zed | Verdict |
|---|---|---|
| Hold the pending save as a `Task`, and have the *next* one `await` the previous rather than drop it | `workspace.rs:1476-1481` | **Take.** Dropping a `Task` cancels it (`scheduler/src/executor.rs:389-390`), so the obvious "replace the slot" cancels a write already in flight |
| A cancel channel, so a new edit restarts the delay | `workspace.rs:1469-1474` | **Refuse.** That is the debounce we just rejected |
| Skip formatting on an autosave | `pane.rs:2539-2551` | **Not applicable.** We have no formatter |
| `atomic_write` via a temp file in the destination's own directory, then rename | `fs/src/fs.rs:974-987` | **Take**, hand-rolled. See "Atomic writes" |
| `tempfile::NamedTempFile` to get there | `fs/src/fs.rs:978` | **Refuse.** A new dependency for about fifteen lines, and its Unix mode is 0600, which is the exact failure we have to correct afterwards anyway |
| Autosave as a user setting with four modes | `workspace.rs:640-649` | **Refuse.** One cadence, one constant, until someone asks |
| Atomic writes for buffer saves | — | **Zed does not.** `Fs::save` for buffers is `File::create`, truncating (`fs/src/fs.rs:1019-1033`); `atomic_write` is used for Zed's own config files. Zed can afford it because its default is `Off` — the user chose the moment. Ours writes unprompted, so we diverge deliberately |

## The tension this slice exists to resolve

A vault entry is identified by `(device, inode)` — `FileIdentity`, and
`Vault::rescan` mints a new `EntryId` for an identity it has not seen
(`crates/vault/src/vault.rs:111-118`).

An atomic write is `write temp; rename(temp, path)`. After it, the path resolves
to the **temp file's inode**. The original inode is unlinked. So every atomic
save changes the note's identity.

Left alone, that is a defect waiting for the next slice. Nothing calls `rescan`
after startup today, so it is latent — but create/rename/delete cannot avoid
calling it. The next `rescan` after a save would see an unknown identity, mint a
fresh `EntryId`, and leave `NoteStore.open` keyed by the old one. The sidebar row
would look unchanged and clicking it would load the file from disk into a new
buffer: the unsaved text and the whole undo history, gone. That is critical
defect 2 of the vault design reappearing through a different door.

The fix is to make the new identity flow back:

- `Fs::save` returns what the write produced, not just its mtime.
- `NoteStore` records it on the open note and hands it to the `Vault`, which
  re-keys the entry so its `EntryId` survives the next rescan.

## Design

### `vault`: `Fs::save` reports identity

```rust
/// What a write produced. `Fs::save` may replace the file rather than rewrite
/// it in place, so the identity afterwards is not the identity before.
pub struct Saved {
    pub mtime: Mtime,
    pub identity: FileIdentity,
}

async fn save(&self, path: &Path, text: &str) -> Result<Saved>;
```

Symmetric with `Loaded`, which already carries both.

### `vault`: `RealFs::save` writes atomically

1. `fs::metadata(path)` — the existing file's permissions, if it exists.
2. Create `<path>.inkra-<pid>-<counter>.tmp` in the **same directory**. Same
   directory, not the system temp dir: a rename across devices fails with
   `EXDEV`, which is why Zed does the same (`fs/src/fs.rs:976-978`).
3. Write and `sync_all`, so the rename cannot publish a name whose contents are
   still in flight.
4. Restore the original file's mode onto the temp file, when there was one. A
   fresh file otherwise keeps the process umask, which is what `fs::write` gives
   today. Skipping this silently narrows a 0644 note to whatever the temp file
   was created with.
5. `fs::rename(temp, path)`.
6. `fs::metadata(path)` → the new `Saved`.

A failure at any step removes the temp file and leaves the original untouched.
That is the whole point: there is no window in which `path` holds a partial note.

> **Corrected.** As written this was prose, not a mechanism — nothing in the
> sequence removed anything, and an early `?` would have leaked the temp file.
> It is an RAII guard in the implementation: a value whose `Drop` unlinks the
> temp file unless the rename disarmed it. Steps 3 and 5 are also less than
> they claim. `sync_all` flushes the temp file's *contents*; it does not make
> the rename durable, which needs the parent directory fsynced. Neither we nor
> Zed does that, so a power cut can still lose the rename itself.

Two costs this buys, stated rather than discovered later:

- **Hard links break.** The other name keeps the old inode and the old content.
  `Vault::rescan` already collapses hard links to one entry
  (`vault.rs:108-110`), so the vault never showed both names anyway.

  > **Corrected — this had the consequence backwards.** Collapsing to one entry
  > is exactly what makes replacing the file *worse*, not harmless: splitting
  > the pair gives the second name its own identity, so it surfaces as a new
  > row holding stale text — a duplicate note appearing because of a write
  > nobody asked for. `RealFs::save` therefore detects `nlink > 1` and writes
  > **in place**, keeping the inode. A symlink is written through for the same
  > reason, and needs its own `symlink_metadata` check because `fs::metadata`
  > follows the link and reports the target's link count.
- **Extended attributes are lost** — on macOS that includes Finder tags. The
  temp file has none, and the rename replaces the original wholesale. Preserving
  them needs `renamex_np(RENAME_SWAP)` or a copy of the xattr set, and neither
  belongs in this slice.

### `vault`: `MemFs::save` mints a new identity

`MemFs::insert_file` deliberately reuses the identity of an existing path
(`mem_fs.rs:91-95`) — that is right for `insert_file`, which models an external
writer rewriting a file in place. It is wrong for `save` once `save` renames.

`MemFs::save` therefore allocates a fresh identity, the way a rename does.
Without this the re-keying path has no test that can fail: every `app` test runs
on `MemFs`, and a fake that preserves identity makes the bug invisible in
exactly the tests meant to catch it.

### `vault`: `Vault::reidentify`

```rust
/// Re-key an entry after a write replaced the file. Without it the next rescan
/// sees an unknown `(device, inode)` and mints a new `EntryId`, orphaning the
/// open buffer keyed by the old one.
pub fn reidentify(&mut self, id: EntryId, identity: FileIdentity);
```

Removes whatever identity currently maps to `id`, inserts the new one, and
leaves `entries` alone — the path did not change.

### `app`: `NoteStore` schedules the writes

`OpenNote` gains two fields:

```rust
identity: FileIdentity,   // as of the last load or save
save: Option<Task<()>>,   // the pending write chain; see below
scheduled: bool,          // a chain is queued and has not yet passed its timer
```

`NoteStore` gains one:

```rust
/// One per open note. A buffer with no observer never schedules a save.
observers: HashMap<EntryId, Subscription>,
```

**Scheduling.** `open_note` and `adopt` register `cx.observe(&buffer)`. The
callback does one thing: `schedule_save(id, AUTOSAVE_DELAY, cx)` when the note
is dirty (`is_dirty`). A clean notify, such as a reload (watcher spec, F8),
schedules nothing. It reads the buffer's version but must not lease the
buffer — an observer callback that leases the buffer it observes panics
(`gpui/app/entity_map.rs:231`).

`schedule_save(id, delay)`:

- returns immediately if `scheduled` is already true — this is what makes the
  cadence periodic rather than debounced. The pending chain keeps its original
  deadline, so continuous typing produces a write every `delay`, not never.
- otherwise sets `scheduled = true`, takes the previous `Task` out of `save`,
  and installs a new one that **awaits the previous task first**, then sleeps
  `delay`, then clears `scheduled` and performs the write.

Awaiting rather than dropping is the whole reason to copy Zed's shape: assigning
over `save` while the old chain is mid-write would cancel that write. The
previous task is moved into the new future, so nothing is dropped.

**Writing** is today's `save_current` body, generalised to a note id: lease the
buffer with `update` (never `as_mut` — a save changes no text and must not
notify), `finalize_transaction`, capture `(text, version)`, return early if
`version == saved_version`, write, then on success record `saved_version`, the
new identity, and call `Vault::reidentify`.

> **Corrected.** Routing `⌘S` through the same queue is necessary but not
> sufficient: a chain that treats a *dropped* cancel sender the same as a
> deliberate one makes `⌘S` write nothing when the next keystroke replaces the
> sender. The arm must return only on `Ok`. See
> `a_keystroke_after_cmd_s_does_not_cancel_the_write_it_asked_for`.

**`⌘S`** becomes `schedule_save(current, Duration::ZERO)`. It keeps its meaning
— write now — and goes through the one queue, so it cannot race a pending
autosave of the same note.

**Note switch.** `set_current` schedules an immediate save of the outgoing note
before recording the new one. It is the single writer of `current`
(`CLAUDE.md`), so it is the only place that knows a switch happened.

### `app`: flush on quit

`cx.on_app_quit` (`gpui/src/app.rs:2454-2470`) registers a handler returning a
future; `App::shutdown` (`:1057-1082`) collects them, clears the windows,
`flush_effects`, then blocks the foreground executor on all of them for
`SHUTDOWN_TIMEOUT`, 200 ms (`:75`). Nothing registers one today, which is the
whole Deferred row.

`NoteStore::flush_all(cx) -> impl Future<Output = ()>` with one hard constraint:

> It captures every dirty note's `(path, text)` **synchronously**, before
> returning the future, and the future then does nothing but `fs.save`.

The future must never call `this.update`. `TestAppContext::quit`
(`gpui/src/app/test_context.rs:233-235`) calls `shutdown()` while holding the
app's `RefCell` borrow; a future that re-enters the `App` to update `NoteStore`
would panic on the second borrow. Bookkeeping at quit is pointless anyway —
nothing reads `saved_version` after the process exits.

Registration lives in `main.rs`, which means hoisting `buffer`, `editor`,
`vault` and `notes` out of the `open_window` closure. They are all `App`-level
entities; only `AppShell` needs the window.

The 200 ms budget is a real limit and not one we can raise. With a 2 s interval,
a quit flush writes at most 2 s of typing per open note, and only for notes
dirtied since their last autosave. A vault where that exceeds 200 ms is the
trigger for the deferred "serialized writes" work, not something to pre-solve.

## Testing

`vault`, plain `#[test]` plus the real-disk suite:

- `MemFs::save` changes the identity; a subsequent `load` reports the new one.
- `Vault::reidentify` keeps the `EntryId` across a `rescan` that follows a save.
  Without `reidentify` the id changes — that is the assertion that fails today.
- `real_fs.rs`: a save changes the inode, preserves the file mode, and leaves the
  content intact. Mode preservation needs a `chmod` in the test, because the
  default already matches by accident.
- `real_fs.rs`: no `*.tmp` file survives a successful save.

`app`, `#[gpui::test]` with `MemFs`:

- Typing and advancing the clock past the interval writes the file with no `⌘S`.
- Typing, then advancing **half** the interval, writes nothing — the write is
  scheduled, not immediate.
- Continuous edits across three intervals produce a write per interval. This is
  the test that fails under an idle-debounce and is the reason the cadence is
  written down as a decision.
- Switching notes writes the outgoing one.
- A save re-keys the store's identity: after a save, a `rescan` leaves
  `buffer_for(id)` returning the same entity.
- `cx.quit()` writes a dirty note. Checked against
  `MemFs::read_to_string` after the call returns.

Clock control: `#[gpui::test]` drives a test executor, so the interval is
advanced explicitly rather than slept through. `advance_clock` is the method to
confirm at implementation time.

## Risks

- **`block_with_timeout` under the test dispatcher.** `shutdown` blocks the
  foreground executor on the quit futures. Whether a `#[gpui::test]` can drive a
  background write to completion inside that block is not established by
  reading; the `flush_all` shape above is designed to need as little of the
  runtime as possible, but this is the one item to prove first, before building
  on it. If it does not hold, the fallback is a plain `#[test]` over
  `flush_all`'s synchronous half plus a manual run for the rest, stated as such.
- **Write amplification.** Every 2 s of editing is a create + write + fsync +
  rename. On a local SSD this is nothing. On a synced vault it is exactly the
  complaint the Obsidian plugin exists to answer. No trigger has fired for us;
  recorded, not pre-solved.
- **A failed autosave is silent.** `save_current` reaches `eprintln!` today
  (`app/src/vault.rs:175`), which a released binary has nowhere to show. Making
  writes automatic makes failures both more likely and less noticeable. This
  slice does not fix it — see below — and that is the weakest point in it.

## Out of scope, with triggers

| Item | Why not here | Trigger |
|---|---|---|
| A home for user-visible errors | Its own slice: it needs a status line in `AppShell` and it serves failed *opens* too, not just failed writes | Unchanged, and this slice raises it: the first failed autosave a user does not see |
| Saving the unnamed launch buffer | Unchanged by this slice, and worse in it: a launch buffer now silently never saves rather than silently ignoring one keystroke | Unchanged |
| Create / rename / delete | The slice this one unblocks — it is the first caller of `rescan` after startup, which is what makes `reidentify` load-bearing rather than latent | Unchanged |
| Serialized writes across notes | The per-note chain already serialises a note against itself, which is where the race was | A vault on a network or FUSE mount |
| Conflict detection | `Saved` now carries the mtime a `saved_mtime` would be seeded from, so this slice moves it closer without doing it | Lands with the watcher |
| CRLF round-trip | Untouched: the write path still emits LF | Unchanged |
| Local history / snapshots | Atomic writes cover the crash case; snapshots cover the "I wrote over it" case, which is a feature | A user asks to recover a previous version |

## Rejected alternatives

**Idle-debounce, Zed's `AfterDelay`.** Fewer writes, and it is the mechanism we
are already copying. Rejected because it bounds nothing: a long uninterrupted
writing session never reaches idle, which is precisely the session whose loss
hurts most.

**Explicit `⌘S` plus a native prompt on quit.** Keeps the modified dot exactly
meaningful and writes the least. Rejected as the most work of the three — a
`cx.prompt` with a cancel path that has to abort the quit, and no way to
exercise it in a headless test.

**Keeping the truncating write.** What Zed does for buffers
(`fs/src/fs.rs:1019-1033`), and it would leave `EntryId` alone entirely.
Rejected because Zed's default is `Off`: a truncating write at a moment the user
chose is a different risk from one every two seconds, unprompted.

**`tempfile`.** The obvious way to get the temp file, and what Zed uses.
Rejected under the dependency rule: about fifteen lines of `std::fs`, and
`NamedTempFile` creates at 0600 on Unix, so the mode has to be corrected
afterwards either way.

**Autosave owned by `EditorView`.** It already observes the buffer. Rejected: it
sees one buffer, and a note edited then switched away from must still be written.
`NoteStore` holds every open note and already owns `saved_version`.

## Slices

Each touches the filesystem layer, the behaviour above it, and its own tests.

1. **Atomic write, identity preserved.** `Saved`, `RealFs` temp-and-rename,
   `MemFs` fresh identity, `Vault::reidentify`, `NoteStore` re-keys after a
   write. End to end: `⌘S` writes atomically and a rescan afterwards keeps the
   note's id.
2. **Autosave.** Observers, `schedule_save`, the chain, `⌘S` routed through it,
   save on note switch. End to end: type, wait, the file changes.
3. **Flush on quit.** `flush_all`, `on_app_quit`, the `main.rs` hoist. End to
   end: type, quit, the file is written.

   > **Corrected — this slice was a third of the size it looked.** The flush
   > cannot route through the per-note chain: `App::shutdown` blocks the
   > foreground session while it polls the quit futures
   > (`gpui/src/app.rs:1073`, `scheduler/src/test_scheduler.rs:100`, `:362`),
   > so the chain's `cx.spawn` half can never resume during the block. It
   > issues its own write, which means one note briefly has two writers whose
   > `rename()`s race — and the loser can be the *older* text. Ordering them
   > needs a monotonic sequence, because `Buffer::version` goes **down** on an
   > undo (`text/src/buffer.rs:114-119`) and `futures::lock::Mutex` guarantees
   > no acquisition order at all. See `Writer` in `app/src/vault.rs`. There was
   > no `main.rs` hoist in the end: the subscription lives in
   > `NoteStore::new`, which is where the state it captures already is.

4. **An empty state when no note is open.** Split out after review found it
   needs a focus design of its own — with `AppShell.editor` optional, nothing
   in the current shape can move window focus to the editor built on the first
   activation. Planned in
   `docs/superpowers/plans/2026-09-21-autosave-quit-flush.md`.

## Documentation updated with the change — all done

- `CLAUDE.md`: the `NoteStore::current` paragraph no longer reads as though
  `current` gates every write; the don't-recreate table carries `Saved`,
  `Vault::reidentify`, `NoteStore::schedule_save`, `SaveFailed`,
  `AUTOSAVE_DELAY`, `write_through`, `Writer`, `flush_all`, `Gate`,
  `block_save` and `make_unwritable`; the testing section records how to quit
  in a test and why write ordering is not a `#[gpui::test]`.
- `docs/roadmap.md`: the Vault section says what actually ships, and names
  conflict detection as the largest remaining hole.
- `2026-09-20-inkra-vault-design.md`: atomic save and flush-on-quit are closed;
  serialized writes is narrowed to the cross-note case; conflict detection, the
  unnamed launch buffer and buffer eviction have current triggers; the "Saving
  is not atomic" note is marked superseded and points here.
