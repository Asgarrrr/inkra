---
paths:
  - "crates/vault/**"
  - "crates/app/src/vault/**"
---

# Vault and the save path

- An entry is identified by its `FileIdentity` (device, inode), never by path
  alone; `EntryId` is the minted id that follows it, so a rename keeps it. A
  new inode at a path keeps the path's id when the walk finds the old inode
  nowhere (an external atomic save), and gets a new one when it finds it
  elsewhere (watcher spec, F5). An open note gone from disk reclaims
  the note file at its last path, unless that id is open (F9). The swap's
  order (reclaim, replay, replace) is `inkra_vault::Scans`'s: the app calls
  `Scans`, never the `Vault` primitives it assembles.
- `NoteStore::current` is the note `⌘S` writes and must be the note on screen.
  Its only writers are `set_current` (right after `EditorView::open_buffer`, no
  `await` between) and the synchronous `adopt` at launch.
- Every filesystem operation on one note is ordered on its `Ops` chain. Do not
  order moves by `Writer`'s sequence: a write pending when a delete is issued
  carries the higher sequence.
- A note being trashed is preempted (`schedule_save(id, Duration::ZERO)`), never
  cancelled through `Ops::cancel_pending`, which would kill its autosave and
  trash the previous save's text. A gone note is the exception: it has no file,
  and its preempted write would recreate one, so its autosave is cancelled.
- The quit flush cannot chain; `Ops::trashing` is what it skips.
- Compare `Stamp` with `!=`, never an ordering; it never includes the identity,
  which our own atomic save changes.
- No write destroys bytes it did not put there: `Fs::save` preserves the current
  bytes beside the note before replacing them. A second `Fs` backend owes the
  same ordering.
- `Fs::save` fails on a missing folder, on every backend. Only a gone note's
  write calls `Fs::create_parents` first (F9): a present note whose folder left
  must fail, not recreate the folder beside a moved copy.
- `vault` takes every `Mtime` from the filesystem, never `SystemTime::now`.
- `RealFs` filters `.md`, dotfiles and symlinks; `vault/tests/real_fs.rs` is the
  one test on a real disk.
