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
    ] {
        let texts = SIZES.map(|n| (n, text(sample, n)));
        bench_texts(c, group, language, &texts);
    }
}

criterion_group!(benches, bench_lex);
criterion_main!(benches);
