use std::time::Duration;

use gpui::{WindowHandle, point, px};

use crate::shell::AppShell;
use crate::shot::stage::Stage;

const BLOCKS: &str = r#"A paragraph before the blocks.

```rust
let first = "a deliberately long first line that wraps across rows so the second row shows its place " + "and carries on well past the right edge of the column";
let x = 1;
```

> A quote line before its block.
> ```sh
> echo "in a quote, long enough to run past the right edge of the column and scroll on its own"
> ls -la
> ```

- A list item before its block.

  ```py
  print("in a list")
  total = 1 + 2
  ```

```
no language here
second line
```

    indented line one
    indented line two

Body text after the blocks."#;

pub(super) fn concealed(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_note(BLOCKS);
    stage.focus(window);
    stage.keys(window, "cmd-up");
    scroll_blocks(stage, window);
    let over = stage.block_center(window, 0);
    stage.hover(window, over);
    window
}

pub(super) fn revealed(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_note(BLOCKS);
    stage.focus(window);
    stage.keys(window, "cmd-up");
    scroll_blocks(stage, window);
    stage.keys(window, "down down down shift-down shift-down");
    window
}

/// `settings.json` written after the open: the watcher reloads it, as in a live edit.
pub(super) fn wrapped(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(&[("blocks.md", BLOCKS)]);
    stage.click_tree_row(window, 0.);
    stage.focus(window);
    stage.write(".inkra/settings.json", r#"{"code_block_wrap":true}"#);
    stage.wait(Duration::from_millis(100));
    stage.keys(window, "cmd-up");
    window
}

/// The rust block well into its line, the quoted one less, so each shows its own scroll.
fn scroll_blocks(stage: &mut Stage, window: WindowHandle<AppShell>) {
    for (ix, x) in [(0, -240.), (1, -120.)] {
        let at = stage.block_center(window, ix);
        stage.scroll(window, at, point(px(x), px(0.)));
    }
}
