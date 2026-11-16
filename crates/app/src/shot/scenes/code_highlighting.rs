use gpui::WindowHandle;

use super::Scene;
use crate::shell::AppShell;
use crate::shot::stage::Stage;

/// Fixed: S6 renders this same text under tree-sitter and compares.
const RUST: &str = r#"// A line comment.
/* A block comment. */
#[derive(Debug)]
pub struct Point<T> { x: T }

impl<T: Copy> Point<T> {
    const MAX: u32 = 1_000;
    fn new(x: T) -> Self { Self { x } }
    fn label(&self) -> &'static str { "a string" }
    fn initial() -> char { 'p' }
    fn get<'a>(&'a self) -> &'a T { &self.x }
    fn show(&self) { println!("{}", self.x.len()); }
}"#;

const PYTHON: &str = r#"# A line comment.
@dataclass(frozen=True)
class Point(Base):
    """A docstring
    across two lines."""
    MAX: int = 1_000

    def label(self, name: str) -> str:
        return f"{name}!" if name else None

async def main():
    await run(Point(1.5, 0x1F), key=r'\d+')"#;

const JSON: &str = r#"{
  // A comment, as jsonc allows.
  "name": "inkra",
  "ratio": -1.5e3,
  "tags": ["notes", "rust"],
  "draft": false,
  "parent": null
}"#;

const BASH: &str = r#"#!/usr/bin/env bash
set -euo pipefail
NAME="inkra-$VERSION"
for file in target/*.dmg; do
  if [[ -f "$file" && $COUNT -gt 0 ]]; then
    cp -v "$file" "${DEST:-/tmp}" 2>&1 | tee x
  fi
done
cat <<EOF > notes.txt
Built $NAME on $(date)
EOF"#;

const JAVASCRIPT: &str = r#"// A line comment.
import { useState } from "react";
const PAGE = 20, re = /\.md$/i;
export async function load(q, page = 0) {
  const url = `/api?q=${q}&p=${page * PAGE}`;
  const res = await fetch(url, { cache: 1 });
  const list = await res.json();
  return list.filter((n) => re.test(n.path));
}
export const List = ({ notes }) => (
  <ul className="notes">
    {notes.map((n) => <li key={n.id}>
      {n.title} &mdash; {n.words / 250}
    </li>)}
  </ul>
);"#;

const TYPESCRIPT: &str = r"/* A typed note store. */
export interface Note<T = string> {
  readonly id: number;
  title?: string;
  body: T;
}
type Index = Map<string, Array<Note>>;
export class Store<T extends Note>
    implements Iterable<T> {
  private notes: T[] = [];
  find(id: number): T | undefined {
    return this.notes.find(n => n.id === id);
  }
}
const first = <T>(xs: T[]): T => xs[0];";

const TSX: &str = r#"type Props = {
  notes: Note[];
  onOpen?: (id: number) => void;
};
function List({ notes, onOpen }: Props) {
  const [id, setId] = useState<number>(-1);
  return (
    <ul className="notes">
      {notes.map((note) => (
        <li key={note.id}
          onClick={() => onOpen?.(note.id)}>
          {note.title}
        </li>
      ))}
    </ul>
  );
}"#;

const CSS: &str = r#"/* Note cards. */
@import url("theme.css") screen;
:root { --accent: #3aa99f; }
.card > h2:hover,
a[href^="http"]::after {
  color: var(--accent);
  margin: 0 auto -1.5em;
  font: 14px/1.4 "Inter", serif !important;
  width: calc(100% - 2rem);
}
@media screen and (min-width: 600px) {
  .card { padding: 8px; }
}"#;

const HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset=utf-8>
  <!-- The card's style. -->
  <style>.card { color: #3aa99f; }</style>
</head>
<body class="note" data-id='7'>
  <p>Fish &amp; chips &copy; 2026<br/></p>
  <script type="module">
    if (n < 1) el.textContent = `a ${n / 2}`;
  </script>
</body>
</html>"#;

const NOPE: &str = "```nope\nlet mut nope = 1;\n```";

const NO_INFO: &str = "```\nlet mut plain = 1;\n```";

const INDENTED: &str = "    let mut indented = 1;";

const QUOTED: &str = "> ```rust\n> let mut quoted = 1;\n> ```";

/// The caret sits on the first prose line, so every fence is concealed.
fn open_blocks(stage: &mut Stage, text: &str) -> WindowHandle<AppShell> {
    let window = stage.open_vault(&[("blocks.md", text)]);
    stage.click_tree_row(window, 0.);
    stage.focus(window);
    stage.keys(window, "cmd-up");
    window
}

fn code_highlighting(stage: &mut Stage) -> WindowHandle<AppShell> {
    open_blocks(
        stage,
        &format!("A prose line before the block.\n\n```rust\n{RUST}\n```\n"),
    )
}

/// One block per scene: the viewport holds one block of this size, not three.
fn one_block(stage: &mut Stage, info: &str, code: &str) -> WindowHandle<AppShell> {
    open_blocks(
        stage,
        &format!("A prose line before the block.\n\n```{info}\n{code}\n```\n"),
    )
}

fn python(stage: &mut Stage) -> WindowHandle<AppShell> {
    one_block(stage, "python", PYTHON)
}

fn json(stage: &mut Stage) -> WindowHandle<AppShell> {
    one_block(stage, "jsonc", JSON)
}

fn bash(stage: &mut Stage) -> WindowHandle<AppShell> {
    one_block(stage, "bash", BASH)
}

fn javascript(stage: &mut Stage) -> WindowHandle<AppShell> {
    one_block(stage, "jsx", JAVASCRIPT)
}

fn typescript(stage: &mut Stage) -> WindowHandle<AppShell> {
    one_block(stage, "ts", TYPESCRIPT)
}

fn tsx(stage: &mut Stage) -> WindowHandle<AppShell> {
    one_block(stage, "tsx", TSX)
}

fn css(stage: &mut Stage) -> WindowHandle<AppShell> {
    one_block(stage, "css", CSS)
}

fn html(stage: &mut Stage) -> WindowHandle<AppShell> {
    one_block(stage, "html", HTML)
}

fn code_highlighting_plain(stage: &mut Stage) -> WindowHandle<AppShell> {
    open_blocks(
        stage,
        &format!(
            "A prose line before the blocks.\n\n{NOPE}\n\n{NO_INFO}\n\n{INDENTED}\n\n{QUOTED}\n"
        ),
    )
}

pub(super) const SCENES: [Scene; 10] = [
    Scene {
        name: "code-highlighting",
        shows: "caret in the first paragraph, every fence concealed: every Rust token class coloured in the rust block",
        open: code_highlighting,
    },
    Scene {
        name: "code-highlighting-python",
        shows: "the python block: comment, decorator, class and def names, docstring across lines, constants, f and r strings, keywords",
        open: python,
    },
    Scene {
        name: "code-highlighting-json",
        shows: "the jsonc block: a comment, keys as properties against string values, numbers, true, false and null",
        open: json,
    },
    Scene {
        name: "code-highlighting-bash",
        shows: "the bash block: shebang comment, commands as functions, flags and numbers as constants, $ expansions, keywords, a heredoc body",
        open: bash,
    },
    Scene {
        name: "code-highlighting-javascript",
        shows: "the jsx block: a regex against a division, a template with substitutions, JSX tags, attributes and an entity, keys, calls, constants",
        open: javascript,
    },
    Scene {
        name: "code-highlighting-typescript",
        shows: "the ts block: interface, class and type names, generic lists, annotations, optional members, modifiers, a generic arrow",
        open: typescript,
    },
    Scene {
        name: "code-highlighting-tsx",
        shows: "the tsx block: a type literal, a generic call, JSX tags and attributes inside a typed component",
        open: tsx,
    },
    Scene {
        name: "code-highlighting-css",
        shows: "the css block: an at-rule with a url, tags and combinators in selectors, properties, values, units, a colour, !important",
        open: css,
    },
    Scene {
        name: "code-highlighting-html",
        shows: "the html block: doctype, tags, attributes and values, a comment, entities, a style block as CSS and a script block as JavaScript",
        open: html,
    },
    Scene {
        name: "code-highlighting-plain",
        shows: "the nope, no-info and indented blocks plain md_code_block; the quoted rust block coloured, its > prefix not",
        open: code_highlighting_plain,
    },
];
