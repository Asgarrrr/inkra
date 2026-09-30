# Inkra

A note app for macOS, written in Rust on GPUI, the UI framework of the Zed
editor. Your notes stay plain markdown files in a folder
(a vault); Inkra reads and writes them in place.

- Markdown live preview: styled as you type, markup hidden until the caret
  or the selection touches it.
- `[[Wikilinks]]`, a backlinks panel, `⌘Enter` to follow a link.
- `⌘P` quick open, `⌘⇧F` vault search, `⌘⇧P` command palette.
- Tabs with per-tab back and forward, `⌘⇧T` to reopen a closed tab.
- Autosave, conflict detection, and a file watcher that follows edits made
  outside the app.
- No persisted index yet: the vault is scanned and indexed at each launch.

## Run

Requires macOS and Rust 1.96 or later.

```sh
./scripts/vendor-gpui.sh          # once: fetches gpui at the pinned commit
cargo run --release -p inkra-app -- ~/path/to/vault
```

## Performance

Inkra against Obsidian on the same generated vaults, measured from outside
both apps by the same tool. Method, isolation and caveats:
[docs/benchmarks.md](docs/benchmarks.md).

Apple M5 Pro, 48 GB, macOS 26.5. Inkra 0.1.0 (commit `698daa8`), Obsidian
1.12.7, 2026-09-30. Warm rows: median of 5 runs. Memory is Activity Monitor's
figure (`phys_footprint`, summed over every process of the app), in MiB.

```text
              Window on screen, warm (lower is better)
              █ Inkra   ▒ Obsidian

   Inkra 500 ┤████████████████ 159          ·         ·         ·
Obsidian 500 ┤▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒ 424    ·
    Inkra 5k ┤███████████████ 150 ·         ·         ·         ·
 Obsidian 5k ┤▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒ 444  ·
   Inkra 50k ┤███████████████ 154 ·         ·         ·         ·
Obsidian 50k ┤▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒ 426   ·
             └┬─────────┬─────────┬─────────┬─────────┬─────────┬
              0        100       200       300       400       500  ms


              Memory at idle, warm (lower is better)
              █ Inkra   ▒ Obsidian

   Inkra 500 ┤██ 50     ·         ·         ·         ·         ·
Obsidian 500 ┤▒▒▒▒▒▒▒▒▒▒▒ 222     ·         ·         ·         ·
    Inkra 5k ┤█████ 106 ·         ·         ·         ·         ·
 Obsidian 5k ┤▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒ 340         ·         ·         ·
   Inkra 50k ┤███████████████████████ 465   ·         ·         ·
Obsidian 50k ┤▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒ 909 ·
             └┬─────────┬─────────┬─────────┬─────────┬─────────┬
              0        200       400       600       800      1000  MiB


              CPU until idle, first open (lower is better)
              █ Inkra   ▒ Obsidian

   Inkra 500 ┤█ 0.29    ·         ·         ·         ·         ·
Obsidian 500 ┤▒ 2.52    ·         ·         ·         ·         ·
    Inkra 5k ┤█ 1.58    ·         ·         ·         ·         ·
 Obsidian 5k ┤▒▒▒▒ 14.98          ·         ·         ·         ·
   Inkra 50k ┤████ 15.02          ·         ·         ·         ·
Obsidian 50k ┤▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒ 161.43   ·
             └┬─────────┬─────────┬─────────┬─────────┬─────────┬
              0        40        80        120       160       200  s


              CPU until idle, warm (lower is better)
              █ Inkra   ▒ Obsidian

   Inkra 500 ┤█ 0.28    ·         ·         ·         ·         ·
Obsidian 500 ┤▒▒ 0.99   ·         ·         ·         ·         ·
    Inkra 5k ┤████ 1.74 ·         ·         ·         ·         ·
 Obsidian 5k ┤▒▒▒▒ 1.49 ·         ·         ·         ·         ·
   Inkra 50k ┤█████████████████████████████████████████████████ 19.58
Obsidian 50k ┤▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒ 7.87     ·         ·         ·
             └┬─────────┬─────────┬─────────┬─────────┬─────────┬
              0         4         8        12        16        20  s
```

| Notes | App | Open | Window on screen (ms) | Busy until idle (s) | CPU until idle (s) | Memory at idle (MiB) | Processes |
|---:|---|---|---:|---:|---:|---:|---:|
| 500 | Inkra | warm | **159** | **0.3** | **0.28** | **50** | 1 |
| 500 | Obsidian | warm | 424 | 1.3 | 0.99 | 222 | 4 |
| 5,000 | Inkra | warm | **150** | **0.3** | 1.74 | **106** | 1 |
| 5,000 | Obsidian | warm | 444 | 1.3 | 1.49 | 340 | 4 |
| 50,000 | Inkra | warm | **154** | **1.6** | 19.58 | **465** | 1 |
| 50,000 | Obsidian | warm | 426 | 13 | 7.87 | 909 | 4 |
| 50,000 | Inkra | first open | **141** | **1.6** | **15.02** | **462** | 1 |
| 50,000 | Obsidian | first open | 476 | 64.3 | 161.43 | 881 | 4 |

Install size: Inkra 12 MB (release binary, no `.app` bundle yet), Obsidian
482 MB (`.app`).

Read these figures with their limits:

- One machine, Obsidian with default settings and no plugins (automatic
  updates off).
- "Window on screen" is not "note readable" for either app.
- Inkra re-indexes the vault at every launch, so its warm runs do the work
  of a first open: at 50,000 notes it spends more CPU at launch than
  Obsidian, which caches its index.
- Idle CPU is left out here: it depends on whether the window was covered.
  See the full table in `docs/benchmarks.md`.

Reproduce with `./scripts/bench-compare.sh` (quit Obsidian first).

## Development

`./scripts/check.sh` runs the build, tests, clippy, fmt and `cargo deny`; green
is the definition of done. Architecture and conventions are in
[CLAUDE.md](CLAUDE.md), design specs in `docs/superpowers/specs/`.
