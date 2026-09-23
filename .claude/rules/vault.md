---
paths:
  - "crates/vault/**"
  - "crates/app/src/vault/**"
---

# Vault and the save path

- An entry is identified by `(device, inode)` (`EntryId`), never by path: a
  rename keeps it, a file recreated at the same path gets a new one.
- `NoteStore::current` is the note `⌘S` writes and must be the note on screen.
  Its only writers are `set_current` (right after `EditorView::open_buffer`, no
  `await` between) and the synchronous `adopt` at launch.
- Every filesystem operation on one note is ordered on its `Ops` chain. Do not
  order moves by `Writer`'s sequence: a write pending when a delete is issued
  carries the higher sequence.
- A note being trashed is preempted (`schedule_save(id, Duration::ZERO)`), never
  cancelled through `Ops::cancel`, which would kill its autosave and trash the
  previous save's text.
- The quit flush cannot chain; `Ops::trashing` is what it skips.
- Compare `Stamp` with `!=`, never an ordering; it never includes the identity,
  which our own atomic save changes.
- No write destroys bytes it did not put there: `Fs::save` preserves the current
  bytes beside the note before replacing them. A second `Fs` backend owes the
  same ordering.
- `vault` takes every `Mtime` from the filesystem, never `SystemTime::now`.
- `RealFs` filters `.md`, dotfiles and symlinks; `vault/tests/real_fs.rs` is the
  one test on a real disk.
