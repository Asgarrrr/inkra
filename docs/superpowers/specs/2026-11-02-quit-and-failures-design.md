# Quit and failures: close is quit, the chained flush, visible failures

Status: **S1 and S2 built 2026-11-02.** Later slices append their sections here.

## Why

Two quits lose text today.

- **The close button.** gpui's default `QuitMode` on macOS is `Explicit`
  (`gpui/src/app.rs:386-395`): a closed window leaves the app running with no
  window. The window owns `AppShell`, and the shell owns the `NoteStore`. So a
  close drops the store, its `on_app_quit` subscription and every pending
  autosave. Nothing writes the last two seconds of typing.
- **`⌘Q` behind a rename.** `NoteStore::flush_all` writes off the note's
  chain. A write behind a pending rename resolves the old path, so it
  recreates `a.md` beside the renamed `z.md`.

## What Zed does

- `zed/src/zed.rs:492-500`: the window's should-close handler calls
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

1. It sets `AppShell.quitting`. A second quit while one is under way does
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
callback from `applicationWillTerminate:` (`gpui_macos/src/platform.rs:1392`),
and gpui's `shutdown` gives `on_app_quit` futures `SHUTDOWN_TIMEOUT`, 200 ms
(`gpui/src/app.rs:75,1073`).

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
- **Quit Anyway** quits, and leaves `quitting` set: the app is going away.
- **Cancel**, or a prompt dismissed without an answer, clears `quitting`, so
  the next quit flushes and asks again.

`quitting` also keeps one prompt at a time: a second `⌘Q` while the first
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

## Accepted limits

- **Dock quit, app-menu quit, logout and restart** run only `flush_all` and
  the session's `on_app_quit`, with 200 ms. A write behind a pending rename
  can recreate the old name, and a slow disk can lose the write. Closing this
  needs `applicationShouldTerminate:` in the vendored `gpui_macos`.
- **The quit's tests cannot see `cx.quit()`.** gpui's test platform quits as a
  no-op (`gpui/src/platform/test/platform.rs:435`). A test sees the step after
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
