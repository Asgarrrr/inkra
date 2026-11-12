# Open a vault from the app

Status: **revised 2026-11-12** after a red/blue review (see Review), against
`main` at `cee05c8b`. First proposed 2026-11-11 against `8ef653b5`. Implementing on branch `open-a-vault`.
Read D2 and D3 first: D2 is the hard-to-reverse choice, D3 is where data can be lost.

## Context

The only way to open a vault is a path on the command line. `launch` reads
`args_os().nth(1)` and returns `Launch::Nothing` without one
(`crates/app/src/bootstrap.rs:124-131`). The shell then draws two lines: "No
vault is open." and "Restart with a folder path to open one."
(`crates/app/src/shell/column.rs:19-30`). A person launched from the Dock or
Finder cannot get past that screen.

The vault spec deferred the native folder picker until "A user who is not us
opens the app" (`docs/superpowers/specs/2026-09-20-inkra-vault-design.md:682`).
The repository went public on 2026-11-08, so the trigger has fired.

A Dock launch needs an app bundle. Packaging is a separate subject and this
spec does not design it. Every slice below also works from `cargo run -p
inkra-app` with no argument, which is how the tests and the developer reach it.

### What belongs to one vault

The state one vault owns today, and where it lives:

| State | Owner | Built |
|---|---|---|
| The `Vault` entity | `NoteStore.vault` (`crates/app/src/vault/store.rs:37-57`) and `VaultPanes.vault` (`crates/app/src/shell/panes.rs:18`) | `root` (`bootstrap.rs:98-105`) |
| The watcher | `NoteStore._watch`, started before the first scan (`vault/store.rs:43-52`) | `NoteStore::new` |
| Open notes, buffers, the save queue, unsaved text | `NoteStore.sessions` and its chains (`vault/save.rs:76-100`) | `NoteStore::new` |
| The last-resort quit flush | `NoteStore._quit`, `on_app_quit(flush_all)` (`vault/store.rs:67-74`) | `NoteStore::new` |
| `IndexSync`, its loads and its cache writer | `VaultPanes.links` (`panes.rs:19,39`); `CacheWriter` (`crates/app/src/index/cache_writer.rs:16-27`) | `VaultPanes::new` |
| The sidebar, backlinks, `Images` | `VaultPanes` (`panes.rs:20-22`) | `VaultPanes::new` |
| The session file, its writer and throttle | `AppShell.session` (`crates/app/src/shell/session.rs:37-45`) | `open_session` (`session.rs:72-106`) |
| The restore | `AppShell.restore` | `open_session` |
| Layout, groups, tabs | `AppShell.layout`, `groups`, from the session read | `restored_groups` (`session.rs:49-68`) |
| The theme choice in `.inkra/` | `AppShell.themes`; first load blocks in `run` (`bootstrap.rs:200-207`) | `run`, then `adopt_theme` (`shell/themes.rs:17`) |
| The session and cache paths | `vault_launch` from the root and `HOME` (`bootstrap.rs:117-121`, `crates/app/src/app_files.rs:15-34`) | `launch` |

Every row is built once, at construction. Nothing here is ever reassigned.
That is the property D2 keeps.

## What Zed does

- **The window's root holds the project, and a new project gets a new
  entity.** The window root is `MultiWorkspace`
  (`vendor/zed-reference/crates/workspace/src/multi_workspace.rs:305`), and it
  persists for the window's life. `MultiWorkspace::open_project`
  (`multi_workspace.rs:1948-2000`) builds or finds a fresh `Workspace` for the
  paths. It calls `prepare_to_close(CloseIntent::ReplaceWindow, ..)` only on an
  empty workspace, and retains a non-empty one. Zed never calls
  `replace_root`, and never drops a workspace that holds edits.
- **`CloseIntent::ReplaceWindow`** is its own intent, beside `Quit` and
  `CloseWindow` (`vendor/zed-reference/crates/workspace/src/workspace.rs:720-726`).
  `prepare_to_close` (`workspace.rs:3831-3910`) runs `save_all_internal` once
  and does not re-check afterwards.
- **The `Open` action** calls `cx.prompt_for_paths` with files and folders
  allowed (`workspace.rs:1054-1072`), through `prompt_for_open_path`
  (`workspace.rs:3556-3600`). A cancelled prompt does nothing.
  `prompt_for_open_path_and_open` then opens in this window or a new one
  (`workspace.rs:950-1046`).
- **The File menu** lists "Open…" (`vendor/zed-reference/crates/zed/src/zed/app_menus.rs:125-131`).
  Zed's keymap files are not in `vendor/zed-reference`, so its `⌘O` binding
  is not checked here.
- **Remembering.** Zed keeps recent workspaces in its SQLite database
  (`vendor/zed-reference/crates/workspace/src/persistence.rs:1826`) and shows
  them in the `recent_projects` picker
  (`vendor/zed-reference/crates/recent_projects/src/recent_projects.rs:2533`).
  The launch with no argument follows `RestoreOnStartupBehavior`, whose
  default is `LastSession`
  (`vendor/zed-reference/crates/settings_content/src/workspace.rs:525-536`).
- **gpui answers a path prompt in tests.** The test platform queues each
  `prompt_for_paths` (`vendor/zed-reference/crates/gpui/src/platform/test/platform.rs:636-646`).
  `TestAppContext::simulate_path_prompt_response` answers it and hands the
  prompt's options to its closure; `did_prompt_for_paths` reports a pending one
  (`vendor/zed-reference/crates/gpui/src/app/test_context.rs:405-420`).
  `VisualTestContext` derefs to `TestAppContext` (`test_context.rs:868-874`).
- **The macOS picker is not modal.** gpui opens `NSOpenPanel` with
  `beginWithCompletionHandler` (gpui_macos `platform.rs:914`). The window keeps
  taking keys, `⌘O` and `⌘Q` while the panel is up. D3's `Leave` covers that.

## Decisions

### D1. The entry points

Slice 1 ships three entry points to one action, `OpenVault`:

- A button in the empty state, "Open Vault…" (D7).
- The key `⌘O`, bound in the `AppShell` context beside `cmd-n`
  (`crates/app/src/keymap.rs:84`). No binding uses `cmd-o` today.
- The command palette. It lists every action on the dispatch path
  (`window.available_actions`, `crates/app/src/command_palette/commands.rs:126-127`),
  so the root div's `on_action` listener (`crates/app/src/shell/render.rs:75`
  is the pattern) makes "Open vault" appear with no palette change.

The action works in every shell: no vault, a detached note, or a vault. The
leave path is the same in all three (D3).

**Rejected: a menu bar item in slice 1.** Inkra sets no menus
(`docs/superpowers/specs/2026-11-02-quit-and-failures-design.md:84-86`).
`cx.set_menus` (`vendor/zed-reference/crates/gpui/src/app.rs:2902`) would add
a File menu, but it also turns the app menu's Quit into a `Quit` dispatch,
which changes the quit spec's Q3. That is a menu-bar feature with its own
review. Deferred; trigger: the menu-bar spec, or the first report that `⌘O`
cannot be found.

### D2. How a running window gains a vault

Three candidates.

**A. Build `VaultPanes` late in the live `AppShell`.** Rejected.
`VaultPanes` is the smallest part of the vault state (see the table). The
`NoteStore` takes its vault and starts its watcher in `NoteStore::new`
(`vault/store.rs:37-52`). The session writer, the restore and the restored
layout are built in `AppShell::new` (`shell.rs:150-217`). Building late
means each of them becomes "set later": `NoteStore.vault`, `session`,
`restore`, `layout` and `themes` turn into fields mutated at runtime. The
comment "built once in `AppShell::new`, never later" (`panes.rs:16`) stops
being true. A switch then also needs a teardown for each field. One vault per
shell would become a runtime check spread over five owners.

**B. Replace the window's root with a fresh `AppShell`.** Chosen. gpui's
`Window::replace_root` builds a new root view and drops the old one
(`vendor/zed-reference/crates/gpui/src/window.rs:2561-2573`). The new shell
comes from the same construction path as a launch: `NoteStore::new` with the
vault, then `AppShell::new` with the session read. One vault per shell then
holds by data shape: `AppShell.notes` and `AppShell.panes` are set in
`AppShell::new` and never reassigned. The comment in `panes.rs:16` stays
true as written. The window, its frame and its blur stay.

This shares Zed's entity shape (a new project gets a new entity, never a
mutated one) but not its lifetime: Zed retains a non-empty workspace, Inkra
drops the old shell. Dropping a shell drops its pending saves, so D3 must
write everything before the drop. That duty is Inkra's alone.

**C. Open a new window and close the old one.** Rejected now. Close is quit
(quit spec, Q1; `quit_on_close`, `crates/app/src/shell/quit.rs:79-86`), so
closing the old window needs a second, non-quitting close path. A second
`NSWindow` appears, the frame can jump, and the launch's first frame is paid
again. For a moment two shells and two `NoteStore`s exist. If both name one
vault, two watchers and two session writers share one session file. Several
windows must solve that anyway, but this feature should not.

**Several windows.** The maintainer wants several windows and tab tear-off
eventually (`docs/roadmap.md`, Application, `180-219`). B does not block it: a
window is one shell, and a second window is `open_window` with the same
builder. The open action then only picks "this window" or "a new window", as
Zed's `OpenPathPromptMode` does (`workspace.rs:1602`). This spec builds no
part of it.

**The builder.** `root` (`bootstrap.rs:83-108`) returns an `Entity<AppShell>`
for `open_window`, whose closure takes `&mut App`
(`vendor/zed-reference/crates/gpui/src/app.rs:1605-1609`). `replace_root`
takes a closure over `&mut Context<AppShell>` that returns the `AppShell`
itself (`window.rs:2561-2566`). So `root`'s body moves into one function that
returns `AppShell` from a `Launch` and an `Option<Opened>`, and both callers
use it. The two callers differ only in who calls `cx.new`.

**What `run` does after `open_window`, and the open repeats.** `run` focuses
the shell and calls `adopt_theme` (`bootstrap.rs:234-239`). The open's commit
turn (D3, step 5) does both. `cx.activate` is not repeated: the window is
already active. `FirstFrame` is a launch mark and is not repeated (D3,
Timeline).

**The old shell's lease.** The replace runs in the leave task's
`cx.update(|window, cx| ..)` on the async window context, not inside
`this.update`. So the old shell is not leased when its last handle drops.
gpui's behaviour when a root is replaced from inside its own update is not
checked here; the async call avoids the question.

### D3. Leaving a shell

One path leaves a shell, whatever it holds. A no-vault shell has nothing to
write. A `Launch::Note` shell holds a real, saveable detached note
(`bootstrap.rs:92-97`, `NoteStore::detached`, `vault/store.rs:87-104`). A vault
shell holds a vault. So the leave, its alert and its writes ship together, in
slice 1: a build that drops a detached note's failed save without asking is
not shippable.

#### The settle, shared with quit

The quit's first half (`quit.rs:21-73`) becomes one function, the settle,
that quit and open both call. It:

1. writes the session through `write_session` (`session.rs:131-138`);
2. waits, bounded by `QUIT_WAIT` (`quit.rs:14`), for the session and for every
   note whose queue is not idle (`Queue::is_idle`, `vault/session/queue.rs:48`):
   a dirty note queues `PendingSave::Write`, as `flush` does today
   (`vault/save.rs:91-100`); a clean note with a busy chain (a rename, a trash)
   queues `PendingSave::Keep`, so the wait joins it;
3. if `NoteStore::unsaved` is not empty, asks with the quit's alert, whose
   first button is the caller's label: "Quit Anyway" or "Open Anyway";
4. answers *proceed* or *cancelled*.

Joining every busy chain, not only the dirty ones, is the one change to quit's
behaviour. A rename or trash on a clean note, queued just before `⌘Q` or `⌘O`,
then lands before the store drops instead of being cancelled with it.

#### The state: `Leave`

The quit's flag, `AppShell.quitting` (`shell.rs:141`), becomes the data it
guards: `leaving: Option<Leave>`, `enum Leave { Quit, Open }`.

| Event | `leaving` before | Result |
|---|---|---|
| `⌘O` | `None` | `Some(Open)`; the prompt opens |
| `⌘O` | `Some(_)` | nothing: no second panel |
| `⌘Q` or close | `None` or `Some(Open)` | `Some(Quit)`; the quit runs. A quit takes over a pending open |
| `⌘Q` or close | `Some(Quit)` | nothing, as today (`quitting.rs:226-234`) |
| Prompt answers `Ok(None)`, `Err(_)` or `Canceled`, the same root, a failed check or read, a timeout, the alert's Cancel | `Some(Open)` | `None` |
| The quit's alert's Cancel | `Some(Quit)` | `None` |

The open task holds the old shell weakly (`cx.spawn_in`, as `quit.rs:32`
does). It never cancels anything: at each step it reads `leaving`, and stops
when it is no longer `Some(Open)`. A stopped open writes nothing and replaces
nothing.

#### The open, step by step

1. **Prompt.** `cx.prompt_for_paths` with folders only (D6). `Err` shows the
   notice (D5); `Ok(None)` does nothing.
2. **Same root.** If the chosen root equals the open vault's root, stop.
   Roots compare after `vault_root` (`bootstrap.rs:111-114`), which is lexical.
3. **Check and read, before anything is left.** In the background, bounded by
   `QUIT_WAIT`: `Fs::check_dir` (`crates/vault/src/fs.rs:175`), then the new
   vault's session through `restore::read` (`crates/app/src/restore/mod.rs:66-78`,
   awaited, not `block_on` as `read_session` does) and its theme through
   `theme::store::load`. A failure or a timeout shows the notice and stops.
   The old shell is untouched.
4. **Settle** with "Open Anyway". *Cancelled* stops.
5. **Commit, in one synchronous `cx.update`, no await inside.** If `leaving`
   is not `Some(Open)`, stop. Otherwise: start the old store's `flush_all`
   (`vault/save.rs:105-152`) and detach its future; build the new shell from
   step 3's reads; `replace_root`; focus the new shell; `adopt_theme`.

Step 5 is the single commit point. Text typed into the old shell during
steps 3 and 4 is not lost: `flush_all` copies every dirty buffer in that turn
and writes through `write_through`, which needs the `Fs` and the note's writer
but not the store, so the writes outlive the drop. It is the quit's own last
resort (`store.rs:67-74`) reached by a second door. An older queued write
cannot overwrite it: both take a sequence number from the store, and
`write_through` holds the note's writer lock and skips a write numbered below
the last one landed (`vault/session/writer.rs:52-80`).

**Rejected: blocking input to the old shell during the leave.** It needs a
modal state in every view, and the window would look frozen during a 3 s wait.
Writing at the commit point covers the same text with no new state.

**Rejected: re-checking `unsaved` in a loop until it is empty.** A user who
types steadily would hold the open forever. One synchronous snapshot at the
commit point has no loop.

**The session file.** The new shell is never built with a session file and no
session read: step 3 reads before step 5 builds. A shell built with
`Opened { file: Some(..), session: None }` from an unread file would reach
`Restore::Done` and its first throttled write would overwrite the vault's
saved tabs (`session.rs:72-106, 109-138`). `.claude/rules/shell.md:81` ("read
once, in `bootstrap::read_session`") becomes "read once per shell, before the
shell is built: by `read_session` at launch, by the open's step 3 after".

**The window frame.** The new shell does not move the window. Its first
session write stores the window's current frame, which is the right frame for
the new vault: the window is the one on screen. A session's saved frame
places the window only at launch (`bootstrap.rs:209-213`).

#### What outlives the old shell

Some old-vault work is detached and runs to its end after the drop. Each is
accepted, with its reason:

| Work | Where | Why accepted |
|---|---|---|
| The scan walk | `vault/scans.rs:54-93`, a detached task holding a strong `Entity<Vault>` | Its result is applied through the store, which is gone: `this.update` fails and it returns. It writes nothing. The cost is a walk at `Low` priority beside the new vault's open; measured if a report names it |
| In-flight index loads, drain, cache read, build, merge | `index/loading.rs:42-53`, `index.rs:86-87, 117-124, 205-209, 217-221, 279-285` | `IndexSync` holds `notes: WeakEntity<NoteStore>` (`index.rs:46`); each lands nowhere. Each read is bounded |
| A cache write already spawned | `cache_writer.rs:53-59, 88-99` | It writes the old vault's cache file, a different file. See Risks for a switch back |
| `flush_all`'s writes | D3, step 5 | Wanted: they are the commit point's writes |

What must die is the old `NoteStore` and the old `Vault`: a live store keeps
its watcher and its `on_app_quit(flush_all)`, which at quit would write old
notes again. Nothing found holds either strongly once `VaultPanes` drops (the
sidebar's `Entity<NoteStore>`, `sidebar/mod.rs:40`, drops with it). A test
pins it (Tests, `the_old_vault_is_released`).

#### Timeline

`run` sets the `Timeline` global (`bootstrap.rs:188`) only for a
`Launch::Vault`. A no-vault launch that later opens a vault would otherwise
print marks measured from process start, picker time included
(`crates/app/src/timeline.rs:78-85`). `Timeline::mark` already does nothing
without the global.

**Rejected: switching only after a restart.** It keeps the restart message
that this feature removes, and the leave path costs nothing extra once it
exists for the detached note.

### D4. Remembering

A launch with no argument reopens the last vault. Restore-tabs already
reopens the tabs of a vault; reopening the vault itself completes it. Zed's
default does the same (`LastSession`, see What Zed does).

The fact lives in one file beside the sessions:
`~/Library/Application Support/Inkra/last-vault.json`, holding
`{ "version": 1, "root": "/Users/me/Notes" }`. A function in
`app_files.rs` beside `session_file` (`app_files.rs:15`) names it.
It is written through `Fs::replace_bytes` (`crates/vault/src/fs.rs:195`),
which creates missing folders, and read through `Fs::load`, so `app` opens no
file. `serde_json` is already a dependency of `app` (restore-tabs spec, R2).

- Written after a vault shell is in place: after `open_window` at a launch
  with a folder argument, and after step 5's `replace_root`. Never before a
  cancel or a failure can still happen. A note launch does not write it.
- A launch with a folder argument rewrites it, including a developer's or a
  benchmark's launch (`scripts/launch-figure.sh` uses the real `HOME` unless
  `--home` is given). Zed does the same; accepted.
- The launch reads it only when `args_os().nth(1)` is `None`. The read
  blocks, as the launch's other reads do.
- A missing or corrupt file opens no vault, silently.

**Rejected: storing it in the session file.** Session files are keyed by
vault root (`app_files.rs:15-22`). The last vault is the one fact that has no
root yet.

**A recent-vaults list: deferred.** Trigger: a user asks to switch between
vaults, or the first report of a second vault in use. The file's shape grows
to `recent: [..]` then; with no users, no migration is needed.

**`HOME` in the running app.** `vault_launch` needs `HOME`
(`bootstrap.rs:117-121`), and today only `launch` reads it
(`bootstrap.rs:129`). `run` stores it once in a gpui global holding
`home: Option<PathBuf>`. The open path reads it with `try_global`, as
`Timeline` does: `shot` scenes and most `#[gpui::test]` shells never run
`run`, so a missing global means `None`, never a panic. A test sets the
global to a path inside its `MemFs`. This is one value, not a trait.

### D5. Failure

A failure shows the notice (`show_notice`, `crates/app/src/shell/notice.rs:80`)
and changes nothing else:

- **Unreadable, deleted, not a folder, or timed out (step 3):** "Couldn't open
  “Notes”: {reason}", with the notice's own `reason` wording
  (`notice.rs:21-30` is the pattern).
- **The picker failed (`Err`):** "Couldn't show the folder picker: {reason}".
- **A cancelled prompt (`Ok(None)`):** nothing.
- **A file chosen:** impossible. The prompt allows folders only (D6).

**The remembered vault at launch.** The launch with no argument goes
`check_dir` → `vault_root` → `vault_launch`, never `open_argument`
(`bootstrap.rs:134-154`). `open_argument` falls back to loading a file on
`NotADirectory` and `NotFound`, so a remembered folder replaced by a file
would open as an editable detached note, and a gone folder would exit
(`bootstrap.rs:192-198`). Any error instead yields no vault and a notice,
worded by kind: "The last vault, “Notes”, is no longer there." for
`NotFound` and `NotADirectory`, the `reason` wording otherwise. It does not
exit. **Rejected: exiting, as a failed argument does.** Every later Dock
launch would then exit too, and nothing in the app could clear the file. A
command-line folder that fails still exits: the person typed that path and
reads stderr.

The notice reaches the shell as data: `Launch::Nothing` becomes
`Launch::Nothing { notice: Option<String> }`, and the builder shows it.

**A folder that holds the app's own files is refused** (decided 2026-11-12).
A root at `~`, `~/Library`, `/Users` or `/` holds
`Library/Application Support/Inkra` or `Library/Caches/Inkra`. `app_file`
(`app_files.rs:30-34`) then returns `None` for the session, the cache, or
both: the vault would remember nothing, and the scan would walk the whole
home folder. Today the command line accepts it silently.

The check is `app_file`'s own answer, so it cannot drift from it:
`vault_launch` refuses when `HOME` is known and `session_file` or
`cache_file` returns `None`. The prompt (step 3), the command line and the
remembered vault all reach `vault_launch`, so one check covers the three
paths. The wording is "“{name}” holds Inkra's own files. Choose a folder
inside it."

- **The prompt:** the notice; the old shell is untouched, as for any failed
  check.
- **The command line:** the message on stderr and exit 1, as for any failed
  argument.
- **The remembered vault:** the notice and no vault, never an exit (see
  above). The file cannot normally name such a folder, because it is written
  only after a vault shell is in place; a hand-edited file still reaches it.

With no `HOME`, nothing can be checked: the launch keeps today's vault
without session or cache. That is the `None` branch `vault_launch` keeps.

What Obsidian does, researched on 2026-11-12: it keeps a vault's settings and
open tabs in `.obsidian/` inside the vault, and its metadata cache in
IndexedDB under `~/Library/Application Support/obsidian`. It does not refuse a
vault at `~`. Its docs warn only against a vault inside its system folder
(obsidian.md/help/data-storage). A 2020 report of `~` as a vault describes a
blank window without a message, and was closed unfixed
(forum.obsidian.md/t/home-user-directory-as-vault/2224). **Deviation:** Inkra
refuses, because its session and cache live outside the vault (restore-tabs
spec, decision 3; persisted index spec, P4), and such a vault cannot keep
them.

### D6. The seam for tests

No new trait. The app calls `cx.prompt_for_paths` (`app.rs:2032`) with
`PathPromptOptions { files: false, directories: true, multiple: false,
prompt: Some("Open Vault") }` (`platform.rs:2935-2944`). In a headless test the
test platform queues the prompt. The test calls
`simulate_path_prompt_response` to answer it, or answers `None` to cancel
(`test_context.rs:405-420`). The options reach only that call's closure: the
test copies them into a `RefCell` there and asserts after. The folder lives in
the shell's `MemFs`: the new shell reuses the old store's `Arc<dyn Fs>`
(`notes.read(cx).fs()`, as `panes.rs:41` does), so tests never touch the real
disk.

The unsaved alert is answered with `simulate_prompt_answer`, as the quit tests
do (`crates/app/src/shell/tests/quitting.rs:204-243`).

`no_vault` (`shell/tests/mod.rs:207-223`) builds its own `MemFs` and returns
no handle, so no test can fill `/vault` before the open or read what the
leave wrote. It takes an `&Arc<MemFs>` instead.

**Rejected: a `FolderPicker` trait with a real and a fake picker.** gpui
already owns both implementations. A trait here would have one implementer
behind a seam gpui provides.

### D7. The empty state

With no vault, the empty state draws:

- "No vault is open." in the placeholder colour, as today
  (`column.rs:32-47`).
- One button, "Open Vault…", with the key hint "⌘O". A click dispatches
  `OpenVault`.

"Restart with a folder path to open one." goes. With a vault, the states
"Select a note." and the blank pending restore stay unchanged
(`column.rs:25-29`).

`ui/` has no text button today (`crates/app/src/ui/` holds `IconButton`,
`ListItem`, `Tab` and others, no `Button`). Slice 1 adds `ui::Button` beside
`IconButton` (`ui/icon_button.rs`), owning its background, hover and padding,
with colours from the theme's tokens, never a literal. Its left press calls
`prevent_default`, as `IconButton` does (`.claude/rules/shell.md`).

`empty_lines` (`column.rs:19`) returns `&'static [&'static str]` today. It
becomes one enum, `Empty { NoVault, Pending, SelectNote }`, and `render_empty`
matches it. The button is then drawn only for `NoVault`, by the data, with no
separate check.

**Rejected: the folder picker opening by itself at launch.** A modal that
appears before the window is useful is surprising, and a cancel leaves the
same empty screen. Obsidian shows a vault chooser instead; that is the
recent-vaults list, deferred in D4.

### D8. Deviations from Zed

1. **One vault per window, and opening replaces and drops the shell.** Zed
   adds a folder to a project, keeps several workspaces in one window
   (`MultiWorkspace`), swaps the active child, and drops only an empty one. A
   vault is one folder, and its session, cache and index are keyed by that
   folder (`app_files.rs`). So Inkra replaces the root and drops the old
   shell. The cost is D3's commit point, which Zed does not need.
2. **Folders only.** Zed's `Open` allows files and folders
   (`workspace.rs:1058-1062`). Opening a loose file from the app has no
   trigger yet; the command line keeps that case.
3. **No `MultiWorkspace` layer.** The window's root is `AppShell` itself.
   Zed's layer exists for several workspaces per window, which is not on the
   roadmap.
4. **One file, not a database.** Zed stores recent workspaces in SQLite. One
   JSON file holds the one fact D4 needs.
5. **No menu bar** (D1).

### D9. Tests

See the Tests section. Every key binding gets a `simulate_keystrokes` test.
Each slice's tests run headless on `MemFs`.

### D10. Slices

See the Plan section.

## Tests

### Slice 1: open a vault from any shell

Written first. `the_open_button_opens_a_vault` fails before the change,
because no button exists.

`shell/tests/open_vault.rs`:

- `cmd_o_prompts_for_one_folder`: `no_vault`, simulate `cmd-o`, assert
  `did_prompt_for_paths`; answer, and assert the captured options:
  `directories`, not `files`, not `multiple`.
- `a_second_cmd_o_opens_no_second_prompt`: `cmd-o` twice; one prompt.
- `the_open_button_opens_a_vault`: click the button by its debug selector,
  answer with `/vault`, run until parked. Assert the root shell has a vault at
  `/vault`, and the tree lists its notes (`vault_names`, `mod.rs:225`). A
  plausible bug it catches: the new shell built but never set as the root.
- `a_cancelled_prompt_changes_nothing`: answer `None`; same root entity, and
  a later `cmd-o` prompts again (`leaving` was cleared).
- `a_missing_folder_shows_a_notice`: answer `/nowhere`; the notice text, the
  same root entity, and a later `cmd-o` prompts again.
- `a_detached_note_is_written_before_the_vault_opens`: `Launch::Note`, type,
  open `/vault` before the autosave delay. Assert the file text in `MemFs`.
- `an_unwritable_detached_note_asks_first`: `Launch::Note`, `make_unwritable`
  as `quit_unwritable` does (`quitting.rs:189-202`), type, `cmd-o`, answer.
  Assert the alert. "Cancel" keeps the root; "Open Anyway" replaces it.
- `text_typed_during_the_leave_is_written`: open `/a`, type, `cmd-o`, answer
  `/b` while a write to `/a` is held (as the quit tests hold one), type more
  into the old note, release, run until parked. Assert the full text in
  `/a`'s file. A plausible bug: the commit point skipping `flush_all`, which
  passes every other test.
- `switching_writes_the_left_vault_first`: open `/a` with a dirty note, open
  `/b`. Assert `/a`'s note text and `/a`'s session file in `MemFs`.
- `a_rename_queued_before_the_open_lands`: rename a clean note, `cmd-o` in the
  same turn, open `/b`. Assert the renamed file in `MemFs`.
- `opening_the_open_vault_does_nothing`: same root entity, no session write.
- `the_new_vault_restores_its_tabs`: a session file for `/b` in `MemFs`;
  after the open, the tabs match it, and the file is unchanged byte for byte
  until the shell writes its own.
- `the_new_vault_adopts_its_theme`: `/b/.inkra/settings.json` names a
  built-in theme; after the open, that theme is installed.
- `a_quit_takes_over_a_pending_open`: `cmd-o`, answer `/b` with a write held,
  `cmd-q`. Assert the quit ran (notes and session written, `cx.quit` called)
  and the root was not replaced.
- `the_old_vault_is_released`: keep a `WeakEntity` of the old `NoteStore` and
  of the old `Vault`, open `/b`, run until parked plus one frame. Both fail to
  upgrade.
- `the_old_shell_hears_nothing`: after the replace, emit one event the old
  shell subscribed to; the old shell sees nothing (Risks).
- `closing_after_a_switch_runs_the_new_quit`: close the window after a
  replace; the new shell's quit runs (Risks).
- `the_palette_lists_open_vault`: the palette's entries include "Open vault".

Elsewhere:

- `quitting.rs`: `a_rename_queued_before_quit_lands`, for the settle's wider
  wait. The existing quit tests pass unchanged.
- `column.rs` tests: the `NoVault` state draws the button; `SelectNote` does
  not.
- Scene: `empty_no_vault` (`crates/app/src/shot/scenes.rs`). Render it and
  look: the button sits under the line, centred.

### Slice 2: remember the last vault

- `app_files` tests: the last-vault file's path under `HOME`.
- Launch tests, on `MemFs`, through the no-argument launch path: a last-vault
  file opens that vault; a gone folder yields no vault and the notice; a file
  at the remembered path yields no vault and the notice, never a note; a
  corrupt last-vault file yields no vault and no notice.
- `opening_a_vault_records_it`: after an open from the app, the file holds
  the new root.
- `a_cancelled_open_records_nothing`: the alert's Cancel leaves the file as
  it was.
- `crates/app/tests/cli.rs` keeps the process-level cases: a folder argument
  that fails still exits non-zero.

### Slice 3: refuse a folder that holds the app's own files

Written first. Each fails before the change, because today such a root opens.

- `bootstrap` tests: `vault_launch` with `HOME` `/home/me` refuses `/home/me`,
  `/home/me/Library`, `/home/me/Library/Caches`, `/home` and `/`; it accepts
  `/home/me/Notes`; with no `HOME` it accepts `/home/me` without session or
  cache. A plausible bug it catches: checking only the session, which accepts
  `/home/me/Library/Caches`, a root that holds the cache alone.
- `open_argument` on `MemFs` with the home folder as the argument: the error
  names the folder.
- `shell/tests/open_vault.rs`: `a_folder_holding_the_app_files_is_refused`:
  answer the prompt with the test's `HOME`; the notice text, the same root
  entity, and a later `cmd-o` prompts again.
- Slice 2's launch tests: a last-vault file naming the home folder yields no
  vault and the notice.
- `crates/app/tests/cli.rs`: the home folder as the argument exits non-zero.

## Risks

- **Two cache writers on one file.** Switching back to the vault just left,
  while its last cache write is still in `RealFs`, gives two writers with
  separate `mark`s (`cache_writer.rs:146-158`). The older write could land
  after the newer one. The cache is a cache: the next start probably sees a
  stale file and rebuilds the difference. Measure whether it can happen in
  practice before adding any ordering.
- **Detached subscriptions of the old shell.** `AppShell::new` detaches
  several subscriptions. gpui holds a weak subscriber
  (`vendor/zed-reference/crates/gpui/src/app/context.rs:93-108`), so they go
  inert once the old shell is released. `the_old_shell_hears_nothing` pins it.
- **The old shell's quit handler.** `quit_on_close` registers a should-close
  callback on the platform window (`quit.rs:79-86`). gpui keeps one such
  callback per window (`window.rs:7164-7173`), and the new shell registers in
  `AppShell::new` before the old root drops, so it replaces the old one.
  `closing_after_a_switch_runs_the_new_quit` pins it.
- **The old vault's scan beside the new open.** Switching away mid-scan from a
  50,000-note vault leaves the walk running at `Low` priority (D3). If the new
  vault's open is measurably slower, give the scan a weak vault.
- **The first frame.** A launch with no argument now reads one more file
  before the window opens. It is a few bytes; `docs/startup-floor.md`'s
  budget should not move. Measure it once with `Timeline`.

## Plan

Each slice is one PR and is green under `./scripts/check.sh` alone.

1. **Open a vault from any shell.** `OpenVault`, `⌘O`, the palette entry,
   `ui::Button`, `Empty`, the empty state's text. The shell builder shared by
   `root` and `replace_root`. The `HOME` global. The settle shared with quit,
   `Leave`, the open's five steps, the commit point. The folder check and its
   notices. The `Timeline` gate. `no_vault` taking the `MemFs`. Doc edits:
   `.claude/rules/shell.md:81`; the vault spec's Deferred row for the folder
   picker (`2026-09-20-inkra-vault-design.md:682`) moves to done; the
   roadmap's Vault paragraph (`docs/roadmap.md:118-128`) drops the picker.
   About 450 lines, more than half of them tests. Commit the builder and the
   settle extraction first, as refactors that change no behaviour, so the
   review reads them apart from the feature.
2. **Remember the last vault.** The last-vault file, its write after a vault
   shell is in place, its read at a launch with no argument through
   `check_dir`, `Launch::Nothing { notice }`. Doc edits: close the
   restore-tabs Deferred row "Launch with no argument reopens the last vault"
   (`2026-09-30-restore-tabs-design.md:357`). About 150 lines.
3. **Refuse a folder that holds the app's own files.** `vault_launch`'s
   refusal (D5), its notice in the prompt and the remembered-vault paths, its
   exit on the command line. Doc edits: restore-tabs R1
   (`2026-09-30-restore-tabs-design.md:81`) and the persisted index's P4
   (`2026-11-06-persisted-index-design.md:77`) say such a vault is refused,
   not opened without a session. About 60 lines, mostly tests.

Slice 1 is past the ~150-line PR target. It is not split further: an open
from no vault without the alert and the session read is a build that loses a
detached note's failed save, or a vault's saved tabs.

## Review

A red team (Opus) attacked the 2026-11-11 draft; a blue team (Sonnet)
verified each finding against `cee05c8b`. What changed:

| Finding | Verdict | Change |
|---|---|---|
| Text typed during the leave is lost when the store drops | Confirmed | D3's commit point: `flush_all` in the replace turn |
| Slice 1 drops a detached note's failed save without the alert | Confirmed | The alert moves to slice 1; old slices 1 and 2 merge |
| Slice 1 could arm a session writer on an unread file | Confirmed | Step 3 reads before the build, in slice 1 |
| The leave swallows quit, and `⌘O` twice races two leaves | Confirmed | `Leave` table; a quit takes over an open; step 3 bounded |
| The teardown list was wrong | Partial: nothing holds the old store | D3's "What outlives the old shell"; `the_old_vault_is_released` |
| A remembered folder that became a file opens as a note | Confirmed; a gone one exits | D5: `check_dir` path, notice by kind, `Launch::Nothing { notice }` |
| The harness cannot fill or read the vault | Confirmed | D6: `no_vault` takes the `MemFs`; options captured |
| D8 understated the deviation from Zed | Confirmed (blue M5) | What Zed does, D2, D8.1 |
| When `last-vault` is written | Partial | D4: after the shell is in place |
| Timeline marks after an open from no vault | Confirmed | D3, Timeline |
| "The no-session case becomes dead code" | Confirmed wrong | Removed; `HOME` unset still reaches it |
| The `HOME` global panics in tests and scenes | Blue, missed by red | D4: `try_global` |
| The picker's `Err` is unhandled | Blue, missed by red | D5 |
| A rename or trash queued before the leave is cancelled | Partial | The settle joins every busy chain, quit included |
| The new shell stores the old vault's frame | Refuted: the window is shared | D3, The window frame, states it |

## Deviations during implementation

- `Bound` and `Shown` moved from `shell.rs` to `shell/shown.rs`: `shell.rs` was at its line
  ceiling, and the open adds two modules to it.
- The walking skeleton's first red test is `cmd_o_opens_the_chosen_folder`, driven by `⌘O`.
  The empty state's button and its test come in the next commit.
- `QUIT_WAIT` lives in `shell/leave.rs` beside the settle that waits on it, not in `quit.rs`.
- The vault tests open `/vault` and `/other` through the existing `saving` helper, not `/a`
  and `/b`.
- `detached` moved from `tests/following.rs` to `tests/mod.rs` and takes the `MemFs`, so the
  open's tests and the link tests share it.
- `a_quit_over_a_pending_open_asks_once` is added: both waits run out on a hung save, and only
  the quit asks. It is the test that fails when the settle's check after the wait is removed.
- The button's key hint comes from the keymap through `keys_label`, never a literal "⌘O": a
  rebound key shows as bound.
- `Home` holds a `PathBuf`, not D4's `home: Option<PathBuf>`: `run` sets it only when `HOME`
  is known, so "no home" has one representation, the absent global, not two.
- `the_old_vault_is_released`, `the_old_shell_hears_nothing` and
  `closing_after_a_switch_runs_the_new_quit` pin behaviour gpui already gives, so they pass
  when written. Each was shown to fail under a temporary mutation: a strong holder of the old
  store, a strong holder of the old shell, and the new shell not registering its close
  handler. `the_old_shell_hears_nothing` resizes the window: the window's appearance cannot be
  changed from outside gpui, and the old store, which emits the theme reload, is released.

## Out of scope

- An app bundle and a Dock icon (packaging).
- A menu bar (D1).
- A recent-vaults list (D4).
- Several windows and tab tear-off (D2).
- Opening a vault from Finder through `on_open_urls`
  (`vendor/zed-reference/crates/gpui/src/app.rs:359`). Trigger: the app
  bundle declares a folder type.
