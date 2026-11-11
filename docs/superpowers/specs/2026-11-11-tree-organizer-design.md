# The tree as an organizer: moving files, rewriting links

Status: **draft 2026-11-11**. Builds on `2026-09-22-create-rename-delete-design.md`
(the tree ops), `2026-09-24-links-and-backlinks-design.md` (resolution),
`2026-11-02-quit-and-failures-design.md` (Q7 `moves`, Q8 retries, Q9 the notice) and
`2026-09-29-note-session-design.md` (the chain). Names of new types and functions here are
proposals; the code may pick others, but the shapes and rules hold.

## Context

The tree can create, rename and trash. It cannot move a file to another folder. A rename
also breaks the links that name the file. The links spec lists "Rewriting links when a note
is renamed" as out of scope (`2026-09-24-links-and-backlinks-design.md:25`). It defers it
because Obsidian's rewrite races with open buffers (`:106-108`), and it says a rewrite "walks
backlinks directly" when it lands (`:70-72`).

Nothing in the code moves a file to another folder or rewrites a link today. The tree's
actions are `Trash`, `SelectPrev`, `SelectNext`, `Rename`, `NewNote` and `NewFolder`
(`crates/app/src/sidebar/mod.rs:33-35`). The tree has no drag or drop handler.

### One rename, today

1. The sidebar checks the name with `check_name` and `Vault::name_taken`
   (`crates/app/src/sidebar/edit.rs:232-238`), then calls `NoteStore::rename`
   (`edit.rs:266-268`).
2. `rename` queues one operation on the note's own chain, with `PendingSave::Write`
   (`crates/app/src/vault/tree_ops.rs:152`). `PendingSave::Write` writes a pending save
   now instead of dropping it (`crates/app/src/vault/session/queue.rs:97-100`).
3. At run time, the operation resolves `from` with `path_of` and builds
   `to = from.parent()?.join(file_name)` (`tree_ops.rs:155-161`). It sets
   `moves[id] = None` (`tree_ops.rs:160`).
4. `Fs::rename` runs on the background executor (`tree_ops.rs:166-171`). On failure,
   `moves` goes back to its previous value and `RenameFailed` is emitted
   (`tree_ops.rs:174-180`).
5. On success, `moves[id] = Some(to)` and a rescan starts (`tree_ops.rs:182-183`). A
   detached task waits for the rescan and removes the move if it still equals `to`
   (`tree_ops.rs:190-206`).
6. `path_of` reads `moves` first, then the vault (`tree_ops.rs:59-66`). A `None` move means
   no path is safe, so a write in that window reports `SaveFailed` with no error
   (`crates/app/src/vault/save.rs:168-171`).
7. The index learns the new name when the rescan swaps. The vault notifies
   (`crates/app/src/vault/scans.rs:122-138`), `IndexSync::reconcile` starts a build
   (`crates/app/src/index.rs:79`, `:250-253`), and the build lands `set_notes`
   (`crates/index/src/index.rs:66-92`).

The `EntryId` follows the inode across `Fs::rename` (`crates/vault/src/fs.rs:199-201`,
`crates/vault/src/real_fs.rs:245-248`, `crates/vault/src/mem_fs.rs:379-381`). So the open
buffer, the tab and the index's source rows survive a rename unchanged.

## What Zed and note apps do

### Zed's project panel

- **Drag onto a folder is the move gesture.** `.on_drag` carries a `DraggedSelection`
  (`vendor/zed-reference/crates/project_panel/src/project_panel.rs:6290-6293`,
  `:6458-6476`). `.on_drop` calls `drag_onto` (`:6486-6494`, `:5245-5443`).
- **A modifier copies instead.** `drag_onto` copies when the modifier is held (`:5277`).
- **A move is a rename with another parent.** `move_worktree_entry` calls the worktree's
  rename (`:4300-4349`, `crates/project/src/worktree_store.rs:616`). There is no second
  operation.
- **No-op cases.** A drop onto the entry itself is a no-op (`:4307`). A drop onto a file
  targets that file's folder (`:4320-4324`). A drop that leaves the path unchanged is a
  no-op (`:4337`). The destination folder expands (`:4346`).
- **A selection moves as its disjoint roots.** `disjoint_entries` drops entries inside a
  selected folder (`:4365-4413`).
- **Hover expands a folder.** A drag held over a closed folder expands it after 500 ms
  (`:6425-6429`).
- **A collision is explained, not retried.** `explain_move_error` words it as "Failed to
  move `{from}` to `{to}`. A file or folder already exists there." (`:8429-8436`).
- **Moves are undoable.** Zed records `Change::Renamed` for each move (`:5431-5440`).
- **Removal lists names and unsaved counts.** `build_removal_prompt` lists up to ten names
  and says how many have unsaved changes (`:2836-2896`).
- **References are LSP's job.** Zed rewrites references through `willRenameFiles`
  (`crates/project/src/lsp_store.rs:11787-11838`, cited in the links spec at `:70-72`).
  Inkra has no language server.

I found no explicit check in `move_worktree_entry` against a drop of a folder into its own
descendant. Zed probably relies on the filesystem's refusal there; I did not confirm it.

### Obsidian

- The setting "Automatically update internal links" decides the rename behaviour. When it
  is on, Obsidian rewrites links on rename. When it is off, Obsidian prompts to update links
  after the rename ([Settings help](https://obsidian.md/help/settings)).
- The rewrite covers wikilinks and markdown links, and applies to moves as well as renames
  (same page). Which value is the default I could not confirm from the help page.
- The rewrite is asynchronous and touches open buffers. Its races are the reason the links
  spec deferred it (`2026-09-24-links-and-backlinks-design.md:106-108`).
- Obsidian has a "New link format" setting (shortest path, relative, absolute). I take its
  existence from memory, not from a source read for this spec.

## Decisions

### D1. Move, the gesture

**Both gestures, "Move to…" first, drag second.**

- `Move to…` is a new tree action, `sidebar::MoveTo`, beside `Rename`
  (`crates/app/src/sidebar/mod.rs:33`). It opens a picker of folders on the
  `PickerDelegate` contract (`crates/app/src/picker/mod.rs:69`). The vault root is the
  first row. The picker lists every folder except the moved entry's own folder, the entry
  itself and its descendants.
- Drag and drop follows the house pattern of tab and dock drags
  (`crates/app/src/shell/tab_bar.rs:222-241`, `crates/app/src/shell/dock_drag.rs:90`). A
  tree row gets `.on_drag`; a folder row and the tree's empty area get `.on_drop`. A drop on
  a note targets that note's folder, as in Zed (`project_panel.rs:4320-4324`). A drag held
  over a closed folder expands it after 500 ms, as in Zed (`:6425-6429`).
- Rejected: drag first. A drag needs the hover target, the expand timer and the drop
  highlight before it moves anything. The picker reuses a contract that exists, is
  keyboard-only, and gets a `simulate_keystrokes` test. The accessibility gap
  (`docs/gpui-accessibility.md`) also argues for a keyboard path first.
- Rejected: only one gesture. A note app's tree is where users reorganize by hand, and
  every tool below offers drag. A keyboard-only user needs the picker.

**Open question:** whether drag must ship before the feature counts as done.
Recommendation: no. Ship `Move to…` in the first slice, drag in the last one.

A folder moves with the same operation as a note (D2). Every note inside it moves with it.

The edge cases, decided before anything is queued:

| Drop or pick | Result | Why |
|---|---|---|
| The entry's own folder | No-op, no notice | Zed does the same (`:4337`); nothing changes on disk. |
| The entry itself, or a descendant of a moved folder | Refused; the picker never lists it, a drop shows no highlight | `rename(2)` refuses it with `EINVAL`. `MemFs::rename` does not check it (`mem_fs.rs:403-437`) and would scramble the subtree. |
| A folder that holds a file of the same name | Refused before queueing, with "That name is already taken." | `Vault::name_taken` already answers it for rename (`edit.rs:232-238`, `crates/vault/src/vault/names.rs:11`). `Fs::rename` refuses `AlreadyExists` as a backstop (`real_fs.rs:250-258`, `mem_fs.rs:393-395`), which emits `RenameFailed`. |

Rejected: Zed's copy on a modifier (`:5277`). `Fs` has no copy primitive (`fs.rs:162-212`),
and a copy of a note is a second identity with its own links. That is a separate feature.

Rejected: moving a multi-selection. The tree has one selection (`Sidebar::selected`); Zed's
`disjoint_entries` (`:4365-4413`) solves a problem the tree does not have yet.

`MemFs::rename` gains the descendant check, so the fake refuses what `rename(2)` refuses
(vault rule: a second `Fs` backend owes the same behaviour).

### D2. Move, the operation

**One operation: `NoteStore::rename` grows a destination folder.** A rename is a move with
the same parent. The proposed shape is `relocate(id, parent: Option<EntryId>, name)`, with
`rename(id, name)` calling it with the current parent.

The only change in the operation body is where `to` comes from. Today it is
`from.parent()?.join(file_name)` (`tree_ops.rs:159`). With a destination it is the
destination folder's path, resolved at run time with `path_of`, joined with the file name.
Run time, not issue time: an operation queued behind a rename must act on the moved name
(`tree_ops.rs:153-154`). The same holds for the destination folder, which may itself be
moving.

Rejected: a second operation `move_to` beside `rename`. It would duplicate the `moves`
protocol (`tree_ops.rs:160-206`) and the failure path. Two copies of a protocol drift, and
this one is where Q7's fixes live (`2026-11-02-quit-and-failures-design.md:144-178`).

What survives a move of a note, and why:

- **Identity.** `Fs::rename` never changes the inode (`fs.rs:199-201`). `RealFs` moves by
  `hard_link` then `remove_file` (`real_fs.rs:245-248`). It falls back to a guarded
  `rename` where hard links fail, a directory included (`real_fs.rs:260-263`). `MemFs`
  moves the stored entry (`mem_fs.rs:379-402`).
- **The open buffer.** A bound note's `Location::Vault` stores no path
  (`crates/app/src/vault/session/lifecycle.rs:12-19`). `write_path` resolves it with
  `path_of` at write time (`save.rs:155-160`), and `path_of` follows `moves`.
- **The tab.** A tab holds the `EntryId`; its title reads the vault entry
  (`crates/app/src/vault/store.rs:137-152`), and the session file follows `moves`
  (`tree_ops.rs:68-75`).
- **Unsaved edits.** `PendingSave::Write` writes the pending save before the move, on the
  same chain (`queue.rs:97-100`). An edit typed after the move saves through `path_of`, so
  it lands under the new path.

What does not survive cleanly: **the notes inside a moved folder.** `moves` records the
folder's id only. An open note inside the folder still resolves its old path until the
rescan swaps. Q7 states this edge case for a folder rename already
(`2026-11-02-quit-and-failures-design.md:144-178`). A save in that window calls `Fs::save`
on a missing folder. It fails with `NotFound` (`fs.rs:180-183`, `mem_fs.rs:270-276`),
because a present note never recreates its folder (`crates/app/src/vault/session/writer.rs:44-48`,
`:64-68`). `NotFound` is not transient (`save.rs:28-38`), so no retry follows. The text
stays in the buffer, and the next edit saves under the new path.

**Decision:** a folder move closes that window for open notes. Before `Fs::rename`, the
operation sets `moves[d] = None` for each open note `d` under the folder. After success it
sets `moves[d] = Some(new path)`, and the rescan's landing removes each one, as for the
folder itself. A save in the `None` window reports `SaveFailed` with no error and keeps the
text (`save.rs:168-171`). This is the existing rename's behaviour, extended to the subtree.
Rejected: leaving Q7's edge case as is. A move of a folder is a common gesture, unlike a
folder rename, and it would show a save failure notice on every open note inside it.
A residual window remains; see R2.

### D3. Which links break

Resolution, as the index does it today:

- `Name` is a wikilink with no `/`; `Suffix` is a wikilink with a `/`; `Relative` is an
  inline link, or a wikilink that starts with `./`, `../` or `/`
  (`crates/index/src/target.rs:31-40`, `:44-78`). An inline link is always `Relative`
  (`target.rs:67`).
- `Name` and `Suffix` find candidates by stem; a `Suffix` candidate must end with the
  written path on a component boundary (`crates/index/src/notes.rs:117-138`).
- `Relative` joins the source's folder, or the root after a leading `/`
  (`notes.rs:103-115`, `:142-158`).
- Ties go to an exact-case tail, then the fewest components, then vault order
  (`notes.rs:130-136`).

"Breaks" below means "no longer resolves to the same file". It covers two outcomes:
unresolved, or **retargeted** to another file of that name.

| Link style | Rename of the target | Move of the target | Move of the source |
|---|---|---|---|
| `Name` `[[b]]` | Breaks; retargets if another `b` exists | Holds, unless another `b` now ties better on components (retarget) | Holds: no source dependence |
| `Suffix` `[[x/b]]` | Breaks; the stem is the last component | Holds only if the new path still ends with `x/b` | Holds: no source dependence |
| `Relative` wikilink `[[../x/b]]` | Breaks | Breaks, unless the source moves with it (same folder move) | Breaks, unless the target moves with it; a root-relative `[[/x/b]]` holds |
| Inline `[t](../x/b.md)` | Breaks | As `Relative` | As `Relative`; a root-relative `/x/b.md` holds |

A folder move is a move of every file inside it. Links between two files inside the folder
keep holding when they are `Relative`, and a `Suffix` that names the folder breaks.
A folder rename is a folder move with the same parent.

Assets are files too: a renamed or moved image breaks its embeds by the same table (K6,
`crates/index/src/index.rs:251-257`).

**Decision:** the `index` crate answers "which links break" exactly, not with this table.
A proposed `NoteIndex::breakage(moved: &[(EntryId, new path)])` resolves each affected link
twice, against the current names and against the names with the moves applied. It returns
every link whose two answers differ, with its source, its index in the source's links and
its old target. The affected links are:

- every backlink of each moved file (`NoteIndex::backlinks`, `index.rs:264-313`), which
  finds all styles because the stem map keys every link by its target's stem
  (`crates/index/src/source.rs:75-80`);
- every `Relative` link whose source is a moved note.

Rejected: shipping the table as a rule in `app`. The table is a model of `notes.rs`, and
its retarget cases depend on tie-breaks only the resolver knows. One resolution function
for every surface is the links spec's first rule (`2026-09-24-links-and-backlinks-design.md:100-102`).

`breakage` reads only indexed sources: `backlinks` has rows only for notes in `sources`
(`index.rs:222-225`). While `Status` is not `Ready` (`crates/index/src/live.rs:28-37`), the
answer is incomplete. See D4 for what the app does then.

### D4. Rewriting, or not

**Rename: rewrite every broken link at once, with no prompt. Move: the same.**

The tree reports the result on the notice line after the fact: "Updated 12 links in 5
notes." Failures follow D5.

- Rejected: rewrite nothing and show broken links. A rename breaks every link to the note
  (D3), and the `Name` row can silently retarget to another note of the old name. The user
  would not see the second outcome at all.
- Rejected: a confirmation with a count before each rename or move ("Update 12 links in 5
  notes?"). It interrupts the most common gesture with a modal. A dismissed prompt leaves
  the vault with broken links, and the user does not remember which ones. Obsidian offers
  this prompt only behind a setting ([Settings help](https://obsidian.md/help/settings)).

**Open question:** whether a large rewrite, for example a folder move that touches hundreds
of notes, needs a confirmation. Recommendation: no prompt and no threshold. A threshold is
a special case, and the notice reports the count either way. Revisit if a user report
shows a surprising rewrite.

**When the index is not `Ready`:** the move or rename still runs, and no link is rewritten.
The notice says "Links to “b” were not updated: the index is still loading." Rejected:
waiting for `Ready` before computing `breakage`. After the rescan swaps, the index holds the
new names, and the old written forms no longer resolve to the moved file, so a late
`breakage` cannot find them. Rejected: blocking the move until `Ready`. A move is a
filesystem operation; it must not depend on the index.

**Open question:** the index-loading case. Recommendation: as decided above, because the
index is `Ready` within seconds of launch with the persisted cache
(`2026-11-06-persisted-index-design.md`).

### D5. Rewriting safely

The rewrite is a plan, computed once, then applied per source note.

**The plan.** At issue time, in the same foreground update that queues the move, the app
calls `breakage` and computes the new written form of each broken link (D6). The plan maps
each source `EntryId` to a set of `(old NoteRef, new written text)` pairs. A `NoteRef` is
the written path, the `.md` flag and the style (`target.rs:23-29`). The plan holds no byte
range: the user may type in a source before the plan applies.

**The order.** The plan applies only after the move's `Task<bool>` resolves `true`, which
happens after its rescan landed (`tree_ops.rs:190-206`). If it resolves `false`, the plan
is dropped. So the rename never depends on the rewrite, and a rewrite failure never undoes
the rename. Before applying, the app checks that the moved entry's `vault_path` equals the
destination the plan assumed. If another operation moved it first, the plan is dropped and
the notice says links were not updated.

**Applying to one source.** Every application re-parses the source's current text with
`inkra_markdown::links` (`crates/markdown/src/links.rs:27-62`) and parses each link with
`Target::parse`. It rewrites each link whose `NoteRef` is in the plan's set for that source.
A link typed after the plan with the same text resolved to the same file, so rewriting it
too is correct. Each new link text is parsed again; if its `Target` does not equal the
planned target, that link is skipped and counted as not updated.

The four source-note cases:

1. **Open, with unsaved edits.** The source has a buffer (`NoteStore::buffer_for`,
   `store.rs:123-126`). The app edits it in one `Buffer::transact`
   (`crates/text/src/buffer/history.rs:101-103`). That is one undo step, which merges with
   neither neighbour (CLAUDE.md, "Code"). The buffer observer schedules an autosave
   (`store.rs:249-254`), on the note's chain, with the expected stamp
   (`writer.rs:52-82`). The unsaved edits and the rewrite save together. If the file
   changed on disk in the meantime, `Fs::save` preserves the disk's bytes as a conflict
   copy before it writes (`fs.rs:180-183`, `mem_fs.rs:280-301`), and `ConflictPreserved`
   reports it (`save.rs:215-220`). Nothing is lost.
2. **Open and clean.** The same path as case 1. The edit makes the buffer dirty, and the
   autosave writes it. Rejected: writing the file directly for a clean open note. The
   buffer would then trail its file, and only the watcher's reload
   (`crates/app/src/vault/reload.rs:49-108`) would catch it up. Two writers for one note
   is what the chain exists to prevent (`queue.rs:12-14`).
3. **Not loaded.** A new store operation, queued on that note's chain with
   `PendingSave::Keep`, since no save is pending on a closed note. On the background
   executor it calls `Fs::load` at `path_of`, applies the plan to the text, and calls
   `Fs::save` with the loaded stamp. On the foreground it re-keys the identity with
   `reidentify` (`scans.rs:36-52`), because the atomic save moves the inode
   (`save.rs:206-207`). Then it emits `NoteSaved`, the index's only signal for an
   ordinary write (`save.rs:208-214`, `index.rs:269-286`). It lives in
   `crates/app/src/vault/`, so the app still reads no file outside `NoteStore`.
   Before its background step, the operation checks whether the note became open
   (`Sessions::note`). If it did, it applies case 1 instead.
4. **Changed on disk meanwhile.** For a loaded note, this is case 1's conflict copy. For an
   unloaded note, a change between `Fs::load` and `Fs::save` takes the same path:
   `Fs::save` sees a stamp mismatch, preserves the disk's bytes as a conflict copy, and
   writes the rewritten old text over them. The external edit survives only in the copy,
   and `ConflictPreserved` names it. That is the save path's existing contract, so no new
   failure mode appears. Rejected: a compare-and-fail save that writes nothing on a
   mismatch. It is a second write primitive on `Fs`, for a window of milliseconds. Risk:
   R3.

**What serializes the writes.** Each source's write runs on that source's own chain
(`queue.rs:84-113`). No two writes to one note overlap, and a source that is also being
moved or trashed sees its operations in issue order. Writes to different notes run
concurrently, with no cap until a profile shows a need.

**What one undo step covers.** In an open note, one undo step reverts that note's rewritten
links and nothing else. A rewritten unloaded note has no undo: its file was written, and it
had no buffer. To undo a move, the user moves the file back, which rewrites the links
again. Rejected: a vault-wide undo of the move and its rewrites, as Zed's
`Change::Renamed` record (`project_panel.rs:5431-5440`). The create/rename/delete spec
already rejected an undo manager over filesystem operations, because the Trash is the
recovery story (`2026-09-22-create-rename-delete-design.md:40-43`).

**When one write of N fails.** An open note's failed save is the save path's failure: it
emits `SaveFailed`, retries when transient (`save.rs:227-255`), and keeps the text in the
buffer. An unloaded note's failed write retries with the same policy
(`RETRY_LIMIT`, `retry_delay`, `is_transient`, `save.rs:18-38`). After the last retry, its
link stays broken on disk. The notice reports the whole rewrite once, as one line, because
the notice shows one line and the latest event wins (`crates/app/src/shell/notice.rs:80-92`).
A proposed `LinksRewritten { updated, notes, skipped, failed }` event carries the counts,
emitted when the last source settles. The wording:

- all written: "Updated 12 links in 5 notes."
- some failed: "Updated 10 links in 4 notes. Couldn't update “a”: permission denied."
- several failed: "Updated 8 links in 3 notes. Couldn't update “a” and 1 other note."
- some skipped (D6): "Updated 10 links in 4 notes. 2 links need a manual update."

Rejected: one notice per failed note. With a one-line notice, each would hide the one
before it, and the user would see only the last.

### D6. The written form after a rewrite

**Keep the link's style, alias, fragment, `.md` flag and embed mark. Write the shortest
form, in that style, that resolves to the file after the move.**

- `Name`: the new stem, if it wins resolution against the post-move names. Otherwise the
  shortest `Suffix` that wins. A `Name` that would retarget becomes a `Suffix`; this is the
  only style change.
- `Suffix`: the fewest trailing components that win, at least as many as written, so that
  `[[x/b]]` does not shrink to `[[b]]`.
- `Relative`: the relative path from the source's post-move folder to the file. A
  root-relative link stays root-relative.
- Inline: as `Relative`. Percent-encoding follows the written link: a destination written
  with `%20` gets `%20`, and one written in `<…>` keeps its brackets.
- A wikilink's alias `[[b|text]]` and an inline link's text `[text](b.md)` are not touched.
  A wikilink without an alias shows the new name, as it does in Obsidian.

Rejected: the form as written with the name replaced. It is correct for a rename, and wrong
for a move: `[[x/b]]` moved out of `x` still names `x`. Rejected: one canonical style
(all `Relative`, or all `Name`). It rewrites the user's choice of form, and a `Name` link
is the one that survives later moves.

**Where the destination is.** `Link.range` covers the whole link (`links.rs:9-16`). No
sub-range for the destination exists, and the wikilink target comes back trimmed and
unescaped (`links.rs:74-90`). **Decision:** `inkra-markdown` gains the destination's byte
range on `Link`, found by a scan bounded by the link's range. It covers a wikilink's text
between `[[` and the first unescaped `|` or `]]`, and an inline link's destination after
`](`. Rejected: a scan in `app`. Finding a span in markdown is parsing, which belongs to
`markdown` (CLAUDE.md, "Where code goes").

**Reference links and autolinks are not rewritten.** `LinkKind::Inline` covers both
(`links.rs:18-24`). A reference link's destination sits in a definition elsewhere in the
note (links spec D1, `2026-09-24-links-and-backlinks-design.md:116-184`). pulldown-cmark
0.13.6 exposes the definitions through `Parser::reference_definitions` (its
`src/parse.rs:308`); I read that in the registry copy, not in `vendor/`. Such a link has no
destination range, so the rewrite counts it as skipped. Rejected: rewriting definitions now.
It is a second parse path for a rare form, and the skipped count tells the user.

### D7. Trashing a folder

**Still deferred.** The trigger in the create/rename/delete spec stands
(`2026-09-22-create-rename-delete-design.md:16-25`).

- `Fs::trash` is specified for notes only (`fs.rs:203-204`). `MemFs::trash` refuses a
  directory with `NotFound` (`mem_fs.rs:453-464`). `RealFs::trash` hands any path to the
  `trash` crate (`real_fs.rs:267-277`); that it trashes a directory on macOS with
  `NsFileManager` (`real_fs.rs:285-296`) is likely but unverified.
- A folder trash is N notes entering `Lifecycle::Trashing` at once (`lifecycle.rs:150-161`),
  each on its own chain, with one `Fs::trash` call for all of them. That is a new lifecycle
  shape, not a move.

Rejected: allowing it in this feature. Moving files needs none of it, and it doubles the
feature's risk surface. If it lands later, the prompt follows the note prompt
(`crates/app/src/sidebar/edit.rs:74-86`) and Zed's counts (`project_panel.rs:2836-2896`):
"Move “Projects” and its 14 notes to the Trash?", with "3 of them have unsaved changes,
which will be lost." when any are dirty.

**Open question:** whether folder trash joins this feature after all. Recommendation:
keep it deferred; a folder can be emptied by moving its notes, then trashed in Finder.

### D8. Deviations

From Zed:

- No copy on a modifier (`project_panel.rs:5277`): `Fs` has no copy (D1).
- One entry per move, not a disjoint selection (`:4365-4413`): the tree has one selection.
- `Move to…` picker as the first gesture: Zed offers drag only in the panel; Inkra needs a
  keyboard path first (D1).
- Links are rewritten by the app, not by a language server
  (`crates/project/src/lsp_store.rs:11787-11838`): there is no server (D4).
- No undo record for a move (`:5431-5440`): the Trash is the recovery story (D5).
- A drop of a folder into its own descendant is refused in the app before any filesystem
  call (D1).

From Obsidian:

- No setting for link updates: always on, reported after the fact (D4).
- A link Inkra cannot rewrite safely (reference links, a re-parse mismatch) is skipped and
  counted, never guessed (D6).
- A `Name` link that a move would retarget becomes a `Suffix` link (D6).
- No rewrite while the index loads (D4).

## Tests

Point 9 of the decisions: tests per slice; see the Plan for the slices.

`index` (S1, S2): `#[test]` plus `proptest`.

- One test per cell of the D3 table: rename of the target, move of the target, move of the
  source, for `Name`, `Suffix`, `Relative` and inline.
- Retarget: two notes `b`, moving one changes which wins; `breakage` reports it.
- Folder move: links between two notes inside hold; a `Suffix` naming the folder breaks.
- An unindexed source is absent from `breakage`.
- D6: each style's shortest form; a `Name` that would retarget becomes a `Suffix`; alias,
  fragment, `.md` and `!` kept.
- Property: for random vaults and moves, every rewritten link resolves after the moves to
  the file it resolved to before them.

`markdown` (S2): the destination range of wikilinks with and without alias, escaped pipes,
embeds, inline links with `<…>` and `%20`; `None` for reference links and autolinks.

`vault` (S1): `MemFs::rename` refuses a folder into its own descendant; a moved subtree
keeps every identity; a real-disk case in `vault/tests/real_fs.rs` for the descendant
refusal.

`app` (S1 to S4): headless `#[gpui::test]` on `MemFs`.

- S1: `Move to…` moves a note; its buffer, tab and unsaved text survive; the edit lands
  under the new path. A move of a folder with an open dirty note inside saves under the new
  path, with no `SaveFailed`. A refused destination leaves the field and the disk unchanged.
  The notice reports broken links. `simulate_keystrokes` for the action and the picker.
- S2: each of D5's four cases, with `fail_next_saves`, `block_after_rename` and the
  load and save gates of `MemFs` (`mem_fs.rs:39-62`). One of three writes fails: two land,
  the notice names the third. A user edit typed between plan and apply survives, and its
  link is rewritten too. A rename that fails leaves every source untouched. A source
  opened while its unloaded rewrite runs keeps one copy of the text.
- S3: a rename rewrites `Name` links; while `Status` is not `Ready`, it rewrites none and
  says so.
- S4: a drop onto a folder moves; onto a note targets its folder; onto its own folder is a
  no-op; a drag over a closed folder expands it. Render the scene and look at the drop
  highlight.

After S1 and S2, run `cargo mutants --in-diff` on `index` and `markdown` (CLAUDE.md,
"Commands").

## Risks

- **R1. Index not ready.** A move during indexing rewrites nothing (D4). The user sees the
  notice, but must fix the links by hand. Likely rare after the persisted cache.
- **R2. Folder move, residual window.** D2 covers open notes. A save already past
  `write_path` when the folder's `Fs::rename` starts writes to the old folder. `RealFs::save`
  probably fails at its temporary file or its final rename, which keeps the text in the
  buffer and reports `SaveFailed`. I have not shown that no interleaving leaves a stray
  file in a recreated folder; `Folder::MustExist` (`writer.rs:44-48`) suggests it cannot,
  but this is unproven.
- **R3. Unloaded note changed on disk.** D5 case 4 writes the rewritten old text and keeps
  the external edit only in a conflict copy. Nothing is lost, but the user must merge.
- **R4. Quit mid-rewrite.** The quit flush covers open notes only and is unchained
  (`save.rs:102-153`). An unloaded rewrite in flight either lands or does not, since each
  save is atomic. The vault can end with some links rewritten and others not, and nothing
  reports it at the next launch.
- **R5. A source opened during its unloaded rewrite.** D5 case 3 checks for it before the
  background step. If the open lands after that check, the buffer holds the old text with
  the old stamp. The rewrite's save then makes the watcher reload the clean buffer
  (`reload.rs:54`), or, if the user typed, the next save preserves a conflict copy. No text
  is lost, but a conflict copy can appear.
- **R6. Large rewrites.** A folder move can touch hundreds of notes, each a load and a save.
  No cap is set; profile before adding one (CLAUDE.md, "Performance").
- **R7. Retarget false negatives.** `breakage` sees only indexed sources and only links the
  index parsed. A link inside code or frontmatter is not a link (`links.rs:27-28`), so it is
  correctly left alone.

## Plan (slices)

Each slice touches data, behaviour and tests, and ships alone.

1. **S1. Move without rewriting, honestly.** `NoteIndex::breakage` (D3), `relocate` (D2)
   with the subtree's `moves`, the `MemFs` descendant check, `sidebar::MoveTo` and its folder
   picker (D1). The notice after a move says "Moved “b” to “x”. 3 links in 2 notes no longer
   resolve." The UI therefore names what the move breaks, as the plan requires before a move
   ships without rewriting.
2. **S2. The rewrite, on move.** The destination range in `inkra-markdown`, the written form
   in `inkra-index` (D6), the plan and D5's four cases in `app`, the `LinksRewritten` notice.
   S1's "no longer resolve" line becomes "Updated N links in M notes."
3. **S3. The rewrite, on rename.** `rename` goes through the same plan. The not-`Ready` notice.
   This slice ends the links spec's deferral (`2026-09-24-links-and-backlinks-design.md:25`).
4. **S4. Drag and drop in the tree.** `.on_drag` on rows, `.on_drop` on folders and the tree's
   empty area, the 500 ms expand, the drop highlight. It calls S1's `relocate`; no new
   filesystem path.

Folder trash is not a slice (D7).
