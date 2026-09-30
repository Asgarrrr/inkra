# Restore tabs on launch

Status: **reviewed 2026-09-30** by a red team and a blue team (Sonnet); their
findings are folded in (R4–R8).

## Why

Tabs do not survive a restart (tabs spec, decision 2). A launch on a vault
opens one empty tab, so the working set is rebuilt by hand every time. The
tabs spec kept a tab's state as plain data so that restoring it "is a
serialization, not a rewrite". The user asked for it on 2026-09-30, which
fires the Deferred row "Restore tabs on launch".

## Decisions taken

Taken by the orchestrator on 2026-09-30 (the user delegated them: "je te
laisse voir comment Zed gère ça"). Red/blue review may overturn any of them.

1. **What survives:** the tabs in bar order, the active tab, and each tab's
   view (selection and top line). Back, forward and the closed stack do not
   survive: Zed persists none of them either.
2. **Only a vault launch restores and writes.** `Launch::Note` and
   `Launch::Nothing` read and write no session.
3. **Stored outside the vault**, per vault, in
   `~/Library/Application Support/Inkra/sessions/`. The open-first spec
   decided this (Deferred, "Reopen the last note"): a state file synced with
   the vault conflicts across devices.
4. **The first frame does not wait.** The session loads behind the window and
   applies after the first whole walk that succeeds, as Zed restores items after its window
   opens.
5. **The user's working set wins.** If the user has opened a note or a tab
   when the restore lands, the restore is dropped, never merged (R6).

## What Zed does

Research on 2026-09-30, in `vendor/zed-reference/crates/`:

- One SQLite file under `~/Library/Application Support/Zed/db/`. A workspace
  is keyed by its sorted root paths (`workspace/src/persistence.rs:1093`).
  Items hold an `item_id` that is a per-process entity id; editors store an
  absolute path, the top row plus a pixel offset, and selections as byte
  offsets (`editor/src/persistence.rs:131-228`).
- Writes: `serialize_workspace` is throttled at 200 ms
  (`workspace/src/workspace.rs:177`, `:7652-7667`). The snapshot is built on
  the foreground, written with `background_spawn` (`:7755-7764`). Scroll saves
  10 ms after a change, selections 200 ms after. On quit,
  `flush_serialization` cancels the throttles and writes everything, and quit
  awaits it (`:7563-7597`).
- Nav history is not persisted; only a 20-path MRU for the file finder is.
- Restore does not block the first frame (`zed/src/main.rs:921-980`). A path
  argument still restores the folder's workspace (`workspace.rs:2194`). An
  item whose file is gone is dropped and indexes are recomputed
  (`workspace/src/persistence/model.rs:413-420`). Selections are clipped to
  the buffer.
- Reads are lenient: an error logs and yields defaults. A DB that fails to
  open falls back to memory.

## Deviations from Zed

| Zed | Inkra | Why |
|---|---|---|
| SQLite, migrations | One JSON file per vault, a `version` field | A few tabs; one document, written whole |
| Throttle 200 ms, per-item channels | One throttle, 1 s, over one snapshot | One window, one pane; a crash loses at most a second of tab changes, never text (autosave) |
| Absolute paths | Vault-relative paths | The vault can move; `EntryId`s are per process |
| Hot exit of dirty text | None | Autosave and the quit flush already write every note |
| Restores on any launch | Vault launch only | A single-file launch has no tabs to speak of |

## Architecture

### R1. The file

`~/Library/Application Support/Inkra/sessions/<key>.json`, where `<key>` is
the 16 hex digits of FNV-1a 64 over the bytes of the vault's absolute root
(R4). FNV is written out (a dozen lines), not `DefaultHasher`, whose
output Rust does not promise to keep stable. The file records `root`; a file
whose `root` differs from the vault's is ignored (a hash collision, or a
copied file).

```json
{
  "version": 1,
  "root": "/Users/me/Notes",
  "active": 1,
  "tabs": [
    { "note": "daily/2026-09-30.md", "view": { "tail": 12, "head": 40, "top": 0, "below": 0.0 } },
    { "note": "projects/inkra.md", "view": { "tail": 880, "head": 880, "top": 512, "below": 6.5 } },
    { "note": null }
  ]
}
```

- `note` is a vault-relative path, `null` for an empty tab. `view` is absent
  when the tab has none (an inactive tab parked with no view).
- Offsets are byte offsets. `ViewState::clipped` already clips a view onto a
  buffer's graphemes, so a file changed between runs restores a valid, if
  shifted, view. `Goal` is not stored: it restores as `Goal::None`.
- A missing file restores nothing, silently: a vault's first launch has none.
  An unreadable, corrupt, other-version or other-root file restores nothing
  and logs one line. Either is overwritten by the next write.

### R2. The format, `app/src/restore/format.rs`

Pure data, no gpui: `Session { version, root, active, tabs: Vec<SavedTab> }`
with `serde` derives. The conversions `SavedView <-> ViewState` live in
`restore/view.rs`, since they need the editor's types. `serde` and
`serde_json` are already in `Cargo.lock` through gpui; `app` declares them
directly (only `app` uses them, so not in `[workspace.dependencies]`).
`TopLine` gains a `pub(crate)` constructor and accessors for its two fields.

### R3. I/O through `Fs`

`app` opens no file (CLAUDE.md). `Fs` gains one method:

```rust
/// Replaces `path` atomically with `text`, creating missing folders above it.
/// No conflict check: for the app's own state, never a note.
async fn replace(&self, path: &Path, text: &str) -> Result<()>;
```

`RealFs` reuses `temp.rs`'s temp-then-rename. `MemFs` stores it. Reading uses
`Fs::load`; its line-ending normalization is harmless for JSON. Writes run on
the background executor.

### R4. Where the path comes from

`bootstrap::launch` makes the vault root absolute once, with
`std::path::absolute` (lexical: no symlink resolved, as `RealFs::trash`
already does, `real_fs.rs:229-231`). That root feeds `Vault::new`, the FNV key
and `Session::root`, so `./Notes`, `Notes/` and `/Users/me/Notes` share one
session. It never canonicalizes: a symlinked vault keeps its own key.

For a vault launch it computes the session path from `$HOME` and carries it:
`Launch::Vault(Vault, Option<PathBuf>)`. `None` (no `$HOME`) disables restore
and writes. Tests pass a path inside their `MemFs`. `AppShell` receives it
through `root`.

### R5. Writing

`app/src/restore/` is pure and imports no shell type: the format, the plan
(R6), the read, and `SessionWriter`, which owns the `Fs` and the path and
orders its writes. `shell/session.rs` holds the shell side: the `Restore`
state (R6) and `SessionFile` (the writer, the throttle task, the quit hook).
None of it lives in `shell/tabs.rs`.

- **One writer.** `AppShell::write_session` returns `None` unless the state
  is `Restore::Done` (R6), then hands the snapshot to
  `SessionWriter::write(&mut self, vault, tabs, active, cx)`. The throttle and
  the quit hook both call it; nothing else writes. A quit in the first 100 ms
  therefore cannot overwrite the saved session with the launch's one empty tab.
- **Snapshot:** built on the foreground from `Tabs::iter()` in bar order. The
  active tab's view is the bound editor's `view_state`, and its note is the
  bound note: a tab whose trash is pending saves empty. An inactive tab's view
  comes from `Parked::peek`: its anchors resolve through
  `Buffer::resolve` (`crates/text/src/buffer.rs:412`, `&self`), which forgets
  nothing. `ParkedView`, `ParkedSelection`
  and `ParkedTop` each gain a `peek(&self, ...)` beside their consuming
  `unpark`. Paths come from `Vault::entry(id).path`, converted with
  `Path::to_str`. A tab on a note with no vault entry (detached, gone from
  disk, being trashed) or a non-UTF-8 path is saved as empty.
- **Throttle:** any change arms a 1 s timer if none is armed (Zed's shape);
  the snapshot runs once per second at most. Sources: `set_shown` (every
  landing, switch, back and forward goes through it), every `Tabs` mutation
  in `shell/tabs.rs`, and the editor's notify (caret and scroll). The editor
  subscription joins `follow_links_of`, renamed `watch_editor`: its two call
  sites (`AppShell::new` and `bind`'s `Empty | Trashing` arm) are already
  every place an `EditorView` is created. Arming is O(1) per keystroke.
- **Quit:** `cx.on_app_quit` calls `write` synchronously and returns its task,
  as `NoteStore::flush_all` does.

### R6. Restoring

States: `Loading` (the file is being read), `Waiting(Session)` (read, the
vault not yet walked whole), `Landing(Task)` (the active note loading),
`Done`. Only `Done` writes.

1. `AppShell::new` spawns the read on the background executor. A missing or
   bad file goes straight to `Done`.
2. **Resolution is level-triggered.** One function, `try_restore`, runs when
   the read completes and on every vault notify. It acts only on
   `Waiting(session)` and only when `Vault::has_read_whole()` holds (made
   `pub`). That flag is set only by a whole walk that succeeded
   (`vault.rs:281`) and never turns back. `is_scanned()` is not used: a
   failed walk sets it too (`scans.rs:159-166`, O8), and resolving against a
   near-empty vault would drop every tab, then overwrite the saved session.
   Whichever of the read and the walk finishes second resolves; a later rescan
   finds the state past `Waiting` and does nothing.
3. One pass over `Vault::entries()` maps each saved path to its `EntryId`,
   byte-exact. A tab whose path has no entry is dropped, as Zed drops a gone
   item; an empty saved tab is dropped too. If nothing survives, or the user
   already holds a working set (`holds_nothing`, step 5), `Done`, and no note
   loads.
4. Every surviving note loads through `NoteStore::open_note`, the loads run
   concurrently and are joined, held in `Landing`, **not** in `activation`: a
   user open replaces `activation` (T4), and must not cancel the restore's
   task, which alone moves the state to `Done`. A note whose load yields no
   buffer is dropped, as a missing note is; if none is left, `Done`. Then the
   active tab is the saved active one if it survived, else the nearest
   survivor to its left, else the first.
5. On landing (`land_restore`), in one update, the restore applies **only if
   applying it replaces nothing the user holds** (`holds_nothing`, checked
   again): one tab, `Shown::Empty`, no `activation`, and no closed tab. Then `Tabs` is replaced by
   `Tabs::restored`, each inactive tab is parked with its view anchored in its
   buffer (R7), and the active note binds with its view through `bind`. Either
   way, `Done`.

The predicate reads current state, not history. A picker opened and
dismissed, or a folder expanded, leaves nothing to replace, so the restore
still applies. A user open pending or landed, `⌘T`, or a closed tab means the
user has a working set, and the restore is dropped.

**Focus.** After binding, the note takes focus only if the shell's root
handle holds it (the launch's resting place). A name being typed in the
sidebar keeps its focus; the note column fills beside it. With a picker open,
`land_focus` hands the note to the picker's dismissal, as for any open.

While `Loading`, `Waiting` or `Landing`, `render_empty` draws the background
without the "Select a note." line, so the launch does not flash the empty
state before the note. At 50,000 notes the wait is about 130 ms after the
first frame (roadmap, row 6: first frame 172–184 ms, names ready 306–318 ms).
At a few thousand notes it is under a frame. If the walk never succeeds, the
state stays `Waiting`, the line never shows and nothing is written for the
process; the saved session survives for the next launch.

### R7. `Tabs::restored` and the restored parked views

`inkra-tabs` gains:

```rust
/// Tabs on `places` in bar order; the caller shows `active`'s (H8). Fresh ids, MRU the active first.
/// # Panics
/// If `places` is empty or `active` is out of range.
pub fn restored(places: Vec<P>, active: usize) -> Self
```

The caller shows the active place (H8: `Tabs` keeps no place for the active
tab); the plan's `Restored` hands it over apart from the parked ones.
Proptests check invariants 1–3 and H8 on the result, as for `insert`.

`Parked` keeps its shape, `(EntryId, Option<ParkedView>)`. Every restored
note has a buffer (R6 step 4), so each inactive tab parks a `ParkedView`
built from its buffer and its saved view, clipped first: its view is
anchored like any parked view, and follows edits made in another tab. Every
tab's note must have a buffer: `display` treats a note with no buffer as one
being trashed. `restore` is the one place outside the `leaving` closures of
`insert` and `activate` that parks is `land_restore`; `.claude/rules/shell.md` says so.

### R8. Accepted limits

- A note renamed, moved, or renamed in case only between runs is not found:
  resolution is byte-exact on the path, and no id survives a restart. Its tab
  is dropped, never its text.
- A session file written by a build before R4's absolute root does not match
  once, then heals on the next write. None exists outside this branch.
- Two processes on one vault share one file; the last writer wins.
- A root spelled with `..` gets its own key: collapsing it lexically is wrong across a symlink.
- A note load that never finishes keeps the restore in `Landing`, and writes
  off, for that run, as a hung open keeps its tab waiting.

## Slices

Each slice is vertical: data, behaviour, tests, and a rendered scene where it
draws.

| Slice | Delivers |
|---|---|
| S1 | `Fs::replace` (`RealFs`, `MemFs`, vault tests); `format.rs` with round-trip and rejection tests (corrupt, other version, other root); the absolute root and FNV key; `Launch::Vault` carries the session path; `Restore` writes through its one writer: snapshot with `peek`, 1 s throttle, quit hook, `watch_editor`. In S1 the state starts `Done` (no read yet). Tests: after tab ops, a caret move and a parked tab, the `MemFs` file holds the expected session; the parked tab's view follows an edit made in another tab; a note launch writes nothing |
| S2 | `Tabs::restored` with proptests; every restored note loaded, inactive views anchored; the read, level-triggered resolution on `has_read_whole`, `Landing` outside `activation`, the predicate, the focus rule, the quiet empty state; `shell.md` updated. Tests: order, active and views restored; a missing note dropped and the active index moved; all missing gives one empty tab; a sidebar open before the walk wins; the read finishing after the walk still restores; a failed walk restores nothing and writes nothing; a picker opened and dismissed before landing still restores; a name being typed keeps its focus; a quit before the restore lands writes nothing; a corrupt file restores nothing; a restored inactive tab shows its saved view when activated, and follows an edit made in another tab; a note that fails to load is dropped. A `restored` shot scene |

Verification per slice: `./scripts/check.sh`; `cargo mutants` on `inkra-tabs`
after S2; the shot scene looked at; then a manual run against a real vault:
open three tabs, quit, relaunch.

## As built (S1)

- The active tab's view is read with `EditorView::view_state`, not `capture`: `capture` seals the
  undo group, so a write each second would split typing into undo steps.
- The writer takes the gathered vault, tabs and active index; `AppShell::write_session`
  (`shell/session.rs`) gathers them, since `restore/` cannot read the shell's fields. Still one writer.
- `Session::decode` and `Rejected` are `#[cfg(test)]` until S2 reads the file.
- No "a note launch writes nothing" test: a note launch has no vault, so `write_session` returns
  `None` whatever path it is given, and the test could not fail.
- `vault_root` rebuilds `std::path::absolute`'s result from its components: `absolute` keeps a
  trailing slash, which would give `Notes/` and `Notes` two keys. `.` goes too; `..` stays (R8).
- Writes are ordered as `write_through`'s: a seq minted on the foreground, a last-written mark
  under a `futures::lock::Mutex`, advanced only by a write that landed. A throttled write still
  running at quit cannot land over the quit write.
- The vault's notify arms the throttle, so a rename or a watcher move is saved. It also arms after
  every scan, including the first; S2's state gate keeps that write from replacing the saved file.

## As built (S2)

- R7's first design, a plain view for a restored inactive tab with no buffer, was found
  contradictory in implementation: `display` treats a note with no buffer as being trashed, so
  activating such a tab dropped the editor. Landing now loads every note; views are anchored.
- The active tab is chosen after the loads (`restore/plan.rs`: `Plan::resolve`, then `restored`).
- Reaching `Done` arms the throttle, so a change made while the restore was pending is saved.
- The focus rule is `focus_note_from_root`, shared with `trash_failed`.
- A finished open stays in `activation` until the next `show`, so any open before landing
  drops the restore, even one whose note was trashed since.
- `Tabs::restored` numbers its ids from 0, as `Tabs::new` does. The predicate guarantees no
  pending open holds an old id; nothing else keeps one across the swap.
- A missing file is silent; any other failure or rejection logs one line (R1).
- The shell side (the `Restore` state, the throttle, the quit write) lives in `shell/session.rs`,
  so `restore/` imports no shell type and dependencies run one way.
- `SessionWriter::write` takes `&mut self`: it mints the write's seq on the foreground (S1).
- `holds_nothing` runs before the loads as well as at landing: a refused restore loads nothing.
- The active tab saves its bound note, so a tab whose trash is pending saves empty (R5).
- `Restore::Loading` and `Landing` name their task fields: a tuple field held only for its
  `Drop` fails `dead_code`, and lints are not disabled.

## Deferred

| Item | Trigger |
|---|---|
| Back, forward and closed stacks across a restart | A user misses history after a relaunch |
| The MRU order across a restart | A close after a relaunch activates the wrong tab for a user |
| Launch with no argument reopens the last vault | The folder picker lands, or a Dock launch |
| Opening the active note before the scan (ids seeded from identities, open-first Deferred "Ids that survive a restart") | The wait after the first frame is noticed on a real vault |
| Restore on a failed walk | A user's vault walk fails and they lose their tabs for that run |
| One session file per process on a shared vault | Two windows or processes on one vault |
