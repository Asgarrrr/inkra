# Inkra and Obsidian, measured

A reproducible comparison of Inkra with Obsidian on the same generated
vaults, on one machine. Every figure is taken from outside both apps, with
the same tool and the same rules. The raw data, the tool and the generator
are in this repository, so anyone can run it again or contest it.

## Reproduce

```sh
./scripts/bench-compare.sh      # all sizes, 5 warm runs each
```

Flags: `--runs N` (default 5), `--sizes 500,5000,50000`,
`--apps inkra,obsidian`, `--cap-first S` (default 600), `--cap-warm S`
(default 180), `--cold` (`sudo -n purge` before every run; `bench-refresh.sh`
keeps `sudo` valid for it). The script needs Obsidian in `/Applications/Obsidian.app`, and
refuses to start while any Obsidian runs.

It writes the raw runs to `target/bench/results-<timestamp>.jsonl`, one JSON
line per run after one `meta` line, and the summary to
`target/bench/summary-<timestamp>.md` and stdout: bar charts of the main
figures, then the full table. Each app's stderr goes to
`target/bench/logs-<timestamp>/`.

`scripts/bench-refresh.sh` runs the Results tables and the audit A/B in one
go. It runs the launch A/B warm and cold, then `bench-compare.sh` warm and with
`--cold`. Run it from a terminal: `sudo` asks for the password once, for
`purge`. It refuses to start while Zed, Obsidian, cargo or rustc run. It writes
everything to `target/bench/refresh-<timestamp>/`.

## The vaults

`crates/app/examples/bench_vault.rs` writes `$TMPDIR/inkra-bench-<notes>` from
a fixed LCG seed, so every run and every machine gets the same bytes. It
writes the vault again only when its parameters change.

- 500, 5,000 or 50,000 notes. One folder per 50 notes, at most 100, two levels
  deep (`Projects/Planning`): 10 folders at 500 notes, 100 at 5,000 and above.
- Each note is 0.5 to 5 KB: a `#` title in two notes of three, then `##`
  headings, paragraphs, bullet and numbered lists, `- [ ]`/`- [x]` tasks, and
  one fenced code block in twenty blocks.
- Inline: `**bold**`, `*italic*`, a few `#tags`, and about 6 `[[wikilinks]]`
  per note, a quarter of them with an `|alias`. Every link names an existing
  note, half of them in the same folder.
- One note in five starts with YAML frontmatter (`created`, `tags`).
- Words: half from 100 common English words, half from a long tail of 20,000
  generated words, so the vocabulary grows like real prose.
- Names are unique: a few words and the note's number (`Budget review 412`).

## Metrics

`scripts/bench/measure.swift` launches the app and watches its process tree
from outside: the spawned process and all its descendants, listed again at
every sample with `proc_listchildpids`, because Electron starts its helper
processes late. Neither app is instrumented.

| Metric | Definition |
|---|---|
| Window on screen | ms from spawn until the WindowServer lists an on-screen, layer-0 window owned by a process of the tree (`CGWindowListCopyWindowInfo`, polled every 1 ms) |
| Busy until idle | ms from spawn to the start of the quiet window: the tree's CPU, as a mean over the last 1 s, stays below 2 % of one core for 3 s in a row |
| CPU until idle | the tree's total CPU time, in s, from spawn to the start of the quiet window: the work the launch did |
| Memory at idle | the sum of `ri_phys_footprint` over the tree at the end of the quiet window |
| Resident | the sum of resident size over the tree at the same moment, for reference |
| Processes | the size of the tree at the same moment |
| Idle CPU | the tree's mean CPU, in % of one core, over the 15 s after the quiet window |

- CPU comes from `proc_pid_rusage` (`RUSAGE_INFO_V4`), user plus system time,
  sampled every 250 ms. A process first seen at a sample counts all of its
  CPU time, since it started after the launch.
- The quiet test takes the mean over the last 1 s at every sample, not one
  250 ms sample, so a short burst does not reset it: breaking a quiet window
  takes 20 ms of CPU inside one second. The quiet window starts where the
  first quiet 1 s mean starts, and it lasts 4 s: that second and the 3 s it
  holds. Busy time has the sample's resolution, 250 ms.
- The quiet window can start only after the window is on screen, so a launch
  that stalls before its window never reads as idle.
- Busy time has a cap: 600 s on a first open, where Obsidian may index 50,000
  notes from nothing, and 180 s on a warm run. A run that reaches it reads
  `timeout`, and its CPU time, memory and idle CPU stay empty.
- `phys_footprint` is what Activity Monitor shows as "Memory": dirty and
  compressed memory the process owns, not shared pages from system
  frameworks. Resident size counts shared pages once per process, so it
  inflates a tree of many processes. The tree matters: Obsidian runs as
  several processes (4 in the 500-note smoke run), so only the tree's sum
  compares with Inkra's one process. MB means 2^20 bytes, as in Activity
  Monitor.
- At the end, `measure` sends SIGTERM to the spawned process, then to any
  process of the tree still alive after 3 s, then SIGKILL after 3 s more. It
  never signals a process outside the tree it started.

## Runs

For each vault size, the apps take turns, so both see the same machine state:

1. **First open**, once per app. Inkra gets an empty `HOME`, so it has no
   session file. Obsidian gets an empty profile and the vault's `.obsidian/`
   is moved aside, so it builds its cache from nothing.
2. **Warm**, N runs per app, interleaved, each reusing the profile or `HOME`
   of the first open. The table shows the median.

The script sleeps 5 s between runs. The vault's files are in the disk cache
for every run, first open included: the generator has just read or written
them. First open means no app state, not a cold disk.

With `--cold`, the script runs `sudo -n purge` before every run, first open
included. Each run then reads the vault and the app from disk, and probably
most of its system frameworks.

Before the first run, the script launches the fresh Inkra build once, untimed,
on an empty vault. The first exec of a new binary pays the code-signature
check (`docs/startup-floor.md`): an install cost, paid once, not a first-open
cost.

## Isolation

- Obsidian runs as `Obsidian --user-data-dir=<profile>`, with a profile under
  `$TMPDIR/inkra-bench-profiles/`. Its `obsidian.json` lists only the
  benchmark vault. Your own Obsidian profile and vaults are never read or
  written.
- The profile sets `updateDisabled: true`, the "Automatic updates" switch.
  Without it, a fresh profile can download a newer Obsidian mid-benchmark, and
  the warm runs would measure another version. It is the one setting changed
  from the defaults.
- Inkra runs with `HOME` set to a directory under the same root, so its
  session file never touches your own, and no tabs carry over from your use.
- The script refuses to start, and checks again before each Obsidian run,
  while any Obsidian runs: it would never stop a process it did not start,
  and a second Obsidian would skew the figures.

## Caveats

- **Both apps persist their index.** Since the persisted index
  (2026-11-07), a warm Inkra run decodes its cache file and reads only the
  notes whose stamp changed; Obsidian reuses the metadata cache in its
  profile. Results before 2026-11-07 show Inkra rebuilding its index on every
  launch.
- **"Window on screen" is not "content visible"** for either app. Inkra
  shows its window before its scan ends (the open-first spec), and Obsidian
  probably shows its window before its workspace loads. Neither figure says
  when a note can be read.
- **Busy until idle is a threshold, not a total.** Work that stays below 2 %
  of one core over each second, however long it runs, counts as idle; a burst
  above it a few seconds late moves the figure by seconds. CPU until idle is
  the steadier figure for the work a launch does. Read both with the per-run
  values in the raw JSON.
- **Obsidian runs with its defaults and no community plugins**, apart from
  automatic updates. A real Obsidian setup with plugins probably starts slower and uses
  more memory; this comparison does not show that.
- **One machine.** The figures hold for the machine in the table's header and
  nothing else. Run-to-run noise on the window time is about ±10 ms
  (`docs/startup-floor.md`).
- **Inkra is not yet a signed `.app` bundle.** Its install size is the release
  binary alone; a bundle would add an icon, a plist and a signature. Obsidian's
  install size is its whole `.app`.
- **The vaults are synthetic.** Their size, structure and markup are
  realistic; their words are not. Search and index sizes depend on the
  vocabulary, so a real vault can differ.

## Results

Full run on 2026-11-08, commit `fb0405bb` (main `68e42bd2` plus the benchmark
scripts), Apple M5 Pro, 48 GB, macOS 26.5 (25F71), on AC power, Zed and
Obsidian closed. Inkra 0.1.0 (release binary, 17 MB, no `.app` bundle yet),
Obsidian 1.14.5 (530 MB `.app`). Warm rows are medians of 5 runs. Memory is in
MiB. Raw lines: `target/bench/refresh-20261108-103637/results-{warm,cold}.jsonl`
(not committed).

### Disk cache warm

```text
Window on screen, warm (lower is better)
   500  inkra     ███████████████ 112 ms
   500  obsidian  ████████████████████████████████████████ 296 ms
  5000  inkra     ███████████████ 112 ms
  5000  obsidian  ████████████████████████████████████████ 295 ms
 50000  inkra     ███████████████ 114 ms
 50000  obsidian  ████████████████████████████████████████ 294 ms

CPU until idle, warm (lower is better)
   500  inkra     █ 0.11 s
   500  obsidian  ██████ 0.75 s
  5000  inkra     █ 0.17 s
  5000  obsidian  ████████ 1.07 s
 50000  inkra     ██████ 0.75 s
 50000  obsidian  ████████████████████████████████████████ 5.42 s

CPU until idle, first open (lower is better)
   500  inkra     ▏ 0.17 s
   500  obsidian  █ 1.84 s
  5000  inkra     ▏ 0.78 s
  5000  obsidian  ████ 9.92 s
 50000  inkra     ████ 11.52 s
 50000  obsidian  ████████████████████████████████████████ 105.77 s

Memory at idle, warm (lower is better)
   500  inkra     ██ 52 MiB
   500  obsidian  ██████████ 228 MiB
  5000  inkra     ████ 82 MiB
  5000  obsidian  ████████████████ 372 MiB
 50000  inkra     ███████████ 261 MiB
 50000  obsidian  ████████████████████████████████████████ 924 MiB
```

| Notes | App | Open | Window on screen (ms) | Busy until idle (s) | CPU until idle (s) | Memory at idle (MiB) | Resident (MiB) | Processes | Idle CPU (%) |
|---:|---|---|---:|---:|---:|---:|---:|---:|---:|
| 500 | Inkra | first | 117 | 0.3 | 0.17 | 60 | 84 | 1 | 0.29 |
| 500 | Inkra | warm | 112 | 0.3 | 0.11 | 52 | 76 | 1 | 0.3 |
| 500 | Obsidian | first | 648 | 1.6 | 1.84 | 309 | 607 | 4 | 0.2 |
| 500 | Obsidian | warm | 296 | 1.1 | 0.75 | 228 | 521 | 4 | 0.06 |
| 5,000 | Inkra | first | 84 | 0.3 | 0.78 | 96 | 145 | 1 | 0.31 |
| 5,000 | Inkra | warm | 112 | 0.3 | 0.17 | 82 | 106 | 1 | 0.29 |
| 5,000 | Obsidian | first | 320 | 9.4 | 9.92 | 499 | 867 | 4 | 0.29 |
| 5,000 | Obsidian | warm | 295 | 1.1 | 1.07 | 372 | 681 | 4 | 0.05 |
| 50,000 | Inkra | first | 92 | 1.1 | 11.52 | 546 | 636 | 1 | 0.32 |
| 50,000 | Inkra | warm | 114 | 0.5 | 0.75 | 261 | 306 | 1 | 0.28 |
| 50,000 | Obsidian | first | 322 | 41.9 | 105.77 | 1393 | 1979 | 4 | 9.11 |
| 50,000 | Obsidian | warm | 294 | 10.3 | 5.42 | 924 | 1463 | 4 | 0.04 |

### Disk cache cold

`--cold`: `sudo purge` before every run. "Warm" still means a second open, with the app's own
state; only the disk cache is cold.

```text
Window on screen, warm (lower is better)
   500  inkra     ████████████████ 527 ms
   500  obsidian  ████████████████████████████████████████ 1317 ms
  5000  inkra     ████████████████ 529 ms
  5000  obsidian  █████████████████████████████████████ 1236 ms
 50000  inkra     ████████████████ 540 ms
 50000  obsidian  ████████████████████████████████████████ 1323 ms

CPU until idle, warm (lower is better)
   500  inkra     ██ 0.33 s
   500  obsidian  ████████ 1.15 s
  5000  inkra     ███ 0.39 s
  5000  obsidian  ███████████ 1.48 s
 50000  inkra     ███████ 0.98 s
 50000  obsidian  ████████████████████████████████████████ 5.41 s

CPU until idle, first open (lower is better)
   500  inkra     ▏ 0.4 s
   500  obsidian  ▏ 2.12 s
  5000  inkra     ▏ 1.3 s
  5000  obsidian  ██ 10.78 s
 50000  inkra     ██ 9.56 s
 50000  obsidian  ████████████████████████████████████████ 236.43 s

Memory at idle, warm (lower is better)
   500  inkra     ██ 52 MiB
   500  obsidian  █████████ 228 MiB
  5000  inkra     ███ 82 MiB
  5000  obsidian  ██████████████ 374 MiB
 50000  inkra     ██████████ 261 MiB
 50000  obsidian  ████████████████████████████████████████ 1069 MiB
```

| Notes | App | Open | Window on screen (ms) | Busy until idle (s) | CPU until idle (s) | Memory at idle (MiB) | Resident (MiB) | Processes | Idle CPU (%) |
|---:|---|---|---:|---:|---:|---:|---:|---:|---:|
| 500 | Inkra | first | 504 | 0.8 | 0.4 | 61 | 85 | 1 | 0.28 |
| 500 | Inkra | warm | 527 | 0.8 | 0.33 | 52 | 76 | 1 | 0.29 |
| 500 | Obsidian | first | 1339 | 2.1 | 2.12 | 304 | 603 | 4 | 0.27 |
| 500 | Obsidian | warm | 1317 | 2 | 1.15 | 228 | 525 | 4 | 0.06 |
| 5,000 | Inkra | first | 529 | 2.6 | 1.3 | 118 | 142 | 1 | 0.3 |
| 5,000 | Inkra | warm | 529 | 0.8 | 0.39 | 82 | 106 | 1 | 0.29 |
| 5,000 | Obsidian | first | 1188 | 11.1 | 10.78 | 499 | 866 | 4 | 0.59 |
| 5,000 | Obsidian | warm | 1236 | 2 | 1.48 | 374 | 683 | 4 | 0.06 |
| 50,000 | Inkra | first | 551 | 2.4 | 9.56 | 516 | 602 | 1 | 7.5 |
| 50,000 | Inkra | warm | 540 | 1 | 0.98 | 261 | 306 | 1 | 0.29 |
| 50,000 | Obsidian | first | 1296 | 161.8 | 236.43 | 960 | 1620 | 4 | 0.09 |
| 50,000 | Obsidian | warm | 1323 | 8.1 | 5.41 | 1069 | 1501 | 4 | 0.82 |

What the run shows, and what it does not:

- **Window on screen:** with a warm disk cache, Inkra 84–117 ms and Obsidian
  294–648 ms over every row. Cold, Inkra 504–551 ms and Obsidian 1,188–1,339 ms. The cold
  figure is mostly the app and its frameworks read from disk: it barely moves
  with the vault's size.
- **Memory at idle, warm:** Inkra uses 4.4× less at 500 notes, 4.5× less at
  5,000, 3.5× less at 50,000 (261 against 924 MiB).
- **Work at launch, warm:** at 50,000 notes Inkra spends 0.75 CPU-seconds
  (0.5 s busy) against Obsidian's 5.42 (10.3 s busy). Cold, 0.98 against 5.41.
- **First open:** at 50,000 notes Obsidian is busy for 41.9 s and 105.8
  CPU-seconds warm, 161.8 s and 236.4 cold; Inkra for 1.1 s and 11.5 warm,
  2.4 s and 9.6 cold.
- **Idle CPU:** gpui runs its display link on every vsync while the window is
  visible, about 0.3 % of a core, which is what most Inkra rows read. The cold
  50,000-note first open idles at 7.5 % after its quiet window. It is a single
  run, and the run does not explain it.
- **Against 2026-09-30** (commit `698daa8`, before the persisted index), Inkra
  at 50,000 notes, warm: CPU until idle 19.58 → 0.75 s, busy 1.6 → 0.5 s,
  memory at idle 465 → 261 MiB, window on screen 154 → 114 ms. Obsidian moved
  from 1.12.7 to 1.14.5 in between: its warm window went from 426 to 294 ms,
  its memory went from 909 to 924 MiB.

## Persisted index baseline (2026-11-06)

What the persisted index (docs/superpowers/specs/2026-11-06-persisted-index-design.md) is gated
against. Release build of `3b6b64ae` plus the slice-0 log line, Apple M5 Pro, 48 GB, macOS 26.5.

Warm launch, 50,000-note `startup_vault` vault, 5 runs. The command was
`scripts/launch-figure.sh --home "$TMPDIR/inkra-home" target/release/inkra-app "$V" 5`, with
`V="$TMPDIR/inkra-startup-50k"`, the vault `cargo run -p inkra-app --release --example startup_vault`
writes. The first run is the slowest, likely a cooler page cache.

`phys_footprint` splits into two groups: 309.4 and 310.9 MB, then 399.2 to 407.5 MB. Its median is
399.2 MB, its spread 309.4 to 407.5 MB. The memory gate compares the medians of 5 runs, rss and
`phys_footprint` each, as slice 2d does.

```
launch: first frame 115.9ms, session read 28.50µs, root 143.8ms, scanned 1.0s, names ready 1.1s, indexed 2.08s (50000 notes, corpus 158045432 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 1.9ms) | rss 340160 kB | phys_footprint 399213528 B
launch: first frame 86.3ms, session read 129.96µs, root 111.2ms, scanned 202.6ms, names ready 218.6ms, indexed 1.09s (50000 notes, corpus 158045432 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 1.8ms) | rss 342672 kB | phys_footprint 309363648 B
launch: first frame 84.2ms, session read 74.54µs, root 92.1ms, scanned 183.3ms, names ready 198.0ms, indexed 1.13s (50000 notes, corpus 158045432 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 1.8ms) | rss 344192 kB | phys_footprint 403276760 B
launch: first frame 77.8ms, session read 82.29µs, root 103.1ms, scanned 211.5ms, names ready 227.1ms, indexed 1.04s (50000 notes, corpus 158045432 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 3.2ms) | rss 344176 kB | phys_footprint 310903744 B
launch: first frame 81.9ms, session read 73.58µs, root 106.5ms, scanned 197.8ms, names ready 213.4ms, indexed 1.30s (50000 notes, corpus 158045432 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 2.3ms) | rss 348288 kB | phys_footprint 407454656 B
```

`⌘⇧F` with the whole corpus resident, `cargo bench -p inkra-index --bench read_search`, on the
`bench_vault -- 50000` vault read from disk with `RealFs`. The term `penrara` is picked as a whole
token in 1,000 notes. The search matches word starts, so it finds more: it stops at the 1,000-hit
limit after 988 notes, as the bench prints. The bench runs the search serial, on one thread. Resident
median 8.19 ms (95% interval 8.14 to 8.79 ms, 10 samples). The slice 2d gate is this figure plus
50 ms.

Slice 2a adds `read_search/read4`: the candidates, then 4 threads that each read a contiguous
quarter through `RealFs::load`, then `TextSearch` over the reads in order. Same vault and term,
`--quick`: resident 7.72 ms (7.71 to 7.72 ms), `read4` 14.43 ms (14.35 to 14.73 ms). Reading the
candidates costs about 6.7 ms over the resident search, inside the 50 ms gate.

Slice 2d removes the corpus text; both gates hold. Same machine, vault and commands. Warm launch,
5 runs, medians: rss 175,008 kB (slice 0: 344,176 kB, 169 MB less), `phys_footprint` 229.1 MB
(slice 0: 399.2 MB, 170 MB less). The corpus line drops from 158,045,432 to 2,200,000 bytes.

```
launch: first frame 126.9ms, session read 603.33µs, root 174.3ms, scanned 636.3ms, names ready 651.9ms, indexed 1.57s (50000 notes, corpus 2200000 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 1.8ms) | rss 175008 kB | phys_footprint 229147464 B
launch: first frame 109.6ms, session read 59.75µs, root 138.1ms, scanned 226.4ms, names ready 241.5ms, indexed 987.54ms (50000 notes, corpus 2200000 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 1.8ms) | rss 176816 kB | phys_footprint 232129352 B
launch: first frame 92.0ms, session read 82.67µs, root 122.7ms, scanned 215.9ms, names ready 232.3ms, indexed 977.25ms (50000 notes, corpus 2200000 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 1.8ms) | rss 173824 kB | phys_footprint 228950832 B
launch: first frame 87.3ms, session read 71.71µs, root 116.7ms, scanned 206.6ms, names ready 223.0ms, indexed 1.02s (50000 notes, corpus 2200000 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 1.8ms) | rss 172496 kB | phys_footprint 135430960 B
launch: first frame 97.1ms, session read 91.17µs, root 127.9ms, scanned 217.5ms, names ready 233.3ms, indexed 983.90ms (50000 notes, corpus 2200000 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 1.7ms) | rss 175536 kB | phys_footprint 230769456 B
```

`read_search --quick`, where `resident` now searches texts held by the bench, not by the index:
resident 7.44 ms (7.44 to 7.47 ms), `read4` 14.31 ms (14.29 to 14.31 ms), under the 57.72 ms gate.

Cold figures (`--cold`, `sudo -n purge` before each run) are in "Architecture audit A/B
(2026-11-08)".

Slice 3c adds `cargo bench -p inkra-index --bench persist -- --quick`: a synthetic index of 50,000
notes of 1 to 5 KB, one link each, words merged; an 8,709,437-byte file. `persist/decode` median
10.39 ms (10.36 to 10.49 ms), under the 60 ms gate; `persist/encode` 13.43 ms (13.33 to 13.81 ms).

Slice 5a-2 writes the file, so the 4b gates are measured here. Same machine, vault and
`launch-figure.sh`, with `--home /tmp/inkra-persist-home`; a first 20 s launch wrote the one
6,921,909-byte `.idx` file. Warm launch, 5 runs: every line has `cache decoded`, `indexed` 230.6 to
270.9 ms (gate 0.35 s), seed turn 1.59 to 1.76 ms (gate 8 ms).

```
launch: first frame 82.9ms, session read 115.71µs, root 113.6ms, scanned 201.8ms, names ready 230.6ms, cache decoded 89.9ms, seed turn 1.59ms, indexed 230.63ms (50000 notes, corpus 3000000 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 1.6ms) | rss 161360 kB | phys_footprint 124617520 B
launch: first frame 121.6ms, session read 64.17µs, root 149.3ms, scanned 240.3ms, names ready 270.9ms, cache decoded 148.3ms, seed turn 1.76ms, indexed 270.92ms (50000 notes, corpus 3000000 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 1.8ms) | rss 163360 kB | phys_footprint 218317640 B
launch: first frame 110.8ms, session read 70.62µs, root 142.5ms, scanned 231.0ms, names ready 260.2ms, cache decoded 118.2ms, seed turn 1.70ms, indexed 260.25ms (50000 notes, corpus 3000000 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 1.7ms) | rss 162768 kB | phys_footprint 217613128 B
launch: first frame 106.2ms, session read 68.62µs, root 135.8ms, scanned 223.3ms, names ready 252.1ms, cache decoded 134.8ms, seed turn 1.69ms, indexed 252.07ms (50000 notes, corpus 3000000 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 1.7ms) | rss 162896 kB | phys_footprint 125584200 B
launch: first frame 106.9ms, session read 66.79µs, root 137.8ms, scanned 224.4ms, names ready 252.9ms, cache decoded 136.4ms, seed turn 1.60ms, indexed 252.94ms (50000 notes, corpus 3000000 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 1.6ms) | rss 162160 kB | phys_footprint 217088816 B
```

Slice 5d adds the scrub, so this is the final check against the slice-0 baseline. Same machine,
vault and command as 5a-2, on the file that launch wrote; no run rejected it, and the file kept its
01:19 stamp. Warm launch, 5 runs, medians against slice 0: `indexed` 306 ms (1.13 s), rss 164,512 kB
(344,176 kB), `phys_footprint` 217.7 MB (399.2 MB). Every line has `cache decoded`, 95.5 to
117.2 ms; seed turn 1.66 to 2.01 ms (gate 8 ms). The first run's scan took 1.2 s, likely a page
cache cooled by the `cargo mutants` runs before it, so its `indexed` is 1.20 s; the other four are
231 to 319 ms (gate 0.35 s). The scrub's 64 reads run after `indexed`, so no line includes them.

```
launch: first frame 86.6ms, session read 777.88µs, root 126.7ms, scanned 1.2s, names ready 1.2s, cache decoded 114.7ms, seed turn 1.89ms, indexed 1.20s (50000 notes, corpus 3000000 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 1.9ms) | rss 166304 kB | phys_footprint 221102920 B
launch: first frame 88.3ms, session read 75.29µs, root 118.8ms, scanned 286.3ms, names ready 319.1ms, cache decoded 95.5ms, seed turn 1.87ms, indexed 319.07ms (50000 notes, corpus 3000000 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 1.9ms) | rss 162704 kB | phys_footprint 217695072 B
launch: first frame 77.6ms, session read 85.12µs, root 107.5ms, scanned 199.4ms, names ready 231.7ms, cache decoded 106.3ms, seed turn 2.01ms, indexed 231.66ms (50000 notes, corpus 3000000 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 2.0ms) | rss 163856 kB | phys_footprint 126616392 B
launch: first frame 87.3ms, session read 82.96µs, root 118.4ms, scanned 272.3ms, names ready 306.2ms, cache decoded 117.2ms, seed turn 1.86ms, indexed 306.22ms (50000 notes, corpus 3000000 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 1.9ms) | rss 164512 kB | phys_footprint 128271176 B
launch: first frame 79.1ms, session read 84.38µs, root 106.6ms, scanned 198.7ms, names ready 231.1ms, cache decoded 105.7ms, seed turn 1.66ms, indexed 231.07ms (50000 notes, corpus 3000000 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 1.7ms) | rss 165824 kB | phys_footprint 220545864 B
```

The file is 6,921,909 bytes at 50,000 notes. A write costs the encode plus the durable replace.
`persist/encode --quick` takes 9.87 ms (9.82 to 9.88 ms) on the bench's synthetic 8.7 MB index. No
log line times the replace, so a throwaway probe outside the app wrote the file's bytes to a temp,
called `F_FULLFSYNC` and renamed it, 7 times: 6.0 to 20.6 ms, median 9.9 ms. A write is about
20 ms, off the foreground. Cold figures are in the next section.

## Architecture audit A/B (2026-11-08)

The architecture audit (#165–#179) was meant to cost nothing on the launch path, and this checks
it. #171 (one sealed cache buffer) and #177 (seed sets as bitsets) touch the decode and the seed.
The A/B compares `4a191391`, the persisted index before the audit, with `fb0405bb`. `fb0405bb` is
main `68e42bd2` plus the benchmark scripts; `68e42bd2` also adds code blocks, file kinds and
horizontal scroll. Same machine as Results, same run.

`scripts/bench-refresh.sh` primes one `HOME` per build with a first launch that writes the cache
file. It then runs `scripts/launch-figure.sh` on the 50,000-note `startup_vault` vault, alternating
the builds, 5 runs each. Cold runs `sudo purge` first.

| Median of 5 | warm, before | warm, after | cold, before | cold, after |
|---|---:|---:|---:|---:|
| first frame | 85.0 ms | 83.6 ms | 547.4 ms | 551.3 ms |
| cache decoded | 100.0 ms | 106.6 ms | 558.9 ms | 652.2 ms |
| seed turn | 2.44 ms | 1.88 ms | 1.83 ms | 1.77 ms |
| indexed | 195.7 ms | 192.2 ms | 686.6 ms | 739.4 ms |
| rss | 155,632 kB | 158,080 kB | 159,648 kB | 158,080 kB |

Warm, no difference stands above the noise. Cold, `indexed` runs fall in two groups for both
builds: 662 to 690 ms and 739 to 774 ms. Three after runs fell in the slower group, against two
before runs, so the cold medians differ by one run.
`phys_footprint` is bimodal as in the slices above (112 to 119 MB or 204 to 232 MB), so its median
is not compared. Cold costs about 460 ms before the first frame, probably the app and its
frameworks read from disk. From the first frame to `indexed`, cold adds about 30 ms over warm
before and about 80 ms after. Within each group the after build is no slower, so the group split
accounts for this 50 ms difference.

Warm:

```
before launch: first frame 75.3ms, session read 121.29µs, root 100.8ms, scanned 183.3ms, names ready 212.5ms, cache decoded 100.0ms, seed turn 2.52ms, indexed 212.50ms (50000 notes, corpus 3000000 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 2.5ms) | rss 155328 kB | phys_footprint 205374280 B
after launch: first frame 83.5ms, session read 120.17µs, root 107.5ms, scanned 168.0ms, names ready 190.7ms, cache decoded 106.6ms, seed turn 1.88ms, indexed 190.70ms (50000 notes, corpus 3000000 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 1.9ms) | rss 158080 kB | phys_footprint 210797384 B
before launch: first frame 85.0ms, session read 97.50µs, root 106.7ms, scanned 168.0ms, names ready 195.7ms, cache decoded 105.9ms, seed turn 1.62ms, indexed 195.72ms (50000 notes, corpus 3000000 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 1.6ms) | rss 155536 kB | phys_footprint 115360584 B
after launch: first frame 79.9ms, session read 103.54µs, root 101.7ms, scanned 161.9ms, names ready 184.1ms, cache decoded 100.9ms, seed turn 2.47ms, indexed 184.12ms (50000 notes, corpus 3000000 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 2.5ms) | rss 153984 kB | phys_footprint 203932488 B
before launch: first frame 81.9ms, session read 118.50µs, root 104.3ms, scanned 165.6ms, names ready 193.3ms, cache decoded 103.1ms, seed turn 2.43ms, indexed 193.31ms (50000 notes, corpus 3000000 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 2.4ms) | rss 156192 kB | phys_footprint 204833608 B
after launch: first frame 83.6ms, session read 94.04µs, root 108.7ms, scanned 170.8ms, names ready 193.4ms, cache decoded 107.8ms, seed turn 1.82ms, indexed 193.37ms (50000 notes, corpus 3000000 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 1.8ms) | rss 158272 kB | phys_footprint 211239752 B
before launch: first frame 86.7ms, session read 97.25µs, root 93.6ms, scanned 154.9ms, names ready 182.8ms, cache decoded 92.7ms, seed turn 2.44ms, indexed 182.83ms (50000 notes, corpus 3000000 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 2.4ms) | rss 155888 kB | phys_footprint 111821640 B
after launch: first frame 87.2ms, session read 96.17µs, root 109.6ms, scanned 169.3ms, names ready 192.2ms, cache decoded 93.0ms, seed turn 2.81ms, indexed 192.17ms (50000 notes, corpus 3000000 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 2.8ms) | rss 153968 kB | phys_footprint 203653960 B
before launch: first frame 85.6ms, session read 96.79µs, root 107.6ms, scanned 169.0ms, names ready 196.7ms, cache decoded 91.3ms, seed turn 2.46ms, indexed 196.69ms (50000 notes, corpus 3000000 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 2.5ms) | rss 155632 kB | phys_footprint 205833032 B
after launch: first frame 88.2ms, session read 87.46µs, root 115.3ms, scanned 176.0ms, names ready 197.8ms, cache decoded 114.0ms, seed turn 1.59ms, indexed 197.76ms (50000 notes, corpus 3000000 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 1.6ms) | rss 158624 kB | phys_footprint 232145736 B
```

Cold:

```
before launch: first frame 547.4ms, session read 647.08µs, root 559.8ms, scanned 656.2ms, names ready 686.6ms, cache decoded 558.9ms, seed turn 1.96ms, indexed 686.58ms (50000 notes, corpus 3000000 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 2.0ms) | rss 159488 kB | phys_footprint 211272520 B
after launch: first frame 534.0ms, session read 1.09ms, root 545.7ms, scanned 636.1ms, names ready 661.7ms, cache decoded 544.6ms, seed turn 1.87ms, indexed 661.66ms (50000 notes, corpus 3000000 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 1.9ms) | rss 158080 kB | phys_footprint 210305864 B
before launch: first frame 547.4ms, session read 596.58µs, root 559.1ms, scanned 654.8ms, names ready 684.7ms, cache decoded 558.2ms, seed turn 2.78ms, indexed 684.70ms (50000 notes, corpus 3000000 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 2.8ms) | rss 155808 kB | phys_footprint 205112136 B
after launch: first frame 575.5ms, session read 1.10ms, root 679.1ms, scanned 741.3ms, names ready 764.6ms, cache decoded 677.3ms, seed turn 1.88ms, indexed 764.64ms (50000 notes, corpus 3000000 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 1.9ms) | rss 158336 kB | phys_footprint 118965088 B
before launch: first frame 539.1ms, session read 610.46µs, root 634.4ms, scanned 717.4ms, names ready 747.5ms, cache decoded 633.0ms, seed turn 1.83ms, indexed 747.52ms (50000 notes, corpus 3000000 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 1.8ms) | rss 160160 kB | phys_footprint 119259976 B
after launch: first frame 561.1ms, session read 1.02ms, root 659.7ms, scanned 724.5ms, names ready 746.9ms, cache decoded 657.7ms, seed turn 1.74ms, indexed 746.90ms (50000 notes, corpus 3000000 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 1.7ms) | rss 157872 kB | phys_footprint 210813768 B
before launch: first frame 539.3ms, session read 602.79µs, root 551.2ms, scanned 644.1ms, names ready 674.4ms, cache decoded 550.2ms, seed turn 1.76ms, indexed 674.40ms (50000 notes, corpus 3000000 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 1.8ms) | rss 159648 kB | phys_footprint 211288904 B
after launch: first frame 551.3ms, session read 866.04µs, root 653.8ms, scanned 716.2ms, names ready 739.4ms, cache decoded 652.2ms, seed turn 1.77ms, indexed 739.43ms (50000 notes, corpus 3000000 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 1.8ms) | rss 158400 kB | phys_footprint 211632968 B
before launch: first frame 580.3ms, session read 1.05ms, root 683.9ms, scanned 745.0ms, names ready 774.2ms, cache decoded 682.9ms, seed turn 1.70ms, indexed 774.23ms (50000 notes, corpus 3000000 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 1.7ms) | rss 159696 kB | phys_footprint 211993416 B
after launch: first frame 510.4ms, session read 920.42µs, root 605.3ms, scanned 666.7ms, names ready 689.5ms, cache decoded 603.3ms, seed turn 1.76ms, indexed 689.54ms (50000 notes, corpus 3000000 bytes, words 4604398 bytes, names 2233894 bytes, longest turn 1.8ms) | rss 156656 kB | phys_footprint 210633544 B
```

## Backlink context on demand, slice 2b (2026-11-11)

Slice 2b of `docs/superpowers/specs/2026-11-08-backlink-context-on-demand-design.md` drops the
context from each link and moves the cache file to version 3. The heap, turn and cache rows
compare `8ee6b500` with slice 2b on top of it; the footprint row compares the current tips,
`f49fcefe` (slice 2a) with `00102742` (slice 2b), re-run with Obsidian quit. Apple M5 Pro, 48 GB,
macOS 26.5, on the 50,000-note `bench_vault` vault, warm, release builds, one primed `HOME` per
build.

| | before | after | gate |
|---|---:|---:|---:|
| links, live heap under `persist::notes::decode` | 78.7 MiB | 21.2 MiB | ≤ 30 MiB |
| links, live heap allocated by `Seed::build` | 0.0 MiB | 0.0 MiB | |
| `phys_footprint` at idle, median of 3 | 252.1 MiB | 188.9 MiB | ≤ 200 MiB |
| longest turn, 5 runs | 1.7 to 2.1 ms | 1.6 to 1.8 ms | ≤ 8 ms |
| cache file | 86,886,659 B | 45,774,080 B | smaller |

- The heap rows come from `MallocStackLogging=lite` and `malloc_history -allBySize`, 10 s after
  the launch line, as `scripts/heap-links.sh` takes them. The release symbols are v0-mangled
  (`7persist5notes6decode`); the script matches both the mangled and the demangled names.
- The footprint is `scripts/bench/measure.swift`'s `footprint_bytes`, the warm row of
  `bench-compare.sh --apps inkra --sizes 50000 --runs 3`, nothing else running. Before: 252.2,
  242.9 and 252.1 MiB. After: 199.0, 188.9 and 186.5 MiB. Every after run is under the gate, the
  highest by 1.0 MiB. The cache decodes in 127 to 154 ms instead of 172 to 184 ms. The next lead,
  from the spec's "Out of scope": one `NoteTable` in vault order, indexed by a
  `hashbrown::HashTable<u32>`, estimated at 11 to 20 MiB.
- The longest turn is read from the launch lines of `scripts/launch-figure.sh`, 5 runs per build.

## Feature registry and switches (2026-11-13)

The feature registry (#41, #42) and vault search on it (branch `vault-search-feature`) were meant
to cost nothing on the launch path. Two warm runs check it, on the 50,000-note `startup_vault`
vault, release builds, one primed `HOME` per build or configuration, 7 runs each, alternated.
Apple M5 Pro, 48 GB, macOS 26.5.

The A/B compares `f0e65ef7`, main before the registry, with `c58a2ea6`.

| Median of 7 | before | after |
|---|---:|---:|
| first frame | 84.5 ms | 85.1 ms |
| cache decoded | 110.5 ms | 112.1 ms |
| indexed | 228.1 ms | 228.1 ms |
| longest turn | 1.7 ms | 1.8 ms |
| rss | 161,760 kB | 161,664 kB |

The switches run launches `c58a2ea6` four times over, each with its own `.inkra/settings.json`:
every feature on, `backlinks` off, `vault_search` off, and every feature off.

| Median of 7 | on | backlinks off | vault search off | all off |
|---|---:|---:|---:|---:|
| first frame | 82.8 ms | 80.7 ms | 85.8 ms | 82.9 ms |
| cache decoded | 104.6 ms | 102.5 ms | 111.1 ms | 101.8 ms |
| indexed | 226.4 ms | 221.2 ms | 232.6 ms | 223.0 ms |
| longest turn | 1.9 ms | 1.8 ms | 1.7 ms | 1.7 ms |
| rss | 162,944 kB | 162,448 kB | 162,736 kB | 162,864 kB |

- No difference stands above the noise: `first frame` spans 74.5 to 88.3 ms and `indexed` 203.6
  to 238.2 ms within single configurations. `phys_footprint` is bimodal as above and not compared.
- The launch line's `registry` and `activated backlinks` marks fall within 0.1 ms of each other:
  activating backlinks costs under 0.1 ms. Quick open and vault search activate on their first
  command, so they cost nothing at launch.
- Not measured: the cold launch, the first `⌘⇧F`, and the backlinks rebuild while typing, where
  switching backlinks off would most likely show.
