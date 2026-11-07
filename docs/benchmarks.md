# Inkra and Obsidian, measured

A reproducible comparison of Inkra with Obsidian on the same generated
vaults, on one machine. Every figure is taken from outside both apps, with
the same tool and the same rules. The raw data, the tool and the generator
are in this repository, so anyone can run it again or contest it.

## Reproduce

```sh
./scripts/vendor-gpui.sh        # once, nothing builds before it
./scripts/bench-compare.sh      # all sizes, 5 warm runs each
```

Flags: `--runs N` (default 5), `--sizes 500,5000,50000`,
`--apps inkra,obsidian`, `--cap-first S` (default 600), `--cap-warm S`
(default 180). The script needs Obsidian in `/Applications/Obsidian.app`, and
refuses to start while any Obsidian runs.

It writes the raw runs to `target/bench/results-<timestamp>.jsonl`, one JSON
line per run after one `meta` line, and the summary to
`target/bench/summary-<timestamp>.md` and stdout: bar charts of the main
figures, then the full table. Each app's stderr goes to
`target/bench/logs-<timestamp>/`.

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

- **Inkra has no persisted index.** It scans and indexes the whole vault on
  every launch, so its warm runs do the work of a first open. Obsidian keeps
  its metadata cache in the profile, so its warm runs reuse it.
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

Full run on 2026-09-30, commit `698daa8`, Apple M5 Pro, 48 GB, macOS 26.5
(25F71). Inkra 0.1.0 (release binary, 12 MB, no `.app` bundle yet), Obsidian
1.12.7 (482 MB `.app`). Warm rows are medians of 5 runs. Memory is in MiB.
Raw lines: `target/bench/results-20260930-213650.jsonl` (not committed).

| Notes | App | Open | Window on screen (ms) | Busy until idle (s) | CPU until idle (s) | Memory at idle (MiB) | Resident (MiB) | Processes | Idle CPU (%) |
|---:|---|---|---:|---:|---:|---:|---:|---:|---:|
| 500 | Inkra | first | 157 | 0.3 | 0.29 | 73 | 90 | 1 | 0.42 |
| 500 | Inkra | warm | 159 | 0.3 | 0.28 | 50 | 79 | 1 | 0.01 |
| 500 | Obsidian | first | 471 | 1.3 | 2.52 | 304 | 593 | 4 | 0.14 |
| 500 | Obsidian | warm | 424 | 1.3 | 0.99 | 222 | 509 | 4 | 0.07 |
| 5,000 | Inkra | first | 145 | 0.5 | 1.58 | 110 | 139 | 1 | 0.01 |
| 5,000 | Inkra | warm | 150 | 0.3 | 1.74 | 106 | 134 | 1 | 0.01 |
| 5,000 | Obsidian | first | 457 | 10 | 14.98 | 441 | 788 | 4 | 0.18 |
| 5,000 | Obsidian | warm | 444 | 1.3 | 1.49 | 340 | 642 | 4 | 0.06 |
| 50,000 | Inkra | first | 141 | 1.6 | 15.02 | 462 | 557 | 1 | 0.02 |
| 50,000 | Inkra | warm | 154 | 1.6 | 19.58 | 465 | 561 | 1 | 0.03 |
| 50,000 | Obsidian | first | 476 | 64.3 | 161.43 | 881 | 1501 | 4 | 0.38 |
| 50,000 | Obsidian | warm | 426 | 13 | 7.87 | 909 | 1483 | 4 | 10.37 |

What the run shows, and what it does not:

- **Window on screen:** Inkra 141–159 ms, Obsidian 424–476 ms, at every size.
- **Memory at idle:** Inkra uses 4.4× less at 500 notes, 3.2× less at 5,000,
  2× less at 50,000. Inkra's figure at 50,000 varies between runs (453–722 MiB);
  the median is 465.
- **Work at launch, warm:** Obsidian caches its index; Inkra rebuilds its index
  on every launch. At 50,000 notes Inkra spends 19.6 CPU-seconds (1.6 s of wall
  time over every core) against Obsidian's 7.9. At 500 and 5,000 notes Inkra
  still spends less. A persisted index (roadmap, Index row 6) is the lever.
- **First open:** at 50,000 notes Obsidian is busy for 64 s and 161
  CPU-seconds; Inkra for 1.6 s and 15.
- **Idle CPU:** gpui runs its display link on every vsync while the window is
  visible, about 0.4 % of a core; a covered window stops it, which is why most
  Inkra rows read 0.01–0.04 %. The runs did not control whether the window was
  covered. Obsidian's warm 50,000-note rows idle at 5–11 % after its quiet
  window, which the run measured but did not explain.
- The first warm Obsidian run at 50,000 was still indexing (busy 161 s); the
  median of 5 absorbs it.

## Persisted index baseline (2026-11-06)

What the persisted index (docs/superpowers/specs/2026-11-06-persisted-index-design.md) is gated
against. Release build of `08afc2a8` plus the slice-0 log line, Apple M5 Pro, 48 GB, macOS 26.5.

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

Cold figures (`--cold`, `sudo -n purge` before each run) are not here. The agent has no passwordless
`sudo`, so the user runs them and adds the lines to this section.

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
