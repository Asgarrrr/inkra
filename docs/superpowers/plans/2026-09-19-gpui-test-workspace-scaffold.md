# gpui-test Workspace Scaffold Implementation Plan

**Status: implemented.** All 7 tasks completed, reviewed (spec compliance, code quality, red team), and merged on `feat/workspace-scaffold` — see `git log`. Step checkboxes below are left as originally written for historical reference, not live tracking.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Scaffold the `gpui-test` Cargo workspace (`core` + `app` crates) per the approved architecture design, with a tested text buffer in `core` and a minimal GPUI window in `app` that renders it, verified by a local CI script.

**Architecture:** Two-crate Cargo workspace. `core` (`gpui-test-core`) is gpui-free: a `Buffer` with byte/char offset newtypes, insert/delete, and snapshot-based undo/redo, covered by unit tests and a proptest model comparison. `app` (`gpui-test-app`) is the gpui-dependent binary: it opens a single window rendering the buffer's text. Editing/input handling (`EntityInputHandler`, UTF-16 offset conversion, cursor/selection) is explicitly **out of scope for this plan** — see the design doc's "Open questions": that work is flagged as the largest source of unknowns and is planned as a separate follow-up.

**Tech Stack:** Rust (edition 2024), Cargo workspaces, `gpui` 0.2 (verified against docs.rs and gpui.rs — Zed's UI framework), `proptest` 1.x for property-based testing.

Design doc: [`docs/superpowers/specs/2026-09-19-gpui-test-architecture-design.md`](../specs/2026-09-19-gpui-test-architecture-design.md).

---

## File structure

```
gpui-test/
  Cargo.toml                       # workspace virtual manifest + [workspace.lints]
  scripts/
    check.sh                       # local CI: fmt, clippy, gpui-isolation check, test, build
  crates/
    core/
      Cargo.toml                   # gpui-test-core, zero gpui dependency
      src/
        lib.rs                     # mod + pub use
        offset.rs                  # ByteOffset, CharOffset newtypes + tests
        buffer.rs                  # Buffer: insert/delete/undo/redo + tests
      tests/
        buffer_proptest.rs         # property test vs Vec<char> reference model
    app/
      Cargo.toml                   # gpui-test-app, depends on gpui-test-core + gpui
      src/
        main.rs                    # Application::new().run(...) + EditorView
```

`src/main.rs` and the single-crate `Cargo.toml`/`Cargo.lock` created by the earlier `cargo new` are removed in Task 1 — they predate the workspace decision.

---

### Task 1: Cargo workspace skeleton

**Files:**
- Delete: `src/main.rs`, `Cargo.lock` (root)
- Modify: `Cargo.toml` (root) — becomes a virtual workspace manifest
- Create: `crates/core/Cargo.toml`, `crates/core/src/lib.rs`
- Create: `crates/app/Cargo.toml`, `crates/app/src/main.rs`

- [ ] **Step 1: Remove the single-crate scaffold, create the crate directories**

```bash
cd ~/Desktop/gpui-test
rm -rf src Cargo.lock
mkdir -p crates/core/src crates/app/src
```

- [ ] **Step 2: Write the root workspace manifest**

Replace `Cargo.toml` (root) with:

```toml
[workspace]
resolver = "3"
members = ["crates/core", "crates/app"]

[workspace.lints.rust]
unsafe_code = "deny"
unreachable_pub = "warn"

[workspace.lints.clippy]
all = { level = "warn", priority = -1 }
pedantic = { level = "warn", priority = -1 }
unwrap_used = "deny"
expect_used = "deny"
```

- [ ] **Step 3: Create `core`**

`crates/core/Cargo.toml`:

```toml
[package]
name = "gpui-test-core"
version = "0.1.0"
edition = "2024"

[lints]
workspace = true
```

`crates/core/src/lib.rs`:

```rust
```

(empty for now — populated in Task 2)

- [ ] **Step 4: Create `app`**

`crates/app/Cargo.toml`:

```toml
[package]
name = "gpui-test-app"
version = "0.1.0"
edition = "2024"

[dependencies]
gpui-test-core = { path = "../core" }

[lints]
workspace = true
```

`crates/app/src/main.rs`:

```rust
fn main() {
    println!("gpui-test-app scaffold: workspace wired, core linked next.");
}
```

- [ ] **Step 5: Verify the workspace builds and runs**

Run: `cargo build --workspace`
Expected: `Compiling gpui-test-core v0.1.0 ...`, `Compiling gpui-test-app v0.1.0 ...`, `Finished` — exit 0.

Run: `cargo run -p gpui-test-app`
Expected: prints `gpui-test-app scaffold: workspace wired, core linked next.`

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "Scaffold Cargo workspace: core + app crates"
```

---

### Task 2: Offset newtypes (`core`)

**Files:**
- Create: `crates/core/src/offset.rs`
- Modify: `crates/core/src/lib.rs`

- [ ] **Step 1: Write the failing test**

`crates/core/src/offset.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_and_char_offsets_are_distinct_types_with_value_equality() {
        let a = ByteOffset(3);
        let b = ByteOffset(3);
        assert_eq!(a, b);

        let c = CharOffset(3);
        assert_eq!(c.0, a.0);
    }

    #[test]
    fn offsets_order_by_value() {
        assert!(ByteOffset(1) < ByteOffset(2));
        assert!(CharOffset(5) > CharOffset(4));
    }
}
```

`crates/core/src/lib.rs`:

```rust
mod offset;
```

- [ ] **Step 2: Run test, verify it fails**

Run: `cargo test -p gpui-test-core`
Expected: FAIL to compile — `error[E0433]: failed to resolve: use of undeclared type 'ByteOffset'` (or equivalent unresolved-name errors for `ByteOffset`/`CharOffset`).

- [ ] **Step 3: Implement the newtypes**

Full contents of `crates/core/src/offset.rs`:

```rust
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ByteOffset(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CharOffset(pub usize);

impl fmt::Display for ByteOffset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for CharOffset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_and_char_offsets_are_distinct_types_with_value_equality() {
        let a = ByteOffset(3);
        let b = ByteOffset(3);
        assert_eq!(a, b);

        let c = CharOffset(3);
        assert_eq!(c.0, a.0);
    }

    #[test]
    fn offsets_order_by_value() {
        assert!(ByteOffset(1) < ByteOffset(2));
        assert!(CharOffset(5) > CharOffset(4));
    }
}
```

`crates/core/src/lib.rs`:

```rust
mod offset;

pub use offset::{ByteOffset, CharOffset};
```

- [ ] **Step 4: Run test, verify it passes**

Run: `cargo test -p gpui-test-core`
Expected: PASS — `test offset::tests::byte_and_char_offsets_are_distinct_types_with_value_equality ... ok`, `test offset::tests::offsets_order_by_value ... ok`, `test result: ok. 2 passed`.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "Add ByteOffset/CharOffset newtypes to core"
```

---

### Task 3: `Buffer` insert/delete (`core`)

**Files:**
- Create: `crates/core/src/buffer.rs`
- Modify: `crates/core/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

`crates/core/src/buffer.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::offset::CharOffset;

    #[test]
    fn new_buffer_holds_initial_text() {
        let buf = Buffer::new("hello");
        assert_eq!(buf.text(), "hello");
    }

    #[test]
    fn insert_at_char_offset_splits_on_char_boundary_not_byte_boundary() {
        // "héllo": h(0) é(1) l(2) l(3) o(4) — 'é' is 2 bytes, 1 char.
        let mut buf = Buffer::new("héllo");
        buf.insert(CharOffset(2), "X");
        assert_eq!(buf.text(), "héXllo");
    }

    #[test]
    fn insert_at_end_appends() {
        let mut buf = Buffer::new("abc");
        buf.insert(CharOffset(3), "d");
        assert_eq!(buf.text(), "abcd");
    }

    #[test]
    fn delete_range_removes_chars_by_char_offset() {
        let mut buf = Buffer::new("héllo");
        buf.delete(CharOffset(1), CharOffset(3)); // removes "él"
        assert_eq!(buf.text(), "hlo");
    }
}
```

`crates/core/src/lib.rs`:

```rust
mod buffer;
mod offset;

pub use offset::{ByteOffset, CharOffset};
```

- [ ] **Step 2: Run tests, verify they fail**

Run: `cargo test -p gpui-test-core`
Expected: FAIL to compile — `error[E0433]: failed to resolve: use of undeclared type 'Buffer'`.

- [ ] **Step 3: Implement `Buffer`**

Full contents of `crates/core/src/buffer.rs`:

```rust
use crate::offset::{ByteOffset, CharOffset};

pub struct Buffer {
    text: String,
}

impl Buffer {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn char_len(&self) -> usize {
        self.text.chars().count()
    }

    pub fn byte_offset(&self, char_offset: CharOffset) -> ByteOffset {
        let byte = self
            .text
            .char_indices()
            .nth(char_offset.0)
            .map(|(b, _)| b)
            .unwrap_or(self.text.len());
        ByteOffset(byte)
    }

    pub fn insert(&mut self, at: CharOffset, s: &str) {
        let byte = self.byte_offset(at);
        self.text.insert_str(byte.0, s);
    }

    pub fn delete(&mut self, start: CharOffset, end: CharOffset) {
        debug_assert!(start <= end, "delete range start must not exceed end");
        let start_byte = self.byte_offset(start);
        let end_byte = self.byte_offset(end);
        self.text.replace_range(start_byte.0..end_byte.0, "");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offset::CharOffset;

    #[test]
    fn new_buffer_holds_initial_text() {
        let buf = Buffer::new("hello");
        assert_eq!(buf.text(), "hello");
    }

    #[test]
    fn insert_at_char_offset_splits_on_char_boundary_not_byte_boundary() {
        let mut buf = Buffer::new("héllo");
        buf.insert(CharOffset(2), "X");
        assert_eq!(buf.text(), "héXllo");
    }

    #[test]
    fn insert_at_end_appends() {
        let mut buf = Buffer::new("abc");
        buf.insert(CharOffset(3), "d");
        assert_eq!(buf.text(), "abcd");
    }

    #[test]
    fn delete_range_removes_chars_by_char_offset() {
        let mut buf = Buffer::new("héllo");
        buf.delete(CharOffset(1), CharOffset(3));
        assert_eq!(buf.text(), "hlo");
    }
}
```

`crates/core/src/lib.rs`:

```rust
mod buffer;
mod offset;

pub use buffer::Buffer;
pub use offset::{ByteOffset, CharOffset};
```

- [ ] **Step 4: Run tests, verify they pass**

Run: `cargo test -p gpui-test-core`
Expected: PASS — 4 new tests in `buffer::tests` plus the 2 from Task 2, `test result: ok. 6 passed`.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "Add Buffer insert/delete with byte/char offset conversion"
```

---

### Task 4: `Buffer` undo/redo (`core`)

**Files:**
- Modify: `crates/core/src/buffer.rs`

- [ ] **Step 1: Write the failing tests**

Add these four tests inside the existing `mod tests` block in `crates/core/src/buffer.rs` (before its closing `}`):

```rust
    #[test]
    fn undo_reverts_last_edit() {
        let mut buf = Buffer::new("abc");
        buf.insert(CharOffset(3), "d");
        assert_eq!(buf.text(), "abcd");
        assert!(buf.undo());
        assert_eq!(buf.text(), "abc");
    }

    #[test]
    fn undo_with_empty_history_returns_false() {
        let mut buf = Buffer::new("abc");
        assert!(!buf.undo());
    }

    #[test]
    fn redo_reapplies_undone_edit() {
        let mut buf = Buffer::new("abc");
        buf.insert(CharOffset(3), "d");
        buf.undo();
        assert!(buf.redo());
        assert_eq!(buf.text(), "abcd");
    }

    #[test]
    fn new_edit_after_undo_clears_redo_stack() {
        let mut buf = Buffer::new("abc");
        buf.insert(CharOffset(3), "d"); // "abcd"
        buf.undo(); // back to "abc"
        buf.insert(CharOffset(0), "X"); // "Xabc" — new edit
        assert!(!buf.redo()); // old redo (re-adding "d") is gone
    }
```

- [ ] **Step 2: Run tests, verify they fail**

Run: `cargo test -p gpui-test-core`
Expected: FAIL to compile — `error[E0599]: no method named 'undo' found for struct 'Buffer'`.

- [ ] **Step 3: Implement undo/redo**

Full contents of `crates/core/src/buffer.rs`:

```rust
use crate::offset::{ByteOffset, CharOffset};

pub struct Buffer {
    text: String,
    undo_stack: Vec<String>,
    redo_stack: Vec<String>,
}

impl Buffer {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn char_len(&self) -> usize {
        self.text.chars().count()
    }

    pub fn byte_offset(&self, char_offset: CharOffset) -> ByteOffset {
        let byte = self
            .text
            .char_indices()
            .nth(char_offset.0)
            .map(|(b, _)| b)
            .unwrap_or(self.text.len());
        ByteOffset(byte)
    }

    fn snapshot_before_edit(&mut self) {
        self.undo_stack.push(self.text.clone());
        self.redo_stack.clear();
    }

    pub fn insert(&mut self, at: CharOffset, s: &str) {
        self.snapshot_before_edit();
        let byte = self.byte_offset(at);
        self.text.insert_str(byte.0, s);
    }

    pub fn delete(&mut self, start: CharOffset, end: CharOffset) {
        debug_assert!(start <= end, "delete range start must not exceed end");
        self.snapshot_before_edit();
        let start_byte = self.byte_offset(start);
        let end_byte = self.byte_offset(end);
        self.text.replace_range(start_byte.0..end_byte.0, "");
    }

    pub fn undo(&mut self) -> bool {
        match self.undo_stack.pop() {
            Some(prev) => {
                self.redo_stack.push(std::mem::replace(&mut self.text, prev));
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self) -> bool {
        match self.redo_stack.pop() {
            Some(next) => {
                self.undo_stack.push(std::mem::replace(&mut self.text, next));
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offset::CharOffset;

    #[test]
    fn new_buffer_holds_initial_text() {
        let buf = Buffer::new("hello");
        assert_eq!(buf.text(), "hello");
    }

    #[test]
    fn insert_at_char_offset_splits_on_char_boundary_not_byte_boundary() {
        let mut buf = Buffer::new("héllo");
        buf.insert(CharOffset(2), "X");
        assert_eq!(buf.text(), "héXllo");
    }

    #[test]
    fn insert_at_end_appends() {
        let mut buf = Buffer::new("abc");
        buf.insert(CharOffset(3), "d");
        assert_eq!(buf.text(), "abcd");
    }

    #[test]
    fn delete_range_removes_chars_by_char_offset() {
        let mut buf = Buffer::new("héllo");
        buf.delete(CharOffset(1), CharOffset(3));
        assert_eq!(buf.text(), "hlo");
    }

    #[test]
    fn undo_reverts_last_edit() {
        let mut buf = Buffer::new("abc");
        buf.insert(CharOffset(3), "d");
        assert_eq!(buf.text(), "abcd");
        assert!(buf.undo());
        assert_eq!(buf.text(), "abc");
    }

    #[test]
    fn undo_with_empty_history_returns_false() {
        let mut buf = Buffer::new("abc");
        assert!(!buf.undo());
    }

    #[test]
    fn redo_reapplies_undone_edit() {
        let mut buf = Buffer::new("abc");
        buf.insert(CharOffset(3), "d");
        buf.undo();
        assert!(buf.redo());
        assert_eq!(buf.text(), "abcd");
    }

    #[test]
    fn new_edit_after_undo_clears_redo_stack() {
        let mut buf = Buffer::new("abc");
        buf.insert(CharOffset(3), "d");
        buf.undo();
        buf.insert(CharOffset(0), "X");
        assert!(!buf.redo());
    }
}
```

- [ ] **Step 4: Run tests, verify they pass**

Run: `cargo test -p gpui-test-core`
Expected: PASS — `test result: ok. 10 passed` (8 in `buffer::tests` + 2 in `offset::tests`).

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "Add snapshot-based undo/redo to Buffer"
```

---

### Task 5: Property test vs. reference model (`core`)

**Files:**
- Modify: `crates/core/Cargo.toml`
- Create: `crates/core/tests/buffer_proptest.rs`

- [ ] **Step 1: Add the `proptest` dev-dependency**

`crates/core/Cargo.toml`:

```toml
[package]
name = "gpui-test-core"
version = "0.1.0"
edition = "2024"

[dev-dependencies]
proptest = "1"

[lints]
workspace = true
```

- [ ] **Step 2: Write the property test**

`crates/core/tests/buffer_proptest.rs`:

```rust
use gpui_test_core::{Buffer, CharOffset};
use proptest::prelude::*;

#[derive(Debug, Clone)]
enum Op {
    Insert(usize, String),
    Delete(usize, usize),
}

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        (any::<usize>(), "[a-zA-Z]{1,3}").prop_map(|(at, text)| Op::Insert(at, text)),
        (any::<usize>(), any::<usize>()).prop_map(|(a, b)| Op::Delete(a.min(b), a.max(b))),
    ]
}

proptest! {
    #[test]
    fn buffer_matches_reference_model(ops in prop::collection::vec(op_strategy(), 0..30)) {
        let mut buf = Buffer::new("");
        let mut model: Vec<char> = Vec::new();

        for op in ops {
            match op {
                Op::Insert(at, text) => {
                    let at = at % (model.len() + 1);
                    buf.insert(CharOffset(at), &text);
                    model.splice(at..at, text.chars());
                }
                Op::Delete(start, end) => {
                    if model.is_empty() {
                        continue;
                    }
                    let start = start % model.len();
                    let end = (start + 1 + end % model.len().max(1)).min(model.len());
                    buf.delete(CharOffset(start), CharOffset(end));
                    model.splice(start..end, std::iter::empty());
                }
            }

            let expected: String = model.iter().collect();
            prop_assert_eq!(buf.text(), expected.as_str());
        }
    }
}
```

This generates random insert/delete sequences with arbitrary `usize` offsets, then clamps each offset into the currently-valid range with modulo before applying it — a standard proptest pattern for keeping generated indices in bounds without hand-writing an interdependent strategy. After every operation, the real `Buffer`'s text is compared against a `Vec<char>` reference model built with the same clamped indices.

- [ ] **Step 3: Run the property test**

Run: `cargo test -p gpui-test-core --test buffer_proptest`
Expected: PASS — `test buffer_matches_reference_model ... ok`, `test result: ok. 1 passed`.

If it fails, proptest writes the shrunk failing case to `crates/core/tests/proptest-regressions/buffer_proptest.txt` — read the failure, fix the bug in `Buffer`, and commit that regressions file once the test passes again, so the case is locked in permanently.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "Add proptest model comparison for Buffer"
```

---

### Task 6: Minimal GPUI window (`app`)

**Files:**
- Modify: `crates/app/Cargo.toml`
- Modify: `crates/app/src/main.rs`

This task wires `app` to `gpui` and to `core`'s `Buffer`, and opens a window that renders the buffer's text read-only. It does **not** implement editing, `EntityInputHandler`, or cursor/selection — that is out of scope for this plan (see the plan header).

- [ ] **Step 1: Add the `gpui` dependency**

`crates/app/Cargo.toml`:

```toml
[package]
name = "gpui-test-app"
version = "0.1.0"
edition = "2024"

[dependencies]
gpui-test-core = { path = "../core" }
gpui = "0.2"

[lints]
workspace = true
```

- [ ] **Step 2: Replace the placeholder with a minimal window**

`crates/app/src/main.rs`:

```rust
use gpui::{div, prelude::*, px, rgb, size, App, Application, Bounds, Context, Window, WindowBounds, WindowOptions};
use gpui_test_core::Buffer;

struct EditorView {
    buffer: Buffer,
}

impl Render for EditorView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size(px(500.0))
            .bg(rgb(0x1e1e1e))
            .text_color(rgb(0xffffff))
            .p_4()
            .child(self.buffer.text().to_string())
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(500.0), px(500.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| EditorView {
                    buffer: Buffer::new("gpui-test scaffold — core wired to app."),
                })
            },
        )
        .unwrap();
    });
}
```

This bootstrap sequence (`Application::new().run(...)`, `cx.open_window`, `Render` impl returning `impl IntoElement`, `cx.new(...)`) is verified against the canonical example on gpui.rs and the `gpui` 0.2.2 docs.rs page, not invented.

- [ ] **Step 3: Verify it builds and runs**

Run: `cargo build -p gpui-test-app`
Expected: PASS. Note: the first build compiles `gpui` itself (a large dependency with native Metal shader compilation on macOS) and can take several minutes — this is expected, not a hang.

Run: `cargo run -p gpui-test-app`
Expected: a window opens (500×500, dark background) showing the text "gpui-test scaffold — core wired to app." Confirm the window appears, then close it.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "Open a minimal GPUI window rendering the core Buffer"
```

---

### Task 7: Local CI script

**Files:**
- Create: `scripts/check.sh`

- [ ] **Step 1: Write the script**

`scripts/check.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

echo "==> cargo fmt --check"
cargo fmt --all -- --check

echo "==> cargo clippy"
cargo clippy --workspace --all-targets --all-features -- -D warnings

echo "==> gpui isolation check (core must not depend on gpui)"
if cargo tree -p gpui-test-core -e normal | grep -qE '(^|[[:space:]])gpui v[0-9]'; then
  echo "ERROR: gpui-test-core depends on gpui — core must stay gpui-free" >&2
  exit 1
fi

echo "==> cargo test"
cargo test --workspace

echo "==> cargo build --release"
cargo build --workspace --release

echo "==> all checks passed"
```

- [ ] **Step 2: Make it executable**

```bash
chmod +x scripts/check.sh
```

- [ ] **Step 3: Run it**

Run: `./scripts/check.sh`
Expected: every section prints its `==>` header and succeeds; final line `==> all checks passed`; exit code 0.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "Add local CI check script"
```

---

## Explicitly out of scope for this plan

Per the design doc's deferred list, and per the plan's own goal:

- `EntityInputHandler`, UTF-16 offset conversion, cursor movement, selection — the next, harder phase.
- `gpui::TestAppContext`/`#[gpui::test]` coverage for `app` — deferred until there is interactive behavior worth testing; `EditorView` today is a static render with nothing to assert beyond what Task 6's manual run already confirms.
- Everything in the design doc's "Rejected and deferred options" (3-crate split, blanket `thiserror`, shared-types crate, `test-utils` crate, full `cargo-deny`, `cargo machete`, Zed-style `core` sub-crates).
