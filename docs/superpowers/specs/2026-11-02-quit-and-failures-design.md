# Quit and failures: close is quit, the chained flush, visible failures

Status: **S1, S2, S3, S4 and S5 built 2026-11-02.** Later slices append their sections here.

## Why

Two quits lose text today.

- **The close button.** gpui's default `QuitMode` on macOS is `Explicit`
  (`gpui/src/app.rs:526-535`): a closed window leaves the app running with no
  window. The window owns `AppShell`, and the shell owns the `NoteStore`. So a
  close drops the store, its `on_app_quit` subscription and every pending
  autosave. Nothing writes the last two seconds of typing.
- **`⌘Q` behind a rename.** `NoteStore::flush_all` writes off the note's
  chain. A write behind a pending rename resolves the old path, so it
  recreates `a.md` beside the renamed `z.md`.

## What Zed does

- `zed/src/zed.rs:494-502`: the window's should-close handler calls
  `MultiWorkspace::close_window` and answers `false`. The close is
  asynchronous: it asks about unsaved items, then calls `remove_window`.
- The app stays alive with no window (`QuitMode::Explicit`). Quitting is a
  separate action.

## Deviations from Zed

1. **Close is quit.** Zed keeps the app alive without windows. Inkra has one
   window, and a closed window drops the `NoteStore` together with its
   `on_app_quit` subscription and its pending autosaves. A windowless Inkra
   holds nothing the user can reach, so the close button quits the app, after
   the same writes as `⌘Q`.
2. **A failed autosave retries.** Zed retries no save: `Pane::autosave_item`
   (`workspace/src/pane.rs:2551-2575`) saves once and returns the error. In
   Zed a save is explicit and autosave is opt-in. Every Inkra save is
   implicit, so a failure nobody retries is a data-loss path (Q8).

## Architecture

### Q1. Close is quit: one quit path

`shell/quit.rs` holds the one quit path. `⌘Q`, the palette's Quit and the
close button all reach `AppShell::quit`:

- `⌘Q` and the palette dispatch `Quit`. The root div's `on_action` listener
  takes it, wherever focus is in the window.
- The close button calls the window's should-close handler, which
  `AppShell::new` installs (`quit_on_close`). The handler calls `quit` and
  answers `false` while the shell lives: the quit ends the app, so the window
  never closes by itself first. As in Zed, a dropped shell answers `true`.

`quit` does four things:

1. It sets `AppShell.leaving` to `Leave::Quit`. A second quit while one is under way does
   nothing, so a second `⌘Q` or close starts no second flush.
2. It writes the session through the one writer, `write_session`.
3. It starts `NoteStore::flush` (Q2).
4. It waits for both, bounded by `QUIT_WAIT` (Q4). Then it quits, or asks
   first when a note is still unsaved (Q5).

Two fallbacks stay:

- `bootstrap`'s global `Quit` handler (`cx.quit()`) answers `⌘Q` when no
  window is focused.
- `NoteStore`'s `on_app_quit(flush_all)` is the last resort (Q3).

### Q2. The chained pre-quit flush

`NoteStore::flush` queues, on each unsaved note's chain, a no-op link behind a
zero-delay write (`queue(id, PendingSave::Write, ..)`). It resolves when every
link has run. So the write waits behind a pending rename, a reload or a trash,
and resolves the note's path after them. A note mid-trash is not open, so
`unsaved` never lists it; its trash already preempted its write.

`flush_all` cannot chain. It runs from `on_app_quit`, inside gpui's
`shutdown`, which holds the `AppCell` borrowed. A link needs the store again
(`this.update`) after an `await`, and that borrow would panic. So `flush_all`
keeps its synchronous shape: it copies each dirty buffer out at once and
spawns only background writes. It skips a `Trashing` note, whose file is being
deleted.

### Q3. Quits that bypass the Quit action

The Dock's Quit, the app menu's Quit, logout and restart send AppKit's
`terminate:`. They never dispatch `Quit`, because Inkra sets no menus.
`gpui_macos` implements no `applicationShouldTerminate:`; it calls the quit
callback from `applicationWillTerminate:` (`gpui_macos/src/platform.rs:1395`),
and gpui's `shutdown` gives `on_app_quit` futures `SHUTDOWN_TIMEOUT`, 200 ms
(`gpui/src/app.rs:80,1073`).

So for those quits, `on_app_quit(flush_all)` is the main path, not a rare last
resort. It is off-chain (Q2) and bounded at 200 ms. `gpui_macos` is vendored
and stays unpatched.

### Q4. `QUIT_WAIT`: the quit waits 3 s at most

`quit` races the session write and `NoteStore::flush` against a
`QUIT_WAIT` timer, 3 s. A local save takes milliseconds, so 3 s passes only
when the disk hangs: a network volume gone, a sleeping drive. Waiting longer
holds a frozen-looking app; waiting less gives up on a slow but live disk.

The timer bounds the quit, not the write. The flush is not cancelled: a write
that lands after the timer still lands, until the process exits.

### Q5. The prompt: only when something is still unsaved

After the wait, `quit` reads `NoteStore::unsaved`. Empty: it quits, with no
prompt. A failed or still-hung write leaves its note unsaved, so the prompt
lists exactly the notes whose text is not on disk:

- A native warning alert, "Some changes are not saved.", with the detail
  "Not on disk: “a”, “b”." The titles are `NoteStore::title`, sorted;
  "Untitled" when a note has none.
- **Quit Anyway** quits, and leaves `leaving` set: the app is going away.
- **Cancel**, or a prompt dismissed without an answer, clears `leaving`, so
  the next quit flushes and asks again.

`leaving` also keeps one prompt at a time: a second `⌘Q` while the first
prompt is open does nothing.

### Q6. The logger: the `log` facade, filtered by target, stderr

`logger.rs` holds the process's only `log::Log`. `run` installs it first.
`NoteStore`'s `on_app_quit` logs each write `flush_all` reports as failed
(`log::error!("quit flush: …")`): a Dock quit has no window left to ask in.

- **The `log` facade**, which gpui already uses: gpui's own errors reach
  the same stderr.
- **By target, no environment filter.** `inkra*` targets print from `Warn`,
  every other target from `Error` only: gpui warns of a duplicate font on
  every launch, and a line must mean something failed. Zed's `zlog` reads
  `ZED_LOG`/`RUST_LOG`, filters per scope and writes a log file; Inkra has
  one user-facing failure channel, the notice, and needs none of that yet.
- **A launch failure is a `log::error!`** in `run`, then exit 1.
  `tests/cli.rs` asserts its `ERROR inkra_app…` line, so a `run` that
  installs no logger fails a test.
- **One line, never a panic:** `LEVEL target: message`, written with
  `writeln!` and its error ignored. `eprintln!` panics on a closed stderr.

`flush_all` keeps its synchronous copy (Q2). Its future joins the background
writes and collects each `Err`. A `None`, a write overtaken by a newer one, is
not an error.

### Q7. A rename's new path, before its rescan lands

`NoteStore.moves` holds our own rename until its rescan swaps in. `None`
means `Fs::rename` runs and no path is safe. `Some(path)` is the new path
while the vault still lists the old one. `NoteStore::path_of` reads `moves`
first, then the vault. Every fs op on a note resolves its path through
`path_of`: `write_path`, `trash_target`, a rename's source and `open_note`.

- **The rename's chain link ends after `Fs::rename`.** It records the new
  path in `moves` and requests the rescan in the same turn. A write, a trash
  or a second rename queued behind it runs at once, at the new path. The
  autosave no longer waits for the rescan.
- **The rename's own rescan drops the move.** A detached task waits for that
  rescan and removes the move when it swaps in, unless a later rename
  replaced it. The `Task<bool>` the caller gets resolves after that rescan,
  as before. Dropping it never skips the cleanup: it mirrors `queue`'s
  oneshot.
- **`flush_all` skips a `None` move.** It cannot join the chain, so a note
  whose rename runs has no safe path. Its text stays unsaved. `write` cannot
  meet a `None` move: it runs on the chain, after the rename's link.
- **A failed `Fs::rename` restores** the move that was there before it.

Edge cases left:

- A note inside a renamed folder resolves the old folder until the swap.
  This is unchanged: such a note was never chained to the folder's rename.
- A failed rescan keeps the move. The next swap that lists the note does not
  drop it; only the rename's own rescan does.
- An external move between our rename and its rescan is handled: that rescan
  is authoritative, and the move goes when it swaps in.

`MemFs::block_after_rename` parks a rename after the move, the gate
`the_quit_flush_while_the_rename_runs_writes_nothing` needs. `mem_fs.rs` ends
near 400 lines and is not split here; its new test lives in
`mem_fs/fault_tests.rs`, because `mem_fs/tests.rs` is over 500.

### Q8. A failed save: the failed dot, transient retries, one conflict copy

`NoteStore::failed` handles both `SaveFailed` emits in `write`: a note with
no path in the vault (`error: None`) and a failed `Fs::save`. It logs the
error, counts it on `OpenNote.failures`, and emits `SaveFailed { id, error }`.
A landed write or an applied reload zeroes the count, where `saved_version`
moves.

- **The failed dot.** `Indicator::of(dirty, failed)` picks the tab's and the
  tree row's dot: `Failed` in `theme::failed_dot()` when the note is dirty and
  its last write failed, `Dirty` when only dirty, nothing when clean.
- **Transient errors retry.** `Interrupted`, `WouldBlock`, `TimedOut` and
  `ResourceBusy` reschedule the save through `schedule_save`, on the note's
  chain, never on a side timer. The wait doubles from 1 s to a 30 s cap
  (`retry_delay`). Other errors wait for the next edit or `⌘S`.
- **Retries are capped at `RETRY_LIMIT` (8).** After 1 + 8 attempts the
  failed dot and the log stay. An edit or `⌘S` still schedules a save, and a
  landed write zeroes the count.
- **A pending autosave absorbs a retry**, because `Queue::arm` returns `None`.
  A keystroke during a retry's wait does not shorten it. `⌘S` and the quit
  flush do: a zero delay preempts.
- **A failure after preserve is `VaultError::Unwritten`.** `Fs::save`
  preserves the note's bytes before it writes. When the write after the
  preserve fails, the error carries the copy's path and the stamp of the
  bytes it kept. `write_through` adopts that stamp, so the retry's
  `Fs::save` finds no mismatch and preserves nothing more: N retries leave
  one conflict copy. `NoteStore::failed` emits `ConflictPreserved` for it and
  requests the rescan, as a landed preserving save does. `is_transient`
  judges an `Unwritten` by its cause.
- **Accepted TOCTOU.** Bytes that change between the preserve and the
  adopted stamp's next save are overwritten unpreserved. Today's save has
  the same window between its stat and its replace.

`MemFs::fail_next_saves` and `MemFs::fail_next_saves_after_preserve` are the
fault seams. The second fires only on a save that preserved.

### Q9. The notice: one line, the latest failure

`shell/notice.rs` holds `Notice` and `AppShell.notice`, the one failure
surface. The store reports a failure as an event; the shell subscribes
(detached, as its other subscriptions) and words it. A view never holds
another view.

- **Five events.** `SaveFailed`, `RenameFailed`, `OpenFailed` and
  `ConflictPreserved` each set the notice. `NoteSaved` clears it.
  `RenameFailed` carries its `VaultError`. `OpenFailed` is new: `open_note`
  no longer drops a load error with `.ok()?`.
- **One line, no path.** The text names the note by its title and gives the
  OS reason (`reason`): "Couldn't save “a”: permission denied". A
  `VaultError::Unwritten` shows its cause's reason. The line is truncated,
  never wrapped, under the gone banner and above the editor.
- **Latest wins; two ways out.** A new failure replaces the notice. The
  dismiss button (`IconButton`) clears it. A `SaveFailed` notice also clears
  when a write of that note lands (`Notice.cleared_by_save`). Other notices
  have no such note, so a later save leaves them. `write` emits `NoteSaved`
  before `ConflictPreserved`, so a conflict notice survives its own write.
- **Deviation from Zed.** Zed stacks workspace toasts. A note app reports
  rarely, and two stacked failures would hide the note. One line stays.
- **Colours.** `theme::notice_background` and `notice_text`; the
  `save-failed` and `open-failed` shots show the contrast is readable.

`MemFs::make_unreadable` now covers a file's load as well as a folder's
listing, for the `open-failed` scene and its test.

### Q10. The refused watch and the refused create

- **The refused watch.** `NoteStore::watch` returns its error. The store logs
  it and keeps it in `unwatched`. `AppShell::new` takes it once and opens with
  the notice "Inkra can't follow changes made outside the app: <reason>". The
  store is built before the shell, so an event would be missed.
- **A create keeps its field open until it resolves.** `EditState.creating`
  ignores a second Enter, so a second create cannot hit `AlreadyExists` on the
  first. `NoteStore::create` returns `Fs::create`'s error. `AlreadyExists`
  shows the Duplicate message; any other error shows `Rejection::Refused`
  ("The file could not be created.") and a warning log. Escape during a create
  closes the field; the create still lands and is selected.
- **Deviation from closing at once.** The field now stays open until the
  rescan lands, which costs one rescan of latency before the row shows.
- **Logging.** Every `eprintln!` outside tests became `log::warn!` (the launch
  failure, `log::error!`, as Q6 has it). Two sites stay on stderr by design:
  `timeline.rs` and `shot/mod.rs`.

## Accepted limits

- **Dock quit, app-menu quit, logout and restart** run only `flush_all` and
  the session's `on_app_quit`, with 200 ms. A slow disk can lose the write,
  and a note whose rename runs is skipped (Q7). Closing this
  needs `applicationShouldTerminate:` in the vendored `gpui_macos`.
- **The quit's tests cannot see `cx.quit()`.** gpui's test platform quits as a
  no-op (`gpui/src/platform/test/platform.rs:536`). A test sees the step after
  `QUIT_WAIT` through the prompt (`pending_prompt`), and a quit with nothing
  unsaved through its writes.

## As built (S1)

- `vault/save.rs`: `NoteStore::unsaved` and `NoteStore::flush`. `flush_all` is
  unchanged.
- `shell/quit.rs`: `QUIT_WAIT`, `AppShell::quit`, `AppShell::begin_quit`,
  `AppShell::quit_on_close`. `AppShell.quitting` is the guard.
- `shell/session.rs`: `write_session` became `pub(super)`, because the quit in
  its sibling module calls it. This file is outside S1's file list; the
  contract's call to `write_session` from `quit.rs` requires the change.
- `shell/tests/quitting.rs`: the close, `⌘Q`, a quit behind a rename, a second
  `⌘Q`, and a hung save past `QUIT_WAIT`.

## As built (S2)

- `shell/quit.rs`: `begin_quit` folded back into `quit`. The prompt makes the
  step after `QUIT_WAIT` observable, so no test needs the quit's task.
- `vault/save.rs`: `flush_all` returns the writes' errors.
- `vault/store.rs`: the `on_app_quit` wrapper logs them.
- `logger.rs`, `bootstrap.rs`: the logger, installed first in `run`.
- `shell/tests/quitting.rs`: the prompt, Cancel, a second `⌘Q`, Quit Anyway.
  The hung-save test now asserts the prompt at `QUIT_WAIT`, not the task's end.

## As built (S3)

- `vault/store.rs`: `NoteStore.moves`; `open_note` resolves through `path_of`.
- `vault/tree_ops.rs`: `path_of`, `trash_target` through it, and `rename`'s
  short chain link with its detached cleanup.
- `vault/save.rs`: `write_path` resolves through `path_of`.
- `inkra-vault`'s `MemFs`: `block_after_rename`, and its test in
  `mem_fs/fault_tests.rs`.
- `vault/tests/moves.rs`: the autosave, the quit flush during the rescan and
  during the rename, a trash, an open, two renames, and the dropped task.

## As built (S4)

- `inkra-vault`: `VaultError::Unwritten`; `RealFs::save` and `MemFs::save`
  return it when the write after a preserve fails. `RealFs::save`'s write
  moved into `overwrite`, so one `map_err` wraps all three failing steps.
- `vault/session/writer.rs`: `write_through` adopts an `Unwritten` stamp.
- `vault/save.rs`: `retry_delay`, `RETRY_LIMIT`, `is_transient`,
  `NoteStore::failed`. `vault/store.rs`: `save_failed`.
- `ui/indicator.rs`: `Indicator` became an enum with `of` and `selector`.
  The shell's `eprintln!` subscriber is gone: the store logs.
- `vault/mod.rs` exports `AUTOSAVE_DELAY` under `feature = "shot"` too, for
  the `save-failed` scene. This file is outside S4's file list.
- `a_failing_note_stops_retrying_after_the_limit` injects 1 + `RETRY_LIMIT`
  faults, not 20, and counts `SaveFailed` events: one more retry would land
  the edit, so the limit shows without reading the fault's remaining count.

## As built (S5)

- `shell/notice.rs`: `Notice`, `watch_failures`, `render_notice`; `shell.rs`
  holds `notice`, `render.rs` draws it under the gone banner.
- `vault/events.rs`: `RenameFailed { id, error }`, `OpenFailed`. `rename` and
  `open_note` emit them.
- `inkra-vault`'s `MemFs`: `refused_load`, called by `load`.
- `shell/tests/notices.rs`: the six notices, dismiss and clear. The conflict
  test types a second edit, so a notice cleared by any `NoteSaved` fails it.
- `shot/scenes/failures.rs`: `open_failed`; `save-failed` shows the notice too.

## As built (S6)

- `vault/watch.rs`, `vault/store.rs`, `shell.rs`: the refused watch (Q10).
  `reason` in `shell/notice.rs` is `pub(super)` for the opening notice.
- `inkra-vault`'s `MemFs`: `make_unwatchable`, with its test in
  `mem_fs/fault_tests.rs`.
- `vault/tree_ops.rs`, `sidebar/edit.rs`: `create` returns the result;
  `Sidebar::created` lands it. The tests use the existing
  `rejection_message`, not a new accessor.
- `shot/stage.rs`, `shot/scenes/failures.rs`: `unwatched` and `create-refused`.
- Sites outside the contract's list that call `create` and now unwrap its
  result: `vault/tests/scans.rs`.
