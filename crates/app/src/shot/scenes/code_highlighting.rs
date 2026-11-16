use gpui::WindowHandle;

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

pub(super) fn code_highlighting(stage: &mut Stage) -> WindowHandle<AppShell> {
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

pub(super) fn python(stage: &mut Stage) -> WindowHandle<AppShell> {
    one_block(stage, "python", PYTHON)
}

pub(super) fn json(stage: &mut Stage) -> WindowHandle<AppShell> {
    one_block(stage, "jsonc", JSON)
}

pub(super) fn bash(stage: &mut Stage) -> WindowHandle<AppShell> {
    one_block(stage, "bash", BASH)
}

pub(super) fn code_highlighting_plain(stage: &mut Stage) -> WindowHandle<AppShell> {
    open_blocks(
        stage,
        &format!(
            "A prose line before the blocks.\n\n{NOPE}\n\n{NO_INFO}\n\n{INDENTED}\n\n{QUOTED}\n"
        ),
    )
}
