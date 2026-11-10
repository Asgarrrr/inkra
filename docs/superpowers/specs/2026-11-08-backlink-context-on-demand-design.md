# Backlink context read on demand, compact links

Status: **draft 2026-11-08, revision 3**, after two red/blue passes and three
research passes (prior art, duplicated bytes, allocators). Builds on
`2026-11-06-persisted-index-design.md` (P8, P10 to P12: racy stamps, the
readers, `stale`, the text cache) and `2026-09-24-links-and-backlinks-design.md`.

## Problem

At 50,000 notes, warm, `phys_footprint` at idle is 268 MiB. The malloc heap
holds 255 MiB. `MallocStackLogging=lite` and `malloc_history -allBySize` on
the release build attribute it as follows:

| MiB | Allocation site | What it holds |
|---:|---|---|
| 107.9 | `persist::notes::decode` | the `SourceLink`s of every note, about 300,000 links |
| 30.1 | `persist::words::decode` | the words segment |
| 20.4 | `NoteTable::new` | the note table |
| 10.8 | `Stems::insert` | the stems |
| 3.5 | `Seed::build` | one `Arc<Source>` per note |
| 33 | gpui, AppKit | the window, the renderer, the platform |

The 107.9 MiB split in two:

- 57 MiB of `String`s, two per link. One is the target `path`: about 300,000
  strings for about 50,000 distinct targets. The other is `context`, up to 160
  bytes of the link's line, read only by the backlinks panel.
- 51 MiB of `Vec<SourceLink>`. Each vector grows by `push` from empty, so it
  holds spare capacity. A `SourceLink` is about 112 bytes, most of it the
  three-variant `Target` enum, though a source holds note links only.

The `context` is a copy of the note text, which the index no longer holds (P9).

## Goals and gates

- `phys_footprint` at idle, warm, 50k notes, backlinks panel hidden, ≤ 200 MiB
  (268 MiB today). Measured with `launch-figure.sh` and the `bench-compare.sh`
  warm row.
- The links hold ≤ 30 MiB live, measured with `malloc_history -allBySize`
  under `persist::notes::decode` and `Seed::build`.
- The cache file shrinks; record the size before and after.
- The longest foreground turn stays ≤ 8 ms, read from the launch line's
  `longest turn`.
- A panel switch to a note whose sources are in the text cache lands its
  rows and contexts in one notify: a test counts the panel's notifies between
  `set_file` and the last context.
- One slow read never hides a row or the count: a test with a `MemFs` read
  that never completes still shows every row and the count.

## Decisions

### B1. A link holds no text of its line

`SourceLink` loses `context` and `context_link`.

### B2. A link is a note link, in its smallest shape

```rust
pub struct SourceLink {
    pub path: Arc<str>,          // as written, `.md` stripped, `/`-separated
    pub fragment: Option<Box<str>>,
    pub md: bool,
    pub style: Style,
    pub range: Range<ByteOffset>,
}
```

- The type matches the data: `Source::parse` keeps note links only, so the
  `Target` enum and its `panic!("a source holds note links only")`
  (`persist/notes.rs:86`) leave `SourceLink`. `Target` stays for parsing and
  for the app.
- Resolution takes a borrowed view, so no link allocates to be resolved:
  `NoteRef<'a> { path: &'a str, md: bool, style: Style }`, `Hash + Eq`.
  `NoteTable::query` (`notes.rs:101`) takes a `NoteRef`. `resolve(&Target)`
  keeps its signature: it answers `External` and `SelfLink`, then builds a
  `NoteRef`. `SourceLink::note()` returns one, and `backlinks` keys its
  per-text cache by it. The bench "500 notes named index"
  (`benches/index.rs:123`) runs before and after.
- `decode` interns `path` through one `HashMap<&str, Arc<str>>` that lives for
  the decode only. It keys the written form, so case and `../` variants stay
  distinct: count the distinct paths on the bench vault before fixing the gate.
  `Source::parse` interns the same way for one parse, so a reload keeps the
  sharing (added after the code review of #11).
- Rejected: a vault-wide interner (it never frees, and parse workers would
  share it with the foreground); `compact_str`/`smol_str` (the bench vault's
  short names would flatter them); a CSR graph (after B3 the per-note overhead
  is about 1 to 2 MiB, and every save would need a delta).
- Rejected: resolving links to ids at index time. Obsidian, SilverBullet and
  Logseq do it, so a create or a rename re-resolves. Inkra resolves at query
  time by stem, so a create or a rename invalidates nothing.

### B3. A note's links are one shared slice

`Source` becomes `Source { links: Arc<[SourceLink]> }`, and the index holds
`Source`, not `Arc<Source>`. One allocation per note, no spare capacity.
`decode` bounds the link count by `count * MIN_LINK_RECORD <= bytes.len()`, as
it bounds notes (`notes.rs:136`), fills a `Vec::with_capacity`, then
`Arc::from`. A reload builds a new slice, so `Arc::ptr_eq` tells whether the
links of a source changed (B7 relies on it).

### B4. `Stems` rows are 8 bytes

A row `(EntryId, usize)` becomes `(EntryId, u32)`: 16 bytes with padding
today. A note with more than `u32::MAX` links panics, as an infallible
operation on a caller bug does.

### B5. One checked context, owned by `index`

`index` exports `link_context(text, link: Range<ByteOffset>) -> Option<LinkContext>`
with the context's range in `text` and the link's range inside it.

- It returns `None` when the range is not in `text`, is not on char
  boundaries, or does not start with `[` or `![`. The parser includes the `!`
  of an embed (`markdown/src/links.rs:10`). A proptest asserts every range
  `Source::parse` returns passes `link_context` on its own text.
- A matching stamp does not prove the range: a racy stamp seeds before its
  reload (P8), and a rewrite can keep the mtime and the size (P16).
- The window is `window::around(text, start..start)`: up to 60 bytes before
  the link's start and 100 after, within the line. The link's range is clamped
  to the window's end, so a 10 KB link draws a 160-byte row, as today.
- `source.rs` loses `CONTEXT`, `BEFORE` and `context()`.

### B6. The panel lands its rows at once and its contexts as they arrive

- `NoteIndex::backlinks` returns each source with its `Candidate` (id, path,
  stamp) and its `Arc<[SourceLink]>`, plus the indices of the occurrences.
- `rebuild` stays synchronous for the rows: sources, names, order, count, and
  each occurrence's link range. The count, read by the dock header
  (`dock_chrome.rs:164`), never waits for a read.
- The panel holds one slot per source, keyed by id:

  ```rust
  enum Slot {
      Pending,
      Read { links: Arc<[SourceLink]>, contexts: Vec<Option<LinkContext>> },
      NoContext { links: Arc<[SourceLink]> },
      Failed { stamp: Stamp },
  }
  ```

  The slot is valid while its `links` are the source's current slice
  (`Arc::ptr_eq`), or for `Failed`, while the stamp is the same. Anything else
  is `Pending`. A rebuild fills `Pending` slots from the text cache on the
  foreground, a hash lookup and a window each. The transitions are a pure
  function in `index`, so `cargo mutants` covers them.
- The read job is a function of the pending set: at most one job, restarted
  when the set changes, the old one dropped. A dropped job's reads in flight
  still fill the text cache, so a restart reads little again. The job runs on
  a new unordered stream in `reads.rs`, beside `stream_with`'s ordered one
  (⌘⇧F keeps its hit order). One slow read holds no other, and `READERS` and
  the 8-failure stop stay one per panel.
- Reads happen only while the panel is drawn. `render` starts the job when
  slots are pending, idempotently. A hidden panel is not drawn
  (`render_view_body`, `dock_chrome.rs:126`), so `set_file` on every
  activation (`activation.rs:222`) costs no read.
- Each read fills its slot and notifies; gpui draws the notifies of one frame
  once. Every occurrence draws its context line from the first frame, empty
  until its context lands, so no row moves.

### B7. A read that does not prove the range shows no context

- `Read::Text` at the candidate's stamp: each occurrence gets `link_context`.
  If any is `None`, the slot is `NoContext` for this slice and the source goes
  to `stale` once. The reload gives a new slice and a new try, or the same
  answer at the same stamp and no new `stale`: no loop.
- `Read::Text` at another stamp, `Gone`, `NotUtf8`: `NoContext` and `stale`.
- `Failed`: `Failed { stamp }`, no `stale`. It is not retried at this stamp;
  `set_file` clears the slots. After 8 failures the job stops and keeps what
  it read.
- `stale` is sent per batch of results, not at the job's end, so a sleeping
  read never holds it.

This amends P11 (`2026-11-06-persisted-index-design.md:153`): a range that
fails at a matching stamp is a stale cause.

Accepted: a job reading between `LiveIndex::saved` and its parse landing
misses the cache, reads the new stamp from disk and queues a redundant load
(`LiveIndex::stale`, `live/loads.rs:106`). The load decides; it is harmless.
Accepted: backlink reads and ⌘⇧F share the 8 MiB text cache and evict each
other's entries.

### B8. At most 1,000 sources read per note

A bound chosen for this panel, not P10's (that one caps hits). Sources past it
show their name only. The slots hold windows, not texts, so their memory is
bounded by 160 bytes per occurrence. The panel draws every row today, not a
virtual list. A note with more than 1,000 backlinks is a roadmap entry, with
that trigger.

### B9. The cache format

`FORMAT_VERSION` goes from 2 to 3, in slice 2b only. A link record drops its
context blob and its context range, and `decode` drops its boundary check
(`notes.rs:217`). The fingerprint probe "a link deep inside a long line"
(`fingerprint.rs:58`) existed for the context window: drop it. The
fingerprint hashes the encoded links, so the pinned `0xEA84_84EC` and its
header bytes (`header_tests.rs:31,36`) are updated. A version 2 file reads as
rejected and is rebuilt. No migration: Inkra has no users yet.

## Deviations

- Zed has no backlinks.
- Obsidian's `LinkCache` keeps a link and its positions, no surrounding text.
  Foam keeps a URI and a range and serves context lines from an LRU. Both
  match B1. Whether Obsidian's pane reads the source files is not verified.
- `LinkContext` owns its window (`Box<str>`, at most 160 bytes). The slot outlives
  the read, and B8 bounds slots by windows.
- There is no `Pending` variant: a source with no valid slot is pending.
- `set_file` clears the slots only on a file change, because activation calls it
  on every switch.
- A dropped job cancels its reads in flight (a gpui `Task` drop), so a restart
  may read up to `READERS` sources again.
- A `Read` slot holds `Vec<LinkContext>`: under B7 any `None` makes the slot
  `NoContext`.
- Validity is `Backlink::reads_as`, not `Arc::ptr_eq` alone. A new note can move
  an occurrence to another target while the slice is kept, and a rename changes
  the candidate's path.
- The rebuild's cache fill keeps only a proven `Read`.

## Out of scope, measured next

From the duplicated-bytes pass, estimates only:

- `NoteTable`: each note's folded `key` and `stem` exist twice, in `Note`
  and as map keys, each with a one-element `Vec<EntryId>`. One table in
  vault order with indices in a `hashbrown::HashTable<u32>` saves about
  11 to 20 MiB.
- `Stems` keyed by a `u64` hash of the stem; a collision is filtered by the
  re-resolution in `backlinks`. About 6 MiB.
- `NoteSet` and `Names` share the vault's `RelPath` instead of copying it.
  About 8 MiB.
- The words segment read in place from an mmapped cache: clean pages, out of
  `phys_footprint`. 30 MiB, a large change.
- A global allocator: not adopted. Reports on macOS show mimalloc and
  jemalloc raising `phys_footprint`, and `malloc_zone_pressure_relief`
  returning little. The 573 MiB after a first open is measured before any
  allocator is tried: it may be parse data held, not the allocator.

## Changes per crate

- `index`: `SourceLink`, `Source`, `NoteRef`, `notes.rs` (`query`),
  `target.rs`, `source.rs`, `Stems`, `Backlink`, `backlinks`, `link_context`,
  the slot transitions, the link record in `persist/notes.rs`,
  `FORMAT_VERSION`, `fingerprint.rs`, and their tests.
- `app`: `index/reads.rs` (the unordered stream), `backlinks/mod.rs` (the
  slots, the job), `backlinks/render.rs`, `backlinks/tests.rs`, and the
  `backlinks` shot scene (`shot/scenes.rs:632`), which waits for the reads.
- `scripts/`: the heap gate, `malloc_history -allBySize` under
  `MallocStackLogging=lite`, summed under `notes::decode` and `Seed::build`.
- Docs: this spec in the CLAUDE.md list; amendment notes in
  `2026-09-24-links-and-backlinks-design.md` (the context leaves the index) and
  `2026-11-06-persisted-index-design.md` (P11, the record);
  `docs/benchmarks.md` (the warm figures); `docs/roadmap.md` (B8's entry and
  the out-of-scope list).

## Slices

| PR | Content | Bug the test must catch |
|---|---|---|
| 1 | B2, B3, B4 in memory: `NoteRef`, interned paths, `Arc<[SourceLink]>`, 8-byte rows. `SourceLink` keeps `context` and `context_link`; the record and `FORMAT_VERSION` 2 do not change. | A decode that drops, reorders or mis-interns links (round-trip proptest); two links to one path not sharing their `Arc`; a link count past the bytes accepted. Gate: the heap script under decode and seed, before and after, and the resolution bench no slower. |
| 2a | B5 to B8: the panel reads its sources and ignores `SourceLink.context`. | An embed never landing its context, or sending `stale` twice at one stamp; a context cut from a text at another stamp; a range past the end; a 10 KB link drawn whole; a slow read hiding a row or another context; a `Failed` retried at its stamp; a read while the panel is hidden; the 1,001st source read; a row moving when its context lands (shot). |
| 2b | B1, B9: `SourceLink` loses `context`, the record loses its blob, version 3. | A version 2 file accepted. Gates: footprint ≤ 200 MiB; links ≤ 30 MiB. If the footprint misses, record the figure and the next lead from "Out of scope" instead of widening this spec. |

`cargo mutants` on `index` after each slice, zero missed; on `app` with
`--test-package inkra-app -f crates/app/src/backlinks` after 2a.
