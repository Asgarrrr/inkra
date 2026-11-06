# Persisted index, text read on demand

Status: **approved 2026-11-06** after three red/blue rounds on each section.
Feature: full-text search is ready when the scan lands, and the index holds no
note text.

## Problem

At 50,000 notes, `indexed` lands at about 1.0 s and the corpus holds about
158 MB of text. The loads are bound by macOS `open()`: about 0.75 s for 50k
files, with a peak near 4 threads. More slots, a higher priority and separate
readers do not help (`docs/roadmap.md`, "Loads measured, 2026-11-06").
The only lever left is to read fewer files.

Two changes do that:

- **Persist the derived index.** A launch reads one file and loads only the
  notes whose stamp changed.
- **Drop the resident text.** ⌘⇧F and ⌘P excerpts read the text of their
  candidates on demand. Zed keeps no corpus either; it reads candidate files
  only.

## Goals and gates

- `indexed` ≤ 0.35 s at 50k notes, warm cache (`launch-figure.sh`).
- RSS and `phys_footprint` drop by about 150 MB at 50k notes.
- ⌘⇧F on a mid-frequency term (about 1,000 one-hit candidates) does not
  regress past 50 ms, page cache warm, against the slice-0 baseline.
- The longest foreground turn stays ≤ 8 ms, the seed's turn included.
- A full decode of 50k notes takes ≤ 60 ms and lands before the scan.

## Decisions

### P1. The cache is applied when the scan lands, not before

`EntryId`s are minted fresh each launch (`vault/src/identity.rs:31`). Before
the scan, no id, no `NoteTable` and no openable path exists (open-first O5).
The first frame lands at about 180 ms and the scan at about 230 ms, so serving
before the scan would gain about 50 ms. The cache is decoded in parallel with
the scan and applied at the first build.

### P2. What the file holds

- The words segment verbatim, the alive bits, the BM25 lengths, `total`,
  `next`.
- An ordinal → path table.
- The `Source`s.
- A per-note table keyed by vault-relative path: the `Stamp` (mtime, size)
  and the `(device, inode)` of the `Loaded` that produced the words. A note
  with no words has no ordinal, so the key is the path.
- A header: format version, fold fingerprint, derivation fingerprint, vault
  root, crc32 over the header and the body.

The stamp is never taken from `indexed` or from the scan. `saved` moves
`indexed` before the parse lands, so a write in that gap would pair a new
stamp with old words, and the old words would survive every later launch.

### P3. Invalidation

- **Format version**: bumped by hand on any layout change.
- **Fold fingerprint**: the Unicode versions of the normalization and
  segmentation crates and `char::UNICODE_VERSION` (the fold calls
  `char::to_lowercase`), each widened to `u64`, plus a crc32 of folded probe
  strings.
- **Derivation fingerprint**: a crc32 of `NoteContent::parse` output on a
  fixed probe corpus. It changes when the parse changes, not on every build;
  a build id would invalidate the cache at each compile.
- Fingerprints use crc32. `foldhash::RandomState` is seeded per process and
  its docs exclude persisted formats.
- A golden-constant test pins the header and both fingerprints, so a drift
  fails a test instead of silently rejecting every cache.

### P4. Where the file lives

In `~/Library/Caches/Inkra/index/`, named from the vault path by
`session_file`'s key function. Caches, not Application Support: the file is
re-creatable, and Time Machine skips Caches. An evicted file is "missing". It is never in the vault: sync clients would copy it, and the
watcher would see it. It is per machine. Two instances: the last rename wins,
and the stamps validate everything at the next launch. No `HOME`: outcome
"missing", and no writer.

### P5. Matching a cached note to a scanned entry

By path first. When the path vanished, by `(device, inode)`, one to one: an
inode never claims an entry already matched by path. A note is reused only
when `(mtime, size)` equal the scan's, compared with `!=`, never ordered
(`vault/src/fs.rs:25`). The inode alone never validates: an atomic save moves
it.

### P6. The seed

Built off the foreground, inside the first `BuildJob`:

- the decoded segment becomes `Words::segment` verbatim;
- `Ordinals` are rebuilt from ordinal → path → new `EntryId`;
- `Alive` holds matched ordinals only; each unmatched ordinal's length is
  subtracted from `total`, so df, N and the mean length match a cold build;
- `next` is taken from the cache;
- `Sources` are rekeyed to `EntryId`; a note without links gets
  `Source::default()`, because `is_indexed` is `sources.contains_key`;
- `by_stem` is rebuilt from the links.

The seed asserts `alive + fresh` equals the ordinals with words and `total`
equals the sum of live lengths. The foreground only moves the built maps.

In `apply_notes`: `set_notes`, then `seed`, then `indexed` pre-filled with
`or_insert` (a save before the build wins), then `queue_changed`. A changed
note keeps its ordinal and reloads through `Words::set`.

### P7. The cache outcome is a turn input

`decoded | rejected | missing` enters `LiveIndex` as a turn input. Names
apply at the first build, so ⌘P by name works at once. `queue_changed` waits
for the outcome, with no deadline: the size cap bounds the decode, and a read
failure is "missing". `Status` counts the wait as pending, so `Startup` never
reaches `Done` on an empty index. With nothing changed, `Done` comes in the
first build's turn. The outcome is a `Timeline` step, so a reject loop is
visible.

### P8. Racy notes

A stamp whose mtime has no sub-second part is racy, and a racy stamp never
matches in P5: the note reloads at every launch. A filesystem with 1-2 s
ticks (HFS+, FAT, SMB) can take a same-size write in the tick after a load,
and no stamp tells it apart. On APFS a load outlasts a tick. No filesystem
mtime is ordered against a clock. An earlier rule (loaded less than 2 s
before the snapshot) checked the wrong interval: the hazard is a write
after the load, in the same tick.

### P9. No resident text

`Slot` keeps `modified` (`find/rank.rs:47`) and loses its text. `NoteContent`
holds text only while parsing.

### P10. ⌘⇧F reads its candidates

- The snapshot returns the lookup's candidates in vault order, from names and
  words, not from `corpus.notes`. A candidate carries the note's
  vault-relative path as scanned (extension kept, no NFC), held by
  `NamesSnapshot` beside the find name: the find name is not a file name.
  P2's per-note table uses the same path.
- An app job reads them with at most 4 readers, through the LRU, then
  `Fs::read`. Four matches the `open()` peak.
- A pure `locate` runs over each text. A note with no hit is skipped, so
  `NoteHits` is never empty.
- The job stops at the global 1,000-hit limit or on the `Ledger`'s cancel.
- The answer lands once. Two specs require it (picker-owns-landing,
  runs-testable), and the bound is about 1,000 reads.

Accepted limit: between the first build and the reload of a changed note, a
hit present only in its new text is missing.

### P11. Text reads never change the index

- `stale(id)` is sent only on `NotFound`, `NotUtf8` or a stamp mismatch,
  batched per query; it queues a load, and the load decides.
- After 8 I/O failures in one query, the query stops and sends nothing. An
  ejected volume must not empty the index.
- A blocking read (a dataless iCloud placeholder) can hold at most 4 threads.
  The `Ledger` drops a cancelled answer; the foreground never waits. Loads
  already read every file today, so the risk is not new.

### P12. The LRU

About 8 MB. An entry is valid only when its stamp equals the snapshot's.
Open notes give their saved `NoteSaved.text`, never the `Buffer`: a `Buffer`
is a foreground entity, and its dirty text differs from the indexed words.
Pins follow the `Sessions` live set, one per note across splits.

### P13. ⌘P

Ranking is unchanged. Each visible row's excerpt reads its text through the
same readers, then calls a pure `excerpt`. An empty excerpt shows the name
alone.

### P14. The writer

- One writer modelled on `SessionWriter`, with `replace_in_order`, but the
  encode runs in the background task.
- It takes `{names, words, sources, stamps}` at one `revision`. `sources`
  becomes `Shards<EntryId, Arc<Source>>` and `stamps` is new, so the snapshot
  costs Arc bumps.
- It encodes `merge(alive, fresh)`: the fresh tier is not in the segment
  until a merge, which is due only every 6,250 notes at 50k.
- First write at `Done` if at least one note reloaded or was pruned.
- Then after 60 s without change, or after 500 changes, in `Status::Ready`
  only.
- Never at quit: a stale cache heals through the stamp diff, and a 30 MB
  write plus `F_FULLFSYNC` would compete with the notes' 200 ms budget.
- Compaction of ordinals when `next > 2 × alive`.
- A failed write keeps the old file and backs off, doubling up to 10 min.

### P15. Rebuild index

A palette action with no binding, like `SelectTheme`. It removes the file
through the writer's seq; the mark moves even when the removal fails, so an
in-flight write cannot bring the old file back. Then `indexed` is cleared
and every live note is requeued.

### P16. The scrub

At the first `Ready`, 64 seeded notes are re-parsed in one batch in the
background. The sample is a pure `index` function seeded with the file's crc32,
so tests are deterministic and no clock is read. A mismatch at an equal stamp
rejects the cache and requeues every note. The sample probably misses a rare
parse bug; Rebuild stays the manual net. Obsidian 1.6.4 shipped a parse fix
that stayed invisible until each file was edited.

### P17. Kill switch

`IndexSync::new` takes `cache: Option<PathBuf>`, built beside `session_file`
in `bootstrap.rs`. `None` disables persistence. Every existing test runs with
`None`, unchanged.

## Error handling

The cache is derived data. None of its errors shows a notice or blocks.

**Read**

- Missing: outcome "missing", no log.
- I/O error: outcome "missing", one `log::warn!("index cache not used: {path}: {rejected}")`.
- `decode` returns `Rejected { Short, Version, Fold, Derivation, Root, Checksum, Corrupt(&'static str) }`,
  beside `decode` in `inkra-index`, after `restore/format.rs:37`. It is a
  list of reasons, not a second error enum.
- A rejected file is never removed: the next write overwrites it, as the
  session file is (restore R1). Removing would let an older and a newer
  binary destroy each other's file.

**Validation, in one pass, in this order**

1. Size cap, from a stat, before any read.
2. crc32.
3. Declared lengths sum to the file size exactly; every count × size uses
   `checked_mul` before `with_capacity`.
4. Words: valid UTF-8, bounds on char boundaries, `bounds[0] == 0`, strictly
   increasing; words strictly sorted; `starts` strictly increasing.
5. Postings: varints of at most 5 bytes, the 5th ≤ `0x0F`; gaps > 0 after
   the first; no overflow in the running sum; every ordinal < `next`;
   counts ≥ 1.
6. `lengths.len() ≤ next`, `notes ≥ alive.len`, `total` equals the sum of
   live lengths.

One `read_checked -> Option<u32>` serves the walk. Release builds have no
overflow checks, so an unvalidated file would return wrong results, not
panic. After a validated decode, only our own bug can panic, and no
arithmetic wraps silently.

**Write**

- `replace_bytes` reuses the temp + rename path; its `TempFile` guard
  removes the temp on any error.
- A crash between create and rename leaves an orphan of about 30 MB. The
  first write of a launch removes temps that carry this file's key and that
  no process holds: the writer keeps `File::try_lock` on its temp, and a lock
  the sweeper acquires marks an orphan. No pid check, so no `libc`.
- Disk full or permission denied: the old file stays; backoff; one log line.

**Edge cases**

- A moved vault gets a new key and leaves an orphan file. Recorded in
  `docs/roadmap.md` with its trigger.
- An older binary rejects a newer format and rewrites the old one. Each
  version flip costs one cold launch, accepted.

## Changes per crate

- `inkra-vault`: `Fs::read_bytes(path, max_len)`, `Fs::replace_bytes`,
  `Fs::remove_file` (`NotFound` is `Ok`; not `remove`, which `MemFs` already
  has as a fixture). `size` and identity on `DirEntry` and `Entry`; identity
  on `NoteSaved`. `MemFs` stores `Vec<u8>` and gains `insert_with_stamp`,
  `block_read_bytes`, `fail_next_replaces`, `block_replace`. `is_temp`
  exported.
- `inkra-index`: `encode`/`decode`, `Rejected`, `Seed`, candidates for
  ⌘⇧F, pure `locate` and `excerpt` over supplied text, `stamps`, `sources`
  in `Shards`, `scrub_sample`, the LRU validity rule.
- `inkra-app`: the cache read and outcome, the readers and LRU, the writer,
  `RebuildIndex`, the `Timeline` step, `cache: Option<PathBuf>`.
- Out of scope: FSEvents replay (notify 8.2 hardcodes `SinceNow`),
  `getattrlistbulk`, `crates/app/src/shell` beyond the action registration.

## Slices

Each PR is about 100–200 lines, vertical, with `./scripts/check.sh` green and
`cargo mutants` at zero missed on `index`. Pure decisions (LRU validity, seq
order, the scrub sample) live in `index` so mutants cover them; the app writer
gets the per-file pass of `.cargo/mutants.toml:38`. Each row names the bug its
key test must catch.

| PR | Content | Bug the test must catch |
|---|---|---|
| 0 | `launch-figure.sh` gains a `HOME` override, `--cold`, and `phys_footprint`. A ⌘⇧F bench with `RealFs` readers on `bench_vault`, mid-frequency term. A release log line `search: first hits, landed, read N`. | Baseline only. |
| 2a | `MemFs` bytes, `read_bytes`. Candidates from names + words, pure `locate`, ≤4 readers, `stale` rules. Text still resident. | A read that changes the index; `stale` on an I/O error; a stop at the 7th or 9th failure; the 1,001st hit; two landings. |
| 2b | LRU valid at equal stamp; pins from `Sessions`. | A hit served at an old stamp; a pin kept after its session closes. |
| 2c | ⌘P excerpts through the readers. | More than 100 reads. |
| 2d | Corpus text removed. The model test compares `entries()` and stamps instead of text. | Gates: RSS −150 MB; the slice-0 bench within 50 ms. |
| 3a | Header, crc32, lengths = size, `Root`, `Rejected`, `read_checked`. Golden constants. | `<` for `<=`; crc over the wrong range; a fingerprint not compared. |
| 3b | Per-note table, `Sources`. | `Sources` keyed by the old `EntryId`. |
| 3c | Segment, alive, BM25. Round-trip proptest equal to `entries()`; mutate-and-reseal proptest, where an `Ok` must survive `entries()`, every lookup and one merge. `benches/persist.rs`. `cargo test -p inkra-index --release` added to `check.sh`. | A resealed body accepted, then a panic at lookup. Gate: decode ≤ 60 ms. |
| 4a | `Seed` in `index`, maps built off the foreground; ghost ordinals; `or_insert`; `size` and identity on `Entry`. `Op::Relaunch` (encode, fresh scan, decode, seed) joins the interleaving proptest. A size-only change reloads; a whole-second stamp (P8) never matches. | A racy stamp reused; a ghost ordinal kept; `insert` for `or_insert`; a stale `by_stem`; a rename with an edit; a reused inode with another stamp. |
| 4b | App decode ∥ scan, outcome as a turn input, `Timeline` step. Tests insert `0.md` between launches so ids shift. | A cache keyed by `EntryId`; `queue_changed` before the outcome. Gates: `indexed` ≤ 0.35 s; seed turn ≤ 8 ms, named in the line. |
| 5a | Writer: snapshot, `merge(alive, fresh)`, seq, orphan temps, `replace_bytes`, `remove_file`. Test "second launch: decoded, zero loads". | Fresh tier dropped (save after the settle, advance 60 s, relaunch, assert the hit); stamp from `indexed` (held reload, write, relaunch, assert a reload). |
| 5b | 60 s and 500 windows, backoff, disk full. A plain `#[test]` for seq order; quit through `TestAppContext::quit`. | 500 vs 501; a write at quit; an older seq over a newer one. |
| 5c | `RebuildIndex`, tested through the palette (`cmd-shift-p`, "rebuild index", `enter`). | The removal ordered after an in-flight write; `indexed` not cleared. |
| 5d | Scrub, 64 notes in one batch, seed = file crc32. | A mismatch that does not requeue. |

Final check: `launch-figure.sh`, 5 cold and 5 warm runs at 50k, against the
slice-0 baseline. Measured for the docs, not gated: the write cost (encode
plus `F_FULLFSYNC`) and the file size at 50k.

## Roadmap entries

- Detect dataless placeholders (`SF_DATALESS`) before reading.
- Collect orphan cache files when the folder exceeds N files.
- Cold-cache figures (`launch-figure.sh --cold`) need root for `purge`; the
  user takes them. After a reboot the first ⌘⇧F reads from a cold page
  cache; measure it there before calling the gate met.
- Stream ⌘⇧F answers, if the slice-0 gate fails; needs both picker specs
  amended.

## Deviations

Each slice appends what it did differently from this spec, and why.

- Slice 0: `--cold` refuses without passwordless sudo, so cold figures are user-run.
- After vetting (2026-11-06): P3 adds `char::UNICODE_VERSION`; P4 moves the
  file to Caches; P8 judges a stamp's tick, not the load's age; P10's
  candidates carry the scanned path; orphan temps are judged by a file lock.
  Slices 4b and 5a change `shell/panes.rs` (`IndexSync::new`) and
  `restore/`, which the open splits stack rewrites: they start after the
  splits merge. 2a's "loads delta is 0" reads "the delta is the search's
  reads": `MemFs` counts every read.
- Slice 2a: reads go through `Fs::load`, which folds line endings as the loads
  whose words they must match did. `read_bytes` moves to 4b, its first caller.
