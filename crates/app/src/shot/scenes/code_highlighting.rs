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

pub(super) fn code_highlighting_plain(stage: &mut Stage) -> WindowHandle<AppShell> {
    open_blocks(
        stage,
        &format!(
            "A prose line before the blocks.\n\n{NOPE}\n\n{NO_INFO}\n\n{INDENTED}\n\n{QUOTED}\n"
        ),
    )
}
