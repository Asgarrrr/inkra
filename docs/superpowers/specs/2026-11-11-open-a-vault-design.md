# Open a vault from the app

Status: **proposed 2026-11-11** against `main` at `8ef653b5`. No code yet.
Read D2 first: it is the hard-to-reverse choice.

## Context

The only way to open a vault is a path on the command line. `launch` reads
`args_os().nth(1)` and returns `Launch::Nothing` without one
(`crates/app/src/bootstrap.rs:122-129`). The shell then draws two lines: "No
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
| The `Vault` entity | `NoteStore.vault` (`crates/app/src/vault/store.rs:37-57`) and `VaultPanes.vault` (`crates/app/src/shell/panes.rs:18`) | `root` (`bootstrap.rs:96-103`) |
| The watcher | `NoteStore._watch`, started before the first scan (`vault/store.rs:43-52`) | `NoteStore::new` |
| Open notes, buffers, the save queue, unsaved text | `NoteStore.sessions` and its chains (`vault/save.rs:76-101`) | `NoteStore::new` |
| The last-resort quit flush | `NoteStore._quit`, `on_app_quit(flush_all)` (`vault/store.rs:67`) | `NoteStore::new` |
| `IndexSync`, its loads and its cache writer | `VaultPanes.links` (`panes.rs:19,39`); `CacheWriter` (`crates/app/src/index/cache_writer.rs:16-27`) | `VaultPanes::new` |
| The sidebar, backlinks, `Images` | `VaultPanes` (`panes.rs:20-22`) | `VaultPanes::new` |
| The session file, its writer and throttle | `AppShell.session` (`crates/app/src/shell.rs:134`, `shell/session.rs:37-45`) | `open_session` (`session.rs:72-106`) |
| The restore | `AppShell.restore` (`shell.rs:135`) | `open_session` |
| Layout, groups, tabs | `AppShell.layout`, `groups` (`shell.rs:119-121`), from the session read | `restored_groups` (`session.rs:49-68`) |
| The theme choice in `.inkra/` | `AppShell.themes` (`shell.rs:146`); first load blocks in `run` (`bootstrap.rs:171-178`) | `run`, then `adopt_theme` (`shell/themes.rs:17`) |
| The session and cache paths | `vault_launch` from the root and `HOME` (`bootstrap.rs:115-119`, `crates/app/src/app_files.rs:15-35`) | `launch` |

Every row is built once, at construction. Nothing here is ever reassigned.
That is the property D2 keeps.

## What Zed does

- **The window's root holds the project, and a new project gets a new
  entity.** The window root is `MultiWorkspace`
  (`vendor/zed-reference/crates/workspace/src/multi_workspace.rs:305`).
  `MultiWorkspace::open_project` opens a folder in the current window
  (`multi_workspace.rs:1948-2000`). It asks the old workspace
  `prepare_to_close(CloseIntent::ReplaceWindow, ..)`, builds or finds a
  fresh `Workspace` for the paths, and detaches the old one. It never
  mutates the old `Workspace` into a new project.
- **`CloseIntent::ReplaceWindow`** is its own intent, beside `Quit` and
  `CloseWindow` (`vendor/zed-reference/crates/workspace/src/workspace.rs:720-726`).
  `prepare_to_close` runs the same checks for each (`workspace.rs:3831-3910`).
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
  `TestAppContext::simulate_path_prompt_response` answers it, and
  `did_prompt_for_paths` reports a pending one
  (`vendor/zed-reference/crates/gpui/src/app/test_context.rs:404-416`).
  `VisualTestContext` derefs to `TestAppContext` (`test_context.rs:868-874`).

## Decisions

### D1. The entry points

Slice 1 ships three entry points to one action, `OpenVault`:

- A button in the empty state, "Open Vault…" (D7).
- The key `⌘O`, bound in the `AppShell` context beside `cmd-n`
  (`crates/app/src/keymap.rs:84`). No binding uses `cmd-o` today
  (`grep -n '"cmd-o"' crates/app/src/keymap.rs` finds nothing).
- The command palette. It lists every action on the dispatch path
  (`window.available_actions`, `crates/app/src/command_palette/commands.rs:127-129`),
  so the root div's `on_action` listener (`crates/app/src/shell/render.rs:76`
  is the pattern) makes "Open vault" appear with no palette change.

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
layout are built in `AppShell::new` (`shell.rs:158-163,217`). Building late
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
true as written. The window, its frame and its blur stay. This is Zed's
shape: a new project gets a new `Workspace` entity, never a mutated one.

**C. Open a new window and close the old one.** Rejected now. Close is quit
(quit spec, Q1; `quit_on_close`, `crates/app/src/shell/quit.rs:79-86`), so
closing the old window needs a second, non-quitting close path. A second
`NSWindow` appears, the frame can jump, and the launch's first frame is paid
again. For a moment two shells and two `NoteStore`s exist. If both name one
vault, two watchers and two session writers share one session file. Several
windows must solve that anyway, but this feature should not.

**Several windows.** The maintainer wants several windows and tab tear-off
eventually. No entry in `docs/roadmap.md` schedules it (its "Application"
section, `docs/roadmap.md:180-214`, has none). B does not block it: a window
is one shell, and a second window is `open_window` with the same builder. The
open action then only picks "this window" or "a new window", as Zed's
`OpenPathPromptMode` does (`workspace.rs:907-911`). This spec builds no part
of it.

**The builder.** `root` (`bootstrap.rs:81-106`) returns an `Entity<AppShell>`
for `open_window`, whose closure takes `&mut App`
(`vendor/zed-reference/crates/gpui/src/app.rs:1605-1609`). `replace_root`
takes a closure over `&mut Context<AppShell>` that returns the `AppShell`
itself (`window.rs:2561-2566`). So `root`'s body moves into one function that
returns `AppShell` from a `Launch`, and both callers use it. The two callers
differ only in who calls `cx.new`.

**The old shell's lease.** The replace runs in the leave task's
`cx.update(|window, cx| ..)` on the async window context, not inside
`this.update`. So the old shell is not leased when its last handle drops.
gpui's behaviour when a root is replaced from inside its own update is not
checked here; the async call avoids the question.

### D3. Switching vaults

Allowed, in slice 2, through the same path as the first open. Even slice 1
has something to leave: a `Launch::Note` shell shows a detached note with no
vault (`bootstrap.rs:90-95`), and its empty tab shows the no-vault state. So
the leave path exists from slice 1, and a switch adds only the trigger.

The leave path is the quit's first half, `AppShell::quit`
(`shell/quit.rs:17-76`), with a different last step:

1. Check the chosen folder first (D5). A refused folder changes nothing.
2. If the chosen root equals the open vault's root, do nothing. Roots
   compare after `vault_root` (`bootstrap.rs:108-112`), which is lexical.
3. Write the session through `write_session` (`session.rs:131-138`), and
   start `NoteStore::flush` (`vault/save.rs:90-101`). Wait for both, bounded
   by `QUIT_WAIT` (`quit.rs:14`).
4. If `NoteStore::unsaved` is not empty, ask with the quit's alert and the
   buttons "Open Anyway" and "Cancel". Cancel keeps the old shell.
5. Read the new vault's session and theme in the background:
   `restore::read` (as `read_session` does, `bootstrap.rs:39-54`, but
   awaited, not `block_on`) and `theme::store::load`.
6. Replace the root (D2), focus the new shell, and call `adopt_theme`.

The quit's one flag, `AppShell.quitting` (`shell.rs:140-141`), becomes the
data it guards: `leaving: Option<Leave>` with `Leave::Quit` and
`Leave::Open`. A second quit or open while one is under way does nothing, as a
second `⌘Q` does today (`quitting.rs:225-233` pins it).

What happens to the rest when the old shell drops:

- **Unsaved notes.** Written by step 3, or listed by step 4's alert. "Open
  Anyway" loses what the alert lists, as "Quit Anyway" does.
- **The watcher.** It drops with `NoteStore._watch`.
- **In-flight index loads.** Their tasks drop with `IndexSync`. A read
  already inside `RealFs` finishes and lands nowhere.
- **A pending cache write.** An armed write drops with `CacheWriter`. A write
  already spawned is detached (`cache_writer.rs:88-99`) and ordered only
  against its own writer's `mark` (`cache_writer.rs:146-158`). See Risks.
- **The window frame.** The new shell does not move the window. A session's
  saved frame places the window only at launch (`bootstrap.rs:181-184`).

**Rejected: switching only after a restart.** It keeps the restart message
that this feature removes, and the leave path costs nothing extra once slice 1
has it.

### D4. Remembering

A launch with no argument reopens the last vault. Restore-tabs already
reopens the tabs of a vault; reopening the vault itself completes it. Zed's
default does the same (`LastSession`, see What Zed does).

The fact lives in one file beside the sessions:
`~/Library/Application Support/Inkra/last-vault.json`, holding
`{ "version": 1, "root": "/Users/me/Notes" }`. A function in
`app_files.rs` beside `session_file` (`app_files.rs:15-22`) names it.
It is written through `Fs::replace_bytes` (`crates/vault/src/fs.rs:190-191`)
and read through `Fs::load`, so `app` opens no file. `serde_json` is already
a dependency of `app` (restore-tabs spec, R2).

- Every successful vault open writes it: a launch with a folder argument and
  an open from the app. A note launch does not.
- The launch reads it only when `args_os().nth(1)` is `None`. The read
  blocks, as the launch's other reads do (`bootstrap.rs:121`).
- A missing or unreadable file opens no vault, silently.

**Rejected: storing it in the session file.** Session files are keyed by
vault root (`app_files.rs:14-22`). The last vault is the one fact that has no
root yet.

**A recent-vaults list: deferred.** Trigger: a user asks to switch between
vaults, or the first report of a second vault in use. The file's shape grows
to `recent: [..]` then; with no users, no migration is needed.

**`HOME` in the running app.** `vault_launch` needs `HOME`
(`bootstrap.rs:115-119`), and today only `launch` reads it
(`bootstrap.rs:127`). `run` stores it once in a gpui global holding
`home: Option<PathBuf>`, and the open path reads it there. A test sets the
global to a path inside its `MemFs`. This is one value, not a trait.

### D5. Failure

The open path checks the folder with `Fs::check_dir` (`fs.rs:171-172`)
before it leaves the old vault. A failure shows the notice (`show_notice`,
`crates/app/src/shell/notice.rs:80`) and changes nothing else:

- **Unreadable or deleted:** "Couldn't open “Notes”: {reason}", with the
  notice's own `reason` wording (`notice.rs:22-29` is the pattern).
- **The remembered vault is gone at launch:** the launch opens no vault and
  the empty state shows the notice "The last vault, “Notes”, is no longer
  there." It does not exit. **Rejected: exiting, as a failed argument
  does.** Every later Dock launch would then exit too, and nothing in the app
  could clear the file. A command-line folder that fails still exits
  (`bootstrap.rs:163-169`): the person typed that path and reads stderr.
- **A cancelled prompt:** nothing.
- **A file chosen:** impossible. The prompt allows folders only (D6).

**Open question: a folder that holds the app's own folders.** A root at `~`
or `/` holds `Library/Application Support/Inkra`. `app_file` then returns
`None` (`app_files.rs:29-34`), so the vault has no session and no cache, and
the scan walks the whole home folder. Today the command line accepts it
silently. Recommendation: refuse it from both the prompt and the command
line, through one check, with "Choose a folder inside “{name}”: it holds
Inkra's own files." The no-session case in `vault_launch` then becomes dead
code and goes. Reason: a vault that remembers nothing and indexes every file
of the home folder looks like a broken app. This is a product choice; the
code cannot settle it.

### D6. The seam for tests

No new trait. The app calls `cx.prompt_for_paths` (`app.rs:2032-2037`) with
`PathPromptOptions { files: false, directories: true, multiple: false,
prompt: Some("Open Vault") }` (`platform.rs:2935-2944`). In a headless test the
test platform queues the prompt. The test calls
`simulate_path_prompt_response` to answer it, or answers `None` to cancel
(`test_context.rs:404-416`, exercised at `test_context.rs:1504-1548`). The
folder lives in the shell's `MemFs`: the new shell reuses the old store's
`Arc<dyn Fs>` (`notes.read(cx).fs()`, as `panes.rs:41` does), so tests never
touch the real disk.

The unsaved alert is answered with `simulate_prompt_answer`, as the quit tests
do (`crates/app/src/shell/tests/quitting.rs:204-243`).

**Rejected: a `FolderPicker` trait with a real and a fake picker.** gpui
already owns both implementations. A trait here would have one implementer
behind a seam gpui provides.

### D7. The empty state

With no vault, the empty state draws:

- "No vault is open." in the placeholder colour, as today
  (`column.rs:32-46`).
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

`empty_lines` returns `&'static [&'static str]` today. It becomes one enum,
`Empty { NoVault, Pending, SelectNote }`, and `render_empty` matches it. The
button is then drawn only for `NoVault`, by the data, with no separate check.

**Rejected: the folder picker opening by itself at launch.** A modal that
appears before the window is useful is surprising, and a cancel leaves the
same empty screen. Obsidian shows a vault chooser instead; that is the
recent-vaults list, deferred in D4.

### D8. Deviations from Zed

1. **One vault per window, and opening replaces it.** Zed adds a folder to a
   project, and keeps several workspaces in one window (`MultiWorkspace`).
   A vault is one folder, and its session, cache and index are keyed by that
   folder (`app_files.rs`). So Inkra replaces the shell. It does not add the
   folder to it.
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

### Slice 1: open a vault from no vault

Written first. `the_open_button_opens_a_vault` fails before the change,
because no button exists.

- `shell/tests/open_vault.rs`:
  - `cmd_o_prompts_for_one_folder`: `no_vault(cx)`
    (`shell/tests/mod.rs:207-223`), simulate `cmd-o`, assert
    `did_prompt_for_paths`, and assert the options `directories`, not
    `files`, not `multiple`.
  - `the_open_button_opens_a_vault`: click the button by its debug selector,
    answer the prompt with `/vault`, run until parked. Assert the window's
    root shell has a vault at `/vault`, and that the tree lists its notes
    (`vault_names`, `mod.rs:225`). A plausible bug it catches: the new shell
    built but never set as the root.
  - `a_cancelled_prompt_changes_nothing`: answer `None`; the root is the same
    entity.
  - `a_missing_folder_shows_a_notice`: answer `/nowhere`; assert the notice
    text and the same root entity.
  - `a_detached_note_is_written_before_the_vault_opens`: launch with
    `Launch::Note`, type, open `/vault` before the autosave delay. Assert the
    file text in `MemFs`. A plausible bug: the leave path skipping the flush,
    so the dropped store drops the pending autosave.
  - `the_palette_lists_open_vault`: the palette's entries include "Open
    vault".
- `column.rs` tests: the `NoVault` state draws the button; `SelectNote` does
  not.
- Scene: `empty_no_vault` (`crates/app/src/shot/scenes.rs:95,478`). Render
  it and look: the button sits under the line, centred.

### Slice 2: switch vaults

- `switching_writes_the_left_vault_first`: open `/a` with a dirty note, open
  `/b`. Assert `/a`'s note text and `/a`'s session file in `MemFs`.
- `an_unwritable_note_asks_before_switching`: `make_unwritable`, as
  `quit_unwritable` does (`quitting.rs:189-202`). Assert the alert. "Cancel"
  keeps the root; "Open Anyway" replaces it.
- `opening_the_open_vault_does_nothing`: the root is the same entity and no
  session write happens.
- `the_new_vault_restores_its_tabs`: a session file for `/b` in `MemFs`;
  after the open, the tabs match it.
- `a_quit_during_a_switch_does_nothing`: `cmd-o`, answer, then `cmd-q`
  before the flush lands; one alert at most.
- `the_new_vault_adopts_its_theme`: `/b/.inkra/settings.json` names a
  built-in theme; after the open, that theme is installed.

### Slice 3: remember the last vault

- `app_files` tests: the last-vault file's path under `HOME`.
- Launch tests, on `MemFs` through `open_argument`'s neighbour: no argument
  and a last-vault file open that vault; a gone vault yields no vault and the
  notice; a corrupt file yields no vault and no notice.
- `opening_a_vault_records_it`: after an open from the app, the file holds
  the new root.
- `crates/app/tests/cli.rs` keeps the process-level cases: a folder argument
  that fails still exits non-zero.

## Risks

- **Two cache writers on one file.** Reopening the vault just left, while its
  last cache write is still in `RealFs`, gives two writers with separate
  `mark`s (`cache_writer.rs:146-158`). The older write could land after the
  newer one. The cache is a cache: the next start probably sees a stale file
  and rebuilds the difference. Slice 2 should measure whether it can happen
  in practice before adding any ordering.
- **Detached subscriptions of the old shell.** `AppShell::new` detaches
  several subscriptions (`shell.rs:172-212`). gpui is expected to stop calling
  them once their subscriber entity is released. Slice 1's tests should
  include one event after the replace, to confirm the old shell sees nothing.
- **The old shell's quit handler.** `quit_on_close` registers a should-close
  callback on the platform window (`quit.rs:79-86`). gpui keeps one such
  callback per window (`window.rs:7164-7173`), so the new shell's call
  replaces it. A test closes the window after a replace and asserts the new
  shell's quit runs.
- **The first frame.** A launch with no argument now reads one more file
  before the window opens. It is a few bytes; `docs/startup-floor.md`'s
  budget should not move. Measure it once with `Timeline`.

## Plan

Each slice is one PR and is green under `./scripts/check.sh` alone.

1. **Open a vault from no vault.** `OpenVault`, `⌘O`, the palette entry,
   `ui::Button`, `Empty`, the empty state's text. The shell builder shared by
   `root` and `replace_root`. The `HOME` global. The leave path with
   `Leave { Quit, Open }`, used for the detached note. The folder check and
   its notice. This is the smallest thing that lets a person open a vault
   from a Dock launch. About 300 lines, half of it tests.
2. **Switch vaults.** The same action with a vault open: the same-root check,
   the unsaved alert's "Open Anyway", the new vault's session read and theme.
   About 150 lines.
3. **Remember the last vault.** The last-vault file, its write on each open,
   its read at a launch with no argument, the gone-vault notice. About 150
   lines.

Slice 2 and slice 3 depend on slice 1 and not on each other. When slice 1
lands, the vault spec's Deferred row for the folder picker
(`2026-09-20-inkra-vault-design.md:682`) moves to "done" and the roadmap's
Vault paragraph (`docs/roadmap.md:118-128`) drops the picker.

## Out of scope

- An app bundle and a Dock icon (packaging).
- A menu bar (D1).
- A recent-vaults list (D4).
- Several windows and tab tear-off (D2).
- Opening a vault from Finder through `on_open_urls`
  (`vendor/zed-reference/crates/gpui/src/app.rs:359`). Trigger: the app
  bundle declares a folder type.
