# One module for "a save landed": dropped

Status: **dropped 2026-09-30.** The candidate fails the deletion test. No code
changes. This file records why, so the idea is not proposed again unchanged.

## The candidate

The candidate put one app-side Module in charge of "a save landed". It would
re-key the vault, record the save for the index, and answer "is this mtime
ours?" for the reload. Rename and create would reuse it. The claim was that
four places decide "this disk change is our own write":

1. The reload's shortcut, `mtime == expected.mtime`
   (`crates/app/src/vault/reload.rs:62-65`).
2. The swap's reload flag, `before != Some(now)`
   (`crates/app/src/vault/session/lifecycle.rs:124-148`, line 142).
3. The re-key log in `Scans::reidentify` (`crates/vault/src/scans.rs:103-120`)
   and `Swapped.notify`.
4. `LiveIndex.indexed`, set by `saved` on `NoteSaved`
   (`crates/index/src/live/loads.rs:37-41`, `crates/app/src/index.rs:189-201`).

## What the code shows

The four places answer four different questions, each at its own Seam.

- **The swap's flag is a prefilter, not an echo rule.** It asks "did this
  entry's mtime move between two vault states?". It is true for an external
  edit as well. It only decides whether to queue a check. It holds no
  knowledge of our writes.
- **The `Writer` is already the one Module that knows our last write.**
  `WriterState.stamp` (`crates/app/src/vault/session/writer.rs:39-42`) is the
  stamp of the file the buffer last matched. Three readers use it: the reload and the
  trash of a gone note through `Writer::expected` (`reload.rs:62`,
  `tree_ops.rs:181-184`), and `Fs::save`'s conflict check through
  `write_through` (`writer.rs:69`). The reload's two comparisons (`reload.rs:64` and `:72`) are
  one rule: the first skips a load when the mtime alone proves the file ours.
- **The re-key log is about identity, not echo.** An atomic save moves the
  inode. Without the re-key, the next scan orphans the id
  (`save.rs:141-142`). A scan that runs during the save must replay the
  re-key onto its result (open-first spec, O2). This holds with no watcher and
  no reload. `Swapped.notify` is false for an echo only as a consequence.
- **`LiveIndex.indexed` is the index's cache key.** It records the mtime the
  index holds or has queued, for loads and saves alike (index-scheduler spec,
  L2). It lives in `inkra-index`, which has no gpui and no `Writer`. It hears
  of a save through the `NoteSaved` event, which is the house rule for crossing
  from the store to the index.

A single "is this mtime ours?" Module would have to serve the index and the
reload. The index cannot read a `Writer`: the `Writer` exists only for bound
notes and lives in `app`. The reload cannot read `indexed`: it lives in another
entity, reached only through events. Merging them couples `IndexSync` to
`NoteStore`'s sessions, and breaks "views talk through events".

## Deletion test

What is left for the new Module is the block in `NoteStore::write`
(`save.rs:138-150`): `note.landed(version)`, `reidentify`, then emit
`NoteSaved`. Delete the Module and these three calls return to their one
caller. Nothing else calls the sequence:

- `flush_all` (`save.rs:48-88`) writes at quit and needs neither a re-key nor
  an index update.
- Create, rename and trash call `rescan()` and do not write a note's text, so
  they have no save to record.

One caller, three lines, no Interface to hide: the Module is shallow. Its
Leverage is zero and it moves no Locality.

## The other claims, checked

- **"`reidentify` must precede `NoteSaved`; only a comment states it."** No
  such ordering exists. The comment at `save.rs:141` explains the re-key
  itself. Either order is safe: a build that reads the entries before `saved`
  runs has a seq below the save's, so `latest > seq` skips the note
  (`crates/index/src/live/builds.rs:182`). A build that reads after finds
  `indexed == mtime`.
- **"Overlapping checks" at `reload.rs:63`.** The comment names the case where
  a scan held the re-key, so the live entry kept the old mtime and the swap
  queued a check. The `Writer` shortcut ends that check without a load. This
  is one rule applied at the one point that knows our stamp (file-watcher
  spec, F7).
- **"Nothing tests the whole concept."** False. These tests drive a save, its
  echo and the rescan end to end, and assert no load of either kind:
  - `an_autosaves_echo_lists_only_the_notes_folder_and_loads_nothing`
    (`crates/app/src/vault/tests/watch.rs:109`): watcher, scoped scan, index
    and reload together. `MemFs::loads` counts the index's loads and the
    reload's.
  - `an_echo_read_after_the_saves_re_key_does_not_notify_the_vault`
    (`watch.rs:137`).
  - `our_own_saves_echo_loads_nothing` and
    `our_own_saves_echo_during_a_scan_loads_nothing`
    (`crates/app/src/vault/tests/reload.rs:161,176`).
  - `a_check_queued_before_our_save_loads_once_while_a_scan_holds_the_rekey`
    (`reload.rs:196`).
  - `two_saves_during_a_held_scan_reload_nothing_after_its_swap` and
    `a_save_during_a_build_is_not_reloaded_when_the_build_applies`
    (`crates/app/src/index/tests.rs:271,293`).

## What Zed does

Zed splits the rule the same way. The worktree builds an identical `File` for
an echo, so nothing downstream changes (`buffer_store.rs:564-592`). Each
`Buffer` keeps `saved_mtime` from `did_save` (`language/src/buffer.rs:1636-1649`)
and emits `ReloadNeeded` only when the disk state moved
(`buffer.rs:1778-1793`). Zed has no Module that owns "a save landed" across
the worktree and its buffers. We match it: `Scans` is the worktree side,
`Writer` the buffer side.

## Vault files touched

None. This spec changes no file under `crates/vault/`, so it does not conflict
with the `Fs::replace` work in `crates/vault/src/{fs,mem_fs,real_fs}.rs`.

## What would reopen it

- A second caller of the `landed` / `reidentify` / `NoteSaved` sequence, for
  example a "save as" or a template write that must not reload. Two callers
  make the sequence a Module with Leverage.
- An echo that loads or reloads in a test or a trace. Fix it by invariant in
  the Module that owns the question it got wrong (`Writer`, `Scans` or
  `LiveIndex`), not with a new layer above them.
