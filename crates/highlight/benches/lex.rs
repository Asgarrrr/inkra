use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use inkra_highlight::{Language, highlight};
use inkra_text::ByteOffset;

const RUST: [(usize, &str); 3] = [
    (20, include_str!("fixtures/rust-20.txt")),
    (500, include_str!("fixtures/rust-500.txt")),
    (5000, include_str!("fixtures/rust-5000.txt")),
];

/// The line counts of the Rust texts, for the other languages' texts.
const SIZES: [usize; 3] = [20, 500, 5000];

const PYTHON: &str = r#"# Index the vault and print what changed.
import os
from pathlib import Path

@dataclass(frozen=True)
class Note(Base):
    """One markdown file in the vault,
    with its links."""
    MAX_LINKS: int = 1_000

    def links(self, text: str) -> list[str]:
        return [m.group(1) for m in LINK.finditer(text) if m]

async def scan(root: Path, *, depth=3) -> None:
    for path in sorted(root.rglob("*.md")):
        note = Note(path.stem, 0x1F, ratio=1.5e-3)
        print(f"{note.name}: {len(note.links(path.read_text()))}")
        await asyncio.sleep(0)
    raise SystemExit(r'\d+ done' if depth > 0 else None)
"#;

const JSON: &str = r#"{
  // The vault's settings, as jsonc allows.
  "name": "inkra",
  "version": 3,
  "ratio": -1.5e3,
  "tags": ["notes", "rust", "gpui"],
  "draft": false,
  "parent": null,
  "theme": {
    "dark": "flexoki-dark",
    "light": "flexoki-light", /* the default */
    "sizes": [12, 14, 16.5]
  },
  "panes": [
    { "id": "files", "open": true, "width": 240 },
    { "id": "backlinks", "open": false, "width": 280 }
  ],
  "escape": "a \"quoted\" \\ path",
  "empty": {}
},
"#;

const BASH: &str = r#"#!/usr/bin/env bash
# Build the release and copy it out.
set -euo pipefail
NAME="inkra-${VERSION:-dev}"
for file in target/release/*.dmg; do
  if [[ -f "$file" && $COUNT -gt 0 ]]; then
    cp -v "$file" "$DEST/" 2>&1 | tee -a log.txt
  elif [ ! -d "$DEST" ]; then
    mkdir -p "$DEST" && echo 'made it'
  fi
done
case "$1" in
  build|b) cargo build --release ;;
  *) echo "usage: $0 build" >&2; exit 1 ;;
esac
cat <<EOF > notes.txt
Built $NAME on $(date) with $((COUNT + 1)) files.
EOF
total=$(( COUNT * 2 ))
printf '%s\n' "$total"
"#;

const JAVASCRIPT: &str = r#"// Fetch the notes and render them.
import { useState } from "react";

const PAGE_SIZE = 20;

export async function fetchNotes(query, page = 0) {
  const url = `/api/notes?q=${encodeURIComponent(query)}&p=${page}`;
  const res = await fetch(url, { method: "GET" });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return (await res.json()).filter((n) => /\.md$/i.test(n.path));
}

export function NoteList({ notes, onOpen }) {
  const [open, setOpen] = useState(null);
  return (
    <ul className="notes">
      {notes.map((note) => (
        <li key={note.id} onClick={() => onOpen(note)}>{note.title}</li>
      ))}
    </ul>
  );
}
"#;

const TYPESCRIPT: &str = r"// A typed note store.
export interface Note<T = string> {
  readonly id: number;
  title?: string;
  body: T;
}

type Index = Map<string, Array<Note>>;

export abstract class Store<T extends Note> implements Iterable<T> {
  private notes: T[] = [];
  static readonly MAX = 1_000;

  constructor(private readonly index: Index) {}

  find(id: number): T | undefined {
    return this.notes.find((n) => n.id === id) as T;
  }
}

const first = <T,>(xs: T[]): T => xs[0];
";

const CSS: &str = r#"/* Note cards, across
   two lines. */
@import url("theme.css") screen;
:root { --accent: #3aa99f; }

.card > h2:hover,
a[href^="http"]::after {
  color: var(--accent);
  margin: 0 auto -1.5em;
  font: 600 14px/1.4 "Inter", sans-serif !important;
  background: url(img/bg.png) no-repeat;
  width: calc(100% - 2rem);
}

@media (min-width: 600px) and (prefers-color-scheme: dark) {
  .card { padding: 8px; }
}
@keyframes fade {
  from { opacity: 0; }
}
"#;

const HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset=utf-8>
  <!-- Styles for the card,
       across lines -->
  <style>
    .card { color: #3aa99f; }
  </style>
</head>
<body class="note"
      data-id='7'>
  <p>Fish &amp; chips &copy; 2026 <br/></p>
  <script type="module">
    const n = 1 / 2;
    if (n < 1) { el.textContent = `a ${n}`; }
  </script>
  <p>after</p>
</body>
</html>
"#;

/// `sample`'s lines, repeated up to `lines` lines.
fn text(sample: &str, lines: usize) -> String {
    let mut text: String = sample
        .lines()
        .cycle()
        .take(lines)
        .flat_map(|line| [line, "\n"])
        .collect();
    text.pop();
    text
}

fn lines(text: &str) -> Vec<(ByteOffset, &str)> {
    let mut start = 0;
    text.split('\n')
        .map(|line| {
            let at = ByteOffset::new(start);
            start += line.len() + 1;
            (at, line)
        })
        .collect()
}

fn bench_texts(c: &mut Criterion, group: &str, language: Language, texts: &[(usize, String)]) {
    let mut group = c.benchmark_group(group);
    for (n, text) in texts {
        let lines = lines(text);
        let mut out = Vec::new();
        group.throughput(Throughput::Bytes(text.len() as u64));
        group.bench_function(BenchmarkId::from_parameter(n), |b| {
            b.iter(|| {
                out.clear();
                highlight(language, lines.iter().copied(), black_box(&mut out));
                black_box(&out);
            });
        });
    }
    group.finish();
}

fn bench_lex(c: &mut Criterion) {
    let rust = RUST.map(|(n, text)| (n, text.to_owned()));
    bench_texts(c, "lex", Language::Rust, &rust);
    for (group, language, sample) in [
        ("lex_python", Language::Python, PYTHON),
        ("lex_json", Language::Json, JSON),
        ("lex_bash", Language::Bash, BASH),
        ("lex_javascript", Language::JavaScript, JAVASCRIPT),
        ("lex_typescript", Language::TypeScript, TYPESCRIPT),
        ("lex_css", Language::Css, CSS),
        ("lex_html", Language::Html, HTML),
    ] {
        let texts = SIZES.map(|n| (n, text(sample, n)));
        bench_texts(c, group, language, &texts);
    }
}

criterion_group!(benches, bench_lex);
criterion_main!(benches);
