# File kinds: the vault sees every file, a group shows any kind

Status: **reviewed 2026-11-06**. Brainstormed in French; sections 1–4 approved
by the user; red and blue teams folded in (see "Review outcome").

## Why

The user wants to know how far Inkra is from Obsidian-like Bases and image
galleries. Two assumptions block both: the vault lists only `.md` files
(`RealFs::listed`, `crates/vault/src/real_fs.rs:63`), and a group shows either
its one `EditorView` or a singleton panel (`Shown`, `crates/app/src/shell.rs:76`).
This spec removes both, so a new kind of file is a local addition. Images are
the second kind: they prove the seam with a second real implementation
(CLAUDE.md, "abstract only at a real seam"). Bases, galleries and inline images
are out of scope.

Nobody uses the app yet: formats and types break freely, with no migration.

## Decisions taken

- **K1. Every centre item is a vault file.** A tab's identity is an `EntryId`;
  its kind follows from the file. `Place::View` (Files, Backlinks in the
  centre) stays as it is.
- **K2. Every file kind joins the tab's history.** Following `![[photo.png]]`
  shows the image in the same tab; `⌘[` returns to the note (Obsidian).
- **K3. A place is data; a group holds at most one editor.** A tab keeps plain
  data (tabs spec, A2). The editor stays as today: `Shown::Bound` holds it and
  rebinds it (A3). An image is a `Shown::Image` variant drawn as an element
  from a shared `Images` store; it has no entity of its own. Leaving a note
  for an image drops the editor exactly as leaving it for `Place::View` does
  today, and the next note recreates it through the existing create arm. Kind
  dispatch lives in one module, `shell/surface.rs`, by exhaustive `match`;
  no trait. The one exception is the tree's icon (`sidebar/render.rs`), which
  matches on `FileKind` to pick the icon and shows nothing.
- **K4. Kind is derived, never stored.** `FileKind::of(path)` is called where
  a kind is needed (display, tree icon, rename, load). A rename across kinds
  keeps its `EntryId` (inode) and changes what the next display shows.
- **K5. The app reads image bytes through `Fs` and decodes them itself.**
  `Fs::read_bytes` returns bytes; `images/decode.rs` decodes them with the `image`
  crate into an `Arc<RenderImage>`, inside the load task, on the background
  executor. gpui never reads a path, so `MemFs` tests show images. The decode
  is ours, not `gpui::Image::from_bytes`, for five reasons:
  - `Window::drop_image` frees an atlas entry from an `Arc<RenderImage>`;
    an `Image` gives none to hand it.
  - A side over 16384 (the GPU texture limit) is refused before any pixel is
    decoded.
  - A side over 4096 is scaled down: no window draws more, so the atlas never
    holds a larger texture.
  - An animated GIF keeps its first frame only.
  - The decoder's pixel buffer is charged against a 256 MiB allocation budget
    (`Limits::reserve`, then `set_limits`, as `ImageReader::decode` does),
    so a small file that decodes huge is refused.
- **K6. Links resolve to files, notes first.** A link target resolves as a
  note first; on a miss, the exact full file name is looked up in the asset
  name table. No extension heuristic: `[[Node.js]]` and `[[v1.2 plan]]` stay
  note links. `Resolution::Note` becomes `Resolution::File`.

## Vocabulary

- **file**: a vault entry that is not a folder.
- **note**: a file of kind `Note` (`.md`, case-insensitive).
- **asset**: any other file.
- **image**: an asset whose `FileKind` is `Image`.

## Deviations from Zed

| Zed | Inkra | Why |
|---|---|---|
| One item entity per tab (`ItemHandle`, `workspace/src/item.rs:476`) | One editor per group, rebound; images are stateless elements | Keeps A3's synchronous switch; memory bounded by groups, not tabs |
| `Item` trait, open set of kinds | Closed `FileKind` enum, dispatch in `shell/surface.rs` | No plugin system ahead of need (CLAUDE.md) |
| `ImageViewer` reads the path through `Project` | Bytes through `Fs`, our own decode to `RenderImage` | `app` opens no file; `MemFs` tests; K5 |

## Vault (`inkra-vault`)

- `Kind { Dir, Note, Asset }`. `.md` (case-insensitive) is `Note`; every
  other regular file is `Asset`.
- One listing predicate, shared by `RealFs` and `MemFs`, drops dotfiles,
  symlinks and our save temp files (`is_temp`, today used only by the
  watcher). Today temp files stay hidden only because they are not `.md`.
- `MemFs` stores bytes (`Vec<u8>`). `Fs::read_bytes(path, max_len) -> Result<Vec<u8>>`
  reads a file's bytes on both backends and refuses one over `max_len`. `load` stays its own
  method (see Deviations).
- Names, by invariant: an asset's display name is its full file name
  (`photo.png`), a note's is its stem. `display_name` takes the `Kind`.
  `Kind::file_name(Asset, typed)` and `on_disk_name` keep the typed name,
  with no `.md` appended. Renaming an asset edits the whole name; the walk
  re-derives its kind, so `photo.png` → `photo.md` makes it a note.
- An asset rename keeps the note name rules: `check_name` rejects `#|^[]`,
  which a wikilink cannot spell, and the separators `/` and `:`.
- OS clutter (`Thumbs.db`, `desktop.ini`) lists as assets; only dotfiles and
  our temp files are hidden.
- `Fs::create` and `trash` take notes and folders only. `create` takes a
  `New` (`Note` or `Dir`), so its type cannot carry an asset.
- The walk, the watcher, identity by inode and rename need no other change.
  Each exhaustive `match` on `Kind` gets an explicit `Asset` arm.
- Every `Kind::Note` filter site decides whether assets enter. Known sites:
  `index/note_set.rs:45`, `index/live/diff.rs:41`, `app/restore/plan.rs:165`,
  `app/sidebar/visible.rs:38`, `index/find/model_cases.rs:136`. The plan lists
  each with its answer.
- The sidebar's level runs (`visible.rs:38-42` asserts all-`Note`) hold files:
  the assert becomes `Kind::is_file`.

### Performance gate

A bench vault of 50k entries, 90% assets, measures the walk's sort,
`name_taken` and `Change::between` before and after slice 1. Assets share the
`Entry` table (one walk, one identity map, one rename path); the bench is the
evidence that this is enough. A regression over 2× on any of the three blocks
the slice.

The `Stamps` path is O(entries) per vault change, assets included. This is
accepted, measured at 50k.

## Index (`inkra-index`)

- Assets enter no word index, no corpus, no text search: the index reads no
  asset content.
- A name table keyed by full file name resolves asset links (K6). The lookup
  runs only after the note lookup misses.
- A link written with `.md` never reaches an asset: `Target` keeps whether the
  written name ended in `.md`, and a note miss on such a link skips the asset
  table. So `[[photo.png.md]]` with no note `photo.png.md` stays unresolved.
- The asset lookup is case-folded, as the note lookup is, and takes the same
  three forms: `Name` (`[[photo.png]]`), `Suffix` (`[[pics/photo.png]]`) and
  `Relative` (`[a](photo.png)`).
- Asset backlinks reuse `by_stem`: an asset's stem key is its full name, the
  stem a link to it is filed under.
- Adding, removing or renaming an asset rebuilds the layout, as a note does:
  the asset table is part of it. About 7 ms at 50k entries, 90% assets,
  within the gate.
- `Resolution { File(EntryId), Unresolved, External, SelfLink }`.
- Backlinks of an asset list the notes whose links resolve to it. This removes
  today's defect: `![[photo.png]]` is stored as a note link that never
  resolves and sits in `by_stem` as a backlink candidate.

## App

### Types

```rust
enum FileKind { Note, Image }   // FileKind::of(path) -> Option<FileKind>; one function, with the ImageFormat table
struct OnFile<V> { file: EntryId, view: Option<V> }
enum Place  { Empty, File(OnFile<ViewState>), View(View) }
enum Parked { Empty, File(OnFile<ParkedView>), View(View) }
enum Shown { Empty, Bound(Bound), Image(EntryId), Trashing(EntryId), View(View) }
```

- An image's `OnFile` has `view: None`; a note's is `None` when it opens at
  the top. No `FileView` enum: an image has no view state yet, so a
  `FileView::Image` would carry nothing. `FileView` arrives with zoom and pan
  (see Deferred).
- `FileKind::of` is `Image` when `ImageFormat::from_extension` knows the
  extension and `reading_enabled` holds for it. That reads the `image`
  features the build unifies, gpui's included: our `Cargo.toml` asks for png,
  jpeg, gif, webp, bmp, tiff, ico and pnm, and gpui adds exr, hdr, qoi, tga
  and ff. So the set of image kinds moves with gpui's features. A file of
  such a kind that is too big to draw is refused by the decode's budget (K5)
  and shows "can't show".

- `Bound` is unchanged: the note and its editor. T2 holds: an editor always
  shows a note.
- `inkra_tabs::Place::note` becomes `file`; `type Note` becomes `type File`.
- `NoteActivated` becomes `FileActivated`. `showing()` returns the file shown.
  `NoteStore::current` stays a note: it is what `⌘S` writes; an image clears it.
- `recent` records notes only until `⌘P` lists assets (deferred).
- An asset with no `FileKind` (a PDF) is inert: the tree shows it, a click
  selects the row and opens nothing. Opening with the system app is deferred
  (see Deferred: it can launch a `.command` on one click).

### `Images`

- An app-side store `EntryId → Slot` (loading, `Arc<RenderImage>`, failed), the image twin of
  `NoteStore`'s buffers. `display` reads it synchronously.
- An image place shows at once. A miss draws the empty image area and starts
  one load (`Fs::read_bytes`, then the decode on the background executor, K5); the
  landing notifies. A second miss on the same id joins the running load.
- Eviction: an entry lives while some group shows it. When no `Shown::Image`
  holds the id, the entry drops and gpui's image cache entry is removed (the
  plan names the call after reading `platform.rs:2989`). Going back to an
  image reloads it.
- Restore loads only the images that a group shows; other image tabs load on
  first switch.

### `shell/surface.rs`

The one place that matches on `FileKind`:

| Operation | Note | Image |
|---|---|---|
| `display(place)` | `Shown::Bound`, editor reused or created | `Shown::Image(id)`, editor dropped |
| `capture` | `editor.capture` | `OnFile { view: None }` |
| `park` / `unpark` | anchors (tabs spec, A2) | plain data |
| an open's `Landing` (`show_landed`, `park_landed`) | bind, or park the view's anchors | `show_image`, or park with no view |
| what follows the shown file | `set_current`, backlinks, recent | `clear_current`, backlinks |

- Focus: tab keys (`⌘W`, `⌘[`, `⌘T`) are bound on `AppShell`
  (`keymap.rs:89-100`), so an image needs no focus handle. Showing an image
  focuses the group.
- A tab's title reads the file (its path), never a view; a kind icon is deferred (Deviations).
- The image draws fitted to the group, centred, aspect kept. A decode failure
  shows a one-line message in place of the image.

### Tree and links

- The tree lists assets with an icon per `FileKind`, a generic file icon for
  the rest. A click on an image emits `FileActivated`.
- Following a link with `Resolution::File(id)` calls `AppShell::open(id, ..)`
  for a note or an image; an inert asset opens nothing.

## Session

- `SavedTab::File(SavedFileTab { file, view })`, declared after `View` in the
  untagged enum: a view tab's `view` name is its one required field, and every
  field of a file tab is optional. `file: Option<String>` is vault-relative;
  `None` is an empty tab. `view` is an untagged `Option<SavedView>`, `None`
  for an image. `VERSION` becomes 3; the v1 and v2 decoders are deleted.
- The restore begins one `Opening` per surviving tab, note or image, and joins
  them as an open does. `Plan` decides only which saved tabs survive. A file
  gone from the vault since the plan resolved lands as a failed load does:
  its tab is not restored.

## Failure handling

- `Fs::read_bytes` fails → the image area shows the one-line message; the tab and
  its history keep the file.
- Decode fails → the same message.
- An asset deleted from disk while shown: the scan swap reports removed ids;
  a removed file that no note session holds closes its tabs, as `NoteClosed`
  does. "Closes" means emptied: the active tab shows `Place::Empty` and a
  parked one is forgotten; the tab itself stays. Notes keep today's gone-note
  path (F9).
- A rename across kinds under an open tab changes the kind at the tab's next
  display (K4). `photo.png` renamed to `photo.md` keeps showing the image:
  the tab holds no buffer for it, so it loads the bytes, which still decode.
  The next launch restores `photo.md` as a note: its load fails, since a PNG
  is not UTF-8 (`VaultError::NotUtf8`), and the restore drops the tab as for
  any failed load. A file of valid UTF-8 would open as its text. Accepted: an
  outside rename to the wrong extension is the user's choice.
- An image changed on disk does not reload until it is shown again (deferred).

## Testing

All app tests are `#[gpui::test]` on `MemFs` with a real PNG's bytes.

- The tree lists an asset and hides a leftover temp file.
- A click on a PNG opens it in the tab; a click on a PDF opens nothing.
- `![[photo.png]]` followed opens the image; `⌘[` returns to the note with its
  view restored.
- Switch to an image tab not yet loaded: it shows at once, then lands.
- The `Images` entry drops when the last group stops showing it.
- Split on an image; restore a session with an image tab.
- An asset deleted on disk empties its tab.
- `simulate_keystrokes` for `⌘W`, `⌘[`, `⌘T` with an image shown.
- `inkra-index`: `![[x.png]]`, `[a](sub/x.png)` and an unknown asset resolve
  as expected; `[[Node.js]]` stays a note link; backlinks of an asset; no word
  from an asset.
- `inkra-vault`: listing maps kinds and hides temp files; `Fs::read_bytes` on both
  backends; an asset's display name keeps its extension; a rename to `.md`
  flips the kind.
- A `shot` scene with an image tab, looked at.

## Slices

Each slice is vertical and leaves `./scripts/check.sh` green.

1. **Assets in the vault and the tree.** The bench fixture first, then
   `MemFs` to bytes, `Fs::read_bytes`, `Kind::Asset`, the shared listing predicate,
   names, every `Kind` match and filter site decided, the run assert. The
   tree lists assets, inert, with icons. The index ignores assets. The bench
   gate passes.
2. **Images in tabs.** `FileKind::of`, `Place::File` and the renames
   (`file()`, `FileActivated`), `Shown::Image`, `shell/surface.rs`, `Images`
   with load, eviction and restore-on-show, session v3, the deleted-asset
   path, the keys, the `shot` scene. A click on a PNG opens it in the tab.
3. **Links to files.** The asset name table, note-first resolution,
   `Resolution::File`, following `![[…]]` and `[a](x.png)`, backlinks of an
   asset, the bogus backlink gone.

## Deferred

Each goes to `docs/roadmap.md` with its trigger.

- Images drawn inside the note text (embeds rendered). Trigger: the first
  inline embed request.
- Assets in `⌘P` and in `recent`. Trigger: `⌘P` lists assets.
- Zoom and pan: `FileView { Note(ViewState), Image(zoom, pan) }` replaces
  `OnFile`'s `ViewState`, and its parked twin `ParkedView`.
- Reload of a shown image changed on disk: key `Images` by `(EntryId, Mtime)`.
  Trigger: an external image editor workflow.
- Open with the system app, from a context menu, with an allowlist of
  extensions, behind a recorded seam. Trigger: the first non-image kind users
  ask for.
- Galleries and a bounded image cache (LRU with a byte budget). Trigger: more
  than one image drawn per group.
- Bases: typed frontmatter, a property column in the index, a structured
  query beside `find_notes`, a table view.

## Deviations

- `Fs::read_bytes(path, max_len)` replaces the drafted `Fs::read(path)`, a plan amendment.
  The persisted index reads its cache file through the same method; an image needs the same
  cap (`MAX_IMAGE_BYTES`, 64 MiB), refused from the opened file's size before any read.
- `load` does not decode from `read_bytes`, so a note's load and a byte read are two
  paths. `load` refuses a file over `MAX_TEXT_BYTES` (64 MiB), as `read_bytes` does over
  its `max_len`. It stats the path before reading, so a race errs toward "changed on
  disk". `MemFs` counts a byte read in `reads`, never in `loads`, so a test tells an
  image's read from a note's load.
- No tab draws a `FileKind` icon, against the drafted "a tab's title and icon read the
  file": a tab draws its title and indicator only. Deferred to `docs/roadmap.md`; trigger: a second
  viewable kind lands, or users mix note and image tabs.

## Review outcome

Red found 19 issues; blue verified them. Arbitration:

- Adopted blue's design over the draft's cached surfaces: the draft kept an
  idle editor per group, which needed an unpin lifecycle across six sites
  (`bind`, `workspace.rs:94-98`, `forget_note`, `remove_group`, `on_release`,
  the drop assert). An image needs no entity, and the editor's create arm
  already serves `Place::View`, so recreating it after an image costs what it
  costs today. This removes the draft's K4 and `Surfaces`.
- The switch to an unloaded image (blocker 1) is solved by `Images` plus a
  synchronous show of the empty area; the switch stays synchronous.
- Extension classification (blocker 2) is replaced by note-first lookup (K6).
- System open moved to Deferred: one click could run a `.command`.
- The draft's slice 2 (a refactor with one kind) was not vertical; it is
  merged into the image slice.
- Points left unverified by blue (`RetainAllImageCache`, the scan result field
  carrying removed ids, `recent` and `⌘P` with asset ids) are for the plan to
  check in code before writing against them.
