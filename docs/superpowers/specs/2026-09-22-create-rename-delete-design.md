# Create, rename, delete — design

Date: 2026-09-22. Status: **revised after red/blue review**.

The vault stops being read-only. That is the trigger the deferred row in
`2026-09-20-inkra-vault-design.md` recorded, and the empty state has been
naming an action it cannot perform since 2026-09-21 (`shell.rs`,
`render_empty`).

A first draft of this document was reviewed by three adversarial passes
(concurrency, gpui, filesystem) and two constructive ones (modularity, YAGNI).
Where the draft was wrong, the correction is recorded in a **Corrected** note
rather than quietly rewritten — the same discipline
`2026-09-21-autosave-design.md` uses.

## Scope

| Operation | Note | Folder |
|---|---|---|
| Create | yes | yes |
| Rename | yes | yes |
| Delete | yes, to the macOS Trash | **no** — deferred |

Folder deletion is the only operation that destroys N notes with one gesture.
Deferred with a trigger: *a user creates a folder they then want rid of.*

## What Zed does, and what we take from it

Taken: one edit state discriminated by an `Option`
(`crates/project_panel/src/project_panel.rs:239-256`); the editable row inside
the list rather than an overlay (`:5914`, `:6573-6608`); a stricter re-check at
confirm (`:2082-2285`); blur tries to confirm and discards when invalid
(`:818-838`); block on a duplicate, never auto-uniquify — Zed auto-suffixes
only for paste (`:3537-3599`); a native prompt whose wording names the unsaved
changes about to be lost (`:2768-2828`); selection follows deterministically —
next sibling, else previous (`:2993-3070`); identity survives a rename by
inode, reconciled during the rescan (`crates/worktree/src/worktree.rs:3302-3325`),
which `Vault::apply` already does; an explicit rescan after our own mutation
rather than waiting on a watcher (`crates/project/src/worktree_store.rs:719-728`).

Rejected: `RelPath` semantics that let a typed name create nested directories;
the undo manager over filesystem operations (`:2159-2181`) — Trash is the
recovery story; `temporarily_unfolded` and folded ancestor chains; the
synthetic `NEW_ENTRY_ID` spliced into a git-aware snapshot (`:93`, `:4475-4502`);
`CreateOptions`/`RenameOptions`/`RemoveOptions` (`crates/fs/src/fs.rs:291-315`),
because every call site of ours wants one policy — never overwrite — and
encoding one policy as a parameter invites a second.

## The keymap, and why its bindings get scoped

> **Corrected.** The draft claimed a context-less binding "beats every scoped
> one, always". It does not. `binding_enabled` scores a predicate-less binding
> at `contexts.len()` (`gpui/src/keymap.rs:246-252`), and `depth_of` scores a
> predicate naming the **leaf** frame at that same value
> (`keymap/context.rs:260-268`). They **tie**. The tie breaks on `BindingIndex`
> — insertion order into the `Vec`, later wins (`keymap.rs:188-190`). So a
> `Sidebar`-scoped binding appended after the editor's would already win today,
> and the real hazard is not that a key cannot be claimed but that which action
> fires depends on an undocumented ordering of a `Vec` literal.

Scoping the editor's bindings to `Some("EditorView")` removes the tie instead
of betting on it: with "EditorView" absent from the sidebar's context stack,
`depth_of` returns `None` and the binding is **excluded from matching**, not
merely outranked. `cmd-q` stays context-less — it is the application's.
`cmd-s` already carries `Some("AppShell")` and is the precedent.

> **Corrected.** Scoping has a consequence the draft missed: once
> `backspace`, `delete`, the arrows, `home`, `end` and `cmd-a` are exclusive to
> `EditorView`, they are excluded while a name field is focused. The draft gave
> `NameField` two bindings. It needs the whole editing set, or a typo in a
> filename cannot be corrected.

## Layering

Unchanged: `text` → `editor` → `app`, with `vault` beside it.

### `inkra-vault`

Three methods on `Fs`. One create, discriminated by `Kind`:

```rust
/// Creates an empty file or a directory. Fails if anything exists at `path`.
async fn create(&self, path: &Path, kind: Kind) -> Result<FileIdentity>;
/// Moves `from` to `to`. Never replaces a different file at `to`.
async fn rename(&self, from: &Path, to: &Path) -> Result<()>;
/// Moves the file to the platform trash. Not a directory: see Scope.
async fn trash(&self, path: &Path) -> Result<()>;
```

`create` returns the identity because that is how `app` finds the entry the
rescan mints — by inode, not by path, so a concurrent rescan cannot hand back
the wrong row. `rename` returns nothing: neither branch below changes the
inode, so the caller already holds the identity.

#### Rename, and why folders are the easy case

`rename(2)` is already atomic — it "guarantees that an instance of new will
always exist, even if the system should crash in the middle of the operation"
(`man 2 rename`, macOS). The hazard is not atomicity, it is that "if new
exists, it is first removed". Which collisions the kernel already refuses,
each row reproduced against real APFS during review:

| Rename | `rename(2)` | Destroys bytes? |
|---|---|---|
| note → free name | moves | no |
| **note → existing note** | **"new is first removed"** | **yes** |
| note → existing folder | `EISDIR` | no |
| folder → free name | moves | no |
| folder → existing note | `ENOTDIR` | no |
| folder → non-empty folder | `ENOTEMPTY` | no |
| folder → empty folder | removes the empty folder | no bytes |

One destructive cell, note-over-note. `renamex_np(RENAME_EXCL)` closes it but
needs `libc` and `unsafe`, which `unsafe_code = "deny"` forbids workspace-wide.
`RealFs::rename` closes it with the stdlib:

1. If `from` is a symlink, plain `fs::rename`. `rename(2)`: "If the final
   component of old is a symbolic link, the symbolic link is renamed, not the
   file or directory to which it points." `fs::hard_link` resolves the link and
   would replace it with a plain link to its target. This mirrors the symlink
   branch `RealFs::save` already carries (`real_fs.rs:154-157`).
2. Otherwise `fs::hard_link(from, to)` — `link(2)` "atomically creates the
   specified directory entry" and returns `EEXIST` if `to` exists
   (`man 2 link`). No TOCTOU window. Then `fs::remove_file(from)`.
3. On `EEXIST`, stat `to`. **If its `(device, inode)` is the source's, the two
   names denote one file** — a case or normalization change — and plain
   `fs::rename` is both safe (nothing to destroy: it is the same file) and
   correct (it rewrites the stored form). Otherwise it is a genuine collision.
4. On `EPERM`/`ENOTSUP`/`EOPNOTSUPP` — a filesystem without hard links, or a
   directory, which POSIX forbids linking — stat `to` then plain `fs::rename`,
   leaving the one window `Fs::save` already accepts and documents.

> **Corrected.** The draft had only steps 2 and 4, and review demonstrated on
> real APFS that this makes a case-only rename impossible: APFS is
> case-insensitive-but-preserving by default, so `link("notes.md","Notes.md")`
> returns `EEXIST` and the user cannot fix their own note's capitalisation. The
> same failure hits a Unicode normalization change — NFC `Réunion.md` against
> NFD `Réunion.md`. Verified during review that `stat` on either spelling
> resolves to the source's inode, that a genuinely different file does not, and
> that `fs::rename` does rewrite the stored case. Step 3 is identity-based and
> not string-based deliberately: normalization equivalence is a property of the
> volume, not of Unicode, and code cannot know whether a volume folds — the
> inode answers.

Costs this leaves standing, stated rather than hidden:

- A crash between `hard_link` and `remove_file` leaves both names on one inode.
  No bytes are lost, and `Vault::apply` collapses hard links into one entry
  (`vault.rs:178-180`) — but which name survives follows `read_dir` order,
  which review showed is neither alphabetical nor chronological on APFS. So a
  crash mid-rename can leave the note under its **old** name with no error.
  Trigger to fix: *a crash mid-rename is observed.*
- Trashing a note with `nlink > 1` removes that name only; the twin keeps the
  bytes and resurfaces on the next rescan under the same `EntryId`. Reachable
  only via the crash above or a hand-made link.
- A vault spanning a mount point fails `EXDEV` on both branches. No copy+delete
  fallback.

#### Names

The **rule** is a property of a vault path and lives here. The **wording** does
not: `app` owns user-visible English.

```rust
pub enum NameProblem { Empty, Separator, LeadingDot, Reserved, TooLong }
pub fn check_name(name: &str) -> Option<NameProblem>;
```

Not a `thiserror` enum and never inside a `Result`: CLAUDE.md reserves that for
`VaultError`, and this is a diagnostic, not a failure.

- `Empty` — after trimming.
- `Separator` — `/` or `:` anywhere. The colon is accepted by the syscall but
  macOS displays it as `/`, so a note named `plan:notes` looks nested
  everywhere outside our own sidebar.
- `LeadingDot` — `read_dir` filters dotfiles, so a note named `.plan` would
  vanish from the vault the instant it was created. A trap, not a preference.
- `Reserved` — `.` and `..`. `RelPath::new` normalizes a lone `.` away rather
  than refusing it (`path.rs:24-25`), so this is the only guard.
- `TooLong` — over 255 UTF-16 code units, which is the APFS limit. Without it
  the name fails deep inside `Fs` as `ENAMETOOLONG`.

Leading and trailing whitespace is **trimmed**, not reported. The draft made it
a warning tier, which put a second severity axis through every caller for a
name nobody means to type.

A property test asserts every name `check_name` accepts is one `RelPath::new`
accepts, so the two lists cannot drift.

```rust
/// The file name a note typed as `name` gets. Always a literal lowercase
/// `.md`, replacing any existing extension whose case-insensitive form is
/// `md`.
pub fn note_file_name(name: &str) -> String;
```

> **Corrected.** The draft said "`.md` appended unless already present", which
> leaves `plan.MD` alone. `RealFs::read_dir`'s filter is case-sensitive
> (`real_fs.rs:82`), so that file would never appear in the vault — the same
> invisibility trap `LeadingDot` exists to prevent, reached by another door.

#### `MemFs`

Gains the three methods and a trash map, so a test can assert the bytes still
exist. `MemFs::rename` already preserves the identity and stops being a test
helper. One new injector, `make_uncreatable(path)`, matching `make_unwritable`.

`MemFs` keys a case-sensitive, normalization-sensitive `BTreeMap`, so it cannot
model step 3 above. **This design fires the deferred "Case-insensitive
`MemFs`" row.** Rather than take that on here, the case and normalization tests
live in `vault/tests/real_fs.rs`, which already exists for exactly this reason —
"a fake filesystem cannot prove the real one's symlink and dotfile filtering".

`RealFs::trash` gets **no automated test**: every run would deposit a file in
the developer's real Trash. Verified once by hand, and stated here so the gap
is not mistaken for coverage.

### `inkra-app`

#### Ordering: the chain, not the sequence

The draft proposed generalising `Writer`'s sequence guard to cover rename and
trash. Review demonstrated this is the wrong mechanism, twice over.

> **Corrected — the `Writer` does not exist for the normal target.** `Writer`
> is a field of `OpenNote`, created only by `track` after a load succeeds
> (`vault.rs:308-341`). Renaming a note nobody has clicked — the primary case —
> has no writer at all, and the draft never said where one would come from.

> **Corrected — the sequence guard cannot order a rename.** `Self::write`
> resolves the path and mints its sequence **inside** the spawned task, after
> the timer (`vault.rs:371-398`, then `:526-551`). So a write pending when the
> rename is issued mints a **higher** sequence, and both orders lose: if the
> rename takes the lock first, `write_through`'s `seq < latest` is false, the
> write proceeds against the old path, `RealFs::save` finds `NotFound`, treats
> it as no conflict (`real_fs.rs:122-126`) and **recreates the note under its
> old name** — after which `write`'s success path re-keys the `EntryId` onto
> the stray file (`vault.rs:579-581`). If the write takes the lock first, the
> rename's lower sequence is dropped as stale and **the rename silently never
> happens**. Sequences order by moment of decision, which is the right policy
> for two competing versions of one text and a category error for a move.

The mechanism that does order these correctly already exists: the **chain**
`schedule_save` builds, where each new task awaits the previous one
(`vault.rs:369`, "Taken, not dropped: the new chain awaits it below"). A rename
appended to that chain runs after every operation decided before it, and every
operation decided after it awaits the rename. No lock ordering to reason about.

One change makes it usable: the chain moves off `OpenNote` and into
`NoteStore`, keyed by `EntryId`, living whether or not the note is open. Loads,
writes, renames and trashes all append to it. `Writer` stays exactly as it is,
for the one caller that cannot chain — the quit flush, which `App::shutdown`
polls while the foreground session is blocked (`gpui/src/app.rs:1073`).

Two consequences to close explicitly:

- **The quit flush can suppress a trash.** The flush mints the highest sequence
  a note will ever see; a trash queued in the chain has a lower one and is
  dropped as stale, so a confirmed delete silently does not happen. A
  synchronous `trashing` flag on the note, set when the command is issued, is
  what `flush_all` reads to skip it. Synchronous, so no lock is involved and
  the "no foreground code may hold this guard" rule stands.
- **The lock must span the filesystem call**, as `write_through` already holds
  it across `fs.save` (`vault.rs:113-117`). A narrower critical section around
  the bookkeeping alone would leave the actual I/O unserialised.

#### Commands

```rust
pub(crate) fn create(&mut self, parent: Option<EntryId>, kind: Kind, name: &str, cx)
    -> Task<Option<EntryId>>;
pub(crate) fn rename(&mut self, id: EntryId, name: &str, cx) -> Task<bool>;
pub(crate) fn trash(&mut self, id: EntryId, cx) -> Task<bool>;
```

Two entry points by design: action dispatch for keyboard and, later, a command
palette; a direct method call for a programmatic caller such as a future
`[[link]]` that creates a missing note. Not an event — `Sidebar` already holds
`Entity<NoteStore>` and reads it synchronously (`sidebar.rs:58`, `:74`, `:121`),
so calling a method adds no coupling that is not already there. `NoteActivated`
is indirect for a different reason: it exists so the sidebar need not know an
editor exists (`sidebar.rs:108-109`), and no such ignorance boundary applies
here.

`Vault` gains one accessor over the map it already keeps:

```rust
#[must_use]
pub fn id_for(&self, identity: FileIdentity) -> Option<EntryId>;
```

Distinct from the deferred `by_path`: `id_for` answers "which id did the rescan
just mint for an identity I already hold", `by_path` answers "what id does this
path have" — the `[[link]]` case, still deferred.

#### Rescans chain too

`NoteStore::rescan` returns a `Task<()>`; the conflict call site keeps
`.detach()`. A full `collect` + `apply` per mutation, not an incremental
update: both already exist, both are keyed by inode, and a second
reconciliation path is a second way for `entries` and `by_identity` to
disagree.

> **Corrected.** The draft raised rescan frequency from "only on a conflict" to
> "every mutation" without noticing that makes concurrent rescans reachable for
> the first time. `Vault::apply` replaces `entries` and `by_identity` outright
> (`vault.rs:200-201`) with no staleness check, so two overlapping walks mean
> the last to apply wins — and a stale one can re-admit a just-trashed entry,
> mint a fresh `EntryId` for a file whose buffer is still tracked under the old
> one, and thereby orphan unsaved text. That is the defect `NoteStore`'s own
> header comment says was fixed, reopened through a side door.

Rescans therefore chain, exactly as writes do: each awaits the previous, so the
last walk applied is the last walk taken. The same mechanism twice, not a
second one.

A related hole this makes more likely, and closes: `write` treats a vanished
vault entry as "write nothing" with no signal (`vault.rs:543-547`). It now
emits `SaveFailed`, so a note whose file was moved out from under us stops
failing silently.

#### Deleting the note on screen

`NoteStore::trash` sets `trashing` synchronously, runs the fs call through the
chain, and only **on success** removes the note from `open` and `observers`,
clears `current` when it matched, and emits:

```rust
/// Fired by trash alone. Buffer eviction, when it lands, is a different
/// signal: an evicted note is not necessarily the one on screen.
pub(crate) struct NoteClosed(pub(crate) EntryId);
```

On failure it clears `trashing` and emits `TrashFailed`. Success-ordered, not
optimistic: an optimistic close that then failed would leave the editor blank
and the buffer unreachable, which is worse than the `SaveFailed` path, where
the note at least stays visibly dirty.

`AppShell` subscribes to `NoteClosed` and sets `self.editor = None`, falling
back to `render_empty`. Same seam as `NoteActivated`, so a tab bar later
subscribes to it rather than being wired in separately.

Unsaved text in a trashed note is lost. The prompt says so.

### The sidebar

#### Selection and focus

The sidebar becomes focusable, which **fires the deferred "Focusable sidebar"
row**. That is a dependency, not scope creep: rename and delete need a target.
`selected: Option<EntryId>`, a `FocusHandle` tracked on the list, and a
selected-row background in `theme`. A row's `on_mouse_down` focuses the
sidebar's own handle directly, as `EditorView::on_mouse_down` already does;
`AppShell::focus_handle` is unchanged, and `cmd-s` still reaches its handler
because dispatch bubbles from the focused leaf up the whole ancestor chain.

#### Edit state

```rust
struct EditState {
    /// `None` while creating — nothing exists yet. `Some` is a rename.
    target: Option<EntryId>,
    parent: Option<EntryId>,
    kind: Kind,
    field: Entity<NameField>,
    /// Set by a rejected `Confirm`, cleared by the next one. Not recomputed
    /// per keystroke.
    rejected: Option<Rejection>,
    _confirmed: Subscription,
}
```

`Rejection` is `app`'s own type: a `NameProblem` from `vault`, or `Duplicate`,
which `vault` cannot answer — it needs the live `Vault` snapshot, not a
property of a path. Checked synchronously against the target parent's children
before the command is issued.

> **Corrected.** The draft listed a "duplicate name refuses to confirm" test
> with no variant to carry it and a `Confirm` that cleared state
> unconditionally. A `Confirm` that fails validation now leaves the field open.

Validation runs at `Confirm`, not per keystroke. The draft copied Zed's live
validation, which amortises over a much larger surface than one gesture ended
by one keystroke; the message persisting until the next attempt is what avoids
the silent no-op Zed's own code was criticised for.

While `edit_state` is `Some`, the sidebar renders one row as the field and does
**not** register its `SelectPrev`/`SelectNext`/`Rename`/`Trash` handlers, so
those keys reach no handler while a name is being typed.

#### Two gpui hazards the field sits on

- **A row scrolled out of `uniform_list`'s visible range loses its dispatch
  node.** `render_items` runs only for the visible range
  (`elements/uniform_list.rs:479-489`), and gpui does not blur a focused handle
  whose element stopped rendering: `focus_node_id_in_rendered_frame` falls back
  to the root (`window.rs:6244-6251`), so every keystroke — Confirm, Cancel,
  every character — vanishes with no panic and no feedback. Entering edit mode
  scrolls the row into view, and any `rebuild` while editing does so again.
- **Every row is measured from item 0** (`elements/uniform_list.rs:658-680`),
  so the field row must be exactly `UI_LINE_HEIGHT` tall, like every other.

#### The name field

A new `app/src/field.rs`. `NameField` is a single-line editable text view:
`Entity<Buffer>` + `EditorState` + `FocusHandle` + `EntityInputHandler` + a
one-line element, emitting `Confirm` and `Cancel`.

`EntityInputHandler` rather than `on_key_down`, because that *is* the macOS
input protocol: a dead key (`^` then `e`) and any IME arrive as marked text,
and a key-down reader drops both. A vault whose notes cannot be named *Réunion*
is not finished.

Most of what looked like duplication is not gpui code at all, and moves down
the stack instead of being copied:

> **Corrected.** The draft accepted ~200 duplicated lines with a trigger to
> abstract later. Review showed only `bounds_for_range` and
> `character_index_for_point` (`editor/input.rs:130-159`) touch
> `DocumentLayout`; every other method operates on `Buffer` and `EditorState`
> alone. The second consumer exists in this same design, so the extraction
> happens now, not at a hypothetical third.

To `inkra-text`, beside the existing single-offset conversions:

```rust
pub fn to_utf16_range(&self, range: &Range<ByteOffset>) -> Range<usize>;
pub fn from_utf16_range(&self, range: &Range<usize>) -> Range<ByteOffset>;
```

To `inkra-editor`:

```rust
pub fn target_range(&self, buffer: &Buffer, range_utf16: Option<&Range<usize>>) -> Range<ByteOffset>;
pub fn replace_in_range(&mut self, buffer: &mut Buffer, now: Instant,
    range_utf16: Option<&Range<usize>>, new_text: &str);
pub fn mark_and_replace_in_range(&mut self, buffer: &mut Buffer, now: Instant,
    range_utf16: Option<&Range<usize>>, new_text: &str,
    new_selected_range_utf16: Option<Range<usize>>);
```

Both `EntityInputHandler` impls become short forwards. The moved logic also
gains plain `#[test]` coverage in crates that need no window, which is where
CLAUDE.md says it belongs.

#### Actions and bindings

| Keystroke | Action | Context |
|---|---|---|
| `cmd-n` | `NewNote` | `AppShell` |
| `cmd-shift-n` | `NewFolder` | `AppShell` |
| `enter` | `Rename` | `Sidebar` |
| `cmd-backspace` | `Trash` | `Sidebar` |
| `up` / `down` | `SelectPrev` / `SelectNext` | `Sidebar` |
| `enter` | `Confirm` | `NameField` |
| `escape` | `Cancel` | `NameField` |
| `left` `right` `shift-left` `shift-right` `backspace` `delete` `home` `end` `cmd-a` | the editor's existing action types | `NameField` |
| the editor's existing set | unchanged actions | **`EditorView`**, was `None` |

The field binds the same action types the editor does, at its own context, and
registers its own handlers. `NewNote`/`NewFolder` scope to `AppShell` so they
fire with the editor focused. `Rename` and `Trash` read `Sidebar::selected`; a
future palette invocation with nothing selected has no target, so the actions
carry a doc comment saying they would need an `Option<EntryId>` payload then.

Where a created entry goes: inside the selection when it is a folder, beside it
when it is a note, at the vault root when there is nothing selected.

#### Delete

`Window::prompt(PromptLevel::Info, …)` — "Move “{name}” to the Trash?", buttons
`["Move to Trash", "Cancel"]`, with the detail "It has unsaved changes, which
will be lost." when the note is dirty. Re-entrancy is not a hazard: the
`prompt_builder` is taken and restored synchronously inside one call
(`window.rs:6459-6481`).

Headless-testable through `cx.has_pending_prompt()`, `cx.pending_prompt()` and
`cx.simulate_prompt_answer("Move to Trash")` (`app/test_context.rs:410`).

**No shot scene may reach the prompt.** `Stage` runs the real macOS platform
(`shot/stage.rs:36`), where `MacWindow::prompt` shows an `NSAlert` via
`beginSheetModalForWindow` and `simulate_prompt_answer` does not exist — a
scene that got there would park forever. Stated in `scenes.rs` beside the
catalogue.

Selection after the fact: next sibling, else previous, else `None`.

### Trash, the dependency

`trash = "5"`, verified during review: 5.2.9 published 2026-09-13, no advisory,
MSRV 1.85 against our 1.96.1. Not Zed's fork — that exists only to support undo
of a delete (their issue #5039), which we do not build.

One trap, and it gets a load-bearing comment in one place: the published
default on macOS is `DeleteMethod::Finder`, an `osascript` round-trip that
raises an Automation permission prompt and fails headless. `NsFileManager` must
be set explicitly. The path must be absolutised with `std::path::absolute` and
never `canonicalize`, or trashing a symlink trashes its target. The call is
blocking and runs on the background executor, as every other `Fs` call already
does.

Cost accepted: `NsFileManager` does not produce Finder's "Put Back" metadata.
A trashed note is recovered by dragging it out of the Trash.

## Slices

Vertical. Each touches the filesystem, the model, the UI and a test, and each
leaves `./scripts/check.sh` green. S0 folded into S1 — scoping the keymap has
no user-visible effect on its own, and S1's own bindings are the first thing
that needs it.

**S1 — Delete a note.** Scope the editor's bindings. Move the operation chain
off `OpenNote` into `NoteStore`. `Fs::trash` and the `MemFs` trash map. The
`trashing` flag and the `flush_all` skip. Rescan chaining. `NoteClosed`,
`TrashFailed`, `SaveFailed` on a vanished entry. Sidebar selection and focus.
The prompt. Selection follow-up. No text field.

This is the heavy slice, and deliberately so: it carries the ordering
restructure, which is the highest-risk work and the thing the other two build
on. The alternative considered was building `NameField` first, in isolation, so
its shape could be reviewed before committing to the ordering design. Risk-first
wins because the ordering defects above were found in review rather than in
code, and they are cheapest to fix while nothing else depends on them.

**S2 — Create a note and a folder.** `Fs::create`, `check_name`,
`note_file_name`, `Vault::id_for`, the `input.rs` extraction down into `text`
and `editor`, `NameField`, the placeholder row, the duplicate check.

**S3 — Rename a note and a folder.** `Fs::rename` with all four branches,
seeding the field with the current stem. Reuses S1's chain and S2's field.

Two shot scenes, not three: a selected row, and a row being typed into. A
rejected name is visually a message and buys little; a delete cannot be shot at
all.

## Tests

The load-bearing set. Anything that would still pass with the mechanism
deleted is not on it.

`vault`:

- Every row of the collision table, against `MemFs`.
- `real_fs.rs`: `hard_link` refuses an existing destination; the inode survives
  a rename; a folder takes the fallback; **a case-only rename succeeds**; **a
  normalization-only rename succeeds**. The last two cannot run against
  `MemFs`.
- The identity proptest grows `Op::Create`, `Op::Rename`, `Op::Trash`.
- `check_name` and `note_file_name` by identity, not by message text, plus the
  property test tying `check_name` to `RelPath::new`.

`app`, `#[gpui::test]` against `MemFs`:

- A rename issued while a write is pending, through `MemFs::block_save`: the
  note ends at the new path with the new text, and nothing exists at the old
  one. This is the test the chain restructure exists for, and the draft's
  design fails it.
- A trash in the same shape: the file does not come back.
- A trash confirmed immediately before quit is not suppressed by `flush_all`.
- Trashing the open note empties the editor and clears `current`.
- A duplicate name is rejected and the field stays open.
- Each of the three commands is a no-op on `EntryId::DETACHED`.
- Every binding in the table, through `simulate_keystrokes`. The
  `Sidebar`-scoped ones need a window rooted at `AppShell`, in `shell.rs`.
- A field on a row that scrolls out of the visible range still receives its
  keystrokes.

Plain `#[test]`, not `#[gpui::test]`, for the rename/trash ordering itself:
`TestScheduler` runs every runnable on the one thread driving the block, so it
can model "queued" but never "a background thread already inside `rename()`".
Precedent: the existing ordering tests in `app/src/vault.rs`.

## Don't recreate — rows this adds

| Need | Use | Where |
|---|---|---|
| Validate a typed vault name | `check_name`, returning `NameProblem` | `vault/src/path.rs` |
| Turn a typed name into a file name | `note_file_name` | `vault/src/path.rs` |
| Create a file or a folder | `Fs::create` | `vault/src/fs.rs` |
| Move one, never clobbering a different file | `Fs::rename` | `vault/src/fs.rs` |
| Move a file to the platform trash | `Fs::trash` | `vault/src/fs.rs` |
| The `EntryId` a rescan minted for a known identity | `Vault::id_for` | `vault/src/vault.rs` |
| Order every filesystem operation on one note | the per-`EntryId` chain | `app/src/vault.rs` |
| Create, rename or trash, from any caller | `NoteStore::create` / `rename` / `trash` | `app/src/vault.rs` |
| Tell anyone the note on screen was closed under it | `NoteClosed` | `app/src/vault.rs` |
| Tell anyone a delete failed | `TrashFailed` | `app/src/vault.rs` |
| A single-line editable text view | `NameField` | `app/src/field.rs` |
| The sidebar's in-place create/rename row | `EditState` | `app/src/sidebar.rs` |
| Confirm before something destructive | `Window::prompt`, first used here | `app/src/sidebar.rs` |
| UTF-16 range conversion | `Buffer::to_utf16_range` / `from_utf16_range` | `text/src/buffer.rs` |
| Apply an IME edit to a buffer | `EditorState::replace_in_range` / `mark_and_replace_in_range` | `editor/src/state.rs` |

## What this leaves open

- A crash between `hard_link` and `remove_file`: the note may reappear under
  its old name, non-deterministically.
- Trashing a note with `nlink > 1` does not destroy its bytes.
- A vault spanning a mount point: `EXDEV`, no fallback.
- `MemFs` stays case-sensitive; the trigger has fired but is paid in
  `real_fs.rs` instead.
- `RealFs::trash` has no automated test.
- Bulk operations: every command ends in its own rescan, so N deletes would be
  N full walks. No trigger yet.
- Folder deletion.
