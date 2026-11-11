# Links and backlinks — design

Date: 2026-09-24. Status: **shipped**, in six slices, each reviewed by a red and a blue pass
before code and by a code review after it.

The application is "a vault of markdown files, links and backlinks, search,
panes". The vault and the editor exist; links do not. `[[target]]` is styled
and concealed (live-preview spec, D2) but goes nowhere, and no note knows what
points at it. This spec creates `inkra-index`, the crate the modular
architecture design placed for "links, backlinks, search"
(`2026-09-20-modular-architecture-design.md`, "Where future work lands").

Full-text search is **not** in this spec. It lands in the same crate, with a
spec of its own, once links have shipped.

A first draft was reviewed by one adversarial pass (correctness against the
code and pulldown-cmark) and one constructive pass (seams and slicing). Where
the draft was wrong, the correction is recorded in a **Corrected** note rather
than quietly rewritten, as `2026-09-22-create-rename-delete-design.md` does.

## Scope

| In | Out, deferred below |
|---|---|
| Extracting `[[wikilinks]]`, `![[embeds]]` and `[text](note.md)` from a note | Rewriting links when a note is renamed |
| Resolving a link to a note, one rule for every surface | Creating the note an unresolved link names |
| Keeping the index current: startup, save, create, rename, trash, rescan | Frontmatter `aliases` |
| Following a link from the editor: `⌘`-click and `⌘↵` | Scrolling to `#heading` or `#^block` |
| A backlinks panel for the current note | Unresolved-link styling, links to non-note files, unlinked mentions, `[[` completion |

> **Corrected.** The draft had following an unresolved link create the note in
> the source's folder. Cut, for two reasons found in review. It needs the
> folder's `EntryId` from a path, and `Vault` has no path lookup
> (`crates/vault/src/vault.rs`; `NoteStore::create` takes `parent:
> Option<EntryId>`, `crates/app/src/vault/tree_ops.rs:80`) — the same
> capability D3 defers as `by_path`. And it picks a folder from what the user
> typed, a rule the create/rename/delete spec refused for typed names. It
> returns with its own trigger once following and backlinks exist.

## What Zed does, and what we take from it

Zed has **no wikilinks** — `markdown` and `markdown_preview` handle CommonMark
links only. What it has is the machinery around them.

Taken:

- **Resolution is one function, separate from the renderer.** `markdown`
  hands a clicked `RenderedLink { source_range, url }`
  (`crates/markdown/src/markdown.rs:4678`, `:4865`) to a caller, and
  `workspace::path_link::resolve_open_target`
  (`crates/workspace/src/path_link.rs:138-460`) decides what it names. Ours is
  `LinkIndex::resolve`; the editor never resolves.
- **Layered resolution, exact first.** Zed tries an exact worktree entry
  (`path_link.rs:285-306`) and falls back to a heuristic suffix scan only last
  (`:432-457`). Our order below has the same shape.
- **Identity survives a rename, so consumers are not told about renames.**
  `ProjectEntryId` stays with the inode and only its path changes
  (`crates/worktree/src/worktree.rs:2707-2742`); there is no `Renamed`
  variant in `PathChange` (`:4009-4022`). `Vault::apply` already does this, so
  the index keys sources by `EntryId` and a rename costs it nothing.
- **Heavy work on the background executor, results handed back.** Zed's
  scanner, search and fuzzy matcher never touch an entity off the main thread.
  Index builds parse on `background_spawn` and apply on the foreground.

Rejected:

- **Case-exact resolution.** Zed matches paths byte-for-byte. A note app on
  APFS, a case-insensitive filesystem, cannot: `[[notes]]` must find
  `Notes.md`, as it does in every tool below.
- **LSP `willRenameFiles`** (`crates/project/src/lsp_store.rs:11787-11838`)
  for rewriting references. There is no language server; rewriting is
  deferred, and when it lands it walks backlinks directly.
- **A references multibuffer** (`crates/editor/src/navigation.rs:1369-1438`,
  `:2193`) for backlinks. It needs excerpts, which the editor does not have. A
  list of sources with one line of context each is enough for a note app.
- **Open-buffer precedence** in project search
  (`crates/project/src/project_search.rs:152-225`). Zed searches unsaved
  buffer text; our index reads what was last saved. See D5.

## What note apps do

Obsidian is the compatibility target, because a user's existing vault is most
likely an Obsidian vault. Its rules are not specified; they are
reverse-engineered, and the ones we rely on are:

- Extension optional: `[[Note]]` and `[[Note.md]]` are the same link.
- Case-insensitive: `[[note]]` finds `Note.md`.
- A bare name resolves by file name anywhere in the vault; a name with `/` is a
  path, matched as a suffix on a component boundary.
- Two notes sharing a name: the one with the fewest path components wins.
- Unresolved links are first-class: tracked, and following one creates the
  note.
- `metadataCache.resolvedLinks` is a forward map `source → {target → count}`;
  backlinks are computed from it, not stored beside it.
- Note names may not contain `# | ^ [ ]`: they cannot be written in a
  wikilink.

Taken from the pitfalls reported against Obsidian and its plugins:

- **One resolution function for every surface.** Navigation, backlinks and
  (later) styling and completion disagreeing is the most-reported bug class in
  Obsidian-compatible tools.
- **Unicode normalization.** APFS keeps a filename in whichever form it was
  typed, and Obsidian lost notes when an NFD name met an NFC listing. Keys are
  NFC.
- **Rename rewriting races.** Obsidian's rewrite of other notes is async and
  touches open buffers; its failures are the reason it is deferred here rather
  than squeezed in.

Logseq and Dendron key by page title or hierarchy, zk and SilverBullet keep a
SQLite database. None fits a folder of files better than Obsidian's in-memory
forward map.

## Decisions

### D1. Extraction lives in `inkra-markdown`, as a separate function

```rust
pub struct Link {
    /// The whole link, `!` included for an embed.
    pub range: Range<ByteOffset>,
    pub kind: LinkKind, // Wiki | Inline
    pub embed: bool,
    /// As written, trimmed: fragment kept, not percent-decoded, never empty.
    pub target: String,
}

pub fn links(text: &str) -> Vec<Link>;
/// The innermost link containing or touching `offset`; the editor's follow (D7).
pub fn link_at(text: &str, offset: ByteOffset) -> Option<Link>;
```

The same parser with the same `options()` as `parse`, so a `[[link]]` inside a
code span or a fence is a link for neither the editor nor the index. A cheaper
regex scan was considered and rejected on that ground: it would have to
re-implement which text is code.

Not a field of `Markdown`: `parse` runs on every keystroke, and the links it
would carry have no reader on that path. `links` runs on a save, in the
background, and on a follow. At 89 µs for a 50 KB note (live-preview spec,
X1), a second parse on a click is not keystroke work.

pulldown-cmark 0.13.4, checked against a scratch build:

| Written | `dest_url` |
|---|---|
| `[[Note#Head\|Alias]]` | `Note#Head` |
| `![[img.png]]` | `img.png`, as `Tag::Image` |
| `[x](my%20note.md#h)` | `my%20note.md#h` |
| `[[t\|al]]`, in a table or not | `t\` — the escaped pipe's backslash stays |
| `[[ spaced ]]` | ` spaced ` |
| `[[a\|b]]` | `a` — everything after the first `\|` is the alias |
| `[[Note]] [[Note]]` | two events, one per occurrence |
| `` `[[code]]` ``, a fenced `[[fence]]` | no event |
| `<div>[[x]]</div>` | no event: an HTML block is one opaque `Html` event |
| `[[x]]` inside YAML frontmatter | no event: the block is one `Text` |
| `<a@b.com>` | `a@b.com`, as `LinkType::Email` — no scheme |
| `[x][r]` with `[r]: note.md` | `note.md`, at the usage only |
| `[missing]`, no definition | no event |
| `[[a\|]][[b\|c]]` | `a`, then `b`, on 0.13.5; 0.13.4 added a second `b` event with the same range, `ShortcutUnknown`, `dest_url` empty |

`links` trims the target; drops the trailing `\` an escaped pipe leaves on a
wikilink target; prefixes an email autolink with `mailto:`, so D2 sees a
scheme; and drops a link whose target is then empty (`[x]()`, `[[ ]]`).
Everything else is the index's to interpret. A link inside raw HTML is not a
link, for the index as for the editor, which does not style it either;
accepted. A wikilink target genuinely ending in `\` loses it; accepted, as `\`
in a note name is not worth an escape rule.

> **Corrected.** The draft read the trailing `\` as a table artifact. Review
> found it on any escaped pipe before the alias separator, table or not.

> **Corrected.** Found in review of slice 1: after an empty wikilink alias,
> pulldown-cmark repeats every later link in the block (last row above). The
> `Unrepeated` adapter `parse` uses for styling skips the *first* copy — the
> one with the right type and target — because it starts at the open link's
> end. Styling cannot tell, since both copies are `Tag::Link`; `links` can.
> So `links` reads pulldown-cmark's events directly and keeps the first link
> per range: two real links never share one.

> **Corrected.** pulldown-cmark 0.13.5 no longer repeats them
> (RUSTSEC-2026-0414, pulldown-cmark #1102). `Unrepeated` and the per-range
> filter in `links` are gone.

### D2. Target parsing

`Target::parse(link) -> Target`, in `inkra-index`:

1. Split at the first `#`: the rest is the fragment, kept, unused (deferred).
2. `Inline` only: anything with a URL scheme (`https:`, `mailto:`) is
   `Target::External`. Percent-decode the rest.
3. Empty path after the split (`[[#Heading]]`): `Target::SelfLink`, never a
   backlink.
4. Otherwise `Target::Note { path, style }`, a trailing `.md` stripped
   case-insensitively, where `style` is `Name` (wiki, no `/`), `Suffix` (wiki,
   with `/`), or `Relative` (inline, or a wiki path starting `./` or `../`).

> **Corrected.** The draft had a rule 4 classifying any path with an extension
> other than `md` as `Target::Asset`, ignored. That made `[[v1.2]]` and
> `[[a.b]]` unresolvable, although `v1.2.md` and `a.b.md` are ordinary notes
> (`note_file_name` keeps any extension but `.md`,
> `crates/vault/src/path.rs`). There is no asset class to guard: `Vault` only
> holds `Kind::Note` and `Kind::Dir` (`crates/vault/src/fs.rs:30-33`), so
> `[[img.png]]` simply finds no note and is `Unresolved`. The variant is gone.

**Names a wikilink cannot spell.** A note named `a|b.md` or `Note#1.md` can be
reached by no wikilink: pulldown-cmark takes `|` as the alias separator before
`dest_url` exists, and rule 1 takes `#` as the fragment. `check_name` allows
both today. It gains one `NameProblem`, `LinkSyntax`, for `#`, `|`, `^`, `[`
and `]`, Obsidian's set, so the app stops creating notes its own links cannot
reach. It applies to folders too: a folder name is a component of a
`[[folder/note]]` link. Confirming a rename with the name unchanged closes
the field before the check, so a note named outside the app is not refused
its own name. A file with such a name made outside the app stays reachable from the
sidebar and by `[text](a%7Cb.md)`.

> **Corrected.** The draft did not notice that `check_name`
> (`crates/vault/src/path.rs`) permits `#` and `|`. Found in review.
> Review of slice 2 then found that the rename field, seeded with the note's
> current name, would have refused `a#b` confirmed unchanged.

### D3. Resolution

Every note path is folded to a **key**: a trailing `.md` stripped once, then
NFC, Unicode lowercase, and NFC again, since lowercasing `İ` leaves a string
that is not NFC. The index holds the vault's `Kind::Note` entries as
`stem key → [EntryId]` and `path key → [EntryId]`, rebuilt from
`Vault::entries()` on every `apply`. A folder is never a candidate, so
`[[projects]]` beside a `projects/` folder is `Unresolved`.

`resolve(source, target) -> Resolution { Note(EntryId), Unresolved,
External(String), SelfLink }`:

| Style | Candidates |
|---|---|
| `Name` | notes whose stem key equals the target's |
| `Suffix` | notes whose path key ends with `/target` or equals `target` |
| `Relative` | notes whose path key equals `source's folder / target`, normalized, `..` above the root refused; after a leading `/`, from the root |

The winner is, in order: the candidate whose on-disk path, NFC, ends with the
written target, NFC, on a component boundary and in its exact case; the fewest
path components; the order `Vault::entries()` yields, which its private
`compare` sets. Deterministic, and Obsidian's rule plus one tie-break it leaves
unspecified.

> **Corrected.** The draft gave `Relative` "at most one candidate". On a
> case-sensitive disk `Notes.md` and `notes.md` share a folded path, and a
> first-wins path map would have sent `[x](notes.md)` to `Notes.md` while
> `[[notes]]` went to `notes.md`. Every style now breaks ties by the same
> chain. Found in review of slice 3, with the case-exact test defined on NFC
> text and on the tail a `Suffix` names.

Accepted limits, each with a test that pins it:

- A note named `x.md.md` is linked as `[[x.md.md]]`; `[[x.md]]` finds `x.md`.
  One `.md` is stripped, on both sides.
- `\` is part of a name, not a separator: `[[work\Plan]]` does not find
  `work/Plan.md`.
- An inline target that starts like a URL scheme (`cd:notes.md`) is
  `External`. `check_name` refuses `:`, so only a file made outside the app
  can be that target.
- A wikilink target is not percent-decoded: `[[my%20note]]` names a note
  called `my%20note`.

A **deviation from Obsidian**: `[text](note.md)` resolves relative to the
source's folder only, as CommonMark and every renderer outside Obsidian read
it. Obsidian falls back to a name search for inline links too; a vault that
relies on that is a trigger, not a default.

`Vault::by_path` stays deferred. Its trigger was "following a `[[link]]`", and
this spec follows links without it: resolution needs a folded key, which
`by_path`, keyed by exact `RelPath`, would not provide.

**`RealFs` recognises `.md` case-insensitively.** `Note.MD` is a note on APFS
and in Obsidian; today `read_dir` drops it
(`crates/vault/src/real_fs.rs:78`, `ext == "md"`). The comparison becomes
`eq_ignore_ascii_case`, in the same slice as the key that relies on it.
`MemFs::read_dir`, which listed every file as a note, gains the same filter,
so tests cannot insert a file the real filesystem would hide.

> **Corrected.** The draft promised case-insensitive resolution without
> checking that such a note reaches the vault at all. Found in review.

### D4. The index stores links, not resolutions

```rust
pub struct LinkIndex {
    notes: NoteTable,                             // stem and path keys → notes
    sources: HashMap<EntryId, Source>,            // per note: its links, parsed
    by_stem: HashMap<Key, Vec<(EntryId, usize)>>, // link's stem key → (source, link)
}
```

Nothing resolved is cached. A link is resolved when asked, against the current
`NoteTable`, so a create, a rename or a trash invalidates nothing: the next
query sees the new table.

**Backlinks without a reverse map.** Any link that resolves to note `T` has
`T`'s stem as its own stem key: in every style the name is the last component,
and both sides fold the same way. So `backlinks(T)` is `by_stem[stem(T)]`,
filtered to the links that resolve to `T`. Its cost is the number of links
sharing `T`'s name, not the vault. The property is a proptest (slice 3), not a
belief.

`Source` keeps, per note link, its `Target` and its byte range, and no text of its line. The
panel reads that line from the note on demand
(`2026-11-08-backlink-context-on-demand-design.md`).

The API, synchronous and pure:

```rust
impl LinkIndex {
    /// Also drops the source of every note gone, so `LinkSync` never diffs.
    pub fn set_notes<'a>(&mut self, entries: impl IntoIterator<Item = &'a Entry>);
    /// Ignored for an id that is not a note: a load landing after a trash leaves nothing.
    pub fn insert(&mut self, id: EntryId, source: Source);
    pub fn remove(&mut self, id: EntryId);
    pub fn is_indexed(&self, id: EntryId) -> bool;
    pub fn resolve(&self, source: EntryId, target: &Target) -> Resolution;
    pub fn resolve_link(&self, source: EntryId, link: &Link) -> Resolution; // D7
    /// Grouped by source note, in vault order; every occurrence kept, in document order.
    pub fn backlinks(&self, note: EntryId) -> Vec<Backlink<'_>>;
}

impl Source {
    pub fn parse(text: &str) -> Self; // calls inkra_markdown::links
}
```

Parsing and inserting are split so the parse runs on the background executor
and only `insert`, which touches no text, runs on the foreground.

`[[Note]]` twice in one source is two occurrences under one source row, not two
rows. A note linking to itself appears in its own backlinks, as in Obsidian.

> **Corrected.** The draft left repeated links unspecified. Found in review.

### D5. The index reads what was saved, not what is typed

A note's links enter the index when it is written, not on each keystroke.
Autosave is periodic, every two seconds while a note has unsaved edits
(`AUTOSAVE_DELAY`, `crates/app/src/vault/store.rs`), so a backlink appears at
most about two seconds after it is typed.

The alternative — feeding the buffer's links on every edit — would put a
second whole-document parse on the keystroke path, which CLAUDE.md forbids,
or would have the editor emit its parse to a model it does not know about.
Recorded as a **deviation from Zed**, which searches unsaved buffers.

Following a link is not affected: it parses the live buffer (D7).

### D6. Keeping the index current — `LinkSync` in `app`

One entity, `crates/app/src/index.rs`, owns the `LinkIndex`. It is fed from
two places:

1. **The vault.** `LinkSync` observes `Entity<Vault>`, as the sidebar does
   (`crates/app/src/sidebar/mod.rs`). On each notify it calls `set_notes`,
   then reconciles sources: a note whose `mtime` differs from the one it was
   indexed at, or that it has never indexed, is loaded (`Fs::load`) and parsed
   in the background; a source whose note is gone is removed. This one loop
   covers startup, create, trash and a rename (a path change, not an `mtime`
   change, so no reload).
2. **Our own writes.** `NoteStore` gains a success event,
   `NoteSaved { id, text: Arc<str>, stamp: Stamp }`, emitted in `write` after
   the write lands, beside the existing failure events
   (`crates/app/src/vault/events.rs`). `LinkSync` parses `text` in the
   background and records `stamp.mtime`, so the next reconcile, which sees the
   same mtime in the rescanned entry, does not reload what it just saw.

The text is captured once as `Arc<str>` in place of today's `String`
(`buffer.text().to_owned()` in `write`), and `write_through` takes the
`Arc<str>`: `Fs::save` only needs `&str`. One copy per write, as today, shared
by the write and the event.

> **Corrected.** The draft's `text: Arc<str>` would have been a second copy
> next to the `String` `write` already makes. Found in review.

`AppShell` builds a `LinkSync` on every launch, as it builds the `Sidebar`;
without a vault it subscribes to nothing and indexes nothing, so slices 5 and
6 hold an `Entity<LinkSync>`, never an `Option`.

**Ordering.** A background load can finish after a newer write of the same
note. `LinkSync` stamps each request, a load or a `NoteSaved`, with a sequence
from one counter at the moment it is issued, and keeps the last sequence
applied per note; a result below it is dropped. It is `Writer`'s pattern
(`crates/app/src/vault/writer.rs:36-54`), not a new one, and it orders by a
counter, never by `Stamp`, which `.claude/rules/vault.md` keeps unordered.

> **Corrected.** Found in review of slice 5: a result acts only if its
> sequence is the latest minted for its note, not the latest applied (search
> spec, D5, "Built in slice 5").

**Freshness, stated plainly.** Nothing rescans on its own. A rescan follows a
create, a rename, a trash or a preserved conflict, and nothing else — an
ordinary write does not (`an_ordinary_write_does_not_rescan`,
`crates/app/src/vault/tests/rescan.rs`). So a closed note edited by another
program keeps its old links in the index until the next of those, or the next
launch. That is the same gap as the stale clean buffer, with the same fix: the
file watcher in the vault design's Deferred table, whose trigger now covers
the index too.

> **Corrected.** The draft said the loop covered "any external change a
> rescan discovers", which read as if rescans recurred. They do not.

**A preserved conflict copy is a note.** `Note (conflict …).md`
(`crates/vault/src/conflict.rs`) is on disk, is scanned, and its links count:
a target it links to shows it as a second source until the user deletes it.
Accepted — it is what the vault contains, and hiding it would make the
backlinks panel disagree with the sidebar.

While a reconcile's loads run, `LinkSync::is_building` is true and the
backlinks panel says so; a write's own parse does not count. Loads run through
one task working the queue in vault order, one file at a time, not one task
per note, so startup on a large vault does not open thousands of files at
once. Zed's scanner walks a worktree in parallel; ours is serial by choice,
until a profile of a real vault's startup says otherwise. A note queued again
before its first entry is reached keeps only the newest entry, so a burst of
rescans loads it once. A write's parse is not queued: it needs no load, and
must not wait behind a startup queue.

An ordinary write neither rescans nor notifies the vault — `reidentify`
updates it silently — so `NoteSaved` is the index's only signal for one. The
emission says so, and `MemFs::loads`, a count of loads per path, lets the
tests pin that neither a write nor a rename is read back from disk. One case
reads a write back once: a rescan whose walk started before the write and
landed after it restores the old mtime, and the rescan after that sees the new
one. The result is right; the cost is one load, accepted.

A note that fails to load loses the links read before, unless a newer write
already replaced them, and is retried at the next reconcile.

> **Corrected.** Found in review of slice 4: a failed load kept the note's old
> links, which described text no longer on disk. A note that fails to load stays unindexed and
is retried on the next reconcile; nothing is shown, for the reason nothing is
shown for a failed open (vault design, Deferred: "a home for user-visible
errors").

> **Corrected.** Found in review of slice 5: a rename keeps the mtime, so a
> load that misses the renamed note must be retried by `IndexSync` itself.
> With no build since the load started, the note is forgotten and the next
> build reloads it. With a build since, and the note still in the vault, the
> note is queued again at once (search spec, D5, "Built in slice 5").

`EntryId::DETACHED`, a file opened outside a vault, is never indexed.

This reconcile loop is the crate's one feed. Full-text search, when it lands,
consumes the same loads rather than adding a second loop.

### D7. Following a link

`⌘`-click on a link, and `⌘↵` with the caret inside one. A plain click keeps
placing the caret, which reveals the syntax: editing a link must stay
possible. Neither is bound today (`crates/app/src/keymap.rs`; `enter` is bound
in the editor context, `cmd-enter` is not). `EditorView::on_mouse_down`
already maps the pointer to an offset (`crates/app/src/editor/mod.rs`,
`offset_for_position`) and receives the modifiers.

`EditorView` calls `inkra_markdown::links` on its buffer text, finds the link
containing the offset, and emits `LinkFollowed(Link)`. It does not know the
index. `AppShell` subscribes, as it does to `NoteActivated`, parses the
target, asks `LinkSync` to resolve it against the current note, and opens the
result through the same path as `NoteActivated`.

`External` opens with `cx.open_url`. `Unresolved` and `SelfLink` do nothing
in this spec, and neither does a link to the note already showing:
`open_note` would rebind it through `open_buffer`, which resets its caret and
scroll.

The modifier is Zed's: `Editor::is_cmd_or_ctrl_pressed` gates its link
following (`crates/editor/src/hover_links.rs:176-221`). A ⌘-click focuses the
editor, then follows and returns before `is_selecting` is set, so no drag
selection starts under a held button; a ⌘-click off any link is a plain
click. `⌘↵` follows the link at the selection's head and inserts nothing when
there is none. `simulate_keystrokes("cmd-enter")` dispatches without IME
character input (`Keystroke::with_simulated_ime` gives a ⌘ chord no
`key_char`), which matches what a ⌘ chord does on macOS but is not the same
code path; the macOS platform source is not vendored to confirm it.

The editor subscription is detached, like `AppShell`'s others: the editor is
created once and rebound after, so it lives as long as the shell.

> **Corrected.** Found in review of slice 5: following a link to the note
> already showing would have jumped its caret to the start.

A ⌘-click follows the innermost link whose *drawn* span holds the pointer:
from its start's x on its first row to its end's x on its last, so hidden
syntax, which shares an x with its neighbours, adds no width. `link_at` on the
click's offset was the first version, and review of slice 5 found it followed
from past a line's end, from below the text, and from the half of the glyph
after a link: the offset of a click is the nearest boundary, and each of those
lands on the link's edge. `⌘↵` keeps `link_at`, since a caret at a link's end
is on it.

### D8. The backlinks panel

A pane right of the editor, listing the notes that link to the current one:
each source's name, then each occurrence's line of context, the link itself in
the link colour. Clicking a row opens the source note. "No backlinks" when the
list is empty; "Indexing…" while `is_building`. With no current note the pane
is not rendered.

A link holds no text of its line. The panel reads each source's text on demand
and cuts each context from it (`2026-11-08-backlink-context-on-demand-design.md`).

The panel holds `Entity<LinkSync>`, a model, as the sidebar holds
`Entity<Vault>`; `AppShell` sets its current note. It never holds the editor.

This **widens the app-shell layout spec**, which kept the shell to two slots
and no toggle (`2026-09-20-app-shell-layout-design.md`, "What does not
transfer"). It adds a third named slot, as that spec's "a one-slot addition
when something needs it" anticipates. `AppShell::render`
(`crates/app/src/shell.rs`) is a two-child `flex_row` that swaps the whole
right side between the editor and the empty state; the panel is a third child,
present only in the editor branch.

It is always visible. A toggle waits for its trigger: "the panel costs width a
user wants back".

As built (`crates/app/src/backlinks/`):

- **Rows are cached**, rebuilt on a `LinkSync` notify, on a vault notify (a
  rename changes a source's name and no link), and on `set_note`; never in a
  draw. Zed's outline panel caches its entries the same way. While the index
  builds, each loaded note notifies, so rebuilds are throttled to one per
  100 ms until it is done; after that a notify rebuilds at once.
- **A rebuild redraws only when what the panel shows changed**: its rows, or
  whether "Indexing…" shows. A redraw of the window reshapes the whole
  editor (`TextElement::request_layout` calls `shape_document` every frame,
  and no view is `cached`), and most index changes — a write to a note that
  does not link here — leave the panel as it was.
- **"Indexing…" shows only while there are no rows yet**, so a rescan after a
  create elsewhere does not blank a list already showing.
- **A source and its occurrences are one clickable group**, and a click emits
  the sidebar's own `NoteActivated`, so the shell opens it by the path it
  already has. Plain `div` children in a scrolling column, not
  `uniform_list`: groups differ in height, and Zed uses `uniform_list` only
  for rows of one height.
- **`AppShell::set_showing` is the one place `showing` changes**, activation
  and `NoteClosed` both, so the panel cannot lag the editor.
- It takes no focus, so a click leaves the editor's; `open_note` focuses the
  editor on the note it opens.
- The panel reuses the sidebar's background. Whether it needs its own beside
  the editor is for the `backlinks` shot to say; the Linux container this was
  built in cannot render shots (`shot` is macOS-only).

**The column pays for it.** At the default 1000 px window, 220 px of sidebar
and 260 px of panel leave the editor 520 px, under the 70 `ch` column
(`theme::BODY_COLUMN_CHARS`), so body text wraps narrower than the soft-wrap
spec intends until the window is widened. Accepted for this slice, and the
first thing the toggle's trigger would name; a wider default window is the
other lever.

## Performance

The costs that grow with the vault:

- **Startup:** one `load` and one `links` per note. At the live-preview
  figures (17 µs per 5 KB note), 10,000 notes parse in about 170 ms of
  background CPU, plus the I/O, sequentially. Nothing waits for it.
- **`set_notes`:** O(notes) per vault `apply`, the same order as `apply`
  itself.
- **`backlinks(T)`:** O(links named like `T`). Called on a note switch and on
  each index change while the panel shows `T`. Within one call a result is
  shared by every link written the same way, since only a relative link's
  resolution depends on its source.
- **A follow:** one `links` over the current note, on a click or `⌘↵`, never
  per keystroke.
- **Memory:** per link, its written path, fragment, style and range. A link
  holds no text of its line: the panel reads contexts on demand. The index
  keeps no note text.

`inkra-index` has a criterion bench (`cargo bench -p inkra-index`): 10,000
notes in 100 folders, 500 of them named `index`, 20 links each, one in five to
`[[index]]`. Release, `--quick`, on the Linux container this was built in:

| Operation | Time |
|---|---|
| `set_notes`, 10,000 notes | 25 ms |
| `backlinks`, an ordinary note | 8 µs |
| `backlinks`, one of 500 notes named `index`, 40,000 links sharing the name | 14 ms |
| `Source::parse`, a 50 KB note of about 1,000 links | 1.0 ms |

> **Corrected.** The first build took 290 ms for the `index` case: every one
> of 40,000 links searched 500 candidates. Sharing a result per written
> target brought it to 14 ms, and an ASCII fast path in the key fold took
> `set_notes` from 41 to 25 ms.

`set_notes` runs on the foreground on every vault notify, and 25 ms at 10,000
notes is a visible frame. It is kept there for now: `Vault::apply`, which
precedes it on every notify, sorts the same entries with a comparator that
allocates per comparison, and has not been measured either. Building
the table on the background executor is deferred with the trigger "a profile
on a real vault shows `set_notes` in a dropped frame".

## Slices

Each slice merges alone with `check.sh` green.

1. **`inkra_markdown::links`**, with a test per row of D1's table and a
   proptest: every link's range lies inside the text, on char boundaries, and
   none starts inside a code span.
2. **Vault groundwork**: `NameProblem::LinkSyntax` in `check_name`, with the
   rename field's message; `RealFs` accepting `.MD`. Small, and independent of
   the index.
3. **`inkra-index`**: `Target`, `NoteTable`, `LinkIndex`, the bench. Proptest
   against a model — a naive resolver scanning every note agrees with
   `resolve`, and `backlinks(T)` equals the naive filter of every link —
   across random vaults, random names (dots, case, NFD) and random renames.
   The crate joins `members`, `[workspace.dependencies]`, `check.sh`'s fmt
   list and its no-gpui list. `unicode-normalization`, its one new dependency,
   is checked against `deny.toml` in the same change.
4. **`LinkSync`** and `NoteSaved`, against `MemFs`: startup indexes every
   note; a write, a create, a rename, a trash and an external edit found by a
   rescan each show in `backlinks`; a stale load does not overwrite a write,
   made deterministic with `MemFs::block_load`.
5. **Following links**: `⌘`-click, `⌘↵` (a `simulate_keystrokes` test),
   external URLs.
6. **The backlinks panel**, rendered in a `shot` scene and looked at.

## Deferred

| Item | Trigger | Notes |
|---|---|---|
| Rewrite links on rename | A user renames a note that has backlinks and expects them kept. The first one will: this is the most visible gap. | Walks `backlinks(old)` before the rename. An open source goes through `Buffer::transact_at`, a closed one through `Fs::save` with its `Stamp`, on that note's `Ops` chain |
| Create the note an unresolved link names | A user follows an unresolved link and expects the note. | Needs the source folder's `EntryId` from a path — a `Vault::by_path` for folders — and a decision on where a link's note goes, against the create/rename/delete spec's refusal to infer folders from typed names |
| Frontmatter `aliases` | An imported vault uses them. | `MetadataBlock` is already a parse event; needs a YAML subset |
| Scroll to `#heading`, `#^block`, and to a backlink's line | Following a link to a long note lands at its top and the user scrolls. | Needs "open at offset" on `EditorView` |
| Hover feedback for ⌘ over a link (underline, pointing hand) | A user cannot tell which text a ⌘-click follows. | Zed tracks it per mouse move (`hover_links.rs`); gpui has no modifier-change event wired here yet |
| Unresolved-link styling | The user asks. | Needs a resolve per link on each parse, or a style the index pushes back |
| Links to non-note files, embeds rendered | Inline images, per the live-preview Deferred table. | `Vault` holds notes and folders only; assets need a `Kind` first |
| Name-search fallback for inline links | A vault relies on Obsidian resolving `[x](Note.md)` by name. | One more row in D3's table |
| Links inside HTML blocks | A vault writes them. | pulldown-cmark gives one opaque `Html` event |
| Unlinked mentions | The user asks. | Needs full-text search |
| `[[` completion | Typing a link to an existing note by hand gets its name wrong. | Zed's `fuzzy::match_path_sets` over `NoteTable` |
| Index from the live buffer | Two seconds of lag is observed. | D5 |
| Backlinks panel toggle | The panel costs width a user wants back. | D8 |
| `NoteTable` built off the main thread | A profile on a real vault shows `set_notes` in a dropped frame. | 25 ms at 10,000 notes, measured; build it in the background and swap it in |
